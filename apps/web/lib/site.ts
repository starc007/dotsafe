/** Links used across the landing page. */
export const links = {
  // The .dmg itself, so the button downloads instead of opening GitHub. Every
  // release uploads an unversioned copy (scripts/release.sh), so this never changes.
  download: 'https://github.com/starc007/dotsafe/releases/latest/download/dotsafe.dmg',
  github: 'https://github.com/starc007/dotsafe',
  releases: 'https://github.com/starc007/dotsafe/releases',
  issues: 'https://github.com/starc007/dotsafe/issues',
  docs: '/docs',
};

/** The walkthrough on the home page (components/landing/DemoVideo.tsx), served
 * from public/demo. The only sound is background music (see its README), so
 * there are no captions. */
export const demo = {
  mp4: '/demo/dotsafe-demo.mp4',
  webm: '/demo/dotsafe-demo.webm',
  poster: '/demo/dotsafe-demo-poster.jpg',
  width: 1856,
  height: 1080,
  length: '1:08',
  duration: 'PT1M8S',
  uploaded: '2026-11-05',
};

/** The licensing API (apps/api). Set NEXT_PUBLIC_API_URL to try checkout against
 * a local `wrangler dev` (http://localhost:8787). */
export const apiUrl = process.env.NEXT_PUBLIC_API_URL ?? 'https://api.dotsafe.dev';

/** Tracwell analytics for the site (components/TracwellAnalytics.tsx). The
 * project key is public. It only runs in production builds. */
export const tracwell = {
  projectKey: 'tw_live_2d31439b54f94658b0b1823716b97c65',
};

/** The site itself, for metadata, the sitemap and structured data. */
export const site = {
  url: 'https://dotsafe.dev',
  name: 'dotsafe',
  title: 'dotsafe: the open source secret manager for developers and AI agents',
  description:
    'A local-first, open source secret manager for developers and AI agents. Keep .env secrets encrypted on your Mac and synced through iCloud, Dropbox, S3 or storage you own.',
  price: '49',
};
