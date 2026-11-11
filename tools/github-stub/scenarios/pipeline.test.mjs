import assert from "node:assert/strict";
import { test } from "node:test";

import { build } from "./make.mjs";

const stages = (kind) => build(kind)._expect.rows.map((row) => row.stage);

test("pipeline-all-stages holds every open stage and three awaiting review ages", () => {
  const found = new Set(stages("pipeline-all-stages"));
  for (const stage of ["draft", "ci_running", "ci_failing", "awaiting_review", "changes_requested", "approved"]) {
    assert.ok(found.has(stage), stage);
  }
  assert.equal(stages("pipeline-all-stages").filter((s) => s === "awaiting_review").length, 3);
});

test("pipeline-two-stages holds only draft and approved", () => {
  assert.deepEqual(stages("pipeline-two-stages"), ["draft", "approved"]);
});

test("pipeline-stage-move moves only the CI running pull request to CI failing", () => {
  const before = stages("pipeline-all-stages");
  const after = stages("pipeline-stage-move");
  const changed = before.flatMap((stage, i) => (stage === after[i] ? [] : [[stage, after[i]]]));
  assert.deepEqual(changed, [["ci_running", "ci_failing"]]);
});

test("pipeline-recent has one open pull request and three ended ones", () => {
  const items = Object.values(build("pipeline-recent").items);
  assert.deepEqual(items.map((item) => item.prState), ["OPEN", "MERGED", "CLOSED", "MERGED"]);
});

test("pipeline-hostile carries a 300 character title, markup, and a bad URL", () => {
  const items = Object.values(build("pipeline-hostile").items);
  assert.equal(items[0].title.length, 300);
  assert.equal(items[1].title, "<img src=x onerror=alert(1)>");
  assert.equal(items[2].url, "javascript:alert(1)");
});

test("a11y-all-views fills every view, drops a scope, and faults the backfill search", () => {
  const scenario = build("a11y-all-views");
  const rows = scenario._expect.rows;
  assert.ok(rows.some((row) => row.turn === "me"));
  assert.ok(rows.some((row) => row.turn === "them"));
  assert.ok(rows.some((row) => row.stage === "ci_failing"));
  assert.deepEqual(scenario.scopes, ["read:user"]);
  assert.equal(scenario.faults.length, 1);
});
