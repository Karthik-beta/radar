import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { compare, expectedFromScenario, itemKey, itemsFromGh, itemsFromRows, parse, report, windowStart } from "./compare-sync.mjs";

test("compare lists each missing and extra item once, sorted", () => {
  const result = compare(["a/b#2", "a/b#2", "a/b#9"], ["a/b#1", "a/b#2"]);
  assert.deepEqual(result, { missing: ["a/b#1"], extra: ["a/b#9"] });
  assert.equal(report(result), "missing a/b#1\nextra a/b#9\n1 missing, 1 extra");
});

test("equal sets print a zero line", () => {
  assert.equal(report(compare(["a/b#1"], ["a/b#1"])), "0 missing, 0 extra");
});

test("keys ignore the case of the repository", () => {
  assert.equal(itemKey("Octo/Widgets", 4), "octo/widgets#4");
});

test("wrangler rows and gh results become the same keys", () => {
  const rows = JSON.stringify([{ results: [{ repo: "Octo/Widgets", number: 3 }], success: true }]);
  const gh = [JSON.stringify([{ repository: { nameWithOwner: "octo/widgets" }, number: 3 }])];
  assert.deepEqual(itemsFromRows(rows), itemsFromGh(gh));
});

test("the window starts on the day 90 days before now", () => {
  assert.equal(windowStart(Date.parse("2026-11-09T12:00:00Z"), 90), "2026-08-11");
});

test("the expected set is the scenario's _expect.items, found with or without the extension", () => {
  const dir = mkdtempSync(join(tmpdir(), "compare-sync-"));
  try {
    const file = join(dir, "kind.json");
    writeFileSync(file, JSON.stringify({ _expect: { items: ["Octo/Widgets#7"] } }));
    assert.deepEqual(expectedFromScenario(file), ["octo/widgets#7"]);
    assert.deepEqual(expectedFromScenario(join(dir, "kind")), ["octo/widgets#7"]);
    writeFileSync(file, JSON.stringify({}));
    assert.throws(() => expectedFromScenario(file), /no _expect.items/);
    assert.throws(() => expectedFromScenario(join(dir, "absent")), /no scenario file/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("arguments are checked", () => {
  assert.throws(() => parse(["--db", "local"]), /exactly one/);
  assert.throws(() => parse(["--db", "remote", "--source", "gh"]), /preview database/);
  assert.throws(() => parse(["--db", "local", "--expect", "x", "--source", "gh"]), /exactly one/);
  assert.equal(parse(["--db", "local", "--expect", "generated/x"]).database, "radar");
});
