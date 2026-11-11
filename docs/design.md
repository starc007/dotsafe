# Design

How dotsafe looks and sounds, across the website and the desktop app. The
app's component rules (shadcn/ui, `Surface`, shortcuts, API wrappers) live in
[AGENTS.md](../AGENTS.md#frontend); this covers the brand and the website, and
the parts both share.

## Logo

The mark is a **split d**: a dot and a stem, kept apart, read together as a
lowercase d. The dot is the secret; the gap keeps it apart from everything else.

| Use | File |
|---|---|
| Source geometry (64 grid: circle `25,39 r14`, stem `x44 y8 8×45 r4`) | `apps/web/src/components/landing/Logo.tsx` |
| Favicon | `apps/web/public/icon.svg` |
| App icon, every size | `apps/desktop/src-tauri/icons/icon.svg` |
| App icon on macOS 26 (Icon Composer) | `apps/desktop/src-tauri/icons/AppIcon.icon` |
| Menu bar (macOS template) | `apps/desktop/src-tauri/icons/tray.svg` |

- **Next to the name**, the mark is exactly as tall as the "d" in "dotsafe":
  its bottom on the baseline, its top on the ascender (`h-[0.73em]`). Don't
  size it to the line box; it will hang below the text.
- **One colour** in running UI: the current text colour. Never lime on white.
- **App icon:** a lime rounded square on macOS's grid (824px body, 100px
  margin, 185px radius) with the mark in `#1a2e05`. Render it with a
  transparent background; Quick Look's renderer fills the corners white. The
  steps are in the comment at the top of `icon.svg`.
- **macOS 26** draws the icon from `AppIcon.icon` (compiled into `Assets.car`
  by `scripts/release.sh`): solid lime fill, the mark as one flat layer, with
  glass, specular and shadow off. Without it, macOS 26 embosses and blurs the
  `.icns`. Keep the two icons in step.
- **Menu bar:** black on transparent at 44px, used as a template image so
  macOS recolours it. Windows and Linux keep the colour app icon.

## Colour

Light only on the website; the app also has a dark theme. Tokens:
`apps/web/src/styles/global.css` (`--ds-*`) and `apps/desktop/src/index.css`.

| Role | Value | Use |
|---|---|---|
| Primary | `#a3e635` acid lime | Fills only: the main button, active states, dots. Text on it is `#1a2e05`. |
| Accent ink | `#3f6212` deep lime | The accent as text or a small icon on white, where lime can't be read. |
| Foreground | `oklch(0.145 0 0)` | Text. |
| Muted | `oklch(0.556 0 0)` | Supporting text, and the second half of two-tone headings. |
| Line | `oklch(0.922 0 0)` | Hairlines, used sparingly. |

- No purple, violet or indigo. No black fills: filled emphasis is the primary.
- Colour comes from tokens, never hardcoded in components. The exceptions are
  the shader scenes and the icon files, which are artwork.

## Type

Inter on both. Weights 400 and 500 only.

- **Two-tone headings:** the claim in the foreground colour, then its
  continuation in muted at the same size, read as one sentence.
  ("Pay once, keep it. <span>One license for everything…</span>")
- Page titles 28px, 36px from `sm`; section titles 22–24px; body 15px; UI 14px.
- Monospace (`ui-monospace`/SF Mono) for versions, commands and values.
- Copy is short and active: "No variables yet", "Wrong password." No hype, no
  em dashes.

## Shader wallpapers

The colour on the website lives in painted frames, never behind headline text.
`Wallpaper.astro` puts a `@paper-design/shaders-react` shader in a rounded frame.
The frame and what sits on it are plain HTML; only the shader (`WallpaperPaint.tsx`)
loads as a React island, as it nears the screen.

| Scene | Look | Used for |
|---|---|---|
| `meadow` | cream, lime, sky (mesh) | Hero product shot, security demo |
| `grove` | deep green swirls (warp) | Feature row, final call to action, the pricing plan |
| `dune` | sand and lime (grain) | Feature row |
| `tide` | sky, lime, teal (mesh) | Feature row, changelog |
| `moss` | lime to near-black stripes (warp) | Feature row |

- Each scene has a CSS gradient of the same colours underneath, so the frame
  never flashes empty before WebGL starts.
- Shaders start once the page has loaded and gone idle, then fade in over the
  gradient, so they never slow the first paint. They only animate on screen,
  and hold still for reduced motion.
- **Text on a shader** gets a dark scrim behind it (see `FinalCta.astro`,
  `PricingPlans.astro`); the paint turns pale in places.
- New scenes stay in the brand's greens, with sky, sand or teal as support.

**In the app**, the screens before the vault opens (activate, setup, unlock,
recovery, new password) sit on a dithered `warp` field
(`apps/desktop/src/components/OnboardingBackdrop.tsx`) in the theme's
`--backdrop-back` and `--backdrop-front` tokens: pale lime on white, deep lime
on near-black. App mounts it once for the whole flow so it doesn't restart
between screens, and removes it when the vault opens.

## Layout

- **Hero:** plain white, left-aligned two-tone headline, two buttons. The
  product sits below it on a wallpaper, like a window on a desktop.
- **Feature rows:** one wide row each, alternating sides. The claim on one
  side, a live demo in a white window on a wallpaper on the other.
- **Cards** are border-only (`rounded-3xl`, `p-2`), never shadows or grey
  fills. Nested corners are concentric: inner radius = outer − padding
  (`rounded-3xl p-2` holds `rounded-2xl`).
- **Few lines.** Separate things with space first. Use a hairline only where
  rows need it, like a table or a list of questions.
- Content width `max-w-6xl` with a 16px side gutter (24px from `sm`). Every
  page works at 375px with no sideways scroll.
- Buttons are pills. Icon-only buttons are round.

## Motion

- **Ease-out, short, nothing bouncy.** Website: `cubic-bezier(0.16, 1, 0.3, 1)`
  (`--ease-ds` in `apps/web/src/styles/global.css`, `EASE` in
  `apps/web/src/lib/motion.ts` for islands). App: `[0.22, 1, 0.36, 1]` at 150ms and 220ms
  (`apps/desktop/src/lib/motion.ts`).
- Content fades up 16px once as it scrolls in (`Reveal.astro`). The hero's
  badge, buttons and product fade up as the page loads; its headline doesn't,
  since it's the largest text on the page and a fade would delay when the page
  counts as shown. Both are CSS, with no JavaScript framework involved;
  `motion` runs only inside the animated demo islands.
- Closing mirrors opening. The FAQ grows its row from `0fr` to `1fr` while the
  answer fades in, and runs the same steps back.
- Everything respects reduced motion: fades without movement in the CSS,
  `MotionConfig reducedMotion="user"` in each animated island (`MotionRoot`),
  `motion-reduce:` on CSS transitions, still shaders.
- The app runs in WebKit. Check fades and layered animation there, not only in
  Chromium (see AGENTS.md).

## Icons

lucide, outline. No sparkles, no emoji in UI. Brand marks for storage services
come from Simple Icons, in one grey.
