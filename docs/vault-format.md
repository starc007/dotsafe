# Vault format (v2 records, v3 header)

A dotsafe vault is a set of objects in a store: today a folder, usually inside storage the user already syncs (iCloud Drive, Google Drive, Dropbox, OneDrive, Syncthing). The store only ever sees ciphertext. Implementation: `crates/core` (`vault.rs`, with the `Store` interface in `store.rs`).

```
<vault>/
  vault.json             header: format version, vault id, key epoch, key slots, MAC
  head.json              sealed newest stamp of every record, for rollback detection
  records/<uuid>.json    one encrypted record per object
```

v1 vaults are upgraded in place on the first unlock by a v2 client (see [Upgrading from v1](#upgrading-from-v1)).

## Keys

```
password ──Argon2id──▶ KEK ─┐
                            ├─ unwraps ─▶ vault key (random, 256-bit)
recovery key ──HKDF────▶ KEK ─┘                │
                                               ├─ HKDF "dotsafe/v1/records"    ─▶ records key
                                               └─ HKDF "dotsafe/v1/header-mac" ─▶ header MAC key
```

The `v1` in these HKDF labels and in the AAD prefixes below is a domain-separation string, not the format version. Labels whose meaning didn't change in v2 kept their v1 spelling, so existing slots and keys stay valid.

- **Vault key:** 32 random bytes, generated when the vault is created. Changing the password doesn't re-encrypt any records.
- **Epochs:** `epoch` in the header names the vault key that seals records (1 for every vault today). Re-keying, which will come with device revocation, adds a new epoch and keeps older keys in `previous_keys`, sealed under the new one, until every record is re-sealed. `previous_keys` is always empty for now.
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
  "version": 2,
  "vault_id": "uuid",
  "epoch": 1,
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
- `epoch` is omitted when 0 (v1) and `previous_keys` when empty, so a v1 header serializes, and verifies, exactly as v1 wrote it.

### Header v3: devices, enrollment codes and re-keying

- **New slot kinds:**
  - `{ "kind": "device", "device_id", "name", "added", "salt" }`: KEK = HKDF-SHA256(salt, device key, "dotsafe/v3/device-kek"). The 32-byte device key lives in that device's keychain.
  - `{ "kind": "enrollment", "expires", "salt" }`: KEK = HKDF-SHA256(salt, 128-bit code, "dotsafe/v3/enrollment-kek"). Ignored after `expires` (Unix ms), and removed on the next header write.
- **Slot epochs:** every slot has an `epoch` (omitted when 0, meaning 1). A slot whose epoch isn't the header's is stale. A stale password or recovery slot is never used. A stale device slot means "unlock with the password once".
- **Re-key** (on revoking a device):
  1. Generate a new vault key, and bump `epoch`.
  2. Store the old key in `previous_keys` as `{ epoch, wrapped_key }`, sealed under the new key with AAD `"dotsafe/v3/previous-key" ‖ vault_id ‖ epoch`.
  3. Recreate the password and recovery slots for the new key; device slots go stale.
  4. Re-seal every record and `head.json` under the new key, then empty `previous_keys`.
  
  A client that unlocks while `previous_keys` isn't empty finishes the re-seal first.
- **Records from a dropped epoch** fail to open with no key, and are skipped, not trusted (`Vault::unreadable_records`).
- **`head.json`** gains an `epoch` field (omitted means 1). A head from an epoch the client has no key for is replaced on the next write.
- **Header writes** re-read the newest header, re-apply the change and write it conditionally on its ETag, retrying on conflict.

### Header v4: merging conflict copies

On a synced folder the conditional write can't be atomic, so two devices changing the header at once can leave the sync client with `vault.json` plus a conflict copy (`vault 2.json`, `vault (1).json`, `vault (conflicted copy …).json`).

- **`removed_slots`:** a list of the ids of slots removed from the header, newest last, capped at 256 (omitted when empty). Every removal adds to it: a password or recovery key replaced, a device removed, an enrollment code used or expired, slots dropped by a re-key.
- **Merging:** a copy is merged only if it's from the same vault and epoch and its `mac` verifies with the vault key. The merged header keeps every slot from either side unless either side removed it, takes the union of `removed_slots`, and is re-signed. So a password replaced on one device can't come back from the other's copy. If both devices set a new password, both work until the next change, which replaces both.
- **When:** on every header write (merged copies are deleted once the write succeeds), and on unlock if there's anything to merge or clean up. A copy from an earlier epoch is obsolete and deleted. A copy from a later epoch (another device re-keyed) can't be checked with this key, so it's left alone. So is a copy whose `mac` doesn't verify.
- **Unlocking:** if the credential doesn't fit `vault.json`, the copies are tried too, because a password changed on another device may so far exist only in a copy.
- v3 headers are read and rewritten as v4 on unlock.

## `records/<uuid>.json`

```json
{ "format": "dotsafe-record", "version": 2, "id": "uuid", "epoch": 1, "sealed": { "nonce": "b64", "ct": "b64" } }
```

- **Ciphertext:** the JSON record encrypted with the records key of its epoch.
- **AAD:** `"dotsafe/v2/record" ‖ vault_id ‖ epoch (u32, big-endian) ‖ id`. The id comes from the object name, so copying a record to another name, another vault or another epoch fails authentication. v1 records (no `epoch`, AAD `"dotsafe/v1/record" ‖ vault_id ‖ id`) are still read, and are rewritten as v2 on the next write.
- **Padding:** the plaintext JSON is padded with trailing spaces to the next multiple of 1 KiB up to 16 KiB, then to the next power of two. Ciphertext sizes reveal only that bucket.
- **Plaintext:** one of the following.
  - `project`: `{ id, name, deleted, updated }`
  - `environment`: `{ id, project_id, name, deleted, updated, vars: { KEY: { value, secret?, stamp } } }`. A variable holds its own `value`, or links to a shared secret by its id in `secret`. Neither set is a tombstone.
  - `secret`: `{ id, name, deleted, updated, policy: { rotate_every_days?, generator? }, versions: { <uuid>: { value | null, state, created, changed, note? } } }`. `state` is `pending`, `current`, `previous` or `retired`. A `null` value has been wiped.

  - `connection`: `{ id, service, name, credential?, settings: { k: v }, deleted, updated }`. An account at a provider or deploy service. `credential` is the id of the shared secret holding its admin token, absent for services that need none (Stripe).
  - `binding`: `{ id, secret, provider?: { connection, settings }, updated, keys: { <version uuid>: key_id }, targets: { <uuid>: { connection, name, settings, pushed?: { sha256, at }, deleted, changed } } }`. Where one secret's values come from and where they're pushed. id = UUIDv5(secret id, "binding").
  - `audit`: `{ id, device, day, deleted, updated, events: { <uuid>: { at, actor, action, outcome, summary, environment, variables, detail? } } }`. One per device per day, id = UUIDv5(device, "audit:<day>"), where `day` counts days since the Unix epoch. Events merge by id. A deleted (pruned after 90 days) record has no events and wins over copies that have them.

### Record version 3 (shared secrets and newer kinds)

Records that use shared secrets (`secret` records, and environments with at least one linked variable) and `audit` records are written with `"version": 3` and AAD `"dotsafe/v3/record" ‖ vault_id ‖ epoch ‖ id`. Everything else stays version 2.

A client that predates shared secrets would read a linked variable as a tombstone and could rewrite the environment without its links. With v3, such a client refuses those records ("unsupported record version 3"), while vaults that don't use shared secrets stay readable. The version is bound into the AAD, so relabelling a v3 record as v2 fails authentication.

**Merging secrets:** `name`, `deleted` and `policy` merge on `updated`. Versions merge by id, and each version's `value`, `state` and `note` merge on its own `changed` stamp, so two devices rotating at once keep both versions. The current value is the `current` version with the newest `changed`, which is how concurrent promotions resolve.

### Record version 4 (integrations)

`connection` and `binding` records, and `audit` days that log a `rotate`, `push` or `revoke` action, are written with `"version": 4` and AAD `"dotsafe/v4/record" ‖ vault_id ‖ epoch ‖ id`. Clients that predate integrations refuse them ("unsupported record version 4") instead of failing to parse an unknown record type, and relabelling one as v3 or v2 fails authentication. Every other record keeps the oldest version that can hold it.

**Merging bindings:** `provider` merges on `updated`. Targets merge by id on their own `changed` stamp. `keys` is the union of both sides; each version's key id is set once, when the provider issues it.

**Retention:** a retired value is wiped 30 days after retirement, and values beyond the 10 newest versions are wiped unless still pending, current or previous. Wiping happens on the next change to the secret.

## `head.json`

```json
{ "format": "dotsafe-head", "version": 2, "sealed": { "nonce": "b64", "ct": "b64" } }
```

- **Plaintext:** `{ "records": { "<uuid>": <stamp> } }`, the newest stamp (`Record::max_stamp`: the latest of `updated` and every variable stamp) of each record. Padded like records.
- Sealed with the records key, AAD `"dotsafe/v2/head" ‖ vault_id`.
- Updated after every record write, with the same conditional-write loop as records. Heads merge per record by taking the newer stamp, and sync-conflict copies (`head (1).json`, …) are merged and removed like record copies. If the head write keeps losing races, it's left a step behind; the next write catches it up.
- **Rollback check:** a vault remembers the newest stamp it has seen for each record, from heads and records. A record read with an older newest stamp is reported by `Vault::stale_records()`. On a synced folder, a sync client can deliver `head.json` before the record it describes, so a stale record there may just be still syncing. Telling the two apart needs a persistent local cache (planned with S3 support).

## Stores and conditional writes

- A store (`Store` in `store.rs`) holds opaque bytes under keys (`vault.json`, `head.json`, `records/<uuid>.json`) and can list, get, put and delete them. Writes take a condition: always, only if absent, or only if still at a known version (ETag).
- `Vault::put` reads the record and its copies, merges, then writes only if the canonical object is unchanged. If another writer got in first, it re-reads, merges and tries again (up to 16 times, then `Error::Busy`). Merging is deterministic, so retries converge.
- **`FolderStore`:** ETags are SHA-256 hashes of the bytes, and the check-then-rename isn't atomic. A sync client can't offer anything stronger, so merge-on-read and conflict-copy handling remain the real safety net there.
- `vault.json` writes are not yet conditional (see limitations).

## Local cache (vaults in a bucket)

A vault in a bucket is opened through `CachedStore` (`cache.rs`): a mirror of the bucket's objects (ciphertext only) in the app data directory, plus an `index.json` of each object's remote ETag.

- **Reads** come from the mirror, so the vault opens fast and works offline.
- **Writes** go to the bucket with the remote ETag as the condition. Unreachable means the write is kept locally as unpushed and pushed by the next sync.
- **Both sides changed:** if the bucket changed under an unpushed local write, the remote version is kept beside it as `<uuid> (remote).json`. The vault merges it like any sync-conflict copy, and the local write isn't pushed until it has been merged.
- **`head.json`** is never simply replaced: the version this device last saw is kept as `head (cached).json` until merged, so a rolled-back bucket can't make the device forget newer stamps. This is what makes rollback detection work across sessions for bucket vaults.
- **Missing objects** (a wiped or rolled-back bucket) are uploaded again from the mirror, never deleted locally. The vault never deletes its own objects (deletes are tombstones), so anything missing was lost remotely.

## Sync and merging

- **Environment ids:** a new environment's id is UUIDv5(project id, name). If two devices both create "development" for a project before syncing, they produce the same record, and the two versions merge. If that id is already taken by an environment that was renamed, the client falls back to a random id.

- **Stamps:** every change carries a `Stamp { at_ms, counter, device }`, a hybrid logical clock reading (`counter` is omitted when 0 and absent in v1). Stamps are ordered by time, then counter, then device id, so every device picks the same winner. `Vault::stamp` never returns a stamp older than one the vault has read, so an edit made after seeing another device's change always orders after it, even if this device's clock is behind.
- **Merge rules:**
  - Environments merge per variable, last writer wins.
  - `name` and `deleted` fields merge on the record's `updated` stamp.
  - Projects merge per record, last writer wins.
  - A deleted variable is kept as a tombstone (`value: null`), so an older copy from another device can't bring it back.
- **Writes merge first:** `Vault::put` reads what's on disk, merges it in, then writes. Changes synced in from another device aren't overwritten.
- **Conflict copies:** a sync client may leave a file whose name starts with the record's UUID, such as `<uuid> (1).json`, `<uuid> 2.json` or `<uuid> (conflicted copy …).json`. dotsafe merges these on read and deletes them after the next write to that record.
- **Atomic writes:** writes go to a hidden `.<name>.<uuid>.tmp` and are then renamed into place. Readers ignore dotfiles and anything that isn't `.json`.

## Upgrading from v1

On the first unlock (password, recovery key or cached session key) of a v1 vault, the client:

1. Re-seals every record as v2 (epoch 1, padded), merging conflict copies along the way, and writes `head.json`.
2. Then rewrites the header as v2 (`epoch: 1`) and re-signs it.

Records go first and the header last, so an interrupted upgrade simply runs again on the next unlock: v2 clients read v1 and v2 records alike. A v1 client refuses a v2 vault (unsupported vault version), so older installs must update.

## Known limitations (v2)

- **Rollback on synced folders:** see `head.json` above. Rollbacks are detected but can't yet be told apart from sync lag, and a device that has never seen the vault trusts the first head it reads. Bucket vaults keep the last-seen head in their cache, so they also notice across sessions.
- **Clock skew:** a device whose clock runs far ahead still pushes everyone's stamps forward. Causally later edits now win, but a future-dated edit beats a concurrent one made elsewhere.
- **Header conflicts on synced folders:** conflict copies of `vault.json` are merged (header v4). A sync client that silently overwrites instead of keeping a copy can still lose one of two truly simultaneous changes; there's nothing left to merge.
- **Deleting a project doesn't cascade:** deleting a project doesn't delete its environments in storage. The app is expected to tombstone them too.
- **Decrypted values aren't wiped:** decrypted variable values are ordinary `String`s and aren't wiped from memory after use.
