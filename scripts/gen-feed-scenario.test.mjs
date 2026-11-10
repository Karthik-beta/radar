import assert from "node:assert/strict";
import { test } from "node:test";

import { build } from "../tools/github-stub/scenarios/make.mjs";

const repos = (scenario) => new Set(scenario._expect.rows.map((row) => row.repo));

test("feed-basic holds 12 events across 3 repos", () => {
  const scenario = build("feed-basic");
  assert.equal(scenario._expect.events, 12);
  assert.equal(repos(scenario).size, 3);
});

test("feed-old holds events 200, 190, and 170 days old", () => {
  const now = Date.parse("2026-11-10T00:00:00Z");
  const scenario = build("feed-old", now);
  assert.equal(scenario._expect.items, 3);
  const created = Object.values(scenario.items).map((item) => item.createdAt);
  assert.deepEqual(created, ["@-200d", "@-190d", "@-170d"]);
});

test("feed-many holds 100 pull requests of 20 events each", () => {
  const scenario = build("feed-many");
  assert.equal(scenario._expect.items, 100);
  assert.equal(scenario._expect.events, 2000);
});
