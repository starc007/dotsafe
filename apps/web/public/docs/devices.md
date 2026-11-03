# Devices

> Touch ID unlock, adding a Mac with a code, and removing a lost device.

Every Mac that opens the vault can have its own key, kept in its keychain. That's what lets you unlock with Touch ID, and cut off one Mac without changing anything on the others.

## Touch ID unlock

Turn it on in Settings → Devices. The vault then unlocks with Touch ID, and asks for the master password every two weeks so you don't forget it.

## Adding a Mac

On a Mac that's already set up, create a one-time code in Settings → Devices. Enter it on the new Mac instead of typing the master password there. The new Mac needs access to the same storage.

## Removing a device

If a Mac is lost, sold or compromised, remove it from any other device:

```bash
dotsafe device list
dotsafe device remove "Old MacBook"
```

dotsafe switches the vault to a new key and re-encrypts everything, so the removed Mac can't read anything written after that. It prints a new recovery key; keep it safe.

> [!WARNING]
> A removed Mac may still have copies of values it read before. So every secret it could read goes into a rotation queue, shown in the sidebar until each one is rotated.

Secrets with a [provider](/docs/integrations) rotate in one step:

```bash
dotsafe secret queue --rotate
```
