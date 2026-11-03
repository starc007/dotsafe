# Security

> How the vault is encrypted, what the app and agents can see, and what dotsafe never stores.

Your secrets are encrypted on your device. dotsafe never sees them, and neither does your storage provider. The code is open source, so you can check every claim on this page.

## Encryption

- Names and values are encrypted with **XChaCha20-Poly1305**, under keys derived from the vault key.
- The vault key is sealed with a key derived from your master password by **Argon2id**, and with your recovery key. Neither is ever stored.
- Every record is authenticated and bound to its vault and its own id, so it can't be changed or moved without being caught.
- A sealed vault head records the newest version of every record, so a Mac notices when storage serves an older copy of one than it has already seen.

## Who can see what

| Who | Can | Can't |
| --- | --- | --- |
| Your storage provider | See encrypted files, their count and sizes; delete them | Read names or values, or change them unnoticed |
| dotsafe (the license server) | Check or revoke a license | Read or change any vault, or lock you out of one |
| A thief with your locked Mac | Copy the encrypted files | Unlock them without your password or your device key |
| A device you removed | Keep what it already read | Read anything written after the removal |
| An AI agent | See names; ask to run commands | Get a value from dotsafe, run anything you didn't approve, or read a proxied key |

## On your Mac

- The vault locks after 15 minutes without activity, by default.
- Device keys and storage credentials live in the macOS keychain. Device keys are read only after Touch ID.
- The app's interface gets a value only when you reveal or edit one. Lists carry names only, and copying happens outside it. Copied values are cleared from the clipboard after 30 seconds.
- The interface can't load remote content, or reach your files or clipboard.
- Updates are signed, checked before they install, and only download when you click.

## What dotsafe doesn't protect against

> [!WARNING]
> Malware running as you while the vault is unlocked can read memory or log your password. No local secret manager can stop that; auto-lock and short agent grants limit the window.

- A command you approved, once it holds a secret. It can do what it likes with it.
- Someone learning that you have a vault, or roughly how many records are in it.

## Read more

- [Threat model](https://github.com/starc007/dotsafe/blob/main/docs/architecture/threat-model.md)
- [Vault format](https://github.com/starc007/dotsafe/blob/main/docs/vault-format.md)
