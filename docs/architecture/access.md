# Access beyond this device

**Status:** steps 1 to 4 of the [build order](#build-order) (standing shares in the CLI and the app, `dotsafe daemon --share` for unattended agents, and approve mode) are built; per-run machine logs are proposed. The format is in [vault-format.md](../vault-format.md#record-version-6-shares). Where the build differs from this text:

- The token carries the path's 16 bytes and the bucket's public URL rather than the whole snapshot URL (shorter, same information).
- `pushed: { sha256: "" }` records that a share's objects were deleted, so a device that couldn't reach the bucket at revoke or expiry knows to try again.
- The CLI adds `dotsafe share setup` (name the bucket, check it's public, unlisted and accepts lifecycle rules) and `dotsafe share push` (retry now).
- There's no limit on the number of shares; [licensing](#licensing) is only that the creating device is licensed.
- The app pushes shares a few seconds after the vault changes (an edit there, the CLI, or a sync from another device) and checks hourly to renew leases, instead of hooking each write. It never shows the token: the dialog shows its start and end and copies it in Rust.
- `dotsafe daemon --share` is the shared broker with a share's snapshot as its source (`broker::Host::share`). It fetches the snapshot when it starts, again before each run, and for listing once it's 30 seconds old. If the bucket can't be reached it keeps using the last copy while that's within its expiry and lease; a missing snapshot (revoked or expired) ends it. It's read-only: `generate_secret` is refused. Only the built-in proxy profiles apply, as with `run --share`, since custom ones live in the vault.
- Approvals there work as in the plain daemon (its terminal, 5 minute wait), plus `dotsafe approve --share`, which tags answers with a key derived from the share key (HKDF, `dotsafe/v6/share-approve`) instead of the vault key. Whoever holds the token can already read every value in the share, and the commands the daemon runs never get it. `--unattended` allows every request the refusal list lets through, for when nobody is there. Requests are logged, names only, in the daemon's output (`daemon.log` when detached), since there's no vault to write to. `--detach` hands the token to the background copy on a pipe, not in its environment.
- **Approve mode needs only the token.** Instead of a storage key for the machine, the user's devices write a sealed mailbox (`m/<path>/mailbox`) with the share's variable names and presigned URLs, signed for 7 days and rewritten daily: a PUT for the doorbell, and for each of 4 request slots a PUT for the request and a DELETE for its answer. This was the first open question below. Format: [vault-format.md](../vault-format.md#approve-mode-mailboxes).
- The app watches each approve share's doorbell through its ETag (a HEAD request every 2 seconds while unlocked) and shows requests in the agent approval dialog, with the machine's name, Touch ID checked in Rust. `dotsafe share approve` answers from any of the user's devices with the master password, never the unlock cache. The answer is "once" or "for a while" (15 minutes or an hour), which the machine keeps in memory for that exact command instead of a short-lived snapshot.
- `dotsafe run --share` and `dotsafe daemon --share` both work in approve mode: they find the mailbox when there's no snapshot, and each run waits (5 minutes, or `--wait`) for an answer. `--unattended` refuses an approve share. Approvals are logged on the device that answers, as runs with actor `share:<name>`.

dotsafe decides which secrets a person or agent can use on the user's own devices. This document extends that to machines dotsafe doesn't run on as a full client: CI, servers, cloud VMs and cloud agents. The question is the same everywhere: which secrets, for how long, approved how, logged where, and what happens when access ends.

## The problem

Code runs in more places than the laptop, and each place gets secrets a different way:

- **CI:** the master password or the whole `.env` as a CI secret.
- **Servers and cron jobs:** `dotsafe daemon`, unlocked with the master password once per boot ([agents.md](agents.md#unattended-servers)). Better, but the box holds the whole vault key in memory.
- **Cloud VMs and cloud agents:** values pasted into the provider's settings page, or a `.env` copied to the box.

In every case the machine ends up with more than it needs (often the whole vault), access doesn't expire, and removing it means rotating everything by hand.

## Goals

- Give a machine **some environments**, never the whole vault, and never the vault key.
- **One value to hand over.** A machine gets a single token, not a token plus storage credentials.
- Access that **expires** and can be **revoked** from the user's devices.
- **Rotations reach machines** the way they reach deploy targets, and an old key is never retired while a machine still has it.
- Optionally, **approve each use** with Touch ID, for machines that should never hold standing access.
- **Log** what each machine used, in the vault's audit log.
- When access ends, show **what to rotate**, as the rotation queue already does for a removed device.
- **No dotsafe server.** Everything travels through storage the user owns.

## Non-goals

- **Writing from a machine.** Machines read. Changes happen on the user's devices.
- **Teams.** Another person needs to read and write, which needs public-key crypto and its own design ([devices-and-keys.md](devices-and-keys.md#teams-future)). Shares don't block it and don't depend on it.
- **Protecting values once a machine has them.** A machine that can run a command with a secret can leak it. Shares limit which secrets and for how long; rotation cleans up.

## The decision: snapshots, not keys

The obvious design is a key per environment, with machines given the keys of the environments they may read. It doesn't fit how dotsafe resolves values:

- An environment's final variables come from its parent chain, its linked shared secrets (which belong to no environment and are linked from many projects) and `${{ NAME }}` references. To read one environment, a machine would need keys to its ancestors and to every secret it links, and each rotation or new link would mean re-wrapping keys.
- Every existing record would be re-sealed under new keys: a format change touching all of them, plus an upgrade path.

Instead, **a share is a sealed snapshot of final values**. The user's devices hold the vault key and run `Resolver` anyway, so they seal the resolved variables of the shared environments into one object under a key made for that share. The machine gets that key and nothing else.

| | Key per environment | Snapshot per share |
|---|---|---|
| What the machine holds | Keys to environments, their parents and every linked secret | One share key |
| What it can decrypt | Raw records, including values it doesn't use | The final values it was given, nothing else |
| Format change | Every record re-sealed | One new record kind, `share` |
| Inheritance, links, references | Resolved on the machine | Resolved on the user's device, as today |
| Keeping it current | Automatic | Pushed by the user's devices, like a deploy target |
| Writes from the machine | Possible later | Not possible |

The cost is the last two rows. A snapshot is only as fresh as the last push, so shares join the rotation flow as targets (see [keeping snapshots current](#keeping-snapshots-current)). Machines that need to write are out of scope anyway.

## Shares

A share has a name (`ci`, `staging-vm`, `cloud-agent`), a list of environments (optionally narrowed to some variable names), an expiry, and a mode:

- **`standing`:** the snapshot is always there. The machine runs whenever it likes until the share expires or is revoked. For CI, servers, and cloud agents that work while the user is away.
- **`approve`:** there's no standing snapshot. Each run asks the user's Mac, which shows the request and asks for Touch ID, and the answer carries the values for that run only. For **attended** use: a cloud agent or remote machine the user is working with right now.

Approve mode needs the user's Mac awake, with dotsafe unlocked. A cloud agent left running overnight, or with the laptop closed, gets nothing in this mode. It should use a standing share, short-lived and narrowed with `only`, served through the secret proxy so the agent's own process never holds the API keys ([unattended agents](#unattended-agents)). The app says this when creating an approve share, instead of letting it fail silently later. Approving from a phone, which is always on, is the later fix ([approving from the Mac](#approving-from-the-mac-approve-mode)).

### Where shares live

Shares live in a **share bucket**: an S3-compatible bucket the user names for this, separate from the vault. It's **public-read with listing off**, and every object in it is sealed and named with 128 random bits.

```
share bucket (public read, no listing)
  s/<128-bit random path>                  standing mode: the sealed snapshot
  m/<128-bit random path>/doorbell         approve mode: touched by the machine on each request
  m/<128-bit random path>/requests/<uuid>  approve mode: sealed requests from the machine
  m/<128-bit random path>/answers/<uuid>   approve mode: sealed answers from a user's device
  m/<128-bit random path>/log/<uuid>       what the machine ran, names only (optional)
```

Why public:

- **The machine needs no storage key.** A standing machine fetches its snapshot over plain HTTPS. That's what makes the token the only thing to hand over, and it removes the problem that R2 keys can't be limited to one prefix.
- **Nothing readable is exposed.** Objects are ciphertext under a key only the token carries, and their names can't be guessed or listed. Someone who learns a URL gets what a storage provider already sees.
- **Vaults in iCloud Drive work too.** The vault never has to be reachable from the cloud, only the share bucket.
- **The machine can't touch the vault.** It has no credentials for anything.

Serving rules, so a public URL behaves:

- **No caching.** Every object is written with `Cache-Control: no-store`, so a CDN in front of the bucket (an R2 custom domain is behind Cloudflare's cache) never serves a stale or revoked snapshot.
- **A custom domain on R2,** not `r2.dev`, which Cloudflare rate-limits and doesn't support for production. S3 uses a bucket policy that allows `GetObject` on `s/*` and `m/*` and nothing else.
- **HTTPS only, no redirects,** like every other network call dotsafe makes.

The user's devices write to the share bucket with a storage key kept like other storage keys: in the OS keychain on each device ([AGENTS.md](../../AGENTS.md)). Approve mode is the one case where the machine writes too, so it also gets a key ([approving from the Mac](#approving-from-the-mac-approve-mode)).

### The `share` record

A new record kind in the vault, written as **record version 6** so older clients refuse it instead of dropping it:

```
share: {
  id, name, deleted, updated,
  mode: "standing" | "approve",
  environments: [ { id, only?: [NAME, …] } ],
  expires,                     // Unix ms
  lease_days,                  // how long a snapshot stays valid without a refresh; default 14
  key,                         // the share key: 32 random bytes
  path,                        // the object path's 128 random bits
  pushed?: { sha256, at },     // the last snapshot written, as for deploy targets
  created_by,                  // device id
  revoked?                     // Unix ms; set on revoke, kept for the rotation queue
}
```

The share key sits inside the record, sealed under the vault key like every record, so any of the user's devices can push the snapshot. Shares merge like other records: fields on `updated`, `pushed` on its `at`, and `revoked` once set is never cleared.

### Keys and sealing

All with the existing primitives: XChaCha20-Poly1305, HKDF-SHA256. No new algorithm.

```
share key (32 random bytes) ──HKDF "dotsafe/v6/share-snapshot" ─▶ snapshot key
                            ──HKDF "dotsafe/v6/share-request"  ─▶ request key
                            ──HKDF "dotsafe/v6/share-answer"   ─▶ answer key
                            ──HKDF "dotsafe/v6/share-log"      ─▶ log key
```

- **Snapshot:** AAD `"dotsafe/v6/share-snapshot" ‖ vault_id ‖ share_id`. Plaintext: `{ stamp, expires, lease_until, environments: { <name>: { KEY: value } } }`, padded like records.
- **Requests, answers and logs:** AAD `"dotsafe/v6/share-<kind>" ‖ vault_id ‖ share_id ‖ object id`, so an object can't be moved to another share or renamed. An answer's id is its request's id, so an answer can't be replayed against another request.
- **Freshness:** `stamp` is the writer's hybrid logical clock stamp. The machine remembers the newest stamp it has seen (in its own config directory) and refuses an older snapshot, so nobody with write access to the bucket can roll it back to values from before a rotation. A machine that has never seen the share trusts the first snapshot it reads, as with `head.json` today.

### The share token

What the machine is given, once:

```
DOTSAFE_SHARE=dss_<Crockford base32 of: version ‖ vault_id ‖ share_id ‖ share key ‖ snapshot URL>
```

About 150 characters, and the only thing a standing machine needs. It's a secret: shown once when the share is created, never written to disk by dotsafe, and put by the user into the CI secret, the VM's environment or the cloud agent's settings.

The trade-off of carrying the URL: with a private bucket, a thief would need the token and a storage key. Here the token alone is enough. That's the same as any CI secret, and the token lives where a CI secret would.

## Lifecycle

### Creating

```bash
dotsafe share create ci -e production --for 30d
dotsafe share create cloud-agent -e development --only OPENAI_API_KEY,DATABASE_URL --for 24h
dotsafe share create pairing-vm -e development --for 8h --approve
```

In the app: **Settings → Access → New share**. A licensed device ([licensing](#licensing)) writes the `share` record and, for `standing`, the first snapshot, then shows the token once. It also sets a lifecycle rule on the share bucket that deletes the share's objects at `expires` ([expiry](#expiry-and-revoking)).

### Keeping snapshots current

**A share is a deploy target.** The rotation flow is already "save the new key, check it, make it live, push it to every target, then revoke the old one" ([rotation.md](rotation.md)). Shares join the push step:

- **After any write** on a user's device, the device re-resolves every live standing share whose environments the write could affect (the environment, its descendants, and environments linking a changed secret) and pushes a new snapshot if the values changed. Rotation, promotion and edits all go through writes.
- **Pushed state is recorded** in the share record (`pushed: { sha256, at }`), as for Vercel or Workers targets. A share whose snapshot is behind the vault shows as **out of date** in the app and in `dotsafe share list`.
- **Retiring waits for shares.** `dotsafe secret retire` (and the app's Retire) refuses while a share that holds the secret is out of date, unless forced, and says which share. CI is never left holding a key that's about to be revoked.
- **Revoked at once:** with `--leaked`, or a provider whose rotation ends the old key immediately (Cloudflare tokens), the old key is dead before any push. The share push happens in the same flow, so the machine is only broken if the bucket can't be reached. Then it's shown out of date and retried.
- **Retries:** an out-of-date share is pushed again on the next sync and on every later write, from any of the user's devices that sees the change.

### Leases

A snapshot is valid for `lease_days` (default 14) after it was written, written into it as `lease_until`. Any of the user's devices pushes a fresh copy of every live standing share at least once a day while unlocked, which renews the lease.

- **If the user stops using dotsafe,** every share stops working within the lease, even on storage without lifecycle rules.
- **Honest machines enforce it:** `dotsafe run --share` refuses a snapshot past `lease_until`. A thief ignores it, which is why the lifecycle rule and revoking are the real enforcement.
- **CI breaks if no device is unlocked for a whole lease.** The app warns three days before a lease runs out, and `lease_days` can be raised per share (up to the share's expiry).

### Using it on a machine

```bash
dotsafe run --share -- ./deploy.sh        # reads DOTSAFE_SHARE from the environment
dotsafe daemon --share                    # the broker, for agents on this machine
```

- `dotsafe run --share` fetches the snapshot from its URL, checks its stamp, expiry and lease, and runs the command with the variables. With several environments, `-e` picks one. Nothing decrypted is written to disk.
- **No license activation on the machine** ([licensing](#licensing)).
- `dotsafe daemon --share` serves the same broker as the app, with the share's environments as its only source of values. Agents on the machine (`dotsafe mcp`) get names, approvals and the secret proxy as they do on a laptop.

### Unattended agents

A cloud agent working while the user is away uses a standing share through `dotsafe daemon --share`:

- **Proxied keys stay out of the agent.** OpenAI, Anthropic and custom proxy profiles go through the secret proxy, so the agent's process gets a placeholder and a local URL. Only the daemon holds the real key ([agents.md](agents.md#secret-proxy)).
- **Other variables** reach commands the daemon runs, with the daemon's own approval rules. With nobody to approve on the machine, the share's `only` list is the real limit, so keep it short.
- **The daemon still holds every value in the share.** Root on the VM, or the agent with enough access, can read them. Short shares and narrow `only` lists limit what that's worth.

### Approving from the Mac (`approve` mode)

```
machine                               share bucket                        user's Mac
writes requests/<id>, touches doorbell ──▶ doorbell changed ──next check──▶ approval dialog
                                                                            Touch ID (in Rust)
           ◀──── polls answers/<id> ────── answers/<id> ◀──sealed answer───┘
runs the command with the values in the answer
```

- **The machine writes,** so in this mode it gets a storage key besides the token, limited to the share's `m/` prefix where the provider allows (S3, B2), or to a bucket of its own on R2.
- **The request** carries what the dialog shows today for agents: program, arguments, working directory, host name, the variables it wants, and a 5-minute expiry. It's sealed, so the bucket sees only that a request was made.
- **The doorbell.** After writing a request, the machine overwrites the share's small `doorbell` object. The Mac fetches only that object every 2 seconds with `If-None-Match`, so an unchanged doorbell costs one cheap `304`. It lists `requests/` only when the doorbell changes. Polling runs only while dotsafe is unlocked and an approve share is live.
  - **Latency:** the dialog appears within about 2 seconds of the request, plus the user's time, plus one poll by the machine for the answer.
  - **Cost:** about 43,000 conditional reads per Mac per day, about 1.3 million a month. That's inside R2's free monthly allowance of class B operations, and roughly $0.50 a month on S3 (GET at $0.0004 per 1,000).
- **The dialog** is the existing agent approval dialog, with the machine's name, and Touch ID is checked in Rust in `answer_agent_request` as today.
- **The answer** is `{ decision, values, expires }` for the variables approved, under the answer key. "Once" is the default. "For an hour" writes a short-lived snapshot instead, so the next runs don't ask.
- **Cleanup:** the machine deletes its request and the answer once read. The Mac deletes requests and answers older than their expiry. An unanswered request just expires.
- **Without the Mac:** nothing runs. That's the point of this mode, and why it's for attended use only.
- **Later, from a phone.** The mailbox doesn't care which device answers. An iPhone app holding the vault (or just the share keys) with Face ID could read the same doorbell and answer, and a phone is always on. Nothing in this design changes for it.

This is the "approving a server's requests from the Mac" step that [agents.md](agents.md#unattended-servers) lists as next, generalised: `dotsafe daemon --share` in approve mode sends its approvals here instead of to `dotsafe approve`.

### Expiry and revoking

- **Expiry** has four layers, because no dotsafe server can enforce it:
  1. **The storage provider deletes the objects.** At creation, the device adds a lifecycle rule that expires the share's `s/` and `m/` paths at `expires` (S3 and R2 both support prefix lifecycle rules, which run within about a day). This works even if the user never opens dotsafe again.
  2. **The user's devices delete them** on their next write after `expires`, which is usually sooner.
  3. **The lease** runs out within `lease_days` if no device refreshes it.
  4. **The machine refuses** a snapshot past `expires` or `lease_until`. This only helps an honest machine.

  Until one of the first two happens, a leaked token can still fetch and read the last snapshot. A machine that kept the values can keep using them; expiry only stops it getting new ones. Providers without lifecycle rules (WebDAV can't serve a public share bucket anyway) aren't offered as share buckets.
- **Revoking** (`dotsafe share revoke ci`, or the app) sets `revoked`, deletes the share's objects (the URL returns 404 from then on), and adds the share to the rotation queue.
- **A leaked token** is fixed by revoking and creating a new share: new key, new path, new token.
- **The rotation queue** lists every secret the share could read: each variable in its environments (resolved, so linked secrets and inherited values count) whose current value was set before the revoke. That's the same rule as for a removed device, scoped to the share. Provider-backed secrets rotate in one click, as today.
- **Approve mode:** the user also deletes the machine's storage key at their provider. The app reminds them; dotsafe can't do it.

## Licensing

**Shares are part of the license of the device that creates them.**

- Creating a share needs a licensed device, like every other command. There's no limit on how many shares a license has: shares cost dotsafe nothing (they live in the user's bucket), and one token can be used on any number of machines, so a count wouldn't limit much.
- The machine needs no activation. It holds no vault, can't change anything, and can only read what a licensed device sealed for it. So `dotsafe run --share` and `dotsafe daemon --share` run without `license.json`.
- This is a **documented rule, not a bypass**: AGENTS.md gets an explicit line that share mode is licensed through the device that created the share, next to the rule against flags that skip the check. Nothing else in the CLI runs without a license, and share mode can't open a vault.
- Ephemeral CI works, since a fresh runner doesn't use an activation.

## Audit

- **Approve mode:** each approval is logged on the Mac, like an agent request. It can't be skipped.
- **Standing mode:** creating, pushing, revoking and lease renewals are logged on the user's devices. Per-run logs need the machine to write, so they're optional: given a storage key, the machine writes a log object per run (`{ at, host, command, environment, variables }`, names only) under the log key, and the user's devices fold these into the vault's audit log (actor `share:<name>`) and delete them. Without a key, standing machines aren't logged per run.
- **Limit:** a machine's own log is only as honest as the machine. It can skip writing it.

## Threat model additions

| Adversary | Can | Can't |
|---|---|---|
| **A machine with a share token** | Read the final values of its share's environments (standing), or the values approved for each run (approve). In approve mode, with its storage key, delete or overwrite objects under its share | Read anything outside its share, any name in the vault, or the vault key. Write to the vault. Roll itself back to older values once it has seen newer ones |
| **Anyone who learns a share URL** | Fetch ciphertext | Read it without the token. Find other shares: names are random and listing is off |
| **The share bucket's provider, or a CDN in front of it** | See ciphertext snapshots and their sizes, and when snapshots and requests are written (so when a machine runs) | Read any value or name. Serve a revoked snapshot from cache (`no-store`) |
| **A leaked token** | Everything the machine could do, until the share is revoked or expires | Anything after revoking: the snapshot is deleted |
| **A revoked or expired share** | Keep values it already read. Until the objects are deleted (at once on revoke; within about a day of expiry through the lifecycle rule; at the latest when the lease runs out for honest clients), read the last snapshot | Get new or rotated values |

One honest gap: the machine holds the share key, so it can seal objects as well as open them. It could write a fake snapshot or answer for itself (in approve mode, where it has write access), but that only gives it values it already had or none, since it can't decrypt anything else. It could also fake a request; the Mac shows it like any other, with the share's name, so it gains nothing a real request wouldn't.

What we may claim: "Give a machine one environment, for a limited time, with one token and without your master password. Revoke it from your Mac." What we must not claim: that a machine can't keep or leak values it was given.

## Format changes

- **Vault:** one new record kind, `share`, at record version 6, documented in [vault-format.md](../vault-format.md) when built. Header, key slots and existing records don't change.
- **Share bucket:** new objects, outside the vault, versioned with `"format": "dotsafe-share-snapshot"` (and `-request`, `-answer`, `-log`) and `"version": 1`.
- **Config:** the share bucket's address on each device; its write key in the keychain.

## Build order

1. **Standing shares, CLI:** the `share` record, snapshots, leases and their push as a target in the rotation flow (core and `crates/integrations`), the share bucket and its lifecycle rule (`crates/stores`), `dotsafe share create|list|revoke`, `dotsafe run --share`, and retire refusing while a share is out of date. This covers CI and servers.
2. **The app:** Settings → Access (create, show the token once, list with pushed and out-of-date state, revoke), lease warnings, the rotation queue for a revoked share, and pushing snapshots from the app's writes and background sync.
3. **Unattended agents:** `dotsafe daemon --share` with the secret proxy.
4. **Approve mode:** the doorbell, requests and answers, the Mac's approval dialog, and the daemon forwarding agent approvals.
5. **Per-run machine logs** folded into the audit log.

Each step ships on its own. Step 1 is useful without the rest. Approving from a phone comes after step 4, with an iPhone app.

## Open questions

- ~~**Dropping approve mode's storage key.**~~ Built this way: see the status above.
- **Forward secrecy for approve mode.** Anyone holding the token and an old answer object can read it, which is why answers are deleted right away. A symmetric trick doesn't help: anyone with the share key can read a one-time key sent inside a request. If teams brings in X25519, a request could carry a one-time public key and the answer be sealed to it, so a token leaked later reads nothing.
- **How fast approvals feel.** 2-second doorbell checks should feel close to instant for a dialog. Worth measuring with real use.
- **Storage prices** in [approving from the Mac](#approving-from-the-mac-approve-mode) are from memory, not the S3 and R2 price pages. Recheck before putting any cost figure in user-facing docs.
- **Daily refresh cost.** Pushing every live share once a day is one write per share per day; negligible, but it means an unlocked device does network work for shares the user isn't touching.
