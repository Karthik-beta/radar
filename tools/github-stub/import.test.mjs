import assert from "node:assert/strict";
import { test } from "node:test";

import {
  answerImportPrs,
  answerImportReviewedPrs,
  answerImportReviews,
  answerImportTimeline,
  answerRepoMeta,
  importCursor,
  importTotals,
  parseImportSearch,
} from "./import.mjs";
import { loadScenario } from "./server.mjs";

const scenario = loadScenario("import-history");
const q = (from, to, login = "karthik") => `is:pr author:${login} created:${from}..${to}`;

function search(from, to, after = null, login = "karthik") {
  return answerImportPrs(scenario, { q: q(from, to, login), first: 50, after }).body.data.search;
}

test("the scenario holds three years, a 1,500 result quarter, and its totals", () => {
  assert.deepEqual(importTotals(scenario), { prs: 1563, reviews: 128 });
  assert.equal(search("2025-04-01", "2025-06-30").issueCount, 1500);
  assert.equal(search("2024-01-01", "2024-12-31").issueCount, 30);
  assert.equal(search("2026-01-01", "2026-12-31").issueCount, 13);
  assert.equal(search("2026-01-01", "2026-12-31", null, "someone").issueCount, 0);
});

test("parseImportSearch reads the author and an inclusive date range", () => {
  const parsed = parseImportSearch(q("2025-01-01", "2025-12-31"));
  assert.equal(parsed.author, "karthik");
  assert.equal(parsed.from, Date.parse("2025-01-01T00:00:00Z"));
  assert.equal(parsed.to, Date.parse("2026-01-01T00:00:00Z"));
});

test("search returns at most 1,000 results, 50 to a page, and the halves cover the quarter once", () => {
  const year = search("2025-01-01", "2025-12-31");
  assert.equal(year.issueCount, 1520);
  assert.equal(year.nodes.length, 50);
  const last = search("2025-01-01", "2025-12-31", importCursor("search", 950));
  assert.equal(last.pageInfo.hasNextPage, false);

  const ids = new Set();
  for (const [from, to] of [
    ["2025-04-01", "2025-05-15"],
    ["2025-05-16", "2025-06-30"],
  ]) {
    let after = null;
    do {
      const page = search(from, to, after);
      assert.ok(page.issueCount <= 1000);
      for (const node of page.nodes) {
        assert.ok(!ids.has(node.id), node.id);
        ids.add(node.id);
      }
      after = page.pageInfo.hasNextPage ? page.pageInfo.endCursor : null;
    } while (after);
  }
  assert.equal(ids.size, 1500);
});

test("a node carries size, repo details, closing issues, rounds, and 20 timeline items", () => {
  const node = search("2026-01-01", "2026-12-31").nodes.find((n) => n.id === "PR_deep");
  assert.equal(node.repository.nameWithOwner, "octo/widgets");
  assert.equal(node.repository.stargazerCount, 1200);
  assert.deepEqual(node.repository.primaryLanguage, { name: "Rust" });
  assert.deepEqual(node.closingIssuesReferences.nodes, [{ number: 650, repository: { nameWithOwner: "octo/widgets" } }]);
  assert.equal(node.reviews.totalCount, 2);
  assert.equal(node.timelineItems.nodes.length, 20);
  assert.equal(node.timelineItems.pageInfo.endCursor, importCursor("timeline", 20));
  assert.ok(!JSON.stringify(node).includes("body"));
});

test("the deep pull request answers on its third timeline page", () => {
  const page = (after) => answerImportTimeline(scenario, { id: "PR_deep", after }).body.data.node.timelineItems;
  const second = page(importCursor("timeline", 20));
  assert.ok(second.nodes.every((n) => ["ci-bot", "karthik"].includes(n.author.login)));
  const third = page(importCursor("timeline", 40));
  const maint = third.nodes.find((n) => n.author.login === "maint");
  assert.equal(maint.submittedAt, "2026-02-03T12:00:00Z");
  assert.equal(third.pageInfo.hasNextPage, false);
  assert.equal(answerImportTimeline(scenario, { id: "PR_missing", after: null }).body.data.node, null);
});

test("the reviewed list pages past 100 and the thrice reviewed pull request has three reviews", () => {
  const year = (after) =>
    answerImportReviewedPrs(scenario, { from: "2025-01-01T00:00:00Z", to: "2025-12-31T23:59:59Z", after }).body.data
      .viewer.contributionsCollection.pullRequestReviewContributions;
  const first = year(null);
  assert.equal(first.nodes.length, 100);
  assert.equal(first.pageInfo.hasNextPage, true);
  assert.equal(year(first.pageInfo.endCursor).nodes.length, 20);

  const nodes = answerImportReviews(scenario, { ids: ["PR_thrice", "PR_nope"], login: "karthik", after: null }).body
    .data.nodes;
  assert.equal(nodes[0].reviews.nodes.length, 3);
  assert.deepEqual(
    nodes[0].reviews.nodes.map((r) => r.state),
    ["CHANGES_REQUESTED", "COMMENTED", "APPROVED"],
  );
  assert.equal(nodes[1], null);
});

test("RepoMeta answers repos by node id and null for an unknown one", () => {
  const { body } = answerRepoMeta(scenario, { ids: ["R_radar", "R_gone"] });
  assert.deepEqual(body.data.nodes[0], {
    id: "R_radar",
    nameWithOwner: "karthik/radar",
    stargazerCount: 12,
    primaryLanguage: { name: "Rust" },
  });
  assert.equal(body.data.nodes[1], null);
});

test("RepoMeta searches repos by name when byName is set", () => {
  const { body } = answerRepoMeta(scenario, { ids: ["R_radar"], q: "repo:Octo/Widgets repo:nobody/none", byName: true });
  assert.equal(body.data.nodes[0].id, "R_radar");
  assert.deepEqual(body.data.search.nodes.map((n) => n.id), ["R_widgets"]);
  assert.equal(answerRepoMeta(scenario, { ids: [], q: "", byName: false }).body.data.search, undefined);
});

test("RepoMeta finds a fork by name only when the query asks for forks", () => {
  const withFork = {
    ...scenario,
    import: { repos: { "karthik/tool": { id: "R_tool", stars: 1, language: "Go", fork: true } } },
  };
  const ask = (q) => answerRepoMeta(withFork, { ids: [], q, byName: true }).body.data.search.nodes.map((n) => n.id);
  assert.deepEqual(ask("repo:karthik/tool"), []);
  assert.deepEqual(ask("fork:true repo:karthik/tool"), ["R_tool"]);
});

test("a pending review does not move a reviewed pull request out of its year", () => {
  const withPending = {
    ...scenario,
    import: {
      reviewed: [
        {
          id: "PR_mixed",
          repo: "octo/widgets",
          number: 9,
          author: "bob",
          reviews: [
            { id: "PRR_done", state: "APPROVED", submitted_at: "2025-06-01T10:00:00Z" },
            { id: "PRR_wip", state: "PENDING", submitted_at: null },
          ],
        },
      ],
    },
  };
  const nodes = answerImportReviewedPrs(withPending, { from: "2025-01-01T00:00:00Z", to: "2025-12-31T23:59:59Z", after: null })
    .body.data.viewer.contributionsCollection.pullRequestReviewContributions.nodes;
  assert.deepEqual(nodes, [{ pullRequest: { id: "PR_mixed" } }]);
});

test("a bad cursor is a GraphQL error", () => {
  const { body } = answerImportPrs(scenario, { q: q("2025-01-01", "2025-12-31"), first: 50, after: "nope" });
  assert.ok(body.errors);
});
