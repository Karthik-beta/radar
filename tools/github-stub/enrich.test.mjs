import assert from "node:assert/strict";
import { test } from "node:test";

import { answerEnrich, withFactFields } from "./enrich.mjs";
import { build, seedSql, threadIds } from "./scenarios/make.mjs";
import { analyzeDocument, createStub, resolveTimes } from "./server.mjs";

const KINDS = [
  "enrich-20",
  "enrich-edge",
  "enrich-basic",
  "enrich-bot",
  "enrich-read",
  "enrich-45",
  "enrich-gone",
  "enrich-limited",
  "enrich-502",
  "enrich-body-marker",
  "enrich-ci-step",
];

const rate = { limit: 5000, remaining: 4990, reset: "@+50m" };

function variablesFor(scenario) {
  const variables = {};
  scenario.notifications.items.forEach((n, i) => {
    const [owner, name] = n.repository.full_name.split("/");
    variables[`o${i}`] = owner;
    variables[`n${i}`] = name;
    variables[`i${i}`] = Number(n.subject.url.split("/").pop());
  });
  return variables;
}

test("every kind builds a scenario with an _expect that matches its threads", () => {
  for (const kind of KINDS) {
    const scenario = build(kind);
    const { _expect: expect } = scenario;
    assert.equal(expect.threads, scenario.notifications.items.length, kind);
    assert.equal(expect.items + expect.gone, expect.threads, kind);
    assert.equal(expect.rows.length, expect.items, kind);
    assert.equal(new Set(threadIds(scenario)).size, expect.threads, `${kind} thread ids are distinct`);
  }
  assert.throws(() => build("enrich-nope"), /unknown kind/);
});

test("the scenarios have the sizes the lanes rely on", () => {
  assert.equal(build("enrich-basic")._expect.threads, 6);
  assert.equal(build("enrich-45")._expect.threads, 45);
  assert.equal(build("enrich-gone")._expect.threads, 20);
  assert.equal(build("enrich-gone")._expect.gone, 1);
  assert.equal(build("enrich-20")._expect.threads, 20);
});

test("a scenario answers every alias from its items", () => {
  const scenario = resolveTimes({ rate, ...build("enrich-basic") });
  const { count, body } = answerEnrich(scenario, variablesFor(scenario));
  assert.equal(count, 6);
  assert.equal(body.errors, undefined);
  for (let i = 0; i < 6; i += 1) assert.ok(body.data[`a${i}`].issueOrPullRequest.id, `a${i}`);
  assert.equal(body.data.rateLimit.remaining, 4990);
  assert.match(body.data.rateLimit.resetAt, /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$/);
});

test("times are whole seconds, the form GitHub sends", () => {
  const scenario = resolveTimes({ rate, ...build("enrich-basic") });
  const { body } = answerEnrich(scenario, variablesFor(scenario));
  assert.doesNotMatch(JSON.stringify(body), /\.\d{3}Z/);
});

test("an unknown key is a null alias and a null item is a null issueOrPullRequest, both NOT_FOUND", () => {
  const scenario = resolveTimes({ rate, ...build("enrich-edge") });
  const { body } = answerEnrich(scenario, variablesFor(scenario));
  assert.equal(body.data.a1, null);
  assert.deepEqual(body.errors.map((e) => e.type), ["NOT_FOUND"]);
  assert.deepEqual(body.errors[0].path, ["a1"]);

  const gone = resolveTimes({ rate, ...build("enrich-gone") });
  const answered = answerEnrich(gone, variablesFor(gone)).body;
  assert.deepEqual(answered.data.a7, { issueOrPullRequest: null });
  assert.deepEqual(answered.errors[0].path, ["a7", "issueOrPullRequest"]);
});

test("the enrich-body-marker scenario carries bodies the query never asks for", () => {
  const text = JSON.stringify(build("enrich-body-marker"));
  assert.match(text, /BODYMARK-7f3a/);
});

test("the seed SQL has one statement per thread and escapes quotes", () => {
  const scenario = build("enrich-basic");
  scenario.notifications.items[0].subject.title = "it's";
  const sql = seedSql(scenario, Date.parse("2026-11-07T12:00:00Z"));
  assert.equal(sql.split("\n").length, 6);
  assert.match(sql, /'it''s'/);
});

test("the stub answers Enrich as one POST /graphql and logs the alias count", async () => {
  const { server, state } = createStub({ scenario: "basic" });
  state.scenario = resolveTimes({ rate, token: state.scenario.token, ...build("enrich-basic") });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  try {
    const query = "query Enrich($o0: String!, $n0: String!, $i0: Int!) { a0: repository(owner: $o0, name: $n0) { id } }";
    assert.equal(analyzeDocument(query).name, "Enrich");
    const response = await fetch(`http://127.0.0.1:${port}/graphql`, {
      method: "POST",
      headers: { authorization: `Bearer ${state.scenario.token}`, "content-type": "application/json" },
      body: JSON.stringify({ query, variables: { o0: "octo", n0: "widgets", i0: 12 } }),
    });
    const body = await response.json();
    assert.equal(body.data.a0.issueOrPullRequest.number, 12);
    const entry = state.log.at(-1);
    assert.equal(entry.aliases, 1);
    assert.equal(entry.opName, "Enrich");
    assert.equal(entry.violation, null);
  } finally {
    server.close();
  }
});

test("a pull request without the fact fields gets defaults, and an issue is left alone", () => {
  const pr = {
    __typename: "PullRequest",
    timelineItems: { nodes: [{ id: "R1", state: "CHANGES_REQUESTED" }] },
    latestReviews: { nodes: [{ id: "R1", state: "CHANGES_REQUESTED" }, { id: "R2", state: "APPROVED" }] },
  };
  const filled = withFactFields(pr);
  assert.deepEqual(filled.closingIssuesReferences, { nodes: [] });
  assert.equal(filled.reviews.totalCount, 1);
  assert.equal(typeof filled.additions, "number");
  assert.equal(withFactFields({ ...pr, additions: 99 }).additions, 99);
  const issue = { __typename: "Issue", id: "I_1" };
  assert.equal(withFactFields(issue), issue);
});

test("facts-live has eight pull requests of mine, an agent head, two closing issues, and a turn step", () => {
  const items = Object.values(build("facts-live").items);
  const mine = items.filter((n) => n.__typename === "PullRequest" && n.author.login === "karthik");
  assert.equal(mine.length, 8);
  const owners = new Set(mine.map((n) => new URL(n.url).pathname.split("/")[1]));
  assert.deepEqual([...owners].sort(), ["karthik", "octo"]);
  assert.deepEqual(mine.filter((n) => n.headRefName.startsWith("agent/")).map((n) => n.number), [312]);
  const closing = mine.flatMap((n) => n.closingIssuesReferences?.nodes ?? []);
  assert.deepEqual(closing.map((c) => `${c.repository.nameWithOwner}#${c.number}`).sort(), [
    "karthik/radar#5",
    "octo/widgets#310",
  ]);
  const before = build("facts-live").items["octo/widgets#305"].timelineItems.nodes;
  const turned = build("facts-live-turn");
  const after = turned.items["octo/widgets#305"].timelineItems.nodes;
  assert.equal(after.length, before.length + 1);
  assert.equal(after.at(-1).actor.login, "maint");
  const row = turned._expect.rows.find((r) => r.number === 305);
  assert.deepEqual([row.stage, row.turn], ["awaiting_review", "me"]);
});
