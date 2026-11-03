# Import and export

> Bring variables in from .env files and other secret managers, and keep them out of git.

## Import from a file

```bash
dotsafe import                        # ./.env into the linked environment
dotsafe import .env.production -e production
dotsafe import secrets.json           # JSON exported by Doppler, Infisical or 1Password
```

In the app, drop `.env` files on the window, or use Import. You see what will change before anything is saved.

## Import from another secret manager

dotsafe can read straight from another tool's CLI, if it's installed and signed in:

```bash
dotsafe import --from doppler             # the project and config set up in this directory
dotsafe import --from infisical           # the project linked in this directory
dotsafe import --from 1password --item "my-api prod"   # one item's fields, by label
```

## Export

Prefer `dotsafe run`, which never writes values to disk. For tools that only read a file:

```bash
dotsafe export                 # print .env lines
dotsafe export -o .env         # write a file
```

A written file is readable by you only. dotsafe refuses to write it if git would commit it; add it to `.gitignore` first, or pass `--force`.

## Keep values out of git

`dotsafe scan` looks through files for any value in your vault, as is or encoded (base64, hex, URL encoding). It reports names and places, never the values.

```bash
dotsafe scan                   # this directory
dotsafe scan src/ config/      # some paths
dotsafe scan --install-hook    # a pre-commit hook that runs `dotsafe scan --staged`
```
