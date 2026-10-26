# Secrets and rotation

**Status:** M3 is implemented (shared secrets, history, the rotation flow, generators, policies, the leaked fast path, CLI and desktop). M5 (provider plugins, push targets) is still proposed, and so are `targets`/`provider` on secrets and the device-revoked bulk queue.

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

## Open questions

- Variable interpolation (`${DB_HOST}`) and environment inheritance (a base environment with overrides). These are related and fit the same reference model, but they're separate from rotation. Proposal: design them with M3, so the format changes land once.
- Retention defaults for retired values: purge after 30 days, or keep them until the user deletes them.
- Whether pending values should ever reach `dotsafe run` (for example `--pending` for testing before promotion).
