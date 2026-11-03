# Install

> Download the app and the CLI, and activate your license.

dotsafe runs on macOS, on Apple silicon and Intel. Both downloads are on the [latest release](https://github.com/starc007/dotsafe/releases/latest).

## Desktop app

1. Download `dotsafe-<version>.dmg`. It's a universal build, signed and notarized.
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

## Activate your license

dotsafe is paid, with no free version: one license is $49 and covers up to 2 Macs. See [pricing](/pricing). Activate it once, in the app or the CLI. Both read the same license, so either is enough.

```bash
dotsafe license activate <key>
dotsafe license status
dotsafe license remove     # free this Mac's activation to use the key elsewhere
```

> [!NOTE]
> Being offline never locks you out. The license is rechecked about twice a day, and after 30 days without a check it only shows as unverified.
