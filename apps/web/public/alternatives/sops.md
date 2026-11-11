# SOPS alternative: dotsafe vs SOPS

> SOPS encrypts the values in YAML, JSON and .env files so you can commit them. dotsafe keeps secrets out of your repo, in an encrypted vault on your Mac, and away from your AI agents.

SOPS (Secrets OPerationS) is a free, open source editor of encrypted files, started at Mozilla in 2015 and now a CNCF Sandbox project. It encrypts each value in a YAML, JSON, ENV or INI file and leaves the keys readable, so encrypted files diff and merge well in git. Each file's data key is wrapped with age, PGP or a cloud KMS.

dotsafe takes the other route: no secrets file in the repo at all. Variables live in an encrypted vault on your Mac, synced through storage you already own, and `dotsafe run` hands them to your command.

## At a glance

| | dotsafe | SOPS |
| --- | --- | --- |
| Where secrets live | Encrypted vault on your Mac, synced through iCloud, Dropbox, S3 or WebDAV | Encrypted files, usually committed to git |
| What's encrypted | Names and values | Values; keys stay in cleartext |
| The key that unlocks them | Your master password, Touch ID, or a device key in the Keychain | age or PGP keys, AWS KMS, GCP KMS, Azure Key Vault, HuaweiCloud KMS or HashiCorp Vault |
| Source code | Open source, MIT | Open source, MPL-2.0 |
| Price | $49 once, up to 2 computers (Mac or Linux) | Free (cloud KMS calls are billed by your provider) |
| Run a command | `dotsafe run -- npm run dev` | `sops exec-env secrets.enc.yaml 'npm run dev'` |
| AI agents | Agents run approved commands and never get values; output is redacted | No agent controls; whatever can use your key can decrypt every value |
| Rotation | Rotates the API key itself, through OpenAI, Cloudflare and AWS | `sops rotate` replaces the file's data key; the secrets stay the same |
| Platforms | macOS app and CLI, Linux app and CLI | macOS, Linux and Windows CLI |

## Where SOPS is the better choice

- **Config that ships with the code.** Encrypted files sit next to the infrastructure config they belong to, and deploy with it.
- **Cloud KMS access control.** Grant or revoke decryption with IAM, and require several keys at once with key groups and Shamir secret sharing.
- **Free and cross-platform.** One Go binary for macOS, Linux and Windows, with no app or license.
- **Any config file.** It encrypts structured YAML and JSON, not just flat variables.

## Where dotsafe fits better

- **Nothing secret in git.** Even encrypted, a committed secret stays in history forever, and removing someone's key doesn't take back old copies. dotsafe keeps secrets out of the repo.
- **Variable names stay private.** SOPS leaves keys readable, so anyone with the repo sees which services you use. dotsafe encrypts names too.
- **Agents never hold your keys.** An agent asks dotsafe to run a command, you approve it with Touch ID, and it reads back output with values replaced by `[dotsafe:NAME]`. OpenAI and Anthropic keys go through a local proxy, so the process never has them.
- **Rotating means a new API key.** dotsafe gets a new key from the provider, makes it live in every environment that uses it, and pushes it to Vercel or Cloudflare Workers.

## Switching from SOPS

Decrypt a flat file to .env lines and import them, without writing the values to disk:

```bash
dotsafe import <(sops decrypt --output-type dotenv secrets.enc.yaml)
```

Then replace `sops exec-env secrets.enc.yaml '<command>'` with `dotsafe run -- <command>` in your scripts. See [Import and export](/docs/import-export).

## Sources

Checked on November 11, 2026.

- [SOPS documentation](https://getsops.io/docs/) and [SOPS on GitHub](https://github.com/getsops/sops) (v3.13.3, MPL-2.0)
- [Security model](https://getsops.io/docs/security/) (keys in cleartext, values encrypted)
- [Passing secrets to other processes](https://getsops.io/docs/usage/advanced/) (`exec-env`)
- [Key management](https://getsops.io/docs/usage/key-management/) (`updatekeys`, `rotate`) and [key groups](https://getsops.io/docs/usage/identities/key-groups/)
