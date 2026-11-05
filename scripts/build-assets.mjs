#!/usr/bin/env node
// Renders the app icons, favicon, web manifest and GitHub social preview from design/brand.
// Needs Chrome or Edge. Set CHROME_PATH if it is not in a standard location.
// Usage: node scripts/build-assets.mjs
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const brand = join(root, 'design/brand');
const pub = join(root, 'public');
const tmp = mkdtempSync(join(tmpdir(), 'radar-assets-'));

const tokens = JSON.parse(readFileSync(join(root, 'design/tokens.json'), 'utf8'));
const bg = tokens.color.bg.$value;

const browser = [
  process.env.CHROME_PATH,
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
].find((p) => p && existsSync(p));
if (!browser) {
  console.error('No Chrome or Edge found. Set CHROME_PATH.');
  process.exit(1);
}

const markInner = readFileSync(join(brand, 'mark.svg'), 'utf8').replace(/^[\s\S]*?<svg[^>]*>|<\/svg>\s*$/g, '').trim();
const tile = ({ radius, scale }) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="${radius}" fill="${bg}"/>
  <g transform="translate(256 256) scale(${scale}) translate(-256 -256)">
    ${markInner.replace(/\n\s*/g, '\n    ')}
  </g>
</svg>
`;

function pngSize(file) {
  const b = readFileSync(file);
  return [b.readUInt32BE(16), b.readUInt32BE(20)];
}

function screenshot(url, w, h, out) {
  const r = spawnSync(browser, [
    '--headless=new', '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=1',
    '--default-background-color=00000000', `--window-size=${w},${h}`, `--screenshot=${out}`, url,
  ], { encoding: 'utf8' });
  if (r.status !== 0 || !existsSync(out)) throw new Error(`render failed for ${out}\n${r.stderr}`);
  const [pw, ph] = pngSize(out);
  if (pw !== w || ph !== h) throw new Error(`${out} is ${pw}x${ph}, expected ${w}x${h}`);
}

let n = 0;
function renderSvg(svg, size, out) {
  const svgFile = join(tmp, `in-${n}.svg`);
  const htmlFile = join(tmp, `in-${n++}.html`);
  writeFileSync(svgFile, svg);
  writeFileSync(htmlFile, `<!doctype html><body style="margin:0;background:transparent"><img src="${pathToFileURL(svgFile)}" style="display:block;width:${size}px;height:${size}px">`);
  screenshot(pathToFileURL(htmlFile).href, size, size, out);
}

function ico(pngs) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(pngs.length, 4);
  let offset = 6 + 16 * pngs.length;
  const entries = pngs.map(({ size, data }) => {
    const e = Buffer.alloc(16);
    e.writeUInt8(size >= 256 ? 0 : size, 0);
    e.writeUInt8(size >= 256 ? 0 : size, 1);
    e.writeUInt16LE(1, 4);
    e.writeUInt16LE(32, 6);
    e.writeUInt32LE(data.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += data.length;
    return e;
  });
  return Buffer.concat([header, ...entries, ...pngs.map((p) => p.data)]);
}

const favicon = readFileSync(join(brand, 'favicon.svg'), 'utf8');
copyFileSync(join(brand, 'favicon.svg'), join(pub, 'icon.svg'));

const icoParts = [16, 32, 48].map((size) => {
  const out = join(tmp, `favicon-${size}.png`);
  renderSvg(favicon, size, out);
  return { size, data: readFileSync(out) };
});
writeFileSync(join(pub, 'favicon.ico'), ico(icoParts));

const rounded = tile({ radius: 112, scale: 0.86 });
writeFileSync(join(brand, 'mark-tile.svg'), rounded);
renderSvg(rounded, 192, join(pub, 'icon-192.png'));
renderSvg(rounded, 512, join(pub, 'icon-512.png'));
renderSvg(tile({ radius: 0, scale: 0.78 }), 180, join(pub, 'apple-touch-icon.png'));
renderSvg(tile({ radius: 0, scale: 0.66 }), 512, join(pub, 'icon-maskable-512.png'));

const manifest = {
  name: 'Radar',
  short_name: 'Radar',
  description: 'What needs me right now, across every open-source repo I work on.',
  start_url: '/',
  display: 'standalone',
  background_color: bg,
  theme_color: bg,
  icons: [
    { src: '/icon-192.png', sizes: '192x192', type: 'image/png' },
    { src: '/icon-512.png', sizes: '512x512', type: 'image/png' },
    { src: '/icon-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
  ],
};
writeFileSync(join(pub, 'manifest.webmanifest'), `${JSON.stringify(manifest, null, 2)}\n`);

screenshot(pathToFileURL(join(brand, 'social-preview.html')).href, 1280, 640, join(brand, 'social-preview.png'));

console.log('wrote public/{favicon.ico,icon.svg,icon-192.png,icon-512.png,apple-touch-icon.png,icon-maskable-512.png,manifest.webmanifest}');
console.log('wrote design/brand/{mark-tile.svg,social-preview.png}');
