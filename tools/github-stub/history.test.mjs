import assert from "node:assert/strict";
import { test } from "node:test";

import { answerHistory, historyCursor } from "./history.mjs";

const rate = { limit: 5000, remaining: 4990, reset: "2026-11-07T13:00:00Z" };

function scenario(count) {
  const pull_requests = Array.from({ length: count }, (_, i) => ({
    occurred_at: new Date(Date.parse("2025-03-01T00:00:00Z") + i * 3_600_000).toISOString(),
    merged_at: i % 2 ? "2025-03-20T00:00:00Z" : null,
    repo: "octo/widgets",
  }));
  pull_requests.push({ occurred_at: "2024-01-01T00:00:00Z", merged_at: null, repo: "octo/old" });
  return { rate, history: { pull_requests } };
}

const window = { from: "2025-01-01T00:00:00Z", to: "2026-01-01T00:00:00Z" };

test("a page holds 100 contributions inside the window and names the next", () => {
  const first = answerHistory(scenario(130), { ...window, after: null });
  const page = first.body.data.viewer.contributionsCollection.pullRequestContributions;
  assert.equal(first.count, 100);
  assert.deepEqual(page.pageInfo, { endCursor: historyCursor(100), hasNextPage: true });
  assert.equal(page.nodes[1].pullRequest.mergedAt, "2025-03-20T00:00:00Z");
  assert.equal(page.nodes[0].pullRequest.mergedAt, null);

  const last = answerHistory(scenario(130), { ...window, after: historyCursor(100) });
  assert.equal(last.count, 30);
  assert.equal(
    last.body.data.viewer.contributionsCollection.pullRequestContributions.pageInfo.hasNextPage,
    false,
  );
});

test("a bad cursor is an error and an empty window has no cursor", () => {
  assert.ok(answerHistory(scenario(1), { ...window, after: "nope" }).body.errors);
  const empty = answerHistory(scenario(1), { from: "2030-01-01T00:00:00Z", to: "2031-01-01T00:00:00Z" });
  assert.equal(empty.count, 0);
  assert.equal(
    empty.body.data.viewer.contributionsCollection.pullRequestContributions.pageInfo.endCursor,
    null,
  );
});
