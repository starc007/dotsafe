# dotsafe architecture

**Status:** proposed, October 2026. M1 (format v2 and the `Store` interface) M2 (S3/R2 with an offline cache) and M3 (shared secrets and rotation) are implemented; see the notes below. Describes where dotsafe is going. [`../vault-format.md`](../vault-format.md) describes what ships today (format v1).

## The promise

1. **Encrypted before it leaves the device.** Storage, networks and dotsafe's own servers only ever see ciphertext.
2. **You choose where it lives.** A synced folder, S3, R2 or several at once. Every storage provider is treated as untrusted.
3. **Rotation is one action.** A secret is defined once, used in many places, and rotated everywhere together.
4. **Agents use secrets without seeing them.** AI agents get names and references, and work only through what you approve.

Every design choice here serves one of these. Where they conflict, security wins over convenience.

## Documents

| Doc | Covers |
|---|---|
| [threat-model.md](threat-model.md) | Assets, adversaries, what dotsafe protects against and what it doesn't |
| [storage.md](storage.md) | The storage interface, folder/S3/R2 backends, local cache, replicas |
| [devices-and-keys.md](devices-and-keys.md) | Per-device keys, Touch ID unlock, revoking a device, re-keying the vault |
| [rotation.md](rotation.md) | Shared secrets, version history, the rotation flow, provider plugins, push targets |
| [agents.md](agents.md) | MCP server, approval broker, redaction, the secret proxy, audit log |

## System overview

```
                         ┌──────────────────────── user's device ─────────────────────────┐
                         │                                                                 │
  AI agent ── MCP ──────▶│  dotsafe mcp ──IPC──▶ ┌──────────── broker ────────────┐          │
  (Claude Code, Cursor)  │  (names + refs only)  │ grants · approvals · redaction │          │
                         │                       │ secret proxy · audit log       │          │
                         │                       └───────────────┬────────────────┘          │
                         │  desktop app (Tauri) ─────────────────┤                           │
                         │  dotsafe CLI ─────────────────────────┤                           │
                         │                                       ▼                           │
                         │                        crates/core: vault, crypto, merge          │
                         │                                       │                           │
                         │              OS keychain ◀── device key, store credentials        │
                         │                                       │                           │
                         │                          encrypted local cache                    │
                         └───────────────────────────────────────┼───────────────────────────┘
                                                                 │ ciphertext only
                                     ┌───────────────┬───────────┴──────┬─────────────────┐
                                     ▼               ▼                  ▼                 ▼
                               synced folder      S3 / R2 / B2       WebDAV          (replicas)
                               (iCloud, Drive)

  dotsafe API (Cloudflare) ◀── license activation only; never sees vaults, keys or secrets
  Provider APIs (Stripe, OpenAI, Vercel…) ◀── rotation and push, called from the device with the user's tokens
```

### Components

- **crates/core:** vault format, crypto, merging, and the `Store` interface. It knows nothing about the network or the UI. All crypto stays here (see AGENTS.md).
- **Store backends:** one crate per backend that needs network code (`crates/store-s3`, …), so core stays small and auditable.
- **Desktop app and CLI:** both are clients of core. The desktop app also hosts the broker. A headless `dotsafe daemon` hosts it on machines without the app.
- **Broker:** the one process that decrypts on behalf of anything that isn't a human at the UI. Agents, the MCP server and CI-style automation all go through it.
- **dotsafe API:** licensing and checkout only (`api/`). Nothing here depends on it being online.

## Principles

- **Every byte that leaves the device is encrypted and authenticated** with keys that never leave the device, except sealed in a key slot.
- **Plaintext lives in as few processes as possible,** for as short a time as possible. The webview gets one value at a time. Agents get none.
- **Storage is dumb.** Backends store and list opaque objects. Merging, integrity and access control live in core.
- **Offline first.** Every feature works offline against the local cache. Sync, licensing and rotation plugins are best effort.
- **No new crypto without a decision.** We reuse XChaCha20-Poly1305, Argon2id, HKDF-SHA256 and HMAC-SHA256. The one known exception, public-key crypto for team sharing, is an open decision ([devices-and-keys.md](devices-and-keys.md#teams-open-decision)).
- **Honest claims.** Marketing says only what the threat model supports.

## Format v2

Several designs change the on-disk format. They ship together as **format v2**, so vaults migrate once:

| Change | From |
|---|---|
| Device key slots | devices-and-keys.md |
| Key epochs (re-keying) | devices-and-keys.md |
| Signed vault head (rollback detection) | storage.md |
| Padded record plaintext | storage.md |
| `secret` records with history; variables that reference them | rotation.md |
| `audit` records | agents.md |
| Hybrid logical clock stamps | storage.md |

Migration happens in place on the first unlock by a v2 client. The v1 limitations in `vault-format.md` (rollback, clock skew, header conflicts) are all addressed by v2.

## Roadmap

```
M1 Format v2 + Store interface ──┬──▶ M2 S3/R2 backend, replicas
                                 ├──▶ M3 Shared secrets, history, rotation flow ──▶ M5 Provider plugins, push targets
                                 └──▶ M4 Device keys, Touch ID, revoke + re-key
                                                   │
M6 Agents: MCP (refs only) ──▶ broker + approvals ──▶ redaction ──▶ secret proxy
```

- **M1** blocks everything else that touches the format, so it comes first. Design it once.
  - **Done:** `Store` + `FolderStore`, conditional writes with merge-and-retry, header `epoch` and `previous_keys`, record epochs in the AAD, padding, hybrid logical clocks, `head.json` and rollback reporting, and in-place upgrade from v1.
  - **M2 done:** `S3Store` in `crates/stores`, `CachedStore` in core, `dotsafe init --bucket` and `dotsafe sync` in the CLI, and bucket setup plus background sync in the desktop app. Replicas, WebDAV and moving a vault between stores are still to do.
  - **M3 done:** `secret` records with versions, linked variables, record version 3 for records that use them, generators, rotation policies, and the stage → promote → retire flow in the CLI and desktop app.
  - **Left to the milestones that use them:** device and enrollment slots (M4), `audit` records (M6). They add new slot kinds and record types inside v2, so every client must be updated before a vault uses them.
- **M6** can start in parallel: the MCP server with names and references only needs no format change.

## Decision log

| # | Decision | Why |
|---|---|---|
| 1 | Keep one encrypted file per record | Works the same on folders and object stores; merges per record; limits conflict blast radius |
| 2 | Device keys are symmetric and live in the OS keychain | No new crypto; keychain and Secure Enclave protect them |
| 3 | Revoking a device re-keys the vault and prompts secret rotation | A revoked device already saw the vault key and the secrets; only re-keying and rotation remove its access to future data |
| 4 | Agents never receive plaintext from dotsafe | A process that holds plaintext can leak it; the only safe agent is one that never holds it |
| 5 | The secret proxy covers HTTP API keys first | Most agent secret use is API keys; base-URL overrides avoid TLS interception |
| 6 | Rotation plugins run on the device with the user's tokens | No dotsafe server gains power over the user's accounts |
