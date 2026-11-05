// Reports gzipped bundle sizes and times the Rust GraphQL parse in V8.
// Run after: cargo build --workspace --release --target wasm32-unknown-unknown
//            wasm-bindgen --target module --out-dir out/worker target/wasm32-unknown-unknown/release/worker_size.wasm
//            wasm-bindgen --target web --out-dir out/ui target/wasm32-unknown-unknown/release/ui_size.wasm
import { readFileSync } from 'node:fs';
import { gzipSync } from 'node:zlib';

const kb = (n) => `${(n / 1024).toFixed(0)} KB`;
const size = (files) => {
  const bytes = files.map((f) => readFileSync(f));
  const raw = bytes.reduce((a, b) => a + b.length, 0);
  const gz = bytes.reduce((a, b) => a + gzipSync(b, { level: 9 }).length, 0);
  return `${kb(raw)} raw, ${kb(gz)} gzipped`;
};
console.log('worker bundle:', size(['out/worker/worker_size_bg.wasm', 'out/worker/worker_size.js']));
console.log('ui bundle:    ', size(['out/ui/ui_size_bg.wasm', 'out/ui/ui_size.js']));

const pr = (i) => ({
  id: `PR_kwDOA${String(i).padStart(8, '0')}`,
  number: 4000 + i,
  title: `Support the D1 batch API in the migrator, part ${i} of a longer series of changes`,
  url: `https://github.com/drizzle-team/drizzle-orm/pull/${4000 + i}`,
  isDraft: i % 7 === 0,
  state: 'OPEN',
  reviewDecision: i % 3 === 0 ? 'CHANGES_REQUESTED' : null,
  createdAt: '2026-10-20T09:14:02Z',
  updatedAt: '2026-11-04T17:40:11Z',
  mergedAt: null,
  closedAt: null,
  author: { login: 'karthik', __typename: 'User' },
  repository: { nameWithOwner: 'drizzle-team/drizzle-orm' },
  reviews: { nodes: Array.from({ length: 5 }, (_, j) => ({ author: { login: `maint${j}`, __typename: 'User' }, state: j % 2 ? 'COMMENTED' : 'CHANGES_REQUESTED', submittedAt: '2026-11-02T08:00:00Z' })) },
  commits: { nodes: [{ commit: { statusCheckRollup: { state: i % 4 === 0 ? 'FAILURE' : 'SUCCESS' } } }] },
  timelineItems: { nodes: Array.from({ length: 10 }, (_, j) => ({ __typename: j % 3 ? 'IssueComment' : 'PullRequestReview', createdAt: '2026-11-03T10:00:00Z', actor: null, author: { login: j % 4 ? 'karthik' : 'codecov', __typename: j % 4 ? 'User' : 'Bot' } })) },
});
const body = JSON.stringify({ data: { search: { nodes: Array.from({ length: 20 }, (_, i) => pr(i)) } } });
console.log('fixture:      20 PRs,', kb(body.length));

const { instance } = await WebAssembly.instantiate(readFileSync('target/wasm32-unknown-unknown/release/parse_bench.wasm'));
const { memory, alloc, dealloc, parse } = instance.exports;
const enc = new TextEncoder();
const runRust = () => {
  const bytes = enc.encode(body);
  const ptr = alloc(bytes.length);
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  const r = parse(ptr, bytes.length);
  dealloc(ptr, bytes.length);
  if (r === 0) throw new Error('rust parse failed');
  return r;
};
const runJs = () => JSON.parse(body).data.search.nodes.length;

const time = (fn, n) => {
  for (let i = 0; i < 50; i++) fn();
  const samples = [];
  for (let i = 0; i < n; i++) {
    const t = performance.now();
    fn();
    samples.push(performance.now() - t);
  }
  samples.sort((a, b) => a - b);
  return `median ${samples[n >> 1].toFixed(3)} ms, p99 ${samples[Math.floor(n * 0.99)].toFixed(3)} ms`;
};
const cold = performance.now();
runRust();
console.log(`rust first call: ${(performance.now() - cold).toFixed(3)} ms (after warm-up below this is JIT-free wasm)`);
console.log('rust parse:  ', time(runRust, 1000));
console.log('JSON.parse:  ', time(runJs, 1000));
