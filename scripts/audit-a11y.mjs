#!/usr/bin/env node
// Runs Lighthouse's accessibility category on each view at the mobile and desktop presets and prints the scores.
// Usage: node scripts/audit-a11y.mjs --jar <curl cookie jar> --base <origin> [--min 95]
// The session cookie comes from the jar and travels as an extra header. Lighthouse is fetched by npx, so the operator approves that download first.
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';

export const VIEWS = [
  { name: 'Your turn', path: '/' },
  { name: 'Their turn', path: '/theirs' },
  { name: 'Pipeline', path: '/pipeline' },
  { name: 'Merged', path: '/merged' },
];
export const PRESETS = ['mobile', 'desktop'];

// A curl jar line is `domain flag path secure expiry name value`, tab separated. `#HttpOnly_` prefixes the domain of an HttpOnly cookie.
export function cookieHeader(jar, host) {
  const pairs = [];
  for (const raw of jar.split(/\r?\n/)) {
    const line = raw.startsWith('#HttpOnly_') ? raw.slice('#HttpOnly_'.length) : raw;
    if (!line || line.startsWith('#')) continue;
    const [domain, , , , , name, value] = line.split('\t');
    if (name === undefined || value === undefined) continue;
    if (host === domain.replace(/^\./, '')) pairs.push(`${name}=${value}`);
  }
  if (!pairs.length) throw new Error(`no cookie for ${host} in the jar`);
  return pairs.join('; ');
}

export function failedAudits(report) {
  return Object.values(report.audits ?? {})
    .filter((audit) => audit.score === 0)
    .map((audit) => audit.id);
}

function lighthouse(url, preset, cookie) {
  const args = [
    '--yes',
    'lighthouse@12',
    url,
    '--only-categories=accessibility',
    '--output=json',
    '--output-path=stdout',
    '--quiet',
    '--chrome-flags=--headless=new',
    `--extra-headers=${JSON.stringify({ Cookie: cookie })}`,
  ];
  if (preset === 'desktop') args.push('--preset=desktop');
  const run = spawnSync('npx', args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, shell: process.platform === 'win32' });
  if (run.status !== 0) throw new Error(`lighthouse failed for ${url} (${preset}): ${run.stderr.trim()}`);
  return JSON.parse(run.stdout);
}

function main() {
  const { values } = parseArgs({
    options: {
      jar: { type: 'string' },
      base: { type: 'string' },
      min: { type: 'string', default: '95' },
    },
  });
  if (!values.jar || !values.base) {
    console.error('usage: node scripts/audit-a11y.mjs --jar <jar.txt> --base <origin> [--min 95]');
    process.exit(2);
  }
  const base = new URL(values.base);
  const cookie = cookieHeader(readFileSync(values.jar, 'utf8'), base.hostname);
  const min = Number(values.min);
  let failing = 0;
  for (const view of VIEWS) {
    for (const preset of PRESETS) {
      const report = lighthouse(new URL(view.path, base).href, preset, cookie);
      const score = Math.round((report.categories.accessibility.score ?? 0) * 100);
      const failed = failedAudits(report);
      const ok = score >= min && failed.length === 0;
      if (!ok) failing += 1;
      console.log(`${ok ? 'ok  ' : 'FAIL'} ${view.name} ${preset} ${score}${failed.length ? ` failed: ${failed.join(', ')}` : ''}`);
    }
  }
  console.log(`${failing} failing runs`);
  process.exit(failing ? 1 : 0);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
