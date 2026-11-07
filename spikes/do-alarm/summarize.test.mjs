import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { run } from './summarize.mjs';

const path = (name) => fileURLToPath(new URL(name, import.meta.url));
const sample = readFileSync(path('testdata/tail-sample.jsonl'), 'utf8');
const lines = sample.trim().split('\n');

const isAlarm = (line) => line.includes('alarm seq=');
const withCold = (cpuTime) => {
  let edited = false;
  return lines
    .map((line) => {
      if (edited || !line.includes('kind=cold')) return line;
      edited = true;
      return JSON.stringify({ ...JSON.parse(line), cpuTime });
    })
    .join('\n');
};
const decision = (out) => out.split('\n').find((l) => l.startsWith('Decision:'));
const row = (out, kind) => out.split('\n').find((l) => l.startsWith(kind)).trim().split(/\s+/);

test('medians_match_the_sample', () => {
  const { out, code } = run(sample);
  assert.equal(code, 0);
  assert.deepEqual(row(out, 'cold'), ['cold', '3', '5', '6', '6']);
  assert.deepEqual(row(out, 'warm'), ['warm', '27', '1', '3', '3']);
});

test('no_alarm_gives_queue_fallback', () => {
  const { out, code } = run(lines.filter((l) => !isAlarm(l)).join('\n'));
  assert.equal(code, 0);
  assert.match(decision(out), /^Decision: QueueFallback/);
});

test('short_run_is_inconclusive', () => {
  const alarms = lines.filter(isAlarm);
  const { out } = run(alarms.slice(0, 20).join('\n'));
  assert.match(decision(out), /^Decision: Inconclusive\. 20 of 30 alarms fired/);
});

test('slow_cold_start_is_inconclusive', () => {
  const { out } = run(withCold(11));
  assert.match(decision(out), /^Decision: Inconclusive\. cold max 11 ms is over 10 ms\. Rerun on the 10-PR fixture/);
});

test('passing_run_gives_syncer_batch_20', () => {
  assert.equal(decision(run(sample).out), 'Decision: Syncer { batch: 20 }');
  assert.equal(decision(run(withCold(7)).out), 'Decision: Syncer { batch: 20 }');
});

test('cold_between_7_and_10_ms_gives_batch_10', () => {
  assert.equal(decision(run(withCold(7.5)).out), 'Decision: Syncer { batch: 10 }');
  assert.equal(decision(run(withCold(10)).out), 'Decision: Syncer { batch: 10 }');
});

test('slow_ten_pr_rerun_gives_queue_fallback', () => {
  assert.equal(decision(run(withCold(10), ['--prs', '10']).out), 'Decision: Syncer { batch: 10 }');
  assert.match(decision(run(withCold(11), ['--prs', '10']).out), /^Decision: QueueFallback\. cold max 11 ms/);
});

test('malformed_lines_are_counted_and_skipped', () => {
  const input = ['> radar-spike-do-alarm@ tail', '> wrangler tail radar-spike --format json', ...lines, '{"cpuTime": 3, "logs": [', '{'].join('\n');
  const { out } = run(input);
  assert.match(out, /^events: 39 parsed, 4 malformed lines skipped$/m);
  assert.equal(decision(out), 'Decision: Syncer { batch: 20 }');
});

test('empty_input_exits_nonzero', () => {
  const result = spawnSync(process.execPath, [path('summarize.mjs')], { input: '' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr.toString(), /no tail events/);
});

test('pretty_printed_tail_parses_like_jsonl', () => {
  const pretty = lines.map((l) => JSON.stringify(JSON.parse(l), null, 4)).join('\n');
  assert.equal(run(pretty).out, run(sample).out);
});

test('reduced_output_summarizes_the_same', () => {
  const reduced = run(sample, ['--reduce']).out;
  assert.equal(reduced.split('\n').length, 30);
  assert.deepEqual(Object.keys(JSON.parse(reduced.split('\n')[0])), ['eventTimestamp', 'outcome', 'cpuTime', 'wallTime', 'log']);
  assert.equal(decision(run(reduced).out), decision(run(sample).out));
});

test('utf16_redirect_from_windows_powershell_is_read', () => {
  const bom = Buffer.from([0xff, 0xfe]);
  const result = spawnSync(process.execPath, [path('summarize.mjs')], { input: Buffer.concat([bom, Buffer.from(sample, 'utf16le')]) });
  assert.equal(result.status, 0);
  assert.match(result.stdout.toString(), /Decision: Syncer \{ batch: 20 \}/);
});
