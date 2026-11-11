# dotenvx alternative: dotsafe vs dotenvx and dotenv-vault

> dotenvx encrypts your .env files so you can commit them. dotsafe takes secrets out of your repo entirely, and keeps them from your AI agents.

dotenvx, from the creator of dotenv, encrypts the values inside your `.env` files with a public key, so the files can live in git. The private key that decrypts them stays outside the repo. It replaced dotenv-vault, which its makers now call legacy.

dotsafe solves the same problem from the other side: there is no `.env` file at all. Variables live in an encrypted vault on your Mac, and `dotsafe run` hands them to your command.

## At a glance

| | dotsafe | dotenvx |
| --- | --- | --- |
| Where secrets live | Encrypted vault on your Mac, synced through storage you own | Encrypted `.env` files committed to git |
| The key that unlocks them | Your master password, Touch ID, or a device key in the Keychain | A private key in `.env.keys`, your OS keychain, a password manager, or the paid Armor service |
| Source code | Open source, MIT | Open source, BSD 3-Clause (Armor is paid) |
| Price | $49 once, up to 2 computers (Mac or Linux) | Free; Armor from $5 a month |
| AI agents | Agents run approved commands and never get values; output is redacted | `dotenvx run` gives a process the real values; `redacted` rules mask them in output |
| Approval | Touch ID per command, with an audit log | None locally; Armor adds approval and audit on decrypt |
| Rotation | Built in, through OpenAI, Cloudflare and AWS | Edit and re-encrypt the file |
| Platforms | macOS and Linux app and CLI | macOS, Linux and Windows CLI, with SDKs |

## Where dotenvx is the better choice

- **Cross-platform.** It runs on Windows too, and in any language with its SDKs.
- **Secrets travel with the code.** CI and production decrypt with one `DOTENV_PRIVATE_KEY`, no other service needed.
- **No app to install.** It's a single CLI.

## Where dotsafe fits better

- **Nothing secret in git.** Even encrypted, a committed secret lives in history forever. dotsafe keeps secrets out of the repo.
- **Agents never hold your keys.** An agent asks dotsafe to run a command, you approve it with Touch ID, and it reads back output with values replaced. OpenAI and Anthropic keys go through a local proxy, so the process never has them.
- **One secret, many projects.** Link a key into every environment that uses it, and rotate it once.

## Switching from dotenvx

Decrypt the file, import it, and remove both the file and its key:

```bash
dotenvx decrypt                 # .env back to plain text
dotsafe import
rm .env .env.keys
```

Then replace `dotenvx run --` with `dotsafe run --` in your scripts.

## Sources

Checked on November 5, 2026.

- [dotenv.org](https://dotenv.org) (dotenv-vault is legacy) and [dotenvx.com](https://dotenvx.com)
- [dotenvx pricing](https://dotenvx.com/pricing) and [Armor](https://dotenvx.com/armor)
- [Agents, meet Dotenvxfile](https://dotenvx.com/agents)
- [dotenvx on GitHub](https://github.com/dotenvx/dotenvx)
