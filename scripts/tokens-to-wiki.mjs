#!/usr/bin/env node
// Renders design/tokens.json into the wiki's Design-tokens page plus colour swatch PNGs.
// Usage: node scripts/tokens-to-wiki.mjs <path-to-radar.wiki-checkout>
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { crc32, deflateSync } from 'node:zlib';
import { collect, format } from './lib/tokens.mjs';

const wikiDir = process.argv[2];
if (!wikiDir) {
  console.error('usage: node scripts/tokens-to-wiki.mjs <wiki-dir>');
  process.exit(1);
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const tree = JSON.parse(readFileSync(join(root, 'design/tokens.json'), 'utf8'));

function swatchPng(hex, size = 20) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const row = Buffer.concat([Buffer.from([0]), Buffer.from(Array.from({ length: size }, () => [r, g, b]).flat())]);
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(Buffer.concat(Array.from({ length: size }, () => row)))),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const headings = { color: 'Colour', font: 'Type', space: 'Spacing', radius: 'Radius', motion: 'Motion', breakpoint: 'Breakpoints' };
const cssName = (t) => `--${t.path.join('-')}`;
const tokens = collect(tree);
const groups = Map.groupBy(tokens, (t) => t.path[0]);

const imageDir = join(wikiDir, 'images/tokens');
mkdirSync(imageDir, { recursive: true });

const sections = [...groups].map(([group, list]) => {
  const isColor = group === 'color';
  const header = isColor ? '| Swatch | Token | Value | Use |\n| --- | --- | --- | --- |' : '| Token | Value | Use |\n| --- | --- | --- |';
  const rows = list.map((t) => {
    const cells = [`\`${cssName(t)}\``, `\`${format(t)}\``, t.description];
    if (isColor) {
      const file = `images/tokens/${t.path.join('-')}.png`;
      writeFileSync(join(wikiDir, file), swatchPng(t.value));
      cells.unshift(`![${t.value}](${file})`);
    }
    return `| ${cells.join(' | ')} |`;
  });
  return `## ${headings[group] ?? group}\n\n${header}\n${rows.join('\n')}`;
});

const css = tokens.map((t) => `  ${cssName(t)}: ${format(t)};`).join('\n');

const page = `# Design tokens

This page is generated from [\`design/tokens.json\`](https://github.com/Karthik-beta/radar/blob/main/design/tokens.json) by \`scripts/tokens-to-wiki.mjs\`. Edit the JSON file, not this page. The reasons behind these values are in [ADR 0003](https://github.com/Karthik-beta/radar/blob/main/docs/adr/0003-design.md).

Radar is dark only. Colour has two jobs: showing how long an item has waited (\`warm\`, then \`hot\`) and marking failing CI (\`red\`). Everything else is greyscale.

${sections.join('\n\n')}

## CSS custom properties

\`\`\`css
:root {
${css}
}
\`\`\`
`;

writeFileSync(join(wikiDir, 'Design-tokens.md'), page);
console.log(`wrote ${tokens.length} tokens to ${join(wikiDir, 'Design-tokens.md')}`);
