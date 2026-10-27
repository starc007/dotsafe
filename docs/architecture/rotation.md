# Secrets and rotation

**Status:** M3 is implemented (shared secrets, history, the rotation flow, generators, policies, the leaked fast path, CLI and desktop). M5's first slice is implemented: OpenAI and Cloudflare API tokens as providers, Vercel and Cloudflare Workers as push targets, in the CLI and desktop (see [As built (M5, slice 1)](#as-built-m5-slice-1)). Since then: the device-removed rotation queue, audit entries for rotations, pushes and revokes, due reminders, and slice 2 (Anthropic and Stripe as guided providers, AWS IAM access keys). Still proposed: GitHub Actions secrets (needs libsodium sealed boxes), verifying with deploy targets, and drift detection from provider timestamps.

## The problem

The same Stripe key sits in four projects, two environments each, and three deploy targets. Rotating it means finding every copy, updating each one, redeploying, and hoping you didn't miss one, so nobody rotates. dotsafe should make rotation one action, with history and a way back.

## Goals

- Define a secret once and use it in many environments.
- Keep every secret's history, and restore any earlier value.
- Rotate with a guided flow: create, distribute, verify, promote, revoke.
- Rotate provider keys and push to deploy targets from the device, with the user's own tokens.
- Show how old each secret is, and what's due.

## Non-goals

- Dynamic or short-lived credentials issued by a server. That needs an always-on service.
- Running third-party plugin code (for now). Plugins hold admin tokens.

## Data model (format v2)

A new record kind, `secret`:

```json
{
  "kind": "secret",
  "id": "uuid",
  "name": "STRIPE_SECRET_KEY",
  "source": "manual | generated | provider",
  "versions": [
    { "id": "uuid", "value": "sk_live_…", "state": "current",  "stamp": {…}, "note": "rotated after offboarding" },
    { "id": "uuid", "value": "sk_live_…", "state": "previous", "stamp": {…} },
    { "id": "uuid", "value": null,        "state": "retired",  "stamp": {…} }
  ],
  "policy": { "rotate_every_days": 90, "generator": { "kind": "random", "length": 48, "charset": "base62" } },
  "provider": { "plugin": "openai", "account": "secret-id-of-admin-token", "key_id": "…" },
  "targets": [ { "plugin": "vercel", "project": "…", "env": "production", "name": "STRIPE_SECRET_KEY", "pushed": {…} } ],
  "deleted": false,
  "updated": {…}
}
```

- **States:**
  - `pending`: created, not yet used.
  - `current`: what environments get.
  - `previous`: still valid during the overlap.
  - `retired`: revoked. Its value is purged after the retention period, default 30 days.
- **History:** the last 10 versions are kept by default. Version lists merge by version id, and state changes merge on their stamps.
- **Variables in environments** become either an inline value (as today) or a reference:

```json
"vars": {
  "DATABASE_URL":      { "value": "postgres://…", "stamp": {…} },
  "STRIPE_SECRET_KEY": { "ref": "<secret uuid>",  "stamp": {…} }
}
```

- `dotsafe run`, export and reveal resolve references to the secret's `current` value.
- **Inline values keep working.** "Make shared" converts one into a secret and replaces matching copies across projects after the user confirms.
- **Where a secret is used** is computed on unlock, by scanning references and targets: "Used in 6 environments and 2 deploy targets".

## The rotation flow

```
  ┌────────┐   create    ┌─────────┐  distribute  ┌─────────────┐  verify  ┌──────────┐  revoke old  ┌──────┐
  │ current│ ──────────▶ │ pending │ ───────────▶ │ distributed │ ───────▶ │ promoted │ ───────────▶ │ done │
  └────────┘             └─────────┘              └─────────────┘          └──────────┘              └──────┘
                              │                          │                       │
                              └──────── cancel ──────────┴──── roll back ────────┘   (any time before revoke)
```

1. **Create:** generate a value (random, UUID, hex or base64 bytes, from the OS CSPRNG), paste one, or have a provider plugin create a key.
2. **Distribute:** push the pending value to deploy targets that support staged values. Otherwise this step is folded into promote.
3. **Verify:** the plugin's check (for example, an authenticated "who am I" call with the new key), or the user confirms by hand.
4. **Promote:** pending becomes `current`, and the old value becomes `previous`. Every referencing environment now resolves to the new value, and targets are pushed.
5. **Revoke old:** the plugin revokes the old key at the provider, or shows a checklist with links when there's no API. The old version becomes `retired`.

Each step is written to the audit log ([agents.md](agents.md#audit-log)). Agents can propose a rotation, but every step that changes a provider or a target needs the user's approval.

## Provider plugins

```rust
trait Provider {
    fn create(&self, account: &Secret, spec: &KeySpec) -> Result<NewKey>;   // value + provider key id
    fn verify(&self, value: &str) -> Result<Verified>;
    fn revoke(&self, account: &Secret, key_id: &str) -> Result<()>;
}
```

- **They run on the device.** The provider admin token is itself a secret in the vault, so it gets history and rotation too. No dotsafe server ever holds it.
- **Built in, written in Rust, reviewed.** First candidates: OpenAI, Anthropic, Stripe, AWS IAM access keys, Cloudflare API tokens, and Postgres/MySQL passwords (using a two-user swap so there's no downtime). We'll check each provider's key-management API during design. Where there's no API, the plugin is a guided checklist with deep links.
- **Later, third-party plugins** run in a WASM sandbox. The host grants HTTPS only to the hosts a plugin declares, and the plugin sees only the secrets it was given.

## Push targets

- First candidates: Vercel, Cloudflare Workers secrets, GitHub Actions secrets, Netlify, Fly.io.
- **Mapping:** a target binds a secret (or a whole environment) to a project, environment and variable name at the provider.
- **Push** happens on promote, on demand, and when an inline value changes, if the user enabled auto-push.
- **Drift:** most targets are write-only (GitHub secrets can't be read back), so dotsafe records the SHA-256 of what it last pushed and when, and shows "changed outside dotsafe" where the provider exposes an update time.

## Age and policy

- Each secret shows its age since the last rotation, and its policy (`rotate_every_days`).
- A **Rotation** view lists secrets that are overdue, due soon, and never rotated, ordered by how many places use them.
- The desktop app sends a local notification when something becomes due. Nothing leaves the device.

## Incident flows

- **"This leaked":** a fast path for one secret. Create, push, promote and revoke immediately, with no overlap window, so the leaked value stops working as fast as the provider allows.
- **Device revoked** ([devices-and-keys.md](devices-and-keys.md#revoking-a-device)): a bulk queue of every secret that device could read (with one vault key, that's all of them). Provider-backed secrets come first, because they rotate automatically. The queue shows progress and can be resumed later.

## CLI

```bash
dotsafe secret share STRIPE_SECRET_KEY      # convert inline copies into one shared secret
dotsafe rotate STRIPE_SECRET_KEY            # guided rotation in the terminal
dotsafe rotate --leaked STRIPE_SECRET_KEY   # fast path
dotsafe secrets due                         # what needs rotating
dotsafe history STRIPE_SECRET_KEY           # versions, without values
dotsafe restore STRIPE_SECRET_KEY <version>
```

## As built (M5, slice 1)

- **Records:** provider and target links live in two new record kinds, not on `secret`, so older clients can't drop them by rewriting a secret. A `connection` is an account at a service, with its admin token kept as an ordinary shared secret (so it has history too). A `binding` per secret holds its provider, the provider's key id for each version, and its targets with the fingerprint of what was last pushed. Both are record format v4 ([vault-format.md](../vault-format.md#record-version-4-integrations)).
- **Code:** `crates/integrations` has the HTTP calls (`Client`) and the flow (`rotate::{rotate, retire, push}`). It's HTTPS only and never follows redirects, so a token only goes to its own service. Core still has no network code.
- **Providers checked against their APIs:**

  | Service | Create | Verify | Revoke | Overlap |
  |---|---|---|---|---|
  | OpenAI | A project service account (`POST /organization/projects/{id}/service_accounts`); its key is the new value | `GET /models` with the new key | Delete the service account | Yes |
  | Cloudflare API token | Roll it (`PUT …/tokens/{id}/value`) | `GET …/tokens/verify` with the new token | Nothing: rolling ended the old one | No |

  Slice 2 added:

  | Service | Create | Verify | Revoke | Overlap |
  |---|---|---|---|---|
  | Anthropic (guided) | You make it in the Console and paste it | `GET /v1/models` with the new key (`x-api-key`) | Found through the Admin API by its `partial_key_hint` (start and end), then `POST /v1/organizations/api_keys/{id}` with `status: inactive` | Yes |
  | Stripe (guided) | You make it in the dashboard and paste it | `GET /v1/balance` with the new key | By hand in the dashboard (Stripe has no key API); dotsafe links there, then marks it retired | Yes |
  | AWS IAM access key | `CreateAccessKey` for the bound IAM user | STS `GetCallerIdentity` signed with the new key, retried while it propagates | `DeleteAccessKey` | Yes |

  - **Guided providers** never create keys; the pasted key goes through the same order as the others (saved staged first, then checked, promoted, pushed). A Stripe connection needs no token, so `connection.credential` is optional.
  - **AWS** requests are signed with SigV4 (`crates/integrations/src/sigv4.rs`), built on `crypto::hmac_sha256` and `crypto::sha256_hex` in core, and checked against AWS's published example. An AWS secret holds the secret access key; the optional `access_key_id_secret` setting names the shared secret holding `AWS_ACCESS_KEY_ID`, which is rotated, pushed and retired along with it. IAM allows two keys per user, so retire the old key before the next rotation.
  - **Revoking by hand:** `retire` can be told the old key was revoked in the provider's console (`--by-hand`, "I revoked it"); it's then just marked retired. If dotsafe doesn't know a previous key's id and can't find it, it says so instead of guessing.
  - **Not checked against the real APIs yet:** Anthropic's Admin API auth header (`x-api-key` is sent; the current reference shows `Authorization: Bearer`), and AWS end to end.
- **Targets:** Vercel (`POST /v10/projects/{p}/env?upsert=true`, type `encrypted`) and Cloudflare Workers (`PUT /accounts/{a}/workers/scripts/{s}/secrets`).
- **The flow, in this order:** create → **save the new key as staged at once** (so a crash never loses the only working key) → verify → promote → push to every target → with `leaked`, or for a provider without overlap, revoke and retire the old key. If verification fails and the provider has overlap, the key stays staged and nothing else changes. Without overlap it goes live anyway, because the old key is already dead. `retire` revokes the previous versions' keys at the provider before marking them retired, and stops at the first failure.
- **Pushes** also happen on manual promote and on demand. Each target records the SHA-256 of what was pushed, so the UI can show "out of date" without reading anything back.
- **Audit:** each provider rotation, revoke and push is written to the audit log (actor "You"), next to agent requests.
- **Connection tokens** stay out of secret lists; they're replaced from the connection ("Replace token…", `dotsafe reconnect`), and the old one is kept retired in the token's history.
- **After removing a device:** the machine that removed it keeps a rotation queue (`rotation_queue` in its local config: when, and which device). The queue isn't stored; it's worked out from the vault each time, as every shared secret whose current value is older than the removal, plus every variable with its own value set before it. Rotating something takes it off the list. Provider-backed secrets rotate in one click (`rotate_queued_with_providers`, `dotsafe secret queue --rotate`). The queue is per machine and isn't synced.
- **Due reminders:** while unlocked, the desktop app checks every 30 minutes and posts a macOS notification with how many secrets are overdue or due soon, at most once a day unless more become due. It shows the count, never the names, so secret names don't end up in Notification Center. Connection tokens are left out.
- **Not yet:** a test against the real services. Every call has been tested against a local fake, but not yet against a real account.

## Open questions

- ~~Variable interpolation and environment inheritance.~~ Built as `${{ NAME }}` references and one parent per environment (record v5; see [vault-format.md](../vault-format.md#record-version-5-inheritance-and-references)).
- Retention defaults for retired values: purge after 30 days, or keep them until the user deletes them.
- Whether pending values should ever reach `dotsafe run` (for example `--pending` for testing before promotion).
