# CLI

> Every dotsafe command and its main options.

Every command takes `--help`. Commands that work on variables take `-e <env>` for an environment other than the linked one.

## Vault

| Command | What it does |
| --- | --- |
| `dotsafe init [--path \| --bucket \| --webdav]` | Create a vault, or connect this Mac to an existing one |
| `dotsafe unlock [--for 8h]` | Keep the vault key in the keychain for a while |
| `dotsafe lock` | Forget the key cached by unlock |
| `dotsafe sync` | Exchange changes with the bucket now |
| `dotsafe backup [--to <dir>]` | Save a dated, still-encrypted copy (the last 10 are kept) |
| `dotsafe move [--path \| --bucket \| --webdav]` | Move the vault; the old copy stays until you delete it |

## Variables

| Command | What it does |
| --- | --- |
| `dotsafe link [name] [-e env]` | Link this directory to a project, creating it if needed |
| `dotsafe set KEY[=VALUE]…` | Set variables; a bare KEY asks for the value |
| `dotsafe get KEY` | Print a variable's value |
| `dotsafe list` | List variable names |
| `dotsafe unset KEY… [--hide]` | Delete variables, or hide inherited ones |
| `dotsafe inherit <parent> \| --none` | Inherit another environment's variables |
| `dotsafe run [--proxy] -- <command>` | Run a command with the variables in its environment |
| `dotsafe import [file] [--from <tool>]` | Import a .env or JSON file, or from Doppler, Infisical, 1Password |
| `dotsafe export [-o file] [--force]` | Print .env lines, or write a gitignored, owner-only file |
| `dotsafe scan [paths] [--staged \| --install-hook]` | Find vault values in files before they're committed |

## Secrets

| Command | What it does |
| --- | --- |
| `dotsafe secret list` | Secrets with their age and where they're used |
| `dotsafe secret share KEY [--all]` | Turn a variable into a secret other environments can link to |
| `dotsafe secret link KEY SECRET` | Link a variable to an existing secret |
| `dotsafe secret unlink KEY` | Give a linked variable its own copy again |
| `dotsafe secret rotate SECRET [--generate \| --leaked \| --stdin]` | New value everywhere it's linked, through its provider if it has one |
| `dotsafe secret retire SECRET [--by-hand]` | Revoke the previous value |
| `dotsafe secret rollback SECRET` | Go back to the previous value |
| `dotsafe secret history SECRET` | Versions and notes, never values |
| `dotsafe secret restore SECRET VERSION` | Make an earlier version current again |
| `dotsafe secret policy SECRET --every <days>` | How often it should be rotated |
| `dotsafe secret due` | Secrets due for rotation |
| `dotsafe secret queue [--rotate \| --dismiss]` | What to rotate after removing a device |
| `dotsafe secret provider SECRET --connection <name>` | Where new values come from |
| `dotsafe secret target add SECRET --connection <name>` | Where each new value is pushed |
| `dotsafe secret push SECRET` | Push the current value to every target now |

## Integrations

| Command | What it does |
| --- | --- |
| `dotsafe connect <service> [--set key=value]` | Connect a provider or deploy target |
| `dotsafe connections` | List connected accounts |
| `dotsafe reconnect <name>` | Give a connection a new admin token |
| `dotsafe disconnect <name>` | Remove a connection and its token |

## Agents

| Command | What it does |
| --- | --- |
| `dotsafe mcp` | The MCP server agents talk to |
| `dotsafe mcp setup` | How to add dotsafe to Claude Code, Cursor and others |
| `dotsafe daemon [--for 8h]` | Serve agents without the app; approve in the terminal |
| `dotsafe proxy list \| add \| remove` | Proxy profiles for more API keys |
| `dotsafe audit [--limit 30]` | What agents asked for, from every device |

## Devices and license

| Command | What it does |
| --- | --- |
| `dotsafe device list` | Devices that can open this vault |
| `dotsafe device remove <name>` | Remove one and switch the vault to a new key |
| `dotsafe license activate <key>` | Activate a license on this Mac |
| `dotsafe license status` | Show the license on this Mac |
| `dotsafe license remove` | Free this Mac's activation |

## Environment variables

| Variable | What it does |
| --- | --- |
| `DOTSAFE_PASSWORD` | The master password, for CI |
| `DOTSAFE_S3_ACCESS_KEY_ID`, `DOTSAFE_S3_SECRET_ACCESS_KEY` | Bucket credentials, instead of the keychain |
| `DOTSAFE_WEBDAV_PASSWORD` | The WebDAV password, instead of asking |
| `DOTSAFE_CONFIG_DIR` | Use another config folder, for testing |

> [!WARNING]
> Anything that can read the environment can read `DOTSAFE_PASSWORD`. On your own Mac, prefer `dotsafe unlock`.
