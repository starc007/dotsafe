# Vault format (v1)

A dotsafe vault is a folder of files, usually inside storage the user already syncs (iCloud Drive, Google Drive, Dropbox, OneDrive, Syncthing). The sync service only ever sees ciphertext. Implementation: `crates/core`.

```
<vault>/
  vault.json             header: format version, vault id, key slots, MAC
  records/<uuid>.json    one encrypted record per file
```

## Keys

```
password ──Argon2id──▶ KEK ─┐
                            ├─ unwraps ─▶ vault key (random, 256-bit)
recovery key ──HKDF────▶ KEK ─┘                │
                                               ├─ HKDF "dotsafe/v1/records"    ─▶ records key
                                               └─ HKDF "dotsafe/v1/header-mac" ─▶ header MAC key
```

- **Vault key:** 32 random bytes, generated once when the vault is created. It never changes, so changing the password doesn't re-encrypt any records.
- **Key slots:** each slot stores the vault key sealed under a key-encryption key (KEK). There is at most one slot of each kind:
  - `password`: KEK = Argon2id(password, 16-byte random salt). The default cost is 64 MiB, 3 passes, 1 lane, and the parameters are stored in the slot. On read, they're capped (2 GiB, 64 passes, 16 lanes) so a tampered header can't exhaust memory.
  - `recovery`: a 32-byte random key shown to the user once, as Crockford base32 (`XXXX-XXXX-…`, 13 groups). KEK = HKDF-SHA256(salt, recovery key). No Argon2 is needed because the key is already high-entropy.
- **Subkeys:** the vault key itself is never used directly. Each purpose gets its own HKDF-SHA256 subkey.

## Primitives

| Use | Algorithm |
|---|---|
| Encryption | XChaCha20-Poly1305, random 24-byte nonce per seal |
| Password KDF | Argon2id v1.3 |
| Subkeys, recovery KEK | HKDF-SHA256 |
| Header integrity | HMAC-SHA256 |
| Randomness | OS CSPRNG (`getrandom`) |

All are RustCrypto crates. Keys are wiped from memory on drop (`zeroize`).

## `vault.json`

```json
{
  "format": "dotsafe-vault",
  "version": 1,
  "vault_id": "uuid",
  "key_slots": [
    {
      "id": "uuid",
      "unlock": { "kind": "password", "kdf": { "alg": "argon2id", "m_cost_kib": 65536, "t_cost": 3, "p_cost": 1, "salt": "b64" } },
      "wrapped_key": { "nonce": "b64", "ct": "b64" }
    },
    {
      "id": "uuid",
      "unlock": { "kind": "recovery", "salt": "b64" },
      "wrapped_key": { "nonce": "b64", "ct": "b64" }
    }
  ],
  "mac": "b64"
}
```

- Each wrapped key's AAD is `"dotsafe/v1/slot" ‖ vault_id ‖ slot_id`, so a slot can't be moved to another vault or reused under another id.
- `mac` = HMAC(header MAC key, header serialized with `mac` empty). It's checked right after unlocking, so anyone without the vault key can't add, swap or edit slots undetected.

## `records/<uuid>.json`

```json
{ "format": "dotsafe-record", "version": 1, "id": "uuid", "sealed": { "nonce": "b64", "ct": "b64" } }
```

- **Ciphertext:** the JSON record encrypted with the records key.
- **AAD:** `"dotsafe/v1/record" ‖ vault_id ‖ id`. The id comes from the file name, so copying a record to another name or into another vault fails authentication.
- **Plaintext:** one of the following.
  - `project`: `{ id, name, deleted, updated }`
  - `environment`: `{ id, project_id, name, deleted, updated, vars: { KEY: { value | null, stamp } } }`

## Sync and merging

- **Environment ids:** a new environment's id is UUIDv5(project id, name). If two devices both create "development" for a project before syncing, they produce the same record, and the two versions merge. If that id is already taken by an environment that was renamed, the client falls back to a random id.

- **Stamps:** every change carries a `Stamp { at_ms, device }`. Stamps are ordered by time first, then by device id, so every device picks the same winner.
- **Merge rules:**
  - Environments merge per variable, last writer wins.
  - `name` and `deleted` fields merge on the record's `updated` stamp.
  - Projects merge per record, last writer wins.
  - A deleted variable is kept as a tombstone (`value: null`), so an older copy from another device can't bring it back.
- **Writes merge first:** `Vault::put` reads what's on disk, merges it in, then writes. Changes synced in from another device aren't overwritten.
- **Conflict copies:** a sync client may leave a file whose name starts with the record's UUID, such as `<uuid> (1).json`, `<uuid> 2.json` or `<uuid> (conflicted copy …).json`. dotsafe merges these on read and deletes them after the next write to that record.
- **Atomic writes:** writes go to a hidden `.<name>.<uuid>.tmp` and are then renamed into place. Readers ignore dotfiles and anything that isn't `.json`.

## Known limitations (v1)

- **Rollback:** someone with write access to the sync folder can restore an older, validly encrypted copy of a record. Merging limits the damage, because newer changes already on a device win when it next writes. Detecting rollback would need a signed per-vault version counter; that's planned.
- **Clock skew:** last-writer-wins trusts device clocks. A device with a clock far in the future wins every conflict. A hybrid logical clock would fix this.
- **Header conflicts:** if two devices change the password at the same moment, only one `vault.json` survives. Records are unaffected, because the vault key doesn't change.
- **Deleting a project doesn't cascade:** deleting a project doesn't delete its environments in storage. The app is expected to tombstone them too.
- **Decrypted values aren't wiped:** decrypted variable values are ordinary `String`s and aren't wiped from memory after use.
