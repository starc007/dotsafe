/** @type {import('next').NextConfig} */
const config = {
  reactStrictMode: true,
  // Every page is static, so the site builds to plain files in `out/`, served
  // by a Cloudflare Worker as static assets (wrangler.jsonc). No server runs.
  output: 'export',
  // No image server in a static export; the screenshots ship as they are.
  images: { unoptimized: true },
};

export default config;
