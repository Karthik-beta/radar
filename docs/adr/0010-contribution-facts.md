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
| `pr_transitions` | Change of stage or turn on a PR I authored | `item_node_id`, `stage`, `turn`, `at` |
| `pr_issue_links` | Issue a PR of mine closes | `pr_node_id`, `issue_repo`, `issue_number`, `issue_authored_by_me`, `issue_commented_by_me` |
| `own_events` | Event by me that the weekly rollup folded, keyed by the `events` ID | the `events` columns |

`first_response_at` is the first review or comment by a human who is not me, after the PR opened, as R19's `first_response` rule defines it. `changes_requested_rounds` counts reviews with state `changes_requested`.

**New columns.**

- `repos` gains `owned_by_me`, `stars`, `primary_language`, and `meta_refreshed_at`.
- `settings` gains `utc_offset_minutes` (default `0`) for the when-I-work chart, and `goal_merged_per_month` (nullable). Both change with `wrangler d1 execute`, like the nudge thresholds.

**Events by me stay.** The weekly rollup keeps its current loop: it folds the week of the oldest event before the cutoff, then deletes every event in that week, so the next fold reaches the next week and no week is added twice. In the same D1 batch, before the delete, it copies that week's events where `actor_is_me = 1` into `own_events` with `ON CONFLICT (id) DO NOTHING`. My own events, about a few thousand a year, stay for good there. A reader of my own events reads `events` where `actor_is_me = 1` together with `own_events`. The two never hold the same row, because the batch that copies a week also deletes it from `events`, and `INSERT` and `INSERT_MANY` in `crates/worker/src/db/events.rs` skip a row whose `id` is already in `own_events`, with `NOT EXISTS (SELECT 1 FROM own_events WHERE own_events.id = <the row's id>)`. An event that enrichment sees again after its week was folded is dropped, so the next fold never adds it twice. A late event with a new `id` still lands in `events` and folds as it does today.

**How the tables fill.**

- Enrich upserts `pr_facts` and `pr_issue_links` for every PR I authored, from fields it already fetches plus `additions`, `deletions`, `changedFiles`, and `closingIssuesReferences(first: 5)`. It writes a `pr_transitions` row whenever it changes `items.stage` or `items.turn`, so a PR that moves from `them` to `me` while it stays in awaiting review still gets a row. S6 measures the time on each side from consecutive rows.
- Enrich upserts `review_facts` when it sees a `reviewed` event with `actor_is_me = 1`.
- The nightly kick refreshes `repos` metadata once a week, in one batched `nodes` query per 50 repos.
- A one-time import, `WorkKind::ImportFacts`, reads my history back to my account's `createdAt`. One unit per year runs `search(type: ISSUE, query: "is:pr author:<login> created:<year>-01-01..<year>-12-31")`, 50 PRs a page. The search API returns at most 1,000 results per query. When a range's `issueCount` is over 1,000, the unit splits it into two halves by date and enqueues each, and keeps halving until every range is under the limit. Each PR node brings its dates, size, repository owner, stars, language, closing issues, `reviews(states: [CHANGES_REQUESTED]) { totalCount }` for `changes_requested_rounds` from every reviewer, and the first page of `timelineItems(itemTypes: [ISSUE_COMMENT, PULL_REQUEST_REVIEW], first: 20)`. When that page has no human response that is not mine, a follow-up unit pages on with `after` until it finds one or the timeline ends. A second unit kind per year reads `contributionsCollection(from, to) { pullRequestReviewContributions(first: 100) }`. That list holds only my latest review on each PR, so it serves only to find the PRs I reviewed. A page past 100 entries is a follow-up unit with `after`, until the list ends. For the PRs on each page, a follow-up unit then reads `nodes(ids: [...])` for up to 50 of those PRs at a time with `reviews(author: <login>, first: 100)`, pages each PR's reviews with `after`, and writes one `review_facts` row per review. The import writes `source = 'imported'`, uses `ON CONFLICT DO NOTHING` so live rows win, and sets `facts_imported_at` in the Syncer's storage only when no import unit remains: every search range, every contribution page, every review page, and every timeline page. It respects R08's rate-limit gate and the 500 calls per hour cap.
- Imported PRs have no `pr_transitions` and no `ci_failures`. Stage and turn history starts the day this ships.

Every new query is a read. The GitHub client keeps only `get` and `query`, and each new document passes R06's `every_registered_document_is_a_query` test. No table stores a title, body, or comment text beyond what `items` already holds.

## Alternatives

| Option | Why not |
| --- | --- |
| Keep every event for good | `events` holds other people's actions on every thread I follow, most of which no metric needs. It grows fastest and still lacks PR size, closing issues, and history before the backfill. |
| Add more columns to `rollups_weekly` | Weekly totals cannot give medians, percentiles, monthly groups, or year-over-year by month, and every new metric would need a new column and a re-roll that the deleted events can no longer feed. |
| Compute stats live from GitHub on page load | It breaks N7 (page loads read D1 only) and spends API budget each visit. |
| Import full timelines for every past PR | Stage times for old PRs would cost one timeline read per PR. The first-response field covers the main question for a bounded cost. |

## Consequences

Any later metric about my PRs or reviews becomes a query on existing tables. Year-over-year works on the day the import finishes for PR and review metrics, except stage times, turn times, and CI failures, which need a year of live data. Habits is a separate case. Its hourly grid reads my own events, the import does not restore them, and the rollup already deleted my events older than 180 days before this change. Habits year-over-year stays incomplete until `events` and `own_events` hold a full year of my events.

D1 grows by a few MB a year. That is far inside the D1 free plan's per-database limit, but the freshness panel should show the table sizes so growth stays visible. The import costs a few hundred GitHub points once, spread over Syncer alarms. Each import page must stay under 10 ms of CPU, so the page size is measured and lowered if needed.

The ownership rule is a choice, not a fact. If PRs to repos in organisations I run should count as my own, F14's filter changes and the stored data stays.

Check later: how long the import takes for my real history, the CPU per import alarm, and whether `closingIssuesReferences` misses issues linked only by a comment.

## PRD sections this record replaces

- **N6, Retention.** Events by other people older than 180 days roll up and are deleted. My own events and the fact tables above are kept for good.
- **Data model.** Adds `pr_facts`, `review_facts`, `pr_transitions`, `pr_issue_links`, and `own_events`, and the new `repos` and `settings` columns.
- **F8, Trends.** F14 replaces it. The Trends charts move into the Stats page.
