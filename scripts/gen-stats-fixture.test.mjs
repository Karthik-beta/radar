import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";

import { buildData, buildSql, totals } from "./gen-stats-fixture.mjs";

const NOW = Date.parse("2026-11-16T12:00:00Z") / 1000;
const DAY = 86_400;

const migrations = () => {
  const dir = new URL("../migrations/", import.meta.url);
  return readdirSync(dir)
    .filter((name) => name.endsWith(".sql"))
    .sort()
    .map((name) => readFileSync(new URL(name, dir), "utf8"));
};

test("the same time gives the same SQL, one statement per line", () => {
  const sql = buildSql(NOW);
  assert.equal(sql, buildSql(NOW));
  for (const line of sql.trimEnd().split("\n")) {
    assert.ok(line.startsWith("--") || line.endsWith(";"), line);
  }
});

test("the facts reach back two years and every outcome appears", () => {
  const { prs, reviews, transitions, events } = buildData(NOW);
  assert.ok(Math.min(...prs.map((p) => p.created_at)) < NOW - 700 * DAY);
  assert.ok(Math.max(...prs.map((p) => p.created_at)) < NOW);
  const t = totals(NOW);
  assert.equal(t.all_prs.merged + t.all_prs.closed_unmerged + t.all_prs.open, prs.length);
  assert.ok(t.external_prs.merged > 0 && t.external_prs.closed_unmerged > 0 && t.external_prs.open > 0);
  assert.ok(reviews.some((r) => r.external === 0) && reviews.some((r) => r.state === "changes_requested"));
  assert.ok(transitions.length > 0 && events.length > 400);
});

test("a PR that is open counts as open, never as closed", () => {
  const open = buildData(NOW).prs.filter((p) => p.closed_at === null);
  assert.ok(open.length > 0);
  assert.ok(open.every((p) => p.merged_at === null));
});

test("the SQL loads into the real schema and its counts equal the totals", async (t) => {
  let sqlite;
  try {
    sqlite = await import("node:sqlite");
  } catch {
    t.skip("node:sqlite is not available");
    return;
  }
  const db = new sqlite.DatabaseSync(":memory:");
  for (const migration of migrations()) db.exec(migration);
  db.exec(buildSql(NOW));
  db.exec(buildSql(NOW));
  const one = (sql, ...args) => Object.values(db.prepare(sql).get(...args))[0];
  const expected = totals(NOW);

  assert.equal(one("SELECT COUNT(*) FROM pr_facts"), expected.all_prs.prs);
  assert.equal(one("SELECT COUNT(*) FROM pr_facts WHERE external = 1 AND merged_at IS NOT NULL"), expected.external_prs.merged);
  assert.equal(
    one("SELECT COUNT(*) FROM pr_facts WHERE external = 1 AND created_at >= ?", expected.window_from),
    expected.last_12_months.prs,
  );
  assert.equal(
    one(
      "SELECT COUNT(*) FROM pr_facts WHERE external = 1 AND merged_at IS NOT NULL AND created_at >= ?",
      expected.window_from,
    ),
    expected.last_12_months.merged,
  );
  assert.equal(
    one("SELECT COUNT(*) FROM review_facts WHERE external = 0 AND submitted_at >= ?", expected.window_from),
    expected.reviews_last_12_months.own,
  );
  assert.equal(
    one(
      "SELECT COUNT(*) FROM review_facts WHERE external = 1 AND state = 'approved' AND submitted_at >= ?",
      expected.window_from,
    ),
    expected.reviews_last_12_months.approved,
  );
  assert.equal(one("SELECT COUNT(*) FROM pr_transitions"), expected.transitions);
  assert.equal(one("SELECT COUNT(*) FROM pr_issue_links"), expected.issue_links);
  assert.equal(
    one("SELECT (SELECT COUNT(*) FROM events) + (SELECT COUNT(*) FROM own_events)"),
    expected.my_events_total,
  );
  assert.equal(
    one(
      "SELECT COUNT(*) FROM (SELECT repo, MIN(merged_at) m FROM pr_facts WHERE external = 1 AND merged_at IS NOT NULL GROUP BY repo) WHERE m >= ?",
      expected.window_from,
    ),
    expected.new_repos_last_12_months,
  );
  assert.equal(
    one("SELECT COUNT(*) FROM pr_facts p JOIN repos r ON r.full_name = p.repo WHERE p.external = 1 AND p.merged_at >= ? AND r.stars >= 1000", expected.window_from),
    expected.star_merges_last_12_months,
  );
  assert.equal(one("SELECT facts_imported_at FROM import_state"), null);
  assert.ok(one("SELECT account_created_at FROM import_state") < expected.this_year_starts_at);
});

test("--imported sets facts_imported_at", () => {
  assert.match(buildSql(NOW, { imported: true }), new RegExp(`facts_imported_at = ${NOW} `));
});
