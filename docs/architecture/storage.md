# Storage

**Status:** M1 and M2 are implemented, and so are WebDAV, backup snapshots and moving a vault (see [As built](#as-built-webdav-backups-moving)). Live replicas (a second store kept in step after every write) are still proposed; backups cover the "a copy elsewhere" need for now.

## Goals

- Let users keep the vault in a synced folder, any S3-compatible bucket (AWS S3, Cloudflare R2, Backblaze B2, MinIO), WebDAV, and later the Google Drive and Dropbox APIs.
- Treat every backend as untrusted: it stores ciphertext and can lie, delete or roll back, and we detect it.
- Work offline, sync when possible, and never lose a write.
- Let a vault live in more than one place (replicas) as a backup.

## Non-goals

- Server-side merging or access control. Backends store opaque bytes.
- Real-time collaboration. Sync is eventually consistent, in seconds to minutes.

## Object layout

Unchanged from v1, plus one object:

```
<prefix>/
  vault.json            header: key slots, MAC (v2 adds device slots and key epochs)
  head.json             v2: sealed manifest for rollback detection
  records/<uuid>.json   one sealed record per object
```

Object names contain only UUIDs. Names of projects, environments and secrets are inside the ciphertext.

## The `Store` interface

Defined in `crates/core`, synchronous like the rest of core. Network backends do blocking IO on worker threads, like the license client does.

```rust
pub trait Store: Send + Sync {
    fn location(&self) -> String;                                   // for messages
    fn list(&self, dir: &str) -> Result<Vec<String>>;               // keys directly inside `dir`
    fn get(&self, key: &str) -> Result<Option<Object>>;             // bytes + etag
    fn put(&self, key: &str, bytes: &[u8], cond: Cond) -> Result<Put>;
    fn delete(&self, key: &str) -> Result<()>;
}

pub enum Cond { Any, IfAbsent, IfMatch(ETag) }
pub enum Put { Written(ETag), Conflict }   // Conflict: someone else wrote first
```

Implemented in M1 (`crates/core/src/store.rs`). `list` returns keys only, so pulls that diff ETags will add a listing with versions when the cache lands.

`Vault` takes a `Box<dyn Store>` instead of a root path. Its write path becomes a compare-and-swap loop:

```
loop {
    current = store.get(key)                 // may be None
    merged  = merge(current, ours)           // existing per-record merge rules
    match store.put(key, seal(merged), IfMatch(current.etag) or IfAbsent) {
        Written(_) => break,
        Conflict   => continue,              // re-read and merge again (bounded retries)
    }
}
```

Because merging is per record and deterministic, retries always converge.

## Backends

| Backend | Conditional writes | Notes |
|---|---|---|
| **Folder** (`FolderStore`) | Best effort. ETag = SHA-256 of the bytes; compare, then atomic rename | Today's behaviour. Sync clients can't do compare-and-swap, so merge-on-read and conflict-copy handling stay |
| **S3 / R2** (`crates/store-s3`) | Yes: `If-None-Match: *` to create, `If-Match: <etag>` to update; 412 means `Conflict` | AWS S3 and R2 both support conditional writes. Check each other S3-compatible provider; fall back to folder-style merging where they don't |
| **WebDAV** | `If-Match` / `If-None-Match` headers | Nextcloud and similar |
| **Drive / Dropbox APIs** (later) | Revision ids | Lets users sync without installing the desktop sync client |

**S3 client:** a small SigV4 signer (for example `rusty-s3`) plus `ureq`, both blocking and rustls-based. Not the AWS SDK: it pulls in tokio and a large dependency tree we'd have to trust.

**M2 status:** `crates/stores` has `S3Store` (rusty-s3 presigned requests over ureq, conditional `If-None-Match`/`If-Match` writes, paginated `ListObjectsV2`) and `open(config)`. It's tested against an in-process S3 stand-in, not yet against real R2 or AWS in CI.

## Local cache and sync

```
             read/write                         push (outbox)
  core  ◀──────────────▶  encrypted cache  ──────────────────▶  primary store
                          (ciphertext +     ◀──────────────────
                           last-seen head)     pull (head diff)
```

- **The cache** is a mirror of the objects, ciphertext only, in the app data directory (`…/dotsafe/cache/<vault_id>/`). Reads come from the cache, so the app works offline and stays fast on slow buckets.
- **Writes** go to the cache and an outbox. The outbox drains to the store with the CAS loop above. A write queued offline is pushed later and merged like any other.
- **Pulls** fetch `head.json` first, compare it with the last-seen head, and download only the records whose entries changed. Listing every object is only a fallback.
- **The folder backend** can skip the cache. The synced folder already is one.
- **Implemented (M2)** as `CachedStore` in core. Conflicts with unpushed changes become `(remote)` copies the vault merges, `head.json` keeps its previous version until merged (rollback detection across sessions), and objects missing from the bucket are re-uploaded rather than deleted. Network calls happen outside the index lock. The desktop app syncs every minute and on demand; the CLI syncs on open and with `dotsafe sync`.

## Configuration and credentials

`Config.vault_path` becomes:

```rust
pub enum StoreConfig {
    Folder { path: PathBuf },
    S3 { endpoint: Url, region: String, bucket: String, prefix: String },
    WebDav { url: Url },
}
```

- **Credentials** (access key id and secret, WebDAV password) are stored in the OS keychain under the vault id. They're never written to `config.json`, never put in the vault, and never sent to the webview.
- **Connecting a new device:** choose a provider, enter the endpoint, bucket and access key, then the master password (or approve on a device that's already set up; see [devices-and-keys.md](devices-and-keys.md)).
- **Least privilege:** the setup screen shows a minimal policy to copy. On R2, a bucket-scoped "Object Read & Write" token. On S3:

```json
{
  "Statement": [
    { "Effect": "Allow", "Action": ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
      "Resource": "arn:aws:s3:::BUCKET/PREFIX/*" },
    { "Effect": "Allow", "Action": "s3:ListBucket", "Resource": "arn:aws:s3:::BUCKET",
      "Condition": { "StringLike": { "s3:prefix": "PREFIX/*" } } }
  ]
}
```

## As built: WebDAV, backups, moving

- **WebDAV** (`crates/stores/src/webdav.rs`): a folder on Nextcloud, ownCloud, a NAS or Fastmail, behind the same `CachedStore` as buckets, so it works offline and syncs. Conditional writes use `If-None-Match: *` and `If-Match` with the server's ETags; a 412 is a conflict, like S3. The `records/` folder is made with `MKCOL` on first write (servers answer a PUT into a missing folder with 409). Listing is `PROPFIND` with `Depth: 1`. HTTPS only (plain HTTP just for `localhost`), redirects off, Basic auth with the password (ideally an app password) in the OS keychain or `DOTSAFE_WEBDAV_PASSWORD`. Set up with `dotsafe init --webdav URL --user NAME` or the WebDAV tab in the app.
- **Backups** (`core::backup`): a dated snapshot folder (`dotsafe-backup-YYYY-MM-DD-HHMM`) of every encrypted object, in a folder each machine chooses (`backup_dir` in its config). The last 10 are kept. A snapshot is a complete vault folder, so restoring is connecting to it (and moving it back if wanted). `dotsafe backup [--to DIR]`; the app backs up once a day while unlocked and has "Back up now". Copies come from the open vault's own store, so a bucket vault's cache isn't opened twice.
- **Moving** (`backup::copy_to_empty`): copies every object to an empty place (refused if it already holds a vault), checks the copy opens with the vault key, then points this machine's config at it. The old copy is left alone until the user deletes it; other devices connect to the new place themselves. `dotsafe move --path | --bucket … | --webdav … --user …`; the app moves to a folder ("Move vault…").

## Replicas

- A vault has one **primary** and any number of **replicas**, for example iCloud as primary and R2 as an offsite backup.
- Replicas receive byte-identical copies of objects, pushed after each successful primary write and reconciled in the background. Nothing is re-encrypted, and replicas need no keys.
- **Restoring** means promoting a replica to primary. Rollback detection (below) still applies, so a stale replica is flagged, not trusted.
- Replicas are write-only from the client's point of view until promoted. This keeps sync to a single source of truth.

## Rollback detection

v1 can't tell when storage serves an older, validly encrypted record. v2 adds `head.json`:

```json
{ "format": "dotsafe-head", "version": 2, "sealed": { "nonce": "b64", "ct": "b64" } }
```

- **Plaintext:** `{ records: { <uuid>: stamp } }`: for every record, the stamp of its newest change. (No ciphertext hash is needed: the only undetectable substitution is an older valid copy, which has an older stamp.)
- Sealed with the records key, with AAD `"dotsafe/v2/head" ‖ vault_id`.
- **Updated with every record write,** using the same CAS loop. Heads merge per entry by taking the higher stamp, so concurrent devices converge, and conflict copies on synced folders merge like records do.
- **Checks:**
  - Each client keeps the last head it saw.
  - A head where any entry goes backwards is a rolled-back head.
  - A record whose newest stamp is older than its head entry is a rolled-back record.
  - On a mismatch the client keeps its newer cached copy, re-pushes it, and warns the user.
- **Limit:** a device that has never seen the vault before has nothing to compare against. It trusts the first head it reads, the same as any trust-on-first-use system.
- **M1 status:** heads are written and checked, and stale records are reported by `Vault::stale_records()` within a session. Keeping the last-seen head across sessions, and telling a rollback apart from a sync client that delivered `head.json` before the record, need the local cache (M2).

## Clock skew: hybrid logical clocks

v1 stamps are wall-clock time, so a device with a clock far in the future wins every conflict. v2 stamps are hybrid logical clocks:

```
Stamp { at_ms, counter, device }
new stamp: at_ms = max(now, highest at_ms seen); counter increments when at_ms didn't advance
```

An edit made after seeing another device's change always orders after it, whatever the clocks say. Stamps more than 24 hours ahead of the local clock trigger a warning about the device's clock.

## Metadata a provider can see

| Visible | Hidden |
|---|---|
| Number of records, roughly | Names of projects, environments, secrets |
| Object sizes, rounded up by padding | Values, history, which records belong to which project |
| When objects change, and from which IP | Which device made a change (inside the ciphertext) |

**Padding (v2):** record plaintext is padded to the next multiple of 1 KiB up to 16 KiB, then to the next power of two. One secret and five look the same size.

## Migration from v1

On the first unlock by a v2 client, a v1 folder vault gets device slots, a `head.json`, and records re-sealed with padding and HLC stamps. A folder vault can then be copied to a bucket from the app with **Move vault…** (copy every object, verify, switch the config, keep the old copy until the user deletes it).

## Open questions

- Whether to offer B2 and MinIO in M2 or after we've checked their conditional-write behaviour.
- Garbage-collecting tombstones once every known device has seen them (needs per-device "last seen head" tracking).
- Rate-limit and cost guardrails for very chatty sync on paid buckets.
