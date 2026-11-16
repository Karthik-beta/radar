// Writes SQL for two years of contribution facts with known totals, for the Stats API (R28).
//   node scripts/gen-stats-fixture.mjs > fixture.sql          SQL for `wrangler d1 execute radar --local --file`
//   node scripts/gen-stats-fixture.mjs --totals               the totals the SQL holds, as JSON
//   node scripts/gen-stats-fixture.mjs --now 2026-11-16T12:00:00Z --imported
// `--imported` also sets import_state.facts_imported_at. The SQL replaces the fixture's own rows, so loading
// it twice gives the same database. Nothing here touches a remote database.
import { parseArgs } from "node:util";
import { fileURLToPath } from "node:url";

const DAY = 86_400;
const HOUR = 3_600;
const EXTERNAL = [
  { name: "acme/alpha", stars: 5000, language: "Rust" },
  { name: "acme/beta", stars: 1200, language: "Go" },
  { name: "lib/gamma", stars: 300, language: "Rust" },
  { name: "lib/delta", stars: 50, language: "TypeScript" },
  { name: "org/epsilon", stars: 2000, language: "Python" },
  { name: "org/zeta", stars: null, language: null },
  { name: "org/eta", stars: 800, language: "Java" },
  { name: "org/theta", stars: 1500, language: "C" },
];
const OWN = { name: "karthik/own", stars: 10, language: "Rust" };
const REVIEW_STATES = ["approved", "commented", "changes_requested", "approved", "commented", "dismissed"];
const SIZES = [
  [10, 5],
  [30, 10],
  [90, 20],
  [150, 40],
  [300, 100],
  [600, 50],
];

const ts = (iso) => Math.floor(Date.parse(iso) / 1000);
const monthStart = (sec, back = 0) => {
  const d = new Date(sec * 1000);
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth() - back, 1) / 1000;
};
const yearStart = (sec, back = 0) => Date.UTC(new Date(sec * 1000).getUTCFullYear() - back, 0, 1) / 1000;
const monday = (sec) => sec - ((((sec - 345_600) % (7 * DAY)) + 7 * DAY) % (7 * DAY));

/** Builds every row, deterministically, from `now` in epoch seconds. */
export function buildData(now) {
  const liveSince = now - 120 * DAY;
  const prs = [];
  for (let m = 0; m < 24; m += 1) {
    const start = monthStart(now, 23 - m);
    const count = 3 + (m % 3);
    for (let k = 0; k < count; k += 1) {
      const idx = prs.length;
      const own = idx % 8 === 7;
      const pool = Math.min(EXTERNAL.length, 2 + Math.floor(m / 3));
      const repo = own ? OWN : EXTERNAL[(m + k) % pool];
      const createdAt = start + (k * 5 + 1) * DAY + 10 * HOUR;
      if (createdAt >= now) continue;
      const outcome = idx % 5;
      let mergedAt = null;
      let closedAt = null;
      if (outcome <= 2) mergedAt = createdAt + (2 + (idx % 4)) * DAY;
      else if (outcome === 3) closedAt = createdAt + 3 * DAY;
      else if (createdAt < now - 90 * DAY) closedAt = createdAt + 5 * DAY;
      if ((mergedAt ?? closedAt ?? 0) > now) {
        mergedAt = null;
        closedAt = null;
      }
      const live = createdAt >= liveSince;
      const [additions, deletions] = SIZES[idx % SIZES.length];
      prs.push({
        node_id: `PR_fx_${idx}`,
        repo: repo.name,
        external: own ? 0 : 1,
        number: idx + 1,
        created_at: createdAt,
        merged_at: mergedAt,
        closed_at: closedAt ?? mergedAt,
        first_response_at: idx % 7 === 0 ? null : createdAt + (2 + (idx % 6)) * HOUR,
        first_response_login: idx % 7 === 0 ? null : "maintainer",
        changes_requested_rounds: idx % 8 === 0 ? 2 : idx % 4 === 0 ? 1 : 0,
        ci_failures: live && idx % 3 === 0 ? 1 : 0,
        additions,
        deletions,
        changed_files: 1 + (idx % 7),
        agent: idx % 4 === 3 ? 1 : 0,
        source: live ? "live" : "imported",
      });
    }
  }

  const transitions = [];
  for (const pr of prs.filter((p) => p.source === "live")) {
    const row = (stage, turn, at) => transitions.push({ item_node_id: pr.node_id, stage, turn, at });
    row("ci_running", "them", pr.created_at);
    row("awaiting_review", "them", pr.created_at + HOUR);
    if (pr.closed_at === null || pr.closed_at > pr.created_at + DAY) {
      row("awaiting_review", "me", pr.created_at + DAY);
    }
    if (pr.merged_at !== null) row("merged", "them", pr.merged_at);
    else if (pr.closed_at !== null) row("closed", "them", pr.closed_at);
  }

  const reviews = [];
  for (let m = 0; m < 24; m += 1) {
    const start = monthStart(now, 23 - m);
    for (let k = 0; k < 3 + (m % 2); k += 1) {
      const idx = reviews.length;
      const own = idx % 5 === 4;
      const submittedAt = start + (k * 6 + 2) * DAY + 14 * HOUR;
      if (submittedAt >= now) continue;
      reviews.push({
        review_id: `RV_fx_${idx}`,
        repo: own ? OWN.name : EXTERNAL[idx % EXTERNAL.length].name,
        external: own ? 0 : 1,
        pr_number: 500 + idx,
        pr_author_login: "someone",
        state: REVIEW_STATES[idx % REVIEW_STATES.length],
        submitted_at: submittedAt,
        source: submittedAt >= liveSince ? "live" : "imported",
      });
    }
  }

  const links = prs
    .filter((p) => p.external === 1 && p.merged_at !== null && p.number % 3 === 0)
    .map((p) => ({
      pr_node_id: p.node_id,
      issue_repo: p.repo,
      issue_number: 1000 + p.number,
      issue_authored_by_me: p.number % 2,
      issue_commented_by_me: p.number % 2 === 0 ? 1 : 0,
    }));

  const events = [];
  const cutoff = now - 150 * DAY;
  const first = monday(now - 730 * DAY);
  for (let d = 0; first + d * DAY < now; d += 1) {
    if (d % 3 === 2) continue;
    const at = first + d * DAY + (9 + (d % 9)) * HOUR + (d % 4) * 900;
    if (at >= now) continue;
    events.push({
      id: `fx_ev_${d}`,
      item_node_id: prs[d % prs.length].node_id,
      repo: EXTERNAL[d % EXTERNAL.length].name,
      kind: "commented",
      actor_login: "karthik",
      actor_is_me: 1,
      actor_is_bot: 0,
      occurred_at: at,
      source: "poll",
      ingested_at: at,
      table: at >= cutoff ? "events" : "own_events",
    });
  }

  // Trends reads `items`, so the pull requests of the last 52 weeks also exist there.
  const trendsFrom = monday(now) - 51 * 7 * DAY;
  const items = prs.filter((p) => p.created_at >= trendsFrom || (p.merged_at ?? p.closed_at ?? 0) >= trendsFrom);
  return { prs, transitions, reviews, links, events, items, liveSince };
}

const q = (value) => {
  if (value === null || value === undefined) return "NULL";
  if (typeof value === "number") return String(value);
  return `'${String(value).replaceAll("'", "''")}'`;
};

function insert(table, rows, columns = Object.keys(rows[0] ?? {})) {
  return rows.map((row) => `INSERT INTO ${table} (${columns.join(", ")}) VALUES (${columns.map((c) => q(row[c])).join(", ")});`);
}

/** The totals the SQL holds, so a smoke test can compare the API with known numbers. */
export function totals(now, data = buildData(now)) {
  const from = monthStart(now, 11);
  const thisYear = yearStart(now);
  const external = data.prs.filter((p) => p.external === 1);
  const tally = (rows) => {
    const merged = rows.filter((p) => p.merged_at !== null).length;
    const closedUnmerged = rows.filter((p) => p.merged_at === null && p.closed_at !== null).length;
    return {
      prs: rows.length,
      merged,
      closed_unmerged: closedUnmerged,
      open: rows.length - merged - closedUnmerged,
      merge_rate: merged + closedUnmerged === 0 ? null : merged / (merged + closedUnmerged),
    };
  };
  const cohort = external.filter((p) => p.created_at >= from);
  const reviews = data.reviews.filter((r) => r.submitted_at >= from);
  const months = (yearBack) =>
    Array.from({ length: 12 }, (_, i) => {
      const year = new Date(now * 1000).getUTCFullYear() - yearBack;
      const start = Date.UTC(year, i, 1) / 1000;
      const inMonth = (at) => at >= start && at < Date.UTC(year, i + 1, 1) / 1000;
      return {
        month: i + 1,
        opened: external.filter((p) => inMonth(p.created_at)).length,
        merged: external.filter((p) => p.merged_at !== null && inMonth(p.merged_at)).length,
      };
    });
  const firstMerge = new Map();
  for (const p of external.filter((p) => p.merged_at !== null)) {
    firstMerge.set(p.repo, Math.min(firstMerge.get(p.repo) ?? Infinity, p.merged_at));
  }
  const stars = new Map(EXTERNAL.map((r) => [r.name, r.stars]));
  const mine = data.events.filter((e) => e.occurred_at >= from);
  return {
    now,
    window_from: from,
    all_prs: tally(data.prs),
    external_prs: tally(external),
    last_12_months: tally(cohort),
    reviews_last_12_months: {
      external: reviews.filter((r) => r.external === 1).length,
      own: reviews.filter((r) => r.external === 0).length,
      approved: reviews.filter((r) => r.external === 1 && r.state === "approved").length,
      changes_requested: reviews.filter((r) => r.external === 1 && r.state === "changes_requested").length,
      commented: reviews.filter((r) => r.external === 1 && r.state === "commented").length,
    },
    new_repos_last_12_months: [...firstMerge.values()].filter((at) => at >= from).length,
    star_merges_last_12_months: external.filter(
      (p) => p.merged_at !== null && p.merged_at >= from && (stars.get(p.repo) ?? 0) >= 1000,
    ).length,
    my_events_last_12_months: mine.length,
    my_events_total: data.events.length,
    transitions: data.transitions.length,
    issue_links: data.links.length,
    this_year: months(0),
    last_year: months(1),
    this_year_starts_at: thisYear,
  };
}

export function buildSql(now, { imported = false } = {}) {
  const data = buildData(now);
  const repoRow = (repo, owned) => {
    const mine = data.prs.filter((p) => p.repo === repo.name);
    return {
      full_name: repo.name,
      open_prs: mine.filter((p) => p.closed_at === null).length,
      merged_prs: mine.filter((p) => p.merged_at !== null).length,
      closed_prs: mine.filter((p) => p.closed_at !== null && p.merged_at === null).length,
      updated_at: now,
      owned_by_me: owned,
      stars: repo.stars,
      primary_language: repo.language,
    };
  };
  const eventColumns = [
    "id",
    "item_node_id",
    "repo",
    "kind",
    "actor_login",
    "actor_is_me",
    "actor_is_bot",
    "occurred_at",
    "source",
    "ingested_at",
  ];
  const itemRow = (p) => {
    const open = p.closed_at === null;
    return {
      node_id: p.node_id,
      repo: p.repo,
      number: p.number,
      kind: "pr",
      title: `Fixture PR ${p.number}`,
      url: `https://github.com/${p.repo}/pull/${p.number}`,
      author_login: "karthik",
      author_kind: "user",
      authored_by_me: 1,
      state: open ? "open" : p.merged_at !== null ? "merged" : "closed",
      created_at: p.created_at,
      updated_at: p.closed_at ?? p.created_at,
      closed_at: p.closed_at,
      turn: "them",
      since: p.created_at,
      stage: open ? "awaiting_review" : p.merged_at !== null ? "merged" : "closed",
      synced_at: now,
    };
  };
  const owned = (repo) => repoRow(repo, repo === OWN ? 1 : 0);
  const lines = [
    "-- Generated by scripts/gen-stats-fixture.mjs. Replaces the fixture rows, so a second load changes nothing.",
    "DELETE FROM pr_facts WHERE node_id LIKE 'PR_fx_%';",
    "DELETE FROM review_facts WHERE review_id LIKE 'RV_fx_%';",
    "DELETE FROM pr_transitions WHERE item_node_id LIKE 'PR_fx_%';",
    "DELETE FROM pr_issue_links WHERE pr_node_id LIKE 'PR_fx_%';",
    "DELETE FROM events WHERE id LIKE 'fx_ev_%';",
    "DELETE FROM own_events WHERE id LIKE 'fx_ev_%';",
    "DELETE FROM items WHERE node_id LIKE 'PR_fx_%';",
    `DELETE FROM repos WHERE full_name IN (${[...EXTERNAL, OWN].map((r) => q(r.name)).join(", ")});`,
    ...insert("repos", [...EXTERNAL.map(owned), owned(OWN)]),
    ...insert("pr_facts", data.prs),
    ...insert("review_facts", data.reviews),
    ...insert("pr_transitions", data.transitions),
    ...insert("pr_issue_links", data.links),
    ...insert("events", data.events.filter((e) => e.table === "events"), eventColumns),
    ...insert("own_events", data.events.filter((e) => e.table === "own_events"), eventColumns),
    ...insert("items", data.items.map(itemRow)),
    `UPDATE import_state SET account_created_at = ${yearStart(now, 3)}, facts_imported_at = ${imported ? now : "NULL"} WHERE id = 1;`,
  ];
  return lines.filter(Boolean).join("\n").concat("\n");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({
    options: {
      now: { type: "string" },
      totals: { type: "boolean", default: false },
      imported: { type: "boolean", default: false },
    },
  });
  const now = values.now ? ts(values.now) : Math.floor(Date.now() / 1000 / HOUR) * HOUR;
  if (!Number.isFinite(now)) {
    console.error(`--now must be an ISO timestamp, got ${values.now}`);
    process.exit(2);
  }
  process.stdout.write(values.totals ? `${JSON.stringify(totals(now), null, 2)}\n` : buildSql(now, { imported: values.imported }));
}
