// Reads `wrangler tail --format json` output on stdin and decides how the Syncer runs.
// Usage: node summarize.mjs [--expect 30] [--prs 20] [--reduce] < raw.jsonl
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const ALARM_LINE = /^alarm seq=(\d+) kind=(cold|warm)$/;

// wrangler prints each event as 4-space indented JSON, so a record is either one line
// or a block from a bare `{` line to a bare `}` line. Reduced files are one line each.
export const parseRecords = (text) => {
  const events = [];
  let malformed = 0;
  let block = null;
  const take = (chunk) => {
    try {
      events.push(JSON.parse(chunk));
    } catch {
      malformed++;
    }
  };
  for (const line of text.split(/\r?\n/)) {
    if (block) {
      block.push(line);
      if (line === '}') {
        take(block.join('\n'));
        block = null;
      } else if (line === '{') {
        malformed++;
        block = [line];
      }
    } else if (line === '{') {
      block = [line];
    } else if (line.trim()) {
      take(line);
    }
  }
  if (block) malformed++;
  return { events, malformed };
};

const alarmLog = (event) =>
  typeof event.log === 'string'
    ? event.log
    : (event.logs ?? []).flatMap((l) => l.message ?? []).find((m) => typeof m === 'string' && ALARM_LINE.test(m));

export const reduce = (event) => {
  const log = alarmLog(event);
  if (!log || !ALARM_LINE.test(log)) return null;
  const { cpuTime, wallTime, outcome, eventTimestamp } = event;
  return { eventTimestamp, outcome, cpuTime, wallTime, log };
};

const stats = (values) => {
  if (values.length === 0) return null;
  const s = [...values].sort((a, b) => a - b);
  const mid = s.length >> 1;
  return {
    median: s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2,
    p99: s[Math.ceil(s.length * 0.99) - 1],
    max: s[s.length - 1],
  };
};

const RERUN = 'Rerun on the 10-PR fixture: node make-fixture.mjs --prs 10, npm run deploy, npm run drive, then node summarize.mjs --prs 10.';

export const decide = ({ fired, expect, prs, coldMax }) => {
  if (fired === 0) return { kind: 'QueueFallback', reason: 'no alarm fired' };
  if (fired < expect) return { kind: 'Inconclusive', reason: `${fired} of ${expect} alarms fired` };
  if (coldMax === undefined) return { kind: 'Inconclusive', reason: 'no cold alarm has a cpuTime. Read the Workers metrics page for radar-spike' };
  if (prs === 20) {
    if (coldMax <= 7) return { kind: 'Syncer', batch: 20 };
    if (coldMax <= 10) return { kind: 'Syncer', batch: 10 };
    return { kind: 'Inconclusive', reason: `cold max ${coldMax} ms is over 10 ms. ${RERUN}` };
  }
  if (coldMax <= 10) return { kind: 'Syncer', batch: 10 };
  return { kind: 'QueueFallback', reason: `cold max ${coldMax} ms is over 10 ms on the 10-PR fixture` };
};

const formatDecision = (d) => {
  const name = d.kind === 'Syncer' ? `Syncer { batch: ${d.batch} }` : d.kind;
  return `Decision: ${name}${d.reason ? `. ${d.reason}` : ''}`;
};

const num = (n) => (n === undefined ? 'n/a' : String(Number(n.toFixed(2))));

export const run = (text, argv = []) => {
  const { values } = parseArgs({
    args: argv,
    options: {
      expect: { type: 'string', default: '30' },
      prs: { type: 'string', default: '20' },
      reduce: { type: 'boolean', default: false },
    },
  });
  const expect = Number(values.expect);
  const prs = Number(values.prs);
  if (!Number.isInteger(expect) || expect < 1) return { out: '--expect must be a positive integer', code: 2 };
  if (prs !== 20 && prs !== 10) return { out: '--prs must be 20 or 10', code: 2 };

  const { events, malformed } = parseRecords(text);
  if (events.length === 0) return { out: `no tail events on stdin (${malformed} malformed lines skipped)`, code: 1 };

  const alarms = events.map(reduce).filter(Boolean);
  if (values.reduce) return { out: alarms.map((a) => JSON.stringify(a)).join('\n'), code: 0 };

  const cpu = (kind) => alarms.filter((a) => a.log.endsWith(`kind=${kind}`) && typeof a.cpuTime === 'number').map((a) => a.cpuTime);
  const count = (kind) => alarms.filter((a) => a.log.endsWith(`kind=${kind}`)).length;
  const outcomes = Object.entries(Object.groupBy(alarms, (a) => a.outcome ?? 'unknown'))
    .map(([k, v]) => `${k} ${v.length}`)
    .join(', ');
  const rows = ['cold', 'warm'].map((kind) => {
    const s = stats(cpu(kind));
    return [kind, String(count(kind)), num(s?.median), num(s?.p99), num(s?.max)];
  });
  const decision = decide({ fired: alarms.length, expect, prs, coldMax: stats(cpu('cold'))?.max });
  const table = [['kind', 'count', 'median ms', 'p99 ms', 'max ms'], ...rows]
    .map(([k, ...cells]) => k.padEnd(6) + cells.map((c) => c.padStart(10)).join(''))
    .join('\n');
  const out = [
    `events: ${events.length} parsed, ${malformed} malformed lines skipped`,
    `alarms fired: ${alarms.length} of ${expect} expected (${outcomes || 'none'}), ${prs}-PR fixture`,
    'cpuTime per alarm invocation:',
    table,
    formatDecision(decision),
  ].join('\n');
  return { out, code: 0, decision };
};

const decodeStdin = () => {
  const bytes = readFileSync(0);
  if (bytes[0] === 0xff && bytes[1] === 0xfe) return new TextDecoder('utf-16le').decode(bytes.subarray(2));
  return new TextDecoder('utf-8').decode(bytes).replace(/^﻿/, '');
};

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { out, code } = run(decodeStdin(), process.argv.slice(2));
  (code === 0 ? console.log : console.error)(out);
  process.exit(code);
}
