# Test pass

A hands-on pass over everything built so far, on the real app and CLI. The automated tests cover the logic. This pass covers what they can't: the native macOS app (WebKit, Touch ID, notifications, the menu bar), real storage servers, real provider accounts, and two machines sharing a vault.

Tick items as you go. When something's wrong, note the item's id (for example `D7`) and what happened.

## Setup

- **Throwaway config:** always run with `DOTSAFE_CONFIG_DIR` set, and keep test vaults out of your real iCloud Drive.

  ```bash
  mkdir -p ~/dotsafe-test/vault
  DOTSAFE_CONFIG_DIR=~/dotsafe-test/config bun run tauri dev
  ```

  For the CLI, use the same config directory: `export DOTSAFE_CONFIG_DIR=~/dotsafe-test/config`, then `cargo run -p dotsafe-cli -- <args>` (or build it once and alias `dotsafe`).
- **A second "device" on one Mac:** a second config directory pointed at the same vault folder acts as another device, with its own device id. Use `~/dotsafe-test/config2`. A real second Mac is better for Touch ID and iCloud sync (round 3).
- **Provider keys:** use throwaway accounts or test-mode keys. Rotation creates and revokes real keys.

Rounds go from no accounts needed to the most setup needed. Rounds 1 and 2 cover most of the app.

## Round 1: the desktop app on one Mac

### Setup and unlock

- [ ] **D1** Create a vault in `~/dotsafe-test/vault`. The recovery key is shown once and can be copied. A weak password is refused.
- [ ] **D2** Quit and relaunch. Unlock with the password. A wrong password says "Wrong password."
- [ ] **D3** Unlock with the recovery key, then set a new password.
- [ ] **D4** Turn on Touch ID unlock in Settings. Relaunch: Touch ID is **not** prompted until you click the button (earlier feedback). Cancelling Touch ID falls back to the password.
- [ ] **D5** Change the password. Generate a new recovery key: the old one stops working.
- [ ] **D6** Auto-lock: leave the app idle past the timeout and it locks. Locking clears what's on screen.

### Projects, environments, variables

- [ ] **D7** Create a project and environments. Delete an environment, then a project (confirmation dialog, solid red button).
- [ ] **D8** Add, edit and delete variables. Reveal a value; copy one, then paste somewhere and check it's cleared from the clipboard after 30 seconds (unless you copied something else meanwhile).
- [ ] **D9** Inheritance: set `staging`'s parent to `development`. Inherited rows are greyed out with a green "From development" badge; editing one gives staging its own value, shown with an amber "Own value" badge, and values stay lined up; hiding one removes it in the child only, and restoring brings it back.
- [ ] **D10** References: `DB_URL = postgres://${{ DB_HOST }}/app`. Override `DB_HOST` in a child: the child's resolved `DB_URL` changes. Editing `DB_URL` shows the `${{ }}` text, not the resolved value. A broken reference shows an error, never the raw text.
- [ ] **D11** Import a `.env` by drag-and-drop and with the file picker. The preview shows names and changes only.
- [ ] **D12** Keyboard: every action has a shortcut or a keyboard path. Shortcuts listed in Settings work, and do nothing while a dialog is open.

### Shared secrets and rotation

- [ ] **D13** Make a variable shared (the Share dialog finds matching copies in other projects). Link it into a second environment.
- [ ] **D14** Rotate by hand: stage → promote → retire the previous version. Roll back. Restore an old version from history.
- [ ] **D15** Leaked fast path: rotate a leaked secret in one step.
- [ ] **D16** Rotation reminders: set a short policy and check that the macOS **notification** appears when the secret is due.

### Menu bar and quick copy

- [ ] **D17** Menu bar icon: Find and copy…, Open dotsafe, Lock and Quit all work.
- [ ] **D18** **⌥⌘K** from another app opens quick copy. Search, copy a value, and the window goes away. When the vault is locked it asks to unlock first.

### Look and feel (WebKit)

- [ ] **D19** Light, dark and system themes; the window chrome follows.
- [ ] **D20** Dialogs open without flicker or a jump from below (earlier feedback, the join-with-code modal). Row actions fade cleanly with no leftover painting.
- [ ] **D21** Turn on Reduce Motion in macOS settings: animations are gone or minimal.
- [ ] **D22** Every list has loading, empty and error states. Copy is short and reads well.

### Backups and moving

- [ ] **D23** Settings → Backups: choose a folder, click Back up now, and check the dated folder appears. Its age shows in Settings.
- [ ] **D24** Move vault to an empty folder. The app keeps working from the new place, and the old copy is untouched. Moving into a folder that already has a vault is refused.
- [ ] **D25** Point a fresh config at a backup snapshot folder and unlock it (this is how a restore works).

### Licensing

- [ ] **D26** Settings → License, against a local API (`cd api && bun run dev`): activate a key, check it, remove it. With the API stopped, the app says offline and keeps working.

## Round 2: CLI and agents

### CLI

- [ ] **C1** `init`, `link` in a project directory, `set` (including `set KEY` to type the value privately), `get`, `list`, `unset`, `inherit`.
- [ ] **C2** `unlock` / `lock`: after `unlock`, commands don't ask for the password; after `lock`, they do.
- [ ] **C3** `run -- <cmd>` gets the resolved variables. `export` prints `.env` lines.
- [ ] **C4** `scan` finds a vault value pasted into a file.
- [ ] **C5** `import --from doppler`, `--from infisical` and `--from 1password --item <item>` (whichever you have installed), plus a Doppler/Infisical/1Password JSON export file.
- [ ] **C6** `secret` subcommands: share, link, rotate, retire, rollback, history, restore, policy, due, queue.
- [ ] **C7** `backup` and `move` (to a folder).
- [ ] **C8** Changes made in the CLI show up in the open app, and the other way round.

### Agents (MCP)

- [ ] **A1** Add `dotsafe mcp` to Claude Code (or Cursor). Ask the agent to list projects and variables: names only, never values.
- [ ] **A2** Ask it to run a command with secrets. The approval dialog shows the program, arguments, directory and secrets. Approving needs Touch ID. Try once, 15 minutes and until lock.
- [ ] **A3** Deny, and check the agent gets a refusal. Commands on the refusal list (for example `env`, `printenv`) are refused without asking.
- [ ] **A4** Output that contains a secret value comes back redacted.
- [ ] **A5** Secret proxy: `run --proxy` with an OpenAI key. The process never sees the real key, and requests still work. Add a custom proxy profile in Settings and use it.
- [ ] **A6** Settings → Agents shows the activity; `dotsafe audit` shows the same, names only. Revoking grants makes the next request ask again.
- [ ] **A7** `dotsafe daemon` with the app quit: approvals are asked in the terminal. With the app running, the daemon refuses to start.

## Round 3: two devices

Use a second config directory, or better a second Mac.

- [ ] **M1** On device 1, Settings → Devices → Add device: a code appears and **can be copied** (earlier feedback). The countdown is clear about what it means (earlier feedback: "what is this timer").
- [ ] **M2** On device 2, join with the code within 10 minutes (earlier feedback: it errored inside the window). Device 2 can unlock with Touch ID straight away.
- [ ] **M3** Edit on both devices. Changes appear on the other side and nothing is lost.
- [ ] **M4** Both devices edit the same variable offline, then reconnect: the newer edit wins, and nothing breaks.
- [ ] **M5** Vault in iCloud Drive (a test folder in iCloud, not the default location) on two real Macs: edit on both, and check iCloud conflict copies of `vault.json` get merged (no "unreadable" warnings).
- [ ] **M6** Remove device 2 from device 1. Device 2 can't read new changes, a new recovery key is shown, and the rotation queue lists what device 2 could read. Rotate from the queue.
- [ ] **M7** The integrity notice: tamper with a record file, or delete one, and the app reports it rather than crashing.

## Round 4: storage servers

- [ ] **S1** Cloudflare R2 (or S3) bucket: create a vault from the app, connect a second device, and sync both ways. Work offline, then sync.
- [ ] **S2** WebDAV with Nextcloud, Fastmail or a NAS (use an app password): create, connect a second device, edit both sides. A wrong password says so clearly.
- [ ] **S3** Move a vault from a folder to a bucket and to WebDAV (`dotsafe move --s3 …`, `--webdav …`).
- [ ] **S4** Storage credentials are in the keychain, not in `config.json`.

## Round 5: provider accounts

With throwaway keys. Each one: connect, set as a secret's provider, rotate, check the new key works, retire or revoke the old one.

- [ ] **P1** OpenAI (admin key).
- [ ] **P2** Cloudflare API token.
- [ ] **P3** Anthropic (guided). Check the admin key header works; the code sends `x-api-key`.
- [ ] **P4** Stripe (guided, revoke by hand in the dashboard).
- [ ] **P5** AWS IAM access key, with the access key id linked secret. Rotate twice (IAM allows two keys, so retire in between).
- [ ] **P6** Push targets: Vercel and Cloudflare Workers. After a rotation, push and check the deployment's variable changed.
- [ ] **P7** Replace a connection's admin token; disconnect a connection.

## Round 6: Linux

- [ ] **L1** Build the CLI on Linux. `init` a vault in a folder or bucket, `run`, `unlock` (Secret Service keychain).
- [ ] **L2** `dotsafe daemon` over SSH with `dotsafe mcp` used by an agent on that machine: approvals in the SSH terminal.

## Not in this pass

Windows, teams, and launch items (signing, notarization, auto-update, distribution, deploying the API) come later.
