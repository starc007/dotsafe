# Install

> Download the app and the CLI, and activate your license.

dotsafe runs on macOS (Apple silicon and Intel) and on Linux (x86_64). Every release is also on [GitHub](https://github.com/starc007/dotsafe/releases/latest).

## Desktop app

1. [Download dotsafe.dmg](https://github.com/starc007/dotsafe/releases/latest/download/dotsafe.dmg). It's a universal build, signed and notarized.
2. Open it and drag dotsafe into Applications.
3. Open dotsafe. The first screen asks for your license key.

The app checks for updates on its own and installs one only when you click.

## CLI

The CLI comes inside the app. In dotsafe, open **Settings → Agents** and click **Install** under Command-line tool. It links `dotsafe` into `/usr/local/bin` (macOS asks for your password once) and updates with the app.

```bash
dotsafe --version
```

Want the CLI without the app? Download `dotsafe-<version>-macos-universal.tar.gz` from the same release. It's signed and notarized. Unpack it and put `dotsafe` on your `PATH`:

```bash
tar -xzf dotsafe-*-macos-universal.tar.gz
sudo mv dotsafe /usr/local/bin/
```

## Linux

The Linux app and CLI are built for x86_64. On Debian or Ubuntu, the .deb is the easiest: it installs the app and the CLI together.

```bash
curl -LO https://github.com/starc007/dotsafe/releases/latest/download/dotsafe.deb
sudo apt install ./dotsafe.deb
```

On other distributions, use the AppImage. It runs as is and updates itself.

```bash
curl -LO https://github.com/starc007/dotsafe/releases/latest/download/dotsafe.AppImage
chmod +x dotsafe.AppImage
./dotsafe.AppImage
```

The AppImage doesn't include the CLI. Download `dotsafe-<version>-linux-x86_64.tar.gz` from the [latest release](https://github.com/starc007/dotsafe/releases/latest) and put `dotsafe` on your `PATH`:

```bash
tar -xzf dotsafe-*-linux-x86_64.tar.gz
sudo mv dotsafe /usr/local/bin/
```

What's different on Linux:

- Storage keys and the unlock cache live in your keyring through the Secret Service, so you need GNOME Keyring, KWallet or another Secret Service provider running.
- There's no Touch ID. Agent requests are approved with a click in the app, and devices join with the master password instead of a one-time code.
- There's no iCloud Drive. Pick any folder a sync client keeps in step (Dropbox, Google Drive, Syncthing), or use a bucket or WebDAV.

## Activate your license

dotsafe is paid, with no free version: one license is $49 and covers up to 2 computers, Mac or Linux. See [pricing](/pricing). Activate it once, in the app or the CLI. Both read the same license, so either is enough.

```bash
dotsafe license activate <key>
dotsafe license status
dotsafe license remove     # free this computer's activation to use the key elsewhere
```

> [!NOTE]
> Being offline never locks you out. The license is rechecked about twice a day, and after 30 days without a check it only shows as unverified.
