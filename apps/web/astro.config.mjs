import { defineConfig } from 'astro/config';
import react from '@astrojs/react';
import tailwindcss from '@tailwindcss/vite';
import { linkHeaders } from './scripts/link-headers.mjs';

// Every page is static: the site builds to plain files in `dist/`, served by a
// Cloudflare Worker as static assets (wrangler.jsonc). No server runs.
export default defineConfig({
  site: 'https://dotsafe.dev',
  // `/docs/install` is written as `docs/install.html`, which Cloudflare serves
  // at the address without a trailing slash.
  build: { format: 'file' },
  trailingSlash: 'never',
  // Plain links between pages: no prefetching, no client router.
  prefetch: false,
  integrations: [react(), linkHeaders()],
  vite: {
    plugins: [tailwindcss()],
    // The share images' renderer is a native module, run only at build time.
    ssr: { external: ['@resvg/resvg-js'] },
    optimizeDeps: { exclude: ['@resvg/resvg-js'] },
  },
});
