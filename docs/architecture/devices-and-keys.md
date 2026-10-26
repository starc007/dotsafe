# Devices and keys

**Status:** proposed. Milestone M4, with format changes in M1 (format v2).

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
  - Within an epoch, slots merge as a set, and removals are kept as tombstones with stamps so a removed slot can't come back.
  - The MAC is recomputed by the merging device, which has the vault key.

## Device list

The Devices screen lists each slot's name, when it was added, and when it last wrote, using the device id in record stamps. It isn't a security boundary, since any device with the vault key could forge stamps. It's there to help users spot devices they forgot.

## Teams (open decision)

Sharing a vault between people adds a new requirement: adding a member without knowing their secrets, and revoking one cleanly. The options:

| Option | Crypto | Trade-off |
|---|---|---|
| **A. Per-member public keys** | X25519 + XChaCha20-Poly1305 sealed boxes to wrap the vault key per member | Clean onboarding and revocation. A new algorithm, so it needs an explicit decision and an AGENTS.md update |
| **B. Shared team password plus enrollment codes** | None new | Works now. Revocation means re-key plus a new team password for everyone. Weak for teams over ~5 people |

Device slots, epochs and re-keying are the foundation for both, so M4 doesn't block on this choice. Decide before any team feature is built.

## Open questions

- Whether the device key should be generated inside the Secure Enclave. The enclave only does P-256, so it would mean ECDH and a new algorithm. The keychain plus biometric access control is the proposal for now.
- How long to keep old epochs when a device that hasn't synced in months comes back. Proposal: it re-reads from storage and re-seals; nothing it holds needs the old key.
