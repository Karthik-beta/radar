import assert from "node:assert/strict";
import { test } from "node:test";

import { build } from "./scenarios/make.mjs";
import { answerSearch, cursorFor, parseSearch } from "./search.mjs";
import { createStub, resolveTimes } from "./server.mjs";

const NOW = Date.parse("2026-11-09T12:00:00Z");
const rate = { limit: 5000, remaining: 4990, reset: "@+50m" };

function scenarioFor(kind) {
  return resolveTimes({ rate, ...build(kind, NOW) }, NOW);
}

function walk(scenario, q, first = 10) {
  const pages = [];
  let after = null;
  for (;;) {
    const { body } = answerSearch(scenario, { q, first, after });
    const { nodes, pageInfo } = body.data.search;
    pages.push(nodes);
    if (!pageInfo.hasNextPage) return pages;
    after = pageInfo.endCursor;
  }
}

test("the query text picks the list, the type and the cutoff", () => {
  assert.deepEqual(parseSearch("is:pr author:@me updated:>=2026-08-11"), {
    list: "authored",
    type: "PullRequest",
    since: Date.parse("2026-08-11T00:00:00Z"),
  });
  assert.equal(parseSearch("involves:@me updated:>=2026-08-11").list, "involves");
  assert.equal(parseSearch("is:issue author:@me").type, "Issue");
});

test("a chain of cursors walks every pull request once", () => {
  const pages = walk(scenarioFor("backfill-150"), "is:pr author:@me updated:>=2026-08-11");
  assert.equal(pages.length, 15);
  const numbers = pages.flat().map((n) => n.number);
  assert.equal(new Set(numbers).size, 150);
  assert.ok(pages.flat().every((n) => n.__typename === "PullRequest"));
});

test("authored issues and involved items come from their own queries", () => {
  const scenario = scenarioFor("backfill-150");
  assert.equal(walk(scenario, "is:issue author:@me updated:>=2026-08-11").flat().length, 20);
  assert.equal(walk(scenario, "involves:@me updated:>=2026-08-11").flat().length, 60);
});

test("an empty account answers one empty page", () => {
  const pages = walk(scenarioFor("backfill-empty"), "is:pr author:@me updated:>=2026-08-11");
  assert.deepEqual(pages, [[]]);
});

test("the cutoff drops items updated before it", () => {
  const scenario = scenarioFor("backfill-old");
  const numbers = walk(scenario, "is:pr author:@me updated:>=2026-08-11").flat().map((n) => n.number);
  assert.deepEqual(numbers.sort(), [4100, 4101]);
  assert.deepEqual(build("backfill-old", NOW)._expect.items, ["octo/widgets#4100", "octo/widgets#4101"]);
});

test("the nightly window keeps only the newer pull request", () => {
  const numbers = walk(scenarioFor("nightly-new"), "is:pr author:@me updated:>=2026-11-02").flat().map((n) => n.number);
  assert.deepEqual(numbers, [4000]);
  assert.deepEqual(build("nightly-new", NOW)._expect.items, ["octo/widgets#4000"]);
});

test("a bad cursor is an error, not an empty page", () => {
  const { body } = answerSearch(scenarioFor("backfill-150"), { q: "is:pr", first: 10, after: "bm9wZQ==" });
  assert.equal(body.data, null);
  assert.ok(body.errors.length);
});

test("overlap lists the same pull requests as notification threads and search nodes", () => {
  const scenario = scenarioFor("backfill-overlap");
  assert.equal(scenario.notifications.items.length, 12);
  const inBoth = scenario.search.authored.filter((n) => scenario.items[`octo/widgets#${n.number}`]);
  assert.equal(inBoth.length, 12);
  assert.equal(scenario._expect.items.length, 17);
});

test("every search kind builds and its expectation counts distinct items", () => {
  for (const kind of [
    "backfill-150",
    "backfill-empty",
    "backfill-overlap",
    "backfill-old",
    "nightly-new",
    "low-budget",
    "backfill-150-502",
    "backfill-150-dead",
    "backfill-marker",
  ]) {
    const scenario = build(kind, NOW);
    assert.equal(new Set(scenario._expect.items).size, scenario._expect.items.length, kind);
    assert.equal(scenario.notifications.etag, `"stub-${kind}"`, kind);
  }
  assert.equal(build("backfill-150", NOW)._expect.items.length, 210);
});

async function post(stub, variables) {
  const query = "query Search($q: String!, $first: Int!, $after: String) { search(query: $q) { nodes { __typename } } }";
  const { port } = stub.server.address();
  const res = await fetch(`http://127.0.0.1:${port}/graphql`, {
    method: "POST",
    headers: { authorization: `Bearer ${stub.state.scenario.token}`, "content-type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  return { status: res.status, body: await res.json() };
}

test("the stub fails only the page a fault names, then recovers", async () => {
  const stub = createStub({ scenario: "basic" });
  stub.state.scenario = scenarioFor("backfill-150-502");
  stub.state.scenario.token = "t";
  stub.state.faults = stub.state.scenario.faults.map((f) => ({ ...f, left: f.times }));
  await new Promise((resolve) => stub.server.listen(0, "127.0.0.1", resolve));
  try {
    const q = "is:pr author:@me updated:>=2026-08-11";
    assert.equal((await post(stub, { q, first: 10, after: null })).status, 200);
    assert.equal((await post(stub, { q, first: 10, after: cursorFor(20) })).status, 502);
    assert.equal((await post(stub, { q: "involves:@me", first: 10, after: cursorFor(20) })).status, 200);
    assert.equal((await post(stub, { q, first: 10, after: cursorFor(20) })).status, 502);
    const third = await post(stub, { q, first: 10, after: cursorFor(20) });
    assert.equal(third.status, 200);
    assert.equal(third.body.data.search.nodes.length, 10);
    const log = stub.state.log.filter((e) => e.path === "/graphql");
    assert.equal(log.at(-1).nodes, 10);
    assert.ok(Number.isFinite(Date.parse(log.at(-1).at)));
    assert.equal(log.at(-1).variables.after, cursorFor(20));
  } finally {
    await new Promise((resolve) => stub.server.close(resolve));
  }
});
