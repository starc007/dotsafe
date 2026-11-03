/** Links used across the landing page. */
export const links = {
  download: 'https://github.com/starc007/dotsafe/releases/latest',
  github: 'https://github.com/starc007/dotsafe',
  releases: 'https://github.com/starc007/dotsafe/releases',
  issues: 'https://github.com/starc007/dotsafe/issues',
  docs: '/docs',
};

/** The licensing API (apps/api). Set NEXT_PUBLIC_API_URL to try checkout against
 * a local `wrangler dev` (http://localhost:8787). */
export const apiUrl = process.env.NEXT_PUBLIC_API_URL ?? 'https://api.dotsafe.dev';

/** Tracwell analytics for the site (components/TracwellAnalytics.tsx). The
 * project key is public. It's off on localhost. */
export const tracwell = {
  projectKey: 'tw_live_2d31439b54f94658b0b1823716b97c65',
};

/** The site itself, for metadata, the sitemap and structured data. */
export const site = {
  url: 'https://dotsafe.dev',
  name: 'dotsafe',
  title: 'dotsafe: the local-first secret manager for developers',
  description:
    'Keep .env secrets encrypted on your Mac and sync them through iCloud, Dropbox, S3 or any storage you own. A CLI, Touch ID unlock and safe access for AI agents.',
  price: '49',
};
