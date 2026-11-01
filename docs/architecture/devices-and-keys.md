# Devices and keys

**Status:** M4 is implemented on macOS (header format v3). See [As built](#as-built-m4) for where it differs from the design below. Team sharing is a future feature (see [Teams](#teams-future)).

## As built (M4)

- **Header v3:** `device` and `enrollment` slots, a per-slot `epoch`, and `previous_keys`. Older clients refuse v3 headers ("unsupported vault version 3") instead of misreading them.
- **Device key storage:** the device key is in the **login keychain** (`app.dotsafe.device`, keyed by this machine's device id). dotsafe asks for Touch ID or the login password in Rust (LocalAuthentication) before reading it. Hardware-bound access control (`biometryCurrentSet`, or the Secure Enclave) needs the signed app's keychain entitlements, so it comes with code signing; until then the gate is dotsafe's own check. Only macOS offers device unlock: elsewhere there's no presence check, so a device key would mean unlocking with nothing.
- **Password freshness:** Touch ID unlock is offered for 14 days after the last password unlock, then the password is needed once.
- **Enrollment codes** are 128 bits shown as 26 Crockford base32 characters in groups of four (no word list or QR code yet). They're valid for 10 minutes and single use, and expired ones are pruned on every header write. Joining with a code enrolls the new Mac for Touch ID unlock straight away.
- **Revoking** needs the password. It removes the slot, generates a new vault key, recreates the password slot (same password) and the recovery slot, which means **a new recovery key** (the old slot can't be re-wrapped without the paper key). It re-seals every record and the head, then drops the old key. It all happens at once, not in the background: vaults are small, and an interrupted re-key finishes on the next unlock.
- **Other devices** keep their slots, which go **stale**: Touch ID unlock answers "needs the password once", and after a password unlock the app re-enrolls the device automatically.
- **Records sealed with a dropped key** (written by a device that hadn't seen the re-key, or by the revoked one) are skipped and reported by `Vault::unreadable_records()`, never trusted.
- **Concurrent header changes:** every header change re-reads the newest header, re-applies itself and writes conditionally, retrying on conflict. Changes from two devices both survive on stores with conditional writes (S3/R2). On synced folders the check can't be atomic, so the sync client may keep a conflict copy of `vault.json`; those copies are merged (header v4, see [vault-format.md](../vault-format.md#header-v4-merging-conflict-copies)).
- **Not built yet:** suggesting a password change on revoke, the bulk rotation queue after revoking, Windows Hello and Linux, and CLI device unlock (the CLI has `dotsafe device list` and `dotsafe device remove`).

## Goals

- Unlock with Touch ID / Windows Hello on a trusted device, without weakening the vault.
- Add a device without typing the master password on it.
- Revoke a lost device so it can't read anything written afterwards.
- Make concurrent changes to `vault.json` safe.

All of this uses the existing primitives: XChaCha20-Poly1305, HKDF-SHA256, HMAC-SHA256, Argon2id. No new crypto.

## Key hierarchy (v2)

```
password ─────Argon2id──▶ KEK ─┐
recovery key ─HKDF──────▶ KEK ─┤
device key ───HKDF──────▶ KEK ─┼─ unwraps ─▶ vault key, epoch N ──HKDF──▶ records key (epoch N)
enrollment code ─HKDF───▶ KEK ─┘   (short-lived)        │                ──▶ header MAC key
                                                        └─ seals ─▶ vault keys of earlier epochs
                                                                   (only while re-sealing)
```

## Device slots

- **Enrolling a device** generates a random 32-byte **device key** and stores it in the OS keychain:
  - **macOS:** Keychain item with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, and optionally access control `biometryCurrentSet`, so the OS itself demands Touch ID. Re-enrolling a fingerprint invalidates it.
  - **Windows:** Credential Manager protected by DPAPI, with Windows Hello as the gate.
  - **Linux:** Secret Service (GNOME Keyring, KWallet). No biometric gate; password unlock stays the default.
- **The slot:** `vault.json` gets a slot `{ kind: "device", device_id, name, added }` holding the vault key sealed under HKDF(device key, slot salt). The AAD binds it to the vault id and slot id, like other slots.
- **Unlocking:** fetch the device key (the OS shows Touch ID), derive the KEK, unwrap the vault key.
- **Password freshness:** the app still asks for the master password every 14 days (configurable) and after the device slot fails, so users don't forget the one thing that rescues them.
- **Desktop:** Tauri has no desktop biometric plugin. A small native module asks the OS (LocalAuthentication through `objc2` on macOS, Windows Hello through the `windows` crate). Access control on the keychain item is the real enforcement; the prompt is only UI.
- **CLI:** `dotsafe unlock` can use the same device slot. The keychain session cache stays as it is.

## Adding a device without the password

On an existing, unlocked device: **Settings → Devices → Add device**.

1. The existing device generates a 128-bit **enrollment code**, shown as 12 words and a QR code.
2. It adds a temporary slot `{ kind: "enrollment", expires }` sealing the vault key under HKDF(code), and syncs `vault.json`.
3. On the new device, the user connects to the same storage and enters the code. The new device unwraps the vault key, creates its own device slot, and removes the enrollment slot.
4. If nobody uses it, the existing device removes the enrollment slot after 10 minutes. Every client ignores expired enrollment slots.

The code carries 128 bits of entropy, so an attacker holding `vault.json` can't brute-force it offline in its 10-minute life, or ever.

## Revoking a device

**Settings → Devices → Revoke**, from any other unlocked device:

1. Remove the device's slot.
2. **Re-key** the vault (below), so the revoked device can't read new changes.
3. **Suggest changing the master password** (checked by default). The password slot is re-wrapped during the re-key, so anyone who has both the password and the revoked device would otherwise still unlock it.
4. **Offer rotation:** "This device could read 42 secrets. Rotate them?" This opens the bulk rotation queue in [rotation.md](rotation.md#incident-flows). Re-keying protects the future; only rotation protects secrets the device already saw.

## Re-keying with key epochs

Re-keying must not need every record rewritten in one step: a vault can be large, and a sync folder can't apply many changes atomically.

- `vault.json` gains `epoch`. Each record file gains `epoch`, and its AAD becomes `"dotsafe/v2/record" ‖ vault_id ‖ epoch ‖ id`.
- **Re-key:**
  1. Generate vault key K(N+1).
  2. Re-wrap every remaining slot with it.
  3. Store K(N) sealed under K(N+1) in `previous_keys`.
  4. Bump `epoch`, and write the header.
- **Background re-seal:** every client re-seals records from older epochs into the current one, in the background and on any write. Progress is visible in `head.json`.
- **Cleanup:** once the head shows no record left in an old epoch, the next writer drops that key from `previous_keys`.
- **What a revoked device can do:** it holds K(N), so it can read records still in epoch N, which it could read before anyway. It can't read epoch N+1 records, and it can't get K(N+1), because `previous_keys` is sealed under the new key.

## Concurrent header changes

v1 loses one of two concurrent `vault.json` writes. v2:

- On stores with conditional writes, header writes use the same compare-and-swap loop as records ([storage.md](storage.md#the-store-interface)).
- On synced folders, header conflict copies are merged:
  - The higher `epoch` wins outright.
  - Within an epoch, slots merge as a set, and removed slot ids are kept (`removed_slots`) so a removed slot can't come back. Only copies whose MAC verifies with the vault key are merged.
  - The MAC is recomputed by the merging device, which has the vault key.

## Device list

The Devices screen lists each slot's name, when it was added, and when it last wrote, using the device id in record stamps. It isn't a security boundary, since any device with the vault key could forge stamps. It's there to help users spot devices they forgot.

## Teams (future)

**Status:** deferred, October 2026. Not being built now; this is the proposal to start from.

Sharing a vault between people adds a new requirement: adding a member without knowing their secrets, and removing one cleanly. The options:

| Option | Crypto | Trade-off |
|---|---|---|
| **A. Per-member public keys** | X25519 + XChaCha20-Poly1305 sealed boxes to wrap the vault key per member | Clean onboarding and removal. A new algorithm, so it needs an explicit decision and an AGENTS.md update |
| **B. Shared team password plus enrollment codes** | None new | Works now. Removing anyone means re-key plus a new team password for everyone. Weak for teams over ~5 people |

The proposal is A:

- **A team shares a vault, not an account.** Each person keeps their personal vault; the team vault lives in storage the team already shares (a bucket, a WebDAV share, a shared Drive or Dropbox folder). The app and CLI hold several vaults at once, with a switcher.
- **Identity keys.** Each person has an X25519 key pair, protected by their personal vault and keychain. The team vault's header (v5) has a **member slot** per person: the vault key sealed to their public key. Sealing is X25519 plus the existing HKDF-SHA256 and XChaCha20-Poly1305; X25519 is the only new primitive.
- **Inviting** uses a short-lived code, like enrollment today; the new member adds their own slot.
- **Removing** a member deletes their slot and re-keys, sealing the new key to every remaining member's public key with nobody else involved. The rotation queue then lists what they could read.
- **Access per vault.** People who shouldn't see production aren't in the production vault. Scopes inside one vault (keys per environment) can come later.
- **No enforced roles at first.** Anyone holding the vault key can change the header, so admin-only actions would need signatures (Ed25519, another algorithm). Every member can invite and remove; the audit log records who did what.
- **Seats** are license activations: a team license allows N, and each member activates it. The license server never learns who is in a vault.

Build order: several vaults in config, CLI and sidebar; identity keys and member slots; team setup and members screens; seat licensing in `apps/api/`.

## Open questions

- Whether the device key should be generated inside the Secure Enclave. The enclave only does P-256, so it would mean ECDH and a new algorithm. The keychain plus biometric access control is the proposal for now.
- How long to keep old epochs when a device that hasn't synced in months comes back. Proposal: it re-reads from storage and re-seals; nothing it holds needs the old key.
