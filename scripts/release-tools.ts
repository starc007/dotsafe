// The parts of scripts/release.sh that are easier in TypeScript than in shell.
//
//   bun scripts/release-tools.ts check-update-key <archive.sig> <tauri.conf.json>
//   bun scripts/release-tools.ts latest-json <out> <version> <url> <archive.sig> [notes.md]
//   bun scripts/release-tools.ts add-platform <latest.json> <platform> <url> <archive.sig>

const [command, ...args] = process.argv.slice(2);

/** A minisign key id: base64 text whose second line is base64 bytes, with
 * the 8-byte key id after a 2-byte algorithm tag. Signatures and public keys
 * both have one, so they can be matched without verifying anything. */
function keyId(b64: string): string {
  const line = Buffer.from(b64.trim(), "base64").toString().split("\n")[1];
  return Buffer.from(line, "base64").subarray(2, 10).toString("hex");
}

switch (command) {
  // Installed apps only take updates signed by the key in tauri.conf.json.
  case "check-update-key": {
    const [sigFile, confFile] = args;
    const signed = keyId(await Bun.file(sigFile).text());
    const expected = keyId(JSON.parse(await Bun.file(confFile).text()).plugins.updater.pubkey);
    if (signed !== expected) {
      console.error(`release: the update is signed with key ${signed}, but installs expect ${expected}`);
      process.exit(1);
    }
    break;
  }

  // What the app's updater reads from the newest release. The universal build
  // serves both Apple silicon and Intel Macs.
  case "latest-json": {
    const [out, version, url, sigFile, notesFile] = args;
    const platform = { signature: (await Bun.file(sigFile).text()).trim(), url };
    const latest = {
      version,
      notes: notesFile ? (await Bun.file(notesFile).text()).trimEnd() : "",
      pub_date: new Date().toISOString(),
      platforms: { "darwin-aarch64": platform, "darwin-x86_64": platform },
    };
    await Bun.write(out, JSON.stringify(latest, null, 2) + "\n");
    break;
  }

  // The Linux build runs on GitHub Actions after the Mac release is published,
  // and adds itself to that release's latest.json.
  case "add-platform": {
    const [file, platform, url, sigFile] = args;
    const latest = JSON.parse(await Bun.file(file).text());
    latest.platforms[platform] = { signature: (await Bun.file(sigFile).text()).trim(), url };
    await Bun.write(file, JSON.stringify(latest, null, 2) + "\n");
    break;
  }

  default:
    console.error("usage: bun scripts/release-tools.ts check-update-key|latest-json|add-platform …");
    process.exit(2);
}
