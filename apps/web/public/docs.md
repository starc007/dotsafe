# Introduction

> What dotsafe is and how it keeps your secrets.

dotsafe is an open source, local-first secret manager for developers and their AI agents. It keeps your environment variables and API keys encrypted on your Mac and syncs them through storage you already own: iCloud Drive, Google Drive, Dropbox, an S3 or R2 bucket, or WebDAV. There's no dotsafe server holding your vault.

It comes as a desktop app and a `dotsafe` command-line tool. Both open the same vault, so a change in one shows up in the other.

## How it works

- **Encrypted on your device.** Your master password never leaves your Mac. The vault is a folder of encrypted files, and whatever syncs it only ever sees ciphertext.
- **Synced by storage you own.** Put the vault in a synced folder or a bucket, and every Mac that opens it stays in step. Each one keeps a copy, so dotsafe works offline.
- **Injected, not written down.** `dotsafe run` starts your app with its variables set, so there's no `.env` file to leak or commit.
- **Safe for agents.** Claude Code, Cursor and other MCP agents can run commands with your secrets after you approve, and dotsafe never hands them a value.

## Where to start

1. [Install](/docs/install) the app and the CLI.
2. Follow the [quick start](/docs/quick-start) to move a project's `.env` into a vault.
3. Read [Security](/docs/security) for what dotsafe protects against, and what it doesn't.

> [!TIP]
> Every docs page is also plain Markdown: add `.md` to its URL. [llms.txt](/llms.txt) lists them all for AI tools.
