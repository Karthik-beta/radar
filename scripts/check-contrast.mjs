#!/usr/bin/env node
// Checks WCAG contrast for every text and background pair the UI uses, from design/tokens.json.
// Usage: node scripts/check-contrast.mjs [--tokens <file>] [--css <dir>] [--self-test]
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

export const PAIRS = [
  { fg: 'text', bg: 'bg', min: 4.5, use: 'primary text' },
  { fg: 'text', bg: 'surface', min: 4.5, use: 'selected row, active tab' },
  { fg: 'muted', bg: 'bg', min: 4.5, use: 'repo, actor, reason' },
  { fg: 'muted', bg: 'surface', min: 4.5, use: 'sync button, panel text on cards' },
  { fg: 'faint', bg: 'bg', min: 4.5, use: 'key hints, agent mark' },
  { fg: 'faint', bg: 'surface', min: 4.5, use: 'sync time, key hints on a selected row' },
  { fg: 'warm', bg: 'bg', min: 4.5, use: 'age past the first threshold' },
  { fg: 'warm', bg: 'surface', min: 4.5, use: 'age on a selected row' },
  { fg: 'hot', bg: 'bg', min: 4.5, use: 'age past the second threshold' },
  { fg: 'hot', bg: 'surface', min: 4.5, use: 'age on a selected row' },
  { fg: 'red', bg: 'bg', min: 4.5, use: 'failing CI' },
  { fg: 'red', bg: 'surface', min: 4.5, use: 'failing CI on a selected row' },
  { fg: 'text', bg: 'bg', min: 3, use: 'focus ring on the page' },
  { fg: 'text', bg: 'surface', min: 3, use: 'focus ring on a card or selected row' },
];

function channel(c) {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

export function luminance(hex) {
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!m) throw new Error(`not a #rrggbb colour: ${hex}`);
  const n = parseInt(m[1], 16);
  return 0.2126 * channel((n >> 16) & 255) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255);
}

export function ratio(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

export function checkPairs(pairs, colors) {
  return pairs.map((pair) => {
    const fg = colors[pair.fg];
    const bg = colors[pair.bg];
    if (!fg || !bg) {
      const missing = !fg ? pair.fg : pair.bg;
      return { ...pair, ratio: null, ok: false, error: `no colour token "${missing}"` };
    }
    const r = ratio(fg, bg);
    return { ...pair, ratio: r, ok: r >= pair.min };
  });
}

export function readColors(file) {
  const tokens = JSON.parse(readFileSync(file, 'utf8'));
  const colors = {};
  for (const [name, token] of Object.entries(tokens.color)) {
    if (!name.startsWith('$')) colors[name] = token.$value;
  }
  return colors;
}

// An outline removed in a stylesheet needs a :focus-visible rule in the same file.
export function checkCss(dir) {
  const problems = [];
  for (const name of readdirSync(dir).filter((f) => f.endsWith('.css'))) {
    const css = readFileSync(join(dir, name), 'utf8');
    if (/outline\s*:\s*(none|0)\b/.test(css) && !css.includes(':focus-visible')) {
      problems.push(`${name}: removes an outline with no :focus-visible replacement`);
    }
  }
  return problems;
}

function selfTest() {
  const failures = [];
  const expect = (label, ok) => {
    if (!ok) failures.push(label);
  };
  expect('black on white is 21:1', Math.abs(ratio('#000000', '#ffffff') - 21) < 0.01);
  expect('#777777 on white is 4.48:1', Math.abs(ratio('#777777', '#ffffff') - 4.48) < 0.01);
  const missing = checkPairs([{ fg: 'nope', bg: 'bg', min: 4.5, use: 'x' }], { bg: '#000000' });
  expect('a missing token fails', missing[0].ok === false && /nope/.test(missing[0].error));
  return failures;
}

function main() {
  const { values } = parseArgs({
    options: {
      tokens: { type: 'string', default: join(root, 'design/tokens.json') },
      css: { type: 'string' },
      'self-test': { type: 'boolean', default: false },
    },
  });
  if (values['self-test']) {
    const failures = selfTest();
    for (const f of failures) console.log(`FAIL ${f}`);
    console.log(failures.length ? `${failures.length} self-test failures` : 'self-test ok');
    process.exit(failures.length ? 1 : 0);
  }
  if (values.css) {
    const problems = checkCss(values.css);
    for (const p of problems) console.log(`FAIL ${p}`);
    console.log(`${problems.length} css problems`);
    process.exit(problems.length ? 1 : 0);
  }
  const results = checkPairs(PAIRS, readColors(values.tokens));
  for (const r of results) {
    const shown = r.ratio === null ? r.error : `${r.ratio.toFixed(2)}:1 (min ${r.min})`;
    console.log(`${r.ok ? 'ok  ' : 'FAIL'} ${r.fg} on ${r.bg} ${shown}  ${r.use}`);
  }
  const failing = results.filter((r) => !r.ok).length;
  console.log(`${failing} failing pairs`);
  process.exit(failing ? 1 : 0);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
