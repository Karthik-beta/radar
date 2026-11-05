# 0002. Architecture

Status: accepted, 2026-11-05. Builds on [0001. Tech stack](0001-tech-stack.md).

Radar has one Worker script and one Durable Object class. The Worker takes triggers and serves the dashboard. The Durable Object, `Syncer`, is the only code that talks to GitHub.

```
notification email ─► email()     ─┐
10-min cron        ─► scheduled() ─┼─► Syncer (Durable Object): kick(reason)
webhook (P2)       ─► fetch /hooks ┘      │ alarm runs one unit of work
                                          ├─► GitHub (GET and query only)
                                          └─► D1 (facts plus derived view columns)
browser ─► fetch /api/* ─► D1 (read only, never GitHub)
```

## Decisions

### A single Syncer owns all GitHub traffic

The PRD requires one writer per token (P5), coalesced bursts (P2), and strongly consistent poll state (P1, P4). One Durable Object instance handles one event at a time and has strongly consistent storage, so it meets all three by design rather than by configuration. It replaces the PRD's Queue and KV.

The Syncer keeps a `work` table in its own storage. Each row is one unit of work, keyed by kind and target:

- `poll_notifications`
- `enrich` (one batch of up to 20 PRs or issues)
- `backfill_page` (one page of the 90-day search, with its cursor)
- `rollup` (nightly)

Each row has a due time and an attempt count. The alarm fires at the earliest due row, runs that one unit, writes the results to D1, and adds follow-up rows. A row is deleted only when its unit succeeds. After 3 failed attempts it is marked dead, and the freshness panel shows it (N9).

Sync state such as `Last-Modified`, the next allowed poll time, and the rate-limit headers lives in the Syncer's storage. The freshness panel reads it through an RPC call.

The 10-minute cron calls `kick('reconcile')`. It is a watchdog. The Syncer decides whether a poll is due.

**Fallback.** One older Cloudflare page says Durable Objects need the paid plan, and newer pages say SQLite-backed objects are free. M0 starts with a spike that deploys an alarm on the free account. If that fails, the Syncer's logic moves into a Queue consumer with `max_concurrency = 1` and its state moves to a D1 row. Nothing else in this record changes.

### Triggers are deferred, not dropped

This replaces PRD rule P2. A kick sets the alarm to the later of now and the next allowed poll time. Several kicks before that time collapse into one alarm. Dropping an early trigger, as P2 says, would leave a mention waiting for the next 10-minute cron and break the 2-minute target.

### One unit of GitHub work per invocation

Workers Free allows 10 ms of CPU per invocation, alarms included. Parsing and validating GraphQL responses counts against that, and waiting on the network does not. Each alarm runs one row from the `work` table, which bounds CPU time and makes the backfill resumable.

### Whose turn it is

Needs me (F1) and Waiting on others (F4) are two answers to one question: for each open PR or issue I am part of, whose move is it? Each item gets:

- `turn`: `me` or `them`
- `reason`: `review_requested`, `changes_requested`, `ci_failed`, `mentioned`, `assigned`, or `replied`
- `since`: when the turn last changed

`replied` covers a maintainer's plain comment on my PR or issue, which F1 does not list. Activity by bot accounts (`author.__typename == "Bot"`) never changes the turn.

An item leaves Needs me when the turn flips to `them`, when the item closes, or when I dismiss it in Radar. Reading the thread on github.com does not clear it, because reading is not handling. So dismiss and snooze (F9) move from P1 to P0, or mentions that need no reply would never leave the list.

### Derived columns are computed at write time

A pure function, `classify(item, me)`, runs in the Syncer on every write. It sets `turn`, `reason`, `since`, and the pipeline `stage`. When `reviewDecision` is empty, `stage` comes from the latest review by each reviewer. The dashboard makes one request, `GET /api/dashboard`, which is one D1 batch of indexed selects. The SPA fetches it again when the tab gets focus and every 60 seconds.

## Code layout

```
src/
  domain/   pure rules: turn, stage, classify; plain types only
  github/   GET-only client, typed queries, valibot parsers
  sync/     Syncer Durable Object and work items
  api/      Hono routes
  auth/     OAuth flow, sessions, token encryption
  db/       Drizzle schema
  web/      React SPA
  worker.ts exports fetch, scheduled, email, and Syncer
migrations/
```

`src/domain/` imports nothing from Cloudflare, so its tests run on plain fixtures.

## PRD sections this record replaces

- Architecture: the Queue and KV `sync:*` are replaced by the Syncer.
- P2: triggers are deferred, not dropped.
- N9: dead work rows replace the dead-letter queue.
- F1 and F4: both are derived from `turn`, and F1 gains `replied`.
- F9: moves from P1 to P0.
