# Quick start

> Create a vault, import a .env file and run your app.

This takes a project with a `.env` file and moves it into a vault. You can do the same in the app; here it's the CLI.

## 1. Create a vault

```bash
dotsafe init
```

It asks for a master password and puts the vault in `iCloud Drive/dotsafe`. Pass `--path` for another synced folder, or see [Storage and sync](/docs/storage) for buckets and WebDAV. If a vault is already there, `init` connects this Mac to it instead.

> [!WARNING]
> Write down the recovery key it prints. It's the only way back in if you forget the password.

## 2. Import your .env

```bash
cd ~/code/my-app
dotsafe import             # reads ./.env into the development environment
dotsafe list               # names only
```

The first write in a directory creates a project named after it, and a `.dotsafe.json` file that links the two. It only holds ids, so commit it: other Macs on the same vault pick the project up.

## 3. Run your app

```bash
dotsafe run -- bun dev
```

The command gets every variable in its environment. Now delete the `.env` file: it isn't needed anymore.

## 4. Add and change variables

```bash
dotsafe set API_KEY                # asks for the value, so it stays out of shell history
dotsafe set -e production A=1 B=2  # another environment
dotsafe get API_KEY
dotsafe unset API_KEY
```

## 5. Skip the password prompt

```bash
dotsafe unlock --for 8h    # keep the key in the macOS keychain for a while
dotsafe lock               # forget it now
```

## Next

- [Projects and environments](/docs/environments): staging, production and inheritance.
- [Secrets and rotation](/docs/secrets): one key used by many projects.
- [AI agents](/docs/agents): let your coding agent run with secrets it never sees.
