# Storage and sync

> Keep the vault in a synced folder, an S3 or R2 bucket, or WebDAV.

A vault is a folder of encrypted files. dotsafe never runs a sync server: you choose where the vault lives, and whatever stores it only ever sees ciphertext. Changes made on two Macs at once are merged, not overwritten.

## A synced folder

The default is `iCloud Drive/dotsafe`. Any folder that syncs works: Google Drive, Dropbox, OneDrive, Syncthing.

```bash
dotsafe init --path ~/Dropbox/dotsafe
```

## An S3-compatible bucket

Cloudflare R2, AWS S3, Backblaze B2 and MinIO all work. Each Mac keeps an encrypted copy, so dotsafe works offline and syncs when it's back online. The app syncs in the background, and `dotsafe sync` syncs now.

```bash
dotsafe init --bucket my-vaults --endpoint https://<account>.r2.cloudflarestorage.com
dotsafe init --bucket my-vaults --endpoint https://s3.us-east-1.amazonaws.com --region us-east-1
dotsafe sync                  # exchange changes now
```

The access key is kept in the macOS keychain, never in a config file. For CI, set `DOTSAFE_S3_ACCESS_KEY_ID` and `DOTSAFE_S3_SECRET_ACCESS_KEY` instead.

> [!TIP]
> Give that key access to this one bucket only.

## WebDAV

For Nextcloud, ownCloud or a NAS:

```bash
dotsafe init --webdav https://cloud.example.com/remote.php/dav/files/me/dotsafe/ --user me
```

The password is asked for and kept in the keychain, or read from `DOTSAFE_WEBDAV_PASSWORD`.

## Backups

```bash
dotsafe backup --to ~/Backups/dotsafe
```

A backup is a dated, still-encrypted copy of the vault. The folder is remembered for next time, and the last 10 backups are kept.

## Moving the vault

```bash
dotsafe move --path ~/Dropbox/dotsafe
dotsafe move --bucket my-vaults --endpoint https://<account>.r2.cloudflarestorage.com
```

The vault is copied and this Mac switches to it. The old copy stays until you delete it. Other Macs connect to the new location with `dotsafe init`.
