# 0010. Contribution facts kept for good

Status: proposed, 2026-11-15.

F14 adds a Stats page with 12 months of detail and a comparison of this year with last year. Today Radar cannot hold that. N6 deletes `events` after 180 days and keeps only four weekly totals in `rollups_weekly`: PRs opened, PRs merged, repos touched, and events. Reviews I give, time to merge, time to first response, stage times, PR size, and when I work are all lost after 180 days. History before the 90-day backfill holds only the seeded weekly totals. The data must start collecting now, and in a shape that later metrics can reuse without another migration each time.

## Decision

Radar keeps one row per contribution fact, for good, in new D1 tables. Raw events from other people still roll up and are deleted after 180 days.

**What counts.** An external repo is a repo whose owner login is not my login. Repos owned by an organisation I belong to count as external. Every table stores all my PRs and reviews with an `external` flag, and F14's metrics filter on it. A later change to the rule needs no new data.

**New tables, never pruned.**

| Table | One row per | Fields |
| --- | --- | --- |
| `pr_facts` | PR I authored, keyed by node ID | `repo`, `external`, `number`, `created_at`, `merged_at`, `closed_at`, `first_response_at`, `first_response_login`, `changes_requested_rounds`, `ci_failures`, `additions`, `deletions`, `changed_files`, `agent` (head ref starts with `agent/`), `source` (`live` or `imported`) |
| `review_facts` | Review I submitted, keyed by review node ID | `repo`, `external`, `pr_number`, `pr_author_login`, `state`, `submitted_at`, `source` |
| `stage_changes` | Stage change on a PR I authored | `item_node_id`, `stage`, `at` |
| `pr_issue_links` | Issue a PR of mine closes | `pr_node_id`, `issue_repo`, `issue_number`, `issue_authored_by_me`, `issue_commented_by_me` |

`first_response_at` is the first review or comment by a human who is not me, after the PR opened, as R19's `first_response` rule defines it. `changes_requested_rounds` counts reviews with state `changes_requested`.

**New columns.**

- `repos` gains `owned_by_me`, `stars`, `primary_language`, and `meta_refreshed_at`.
- `settings` gains `utc_offset_minutes` (default `0`) for the when-I-work chart, and `goal_merged_per_month` (nullable). Both change with `wrangler d1 execute`, like the nudge thresholds.

**Events by me stay.** The weekly rollup deletes only events where `actor_is_me = 0`. My own events, about a few thousand a year, stay for good. They feed the when-I-work chart and any later metric about my own actions.

**How the tables fill.**

- Enrich upserts `pr_facts` and `pr_issue_links` for every PR I authored, from fields it already fetches plus `additions`, `deletions`, `changedFiles`, and `closingIssuesReferences(first: 5)`. It writes a `stage_changes` row whenever it changes `items.stage`.
- Enrich upserts `review_facts` when it sees a `reviewed` event with `actor_is_me = 1`.
- The nightly kick refreshes `repos` metadata once a week, in one batched `nodes` query per 50 repos.
- A one-time import, `WorkKind::ImportFacts`, reads my history back to my account's `createdAt`. One unit per year runs `search(type: ISSUE, query: "is:pr author:<login> created:<year>")`, 50 PRs a page. The search API returns at most 1,000 results per query, so a year with more splits into quarters. Each PR node brings its dates, size, repository owner, stars, language, closing issues, and `timelineItems(itemTypes: [ISSUE_COMMENT, PULL_REQUEST_REVIEW], first: 10)` for the first response. A second unit kind per year reads `contributionsCollection(from, to) { pullRequestReviewContributions(first: 100) }` for `review_facts`. The import writes `source = 'imported'`, uses `ON CONFLICT DO NOTHING` so live rows win, and sets `facts_imported_at` in the Syncer's storage when it finishes. It respects R08's rate-limit gate and the 500 calls per hour cap.
- Imported PRs have no `stage_changes` and no `ci_failures`. Stage history starts the day this ships.

Every new query is a read. The GitHub client keeps only `get` and `query`, and each new document passes R06's `every_registered_document_is_a_query` test. No table stores a title, body, or comment text beyond what `items` already holds.

## Alternatives

| Option | Why not |
| --- | --- |
| Keep every event for good | `events` holds other people's actions on every thread I follow, most of which no metric needs. It grows fastest and still lacks PR size, closing issues, and history before the backfill. |
| Add more columns to `rollups_weekly` | Weekly totals cannot give medians, percentiles, monthly groups, or year-over-year by month, and every new metric would need a new column and a re-roll that the deleted events can no longer feed. |
| Compute stats live from GitHub on page load | It breaks N7 (page loads read D1 only) and spends API budget each visit. |
| Import full timelines for every past PR | Stage times for old PRs would cost one timeline read per PR. The first-response field covers the main question for a bounded cost. |

## Consequences

Any later metric about my PRs or reviews becomes a query on existing tables. Year-over-year works on the day the import finishes, for every metric except stage times and CI failures, which need a year of live data.

D1 grows by a few MB a year. That is far inside the D1 free plan's per-database limit, but the freshness panel should show the table sizes so growth stays visible. The import costs a few hundred GitHub points once, spread over Syncer alarms. Each import page must stay under 10 ms of CPU, so the page size is measured and lowered if needed.

The ownership rule is a choice, not a fact. If PRs to repos in organisations I run should count as my own, F14's filter changes and the stored data stays.

Check later: how long the import takes for my real history, the CPU per import alarm, and whether `closingIssuesReferences` misses issues linked only by a comment.

## PRD sections this record replaces

- **N6, Retention.** Events by other people older than 180 days roll up and are deleted. My own events and the fact tables above are kept for good.
- **Data model.** Adds `pr_facts`, `review_facts`, `stage_changes`, and `pr_issue_links`, and the new `repos` and `settings` columns.
- **F8, Trends.** F14 replaces it. The Trends charts move into the Stats page.
