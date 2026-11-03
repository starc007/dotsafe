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
