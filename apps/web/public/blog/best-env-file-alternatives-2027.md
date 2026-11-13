# Best .env file alternatives in 2027

> Seven ways to stop keeping secrets in a plain .env file, grouped by approach: encrypted files in git, references to a password manager, hosted services and a local vault. With the command to run your app and to move an existing .env into each.

A `.env` file is the simplest way to give an app its secrets, and the easiest to leak. It sits in plain text in your project, gets copied between machines over chat, slips into a commit, and is read by every tool and AI agent that can open files in the repo.

There are good alternatives, and they work in very different ways. This post groups them by approach, so you can pick the trade-off first and the tool second.

We make dotsafe, one of the tools below. Each section says who a tool suits, and every claim about another tool links to its own docs, checked on the date at the bottom.

## At a glance

| | Approach | What goes in git | What unlocks the secrets | Run your app | Price |
| --- | --- | --- | --- | --- | --- |
| direnv | Loads env per directory | Up to you; usually nothing | Nothing: it doesn't encrypt | `cd` into the directory | Free, MIT |
| dotenvx | Encrypted `.env` in git | The encrypted `.env` | A private key in `.env.keys` | `dotenvx run -- npm run dev` | Free, BSD 3-Clause |
| SOPS | Encrypted file in git | The encrypted file | age, PGP or a cloud KMS key | `sops exec-env secrets.enc.env 'npm run dev'` | Free, MPL 2.0 |
| 1Password | References in `.env` | `op://` references, no values | Your 1Password account | `op run --env-file=.env -- npm run dev` | From $3.99 a month |
| Doppler | Hosted service | Nothing | A Doppler login or token | `doppler run -- npm run dev` | Free for 3 users, then $8 a user |
| Infisical | Hosted or self-hosted service | A project config file | An Infisical login or machine identity | `infisical sm run -- npm run dev` | Free for 5 identities; Pro $20 an identity |
| dotsafe | Local encrypted vault | A link file with ids only | Your master password or Touch ID | `dotsafe run -- npm run dev` | $49 once, 2 computers |

Monthly prices are billed yearly where the vendor bills that way.

## direnv: convenience, not security

direnv loads and unloads environment variables as you move between directories. You put shell code in an `.envrc`, run `direnv allow .` once, and its shell hook loads it whenever you `cd` in. Its `dotenv` function loads a `.env` file:

```bash
echo 'dotenv' > .envrc
direnv allow .
```

direnv doesn't encrypt anything: `.envrc` is plain shell code, and a `.env` it loads is still plain text on disk. It's great at "the right variables in the right directory", and pairs well with any tool below that can print or inject them. On its own, it doesn't make secrets safer.

**Best for:** switching variables per project, on top of something that actually protects them.

## dotenvx: encrypt the .env and commit it

dotenvx, from the creator of dotenv, encrypts each value in your `.env` in place and writes the private key to `.env.keys`. Its docs tell you to commit the encrypted `.env` with your code. At run time it decrypts and injects the values:

```bash
dotenvx encrypt                  # encrypts .env, writes the key to .env.keys
dotenvx run -- npm run dev
```

In CI and production, you set `DOTENV_PRIVATE_KEY` instead of shipping `.env.keys`. Keep `.env.keys` out of git: it decrypts everything.

**Best for:** small teams who want secrets to travel with the code, for free.
**The catch:** the encrypted secrets live in git history forever, so a leaked key decrypts every old version too.

## SOPS: encrypted config files, with real key management

SOPS encrypts the values in YAML, JSON, INI and `.env` files and leaves the keys readable, so diffs and merges stay sane. The data key is protected by age, PGP or a cloud KMS (AWS, GCP, Azure or HuaweiCloud), so access can be granted and revoked with IAM.

```bash
sops encrypt --age <public key> .env > secrets.enc.env
sops exec-env secrets.enc.env 'npm run dev'
```

SOPS picks the format from the file name, so name the encrypted file ending in `.env`, or pass `--input-type dotenv`.

**Best for:** infrastructure and GitOps teams, and anyone who wants cloud KMS access control.
**The catch:** like dotenvx, everything is in git history, and variable names stay readable. More in [dotsafe vs SOPS](/alternatives/sops).

## 1Password: keep the .env, swap values for references

With the 1Password CLI, your `.env` holds secret references instead of values, like `DB_PASSWORD=op://app-dev/db/password`. The file has no secrets in it, and `op run` swaps in the real values when your app starts:

```bash
op run --env-file=.env -- npm run dev
```

1Password Environments go further: import an existing `.env` file in the app, run with `op run --environment <id>`, and on Mac and Linux mount a `.env` that's served from 1Password and never written to disk.

**Best for:** people and teams who already use 1Password.
**The catch:** a subscription and an account, and your secrets live in 1Password's cloud, end to end encrypted.

## Doppler: a hosted secrets service

Doppler keeps your secrets in its cloud and injects them into commands. Link a directory to a project, upload your `.env`, then run through it:

```bash
doppler setup
doppler secrets upload .env
doppler run -- npm run dev
```

It syncs the same secrets to CI and many hosting platforms, which is where it shines.

**Best for:** teams that want one hosted source of truth synced everywhere.
**The catch:** your secrets live on Doppler's servers, and paid plans are per user.

## Infisical: a secrets platform you can self-host

Infisical works like Doppler, as a cloud service or on your own server, and its code is open source (MIT, except enterprise features). Link a directory, import your `.env`, and run:

```bash
infisical init
infisical sm secrets create --file=./.env
infisical sm run -- npm run dev
```

**Best for:** teams that want a shared platform and the option to host it themselves.
**The catch:** a server to run, or a service to trust, and paid plans are per identity.

## dotsafe: a local vault, no server

dotsafe keeps secrets in an encrypted vault on your Mac or Linux machine, synced through storage you already have, like iCloud Drive, Dropbox or an S3 bucket. There's no dotsafe server holding them, and nothing secret in your repo: the only file it adds is `.dotsafe.json`, which holds ids and is meant to be committed.

```bash
dotsafe import                    # ./.env into this project's development environment
rm .env
dotsafe run -- npm run dev
```

`dotsafe example -o .env.example` writes the variable names for your repo, and AI agents can run commands with your secrets after you approve them, without ever being given a value. See [Your AI coding agent shouldn't read your .env](/blog/ai-agents-and-env-files).

**Best for:** developers working alone who want secrets out of the repo and away from AI agents.
**The catch:** one person per license, no team sharing yet, and no Windows.

## How to choose

- **You only want the right variables per directory:** direnv, paired with one of the others.
- **You want secrets in git, encrypted, for free:** dotenvx for plain `.env` files, SOPS for config files and KMS-backed access.
- **You already pay for 1Password:** put references in your `.env` and use `op run`.
- **Your team needs one hosted source of truth:** Doppler, or Infisical if you might self-host.
- **You work alone and want nothing secret in the repo:** dotsafe.

Whichever you pick, a `.env` that was ever committed is still in your git history. Rotate the keys that were in it; our [key rotation guides](/rotate) cover OpenAI, Anthropic, Stripe, AWS and Cloudflare.

## Sources

Checked on November 13, 2026.

- direnv: [README](https://github.com/direnv/direnv/blob/master/README.md), [stdlib](https://github.com/direnv/direnv/blob/master/man/direnv-stdlib.1.md), [license](https://github.com/direnv/direnv/blob/master/LICENSE)
- dotenvx: [quickstart](https://dotenvx.com/docs/quickstart), [README](https://github.com/dotenvx/dotenvx), [license](https://github.com/dotenvx/dotenvx/blob/main/LICENSE)
- SOPS: [docs](https://getsops.io/docs/), [common operations](https://getsops.io/docs/usage/common-operations/), [passing secrets to other processes](https://getsops.io/docs/usage/advanced/), [age keys](https://getsops.io/docs/usage/identities/age/), [README](https://github.com/getsops/sops), [license](https://github.com/getsops/sops/blob/main/LICENSE)
- 1Password: [secrets in environment variables](https://www.1password.dev/cli/secrets-environment-variables), [Environments](https://www.1password.dev/environments), [local .env files](https://www.1password.dev/environments/local-env-file), [pricing](https://1password.com/pricing/password-manager)
- Doppler: [CLI](https://docs.doppler.com/docs/cli), [`secrets upload` in the CLI source](https://github.com/DopplerHQ/cli/blob/master/pkg/cmd/secrets.go), [pricing](https://www.doppler.com/pricing)
- Infisical: [CLI reference](https://infisical.com/docs/cli/reference-agents), [pricing](https://infisical.com/pricing), [license](https://github.com/Infisical/infisical/blob/main/LICENSE)
- dotsafe: [import and export](/docs/import-export), [quick start](/docs/quick-start)
