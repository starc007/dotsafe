import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { ImageResponse } from 'next/og';

export const alt = 'dotsafe: secrets that never leave your Mac';
export const size = { width: 1200, height: 630 };
export const contentType = 'image/png';
// Rendered once at build time: the site is a static export.
export const dynamic = 'force-static';

/** Inter's font files. The package is in this app's node_modules or the
 * workspace root's, depending on how bun linked the install, so look upward
 * from here like Node's resolver does. */
function fontsDir() {
  for (let dir = process.cwd(); ; dir = dirname(dir)) {
    const candidate = join(dir, 'node_modules/@fontsource/inter/files');
    if (existsSync(candidate)) return candidate;
    if (dirname(dir) === dir) throw new Error('@fontsource/inter is not installed; run bun install');
  }
}

const fonts = fontsDir();

/** The link preview: the hero in one frame. The name and claim on white,
 * the app on a painted frame running off the right edge. The OG renderer has
 * no WebGL, so the meadow scene is layered gradients in the same colours. */
export default async function Image() {
  const [regular, medium, shot] = await Promise.all([
    readFile(join(fonts, 'inter-latin-400-normal.woff')),
    readFile(join(fonts, 'inter-latin-500-normal.woff')),
    readFile(join(process.cwd(), 'public/app.png')),
  ]);
  const screenshot = `data:image/png;base64,${shot.toString('base64')}`;

  return new ImageResponse(
    (
      <div style={{ display: 'flex', width: '100%', height: '100%', background: '#fff', fontFamily: 'Inter' }}>
        <div style={{ display: 'flex', flexDirection: 'column', justifyContent: 'space-between', padding: '64px 0 60px 72px', width: 560 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, fontSize: 34, fontWeight: 500, letterSpacing: '-0.02em', color: '#171717' }}>
            <svg viewBox="11 8 41 45" width={23} height={25} fill="#171717">
              <circle cx="25" cy="39" r="14" />
              <rect x="44" y="8" width="8" height="45" rx="4" />
            </svg>
            dotsafe
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 36 }}>
            <div style={{ display: 'flex', flexDirection: 'column', maxWidth: 400, fontSize: 38, fontWeight: 500, lineHeight: 1.15, letterSpacing: '-0.025em' }}>
              <span style={{ color: '#171717' }}>Secrets that never leave your Mac.</span>
              <span style={{ color: '#8a8a8a', marginTop: 6 }}>Synced through storage you own.</span>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 14, fontSize: 22, color: '#8a8a8a' }}>
              <span style={{ display: 'flex', width: 10, height: 10, borderRadius: 5, background: '#a3e635' }} />
              dotsafe.dev
            </div>
          </div>
        </div>
        <div
          style={{
            display: 'flex',
            position: 'absolute',
            left: 664,
            top: 64,
            width: 620,
            height: 640,
            borderRadius: 28,
            overflow: 'hidden',
            backgroundColor: '#bef264',
            backgroundImage: [
              'radial-gradient(circle at 10% 8%, #f7f3e3 0%, rgba(247,243,227,0) 40%)',
              'radial-gradient(circle at 90% 12%, #7dd3fc 0%, rgba(125,211,252,0) 45%)',
              'radial-gradient(circle at 85% 92%, #4d7c0f 0%, rgba(77,124,15,0) 55%)',
              'radial-gradient(circle at 5% 85%, #84cc16 0%, rgba(132,204,22,0) 50%)',
              'radial-gradient(circle at 45% 50%, #d9f99d 0%, rgba(217,249,157,0) 60%)',
            ].join(', '),
          }}
        >
          <div
            style={{
              display: 'flex',
              position: 'absolute',
              left: 48,
              top: 56,
              padding: 8,
              borderRadius: 18,
              background: 'rgba(255,255,255,0.45)',
            }}
          >
            <img src={screenshot} width={900} height={485} style={{ borderRadius: 12 }} alt="" />
          </div>
        </div>
      </div>
    ),
    {
      ...size,
      fonts: [
        { name: 'Inter', data: regular, weight: 400, style: 'normal' },
        { name: 'Inter', data: medium, weight: 500, style: 'normal' },
      ],
    },
  );
}
