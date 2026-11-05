import { links } from '@/lib/site';

/** Every release uploads unversioned copies of the installers (scripts/release.sh
 * for macOS, .github/workflows/release-linux.yml for Linux), so these never change. */
const latest = `${links.releases}/latest/download`;

export type Platform = {
  id: 'macos' | 'linux';
  name: string;
  /** Its mark in public/brands. */
  logo: string;
  detail: string;
  downloads: { label: string; href: string }[];
  notes: string[];
  /** The standalone CLI's file name in each release. */
  cli: string;
};

/** The download page and its Markdown copy for agents. */
export const PLATFORMS: Platform[] = [
  {
    id: 'macos',
    name: 'macOS',
    logo: 'apple',
    detail: 'Apple silicon and Intel, macOS 13 or later',
    downloads: [{ label: 'Download .dmg', href: links.download }],
    notes: [
      'One universal build, signed and notarized',
      'The CLI is inside: Settings → Agents → Install',
      'Updates itself when you click',
    ],
    cli: 'dotsafe-<version>-macos-universal.tar.gz',
  },
  {
    id: 'linux',
    name: 'Linux',
    logo: 'linux',
    detail: 'x86_64, Debian, Ubuntu and other distributions',
    downloads: [
      { label: 'Download .deb', href: `${latest}/dotsafe.deb` },
      { label: 'AppImage', href: `${latest}/dotsafe.AppImage` },
    ],
    notes: [
      'The .deb installs the app and the CLI',
      'The AppImage runs on any distribution',
      'Both update themselves when you click',
      'Needs a Secret Service keyring, such as GNOME Keyring or KWallet',
    ],
    cli: 'dotsafe-<version>-linux-x86_64.tar.gz',
  },
];

/** Release assets, for the page's analytics: any link into a release download. */
export const isDownloadLink = (href: string) => href.startsWith(`${links.releases}/latest/download/`);
