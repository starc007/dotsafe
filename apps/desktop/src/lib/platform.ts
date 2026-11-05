// dotsafe runs on macOS and Linux. Copy names the computer the way its owner
// would, and Mac-only features (Touch ID, iCloud Drive) stay out of sight elsewhere.

export const isMac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** "this Mac", or "this computer" on Linux. */
export const thisDevice = isMac ? "this Mac" : "this computer";

/** Where storage keys are kept, as a phrase: "your Mac's keychain". */
export const keychain = isMac ? "your Mac's keychain" : "your system keyring";
