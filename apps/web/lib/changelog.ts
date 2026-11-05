import type { Scene } from '@/components/landing/Wallpaper';

/** What changed in each release, newest first. Add an entry with every
 * release (see docs/releasing.md); leave `date` out until it ships. */
export type Release = {
  version: string;
  /** ISO date the release went out. */
  date?: string;
  title: string;
  summary?: string;
  /** A screenshot from /public, shown on a painted wallpaper. */
  image?: { src: string; alt: string; width: number; height: number };
  scene?: Scene;
  changes: string[];
};

export const RELEASES: Release[] = [
  {
    version: '0.1.3',
    date: '2026-11-05',
    title: 'Cron jobs on servers, and arm64 Linux',
    summary: 'Nightly jobs on a server can use your secrets without the master password or a token stored on it.',
    changes: [
      '`dotsafe daemon --detach` unlocks once after a reboot and keeps the vault key in memory only.',
      'Cron jobs run with `dotsafe run -- ./job.sh`. Each new job, key or changed script is approved once with `dotsafe approve`, which asks for the master password.',
      '`dotsafe run --only A,B` passes just the keys a command needs.',
      'Linux builds for arm64: the app, the .deb and the CLI.',
    ],
  },
  {
    version: '0.1.2',
    date: '2026-11-05',
    title: 'dotsafe on Linux',
    summary: 'The app and the CLI now run on Linux too, with the same vault and the same license.',
    changes: [
      'A Linux app for x86_64: a .deb that installs the app and the CLI, and an AppImage for any distribution. Both update themselves.',
      'A standalone Linux CLI download.',
      'A busy license server now counts as offline, so it never shows as an error.',
    ],
  },
  {
    version: '0.1.1',
    date: '2026-11-03',
    title: 'A sharper icon',
    changes: ['The app icon is crisp on macOS 26, instead of embossed and blurred.'],
  },
  {
    version: '0.1.0',
    date: '2026-11-03',
    title: 'The first release',
    summary: 'A desktop app and a CLI that share one encrypted vault, synced through storage you already own.',
    image: {
      src: '/app.png',
      alt: "The dotsafe app: a project's variables, with a secret linked across environments",
      width: 2080,
      height: 1120,
    },
    scene: 'tide',
    changes: [
      'Projects with environments, variables and hidden values, encrypted on your Mac.',
      'Vaults in iCloud Drive, Google Drive, Dropbox, an S3 or R2 bucket, or WebDAV.',
      'Secrets linked across environments, with rotation through OpenAI, Cloudflare and AWS.',
      'Agent access approved with Touch ID, with values taken out of what agents read back.',
      'Imports from .env files, Doppler, Infisical and 1Password.',
      'Touch ID unlock, a recovery key, and quick copy from anywhere with ⌥⌘K.',
    ],
  },
];
