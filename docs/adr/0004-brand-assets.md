# 0004. Brand assets and self-hosted fonts

Status: accepted, 2026-11-05. Builds on [0003. Visual design](0003-design.md).

Radar needs a mark, browser and home-screen icons, its two typefaces, and a preview image for the public GitHub repository. PRD rule N4 forbids third-party scripts and asks for a strict Content Security Policy. So every asset is served from Radar's own origin.

## Decision

### The mark

Two radar rings around a centre dot, with one coral blip on the outer ring. The blip is the item that is waiting on you, and it uses the `hot` colour, the same colour an item turns when it has waited longest. The rings use `muted` and `faint`, and the tile behind them uses `bg`.

- [`design/brand/mark.svg`](../../design/brand/mark.svg) is the master drawing, with no background.
- [`design/brand/favicon.svg`](../../design/brand/favicon.svg) is a separate drawing for 16 to 48 px. It drops the inner ring and uses thicker strokes, because the full mark blurs at that size.

### Icons and manifest

[`scripts/build-assets.mjs`](../../scripts/build-assets.mjs) renders every raster file with headless Chrome or Edge. It needs no npm packages. Run it after changing anything in `design/brand/`.

| File | Size | Use |
| --- | --- | --- |
| `public/favicon.ico` | 16, 32, 48 | Browsers that do not read SVG icons |
| `public/icon.svg` | Vector | Browser tab icon |
| `public/apple-touch-icon.png` | 180 | iOS home screen. Full bleed, because iOS rounds the corners itself |
| `public/icon-192.png`, `public/icon-512.png` | 192, 512 | Web app manifest, rounded tile |
| `public/icon-maskable-512.png` | 512 | Android adaptive icon. The mark sits inside the safe zone |
| `public/manifest.webmanifest` | | Lets the phone add Radar to the home screen (F12) |
| `design/brand/social-preview.png` | 1280 × 640 | The GitHub repository's social preview |

GitHub has no API for the social preview. Upload it by hand in the repository's **Settings**, under **Social preview**.

### Fonts

Schibsted Grotesk and Funnel Display are served from `public/fonts/` as variable WOFF2 files, Latin subset only. Each file covers every weight we use. The files come from the Fontsource packages `@fontsource-variable/schibsted-grotesk` and `@fontsource-variable/funnel-display`, version 5.3.0. Both fonts use the SIL Open Font License 1.1, and the license texts sit next to the files. [`public/fonts/fonts.css`](../../public/fonts/fonts.css) declares both faces.

### UI icons

We have none for now. The design shows each reason as a word, not an icon. If an icon is needed later, take it from Lucide (ISC license) and store it as SVG in the repository.

## Alternatives

| Option | Why not |
| --- | --- |
| Load fonts from Google Fonts | Adds a third-party origin to the CSP. Tells Google each time the page loads. Slower first load |
| Fonts as npm dependencies | Also self-hosted, but the asset build needs the files before any `package.json` exists. Switching in M0 is fine if the files stay the same |
| A rotating radar sweep as the mark | The obvious radar image, and it does not survive 16 px |
| `sharp` or `resvg` to rasterize | Adds dependencies, and resvg's support for variable fonts is uncertain. The browser renders the same way the app will |

## Consequences

The app's `index.html` head needs:

```html
<link rel="icon" href="/favicon.ico" sizes="32x32">
<link rel="icon" href="/icon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<link rel="manifest" href="/manifest.webmanifest">
<meta name="theme-color" content="#141519">
<link rel="preload" href="/fonts/schibsted-grotesk.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="/fonts/fonts.css">
```

The CSP can then keep `font-src 'self'` and `img-src 'self'`, plus GitHub avatar hosts if avatars are added.
