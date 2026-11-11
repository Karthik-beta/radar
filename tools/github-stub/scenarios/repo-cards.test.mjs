import assert from "node:assert/strict";
import { test } from "node:test";

import { build } from "./make.mjs";

const prs = () => Object.values(build("repo-cards").items);
const inRepo = (name) => prs().filter((item) => item.url.includes(`/${name}/`));

test("repo-cards holds 60 pull requests across 4 repos", () => {
  assert.equal(prs().length, 60);
  assert.equal(new Set(build("repo-cards")._expect.rows.map((row) => row.repo)).size, 4);
});

test("octo/widgets has 3 merged, 1 closed, and 2 open pull requests", () => {
  const states = inRepo("widgets").map((item) => item.prState);
  const count = (state) => states.filter((s) => s === state).length;
  assert.deepEqual([count("MERGED"), count("CLOSED"), count("OPEN")], [3, 1, 2]);
});

test("four octo/widgets pull requests get a maintainer reply, and octo/tools none", () => {
  const replied = (item) => item.timelineItems.nodes.some((node) => node.actor.login === "bob");
  assert.equal(inRepo("widgets").filter(replied).length, 4);
  assert.equal(inRepo("tools").filter(replied).length, 0);
});
