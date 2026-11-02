import { ImageResponse } from 'next/og';

export const size = { width: 180, height: 180 };
export const contentType = 'image/png';

/** The home screen icon: the logo mark on lime, square (iOS rounds the corners). */
export default function AppleIcon() {
  return new ImageResponse(
    (
      <div style={{ display: 'flex', width: '100%', height: '100%', alignItems: 'center', justifyContent: 'center', background: '#a3e635' }}>
        <svg viewBox="11 8 41 45" width={88} height={97} fill="#1a2e05">
          <circle cx="25" cy="39" r="14" />
          <rect x="44" y="8" width="8" height="45" rx="4" />
        </svg>
      </div>
    ),
    size,
  );
}
