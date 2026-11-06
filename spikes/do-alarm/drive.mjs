// Arms alarm chains on the spike and waits for each to finish. Reads the key from SPIKE_KEY.
// Usage: SPIKE_KEY=... node drive.mjs --url <worker url> [--rounds 3] [--chain 10] [--gap-ms 2000] [--idle-s 900] [--fixture ok|throw]
import { setTimeout as sleep } from 'node:timers/promises';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: {
    url: { type: 'string' },
    rounds: { type: 'string', default: '3' },
    chain: { type: 'string', default: '10' },
    'gap-ms': { type: 'string', default: '2000' },
    'idle-s': { type: 'string', default: '900' },
    fixture: { type: 'string', default: 'ok' },
  },
});

const fail = (message) => {
  console.error(message);
  process.exit(2);
};
const int = (name, min) => {
  const n = Number(values[name]);
  if (!Number.isInteger(n) || n < min) fail(`--${name} must be an integer of at least ${min}`);
  return n;
};

const key = process.env.SPIKE_KEY;
if (!key) fail('Set SPIKE_KEY in the environment. It is never read from an argument.');
if (!values.url) fail('--url is required, for example https://radar-spike.<account>.workers.dev');
if (!['ok', 'throw'].includes(values.fixture)) fail('--fixture must be ok or throw');
const base = values.url.replace(/\/+$/, '');
const rounds = int('rounds', 1);
const chain = int('chain', 1);
const gapMs = int('gap-ms', 0);
const idleS = int('idle-s', 0);
const POLL_LIMIT_MS = 120_000;

const call = async (method, path) => {
  const res = await fetch(`${base}${path}`, { method, headers: { authorization: `Bearer ${key}` } });
  if (!res.ok) throw new Error(`${method} ${path} answered ${res.status}: ${(await res.text()).slice(0, 200)}`);
  return res.status === 204 ? null : res.json();
};

let incomplete = 0;
for (let round = 1; round <= rounds; round++) {
  const armed = await call('POST', `/arm?chain=${chain}&gap_ms=${gapMs}&fixture=${values.fixture}`).catch((err) =>
    fail(`round ${round}: arm failed: ${err.cause?.code ?? err.message}`),
  );
  console.log(`round ${round}: armed seq ${armed.first_seq} to ${armed.last_seq}, first due ${new Date(armed.scheduled_for).toISOString()}`);
  const deadline = Date.now() + POLL_LIMIT_MS;
  let mine = [];
  let done = false;
  while (Date.now() < deadline) {
    await sleep(1000);
    const log = await call('GET', '/log').catch((err) => console.log(`  poll failed: ${err.cause?.code ?? err.message}`));
    if (!log) continue;
    mine = log.runs.filter((r) => r.seq >= armed.first_seq && r.seq <= armed.last_seq);
    done = mine.some((r) => r.seq === armed.last_seq && r.outcome === 'ok');
    if (done) break;
  }
  const kinds = mine.map((r) => `${r.seq}:${r.kind}:${r.outcome}#${r.attempt}`).join(' ');
  const maxLag = Math.max(0, ...mine.map((r) => r.started_at - r.scheduled_for));
  console.log(`round ${round}: ${done ? 'complete' : `incomplete after ${POLL_LIMIT_MS / 1000} s`}, ${mine.length} rows, max start lag ${maxLag} ms`);
  console.log(`  ${kinds}`);
  if (!done) incomplete++;
  if (round < rounds && idleS > 0) {
    console.log(`idling ${idleS} s so the next round starts in a fresh instance`);
    await sleep(idleS * 1000);
  }
}
process.exit(incomplete ? 1 : 0);
