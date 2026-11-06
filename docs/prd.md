# Radar — Product Requirements Document

Nov 5, 2026 · @Karthik

## Overview

Radar is a private, read-only dashboard at radar.karthik.nz that shows, in one screen, everything happening across my open-source contributions: who is waiting on me, where my PRs stand, and what my AI agents are working on.

**Problem.** I contribute to several open-source projects at once, often through AI agents running in parallel. GitHub's web UI is built around one repo and one thread at a time. Mentions, review feedback and CI failures are scattered across notifications, PR pages and email, so things that need my attention get missed and stale PRs go unnoticed.

**Product.** A single-user web app, signed in with GitHub OAuth, that ingests GitHub activity event-first (notification emails as push triggers, conditional API polling as reconciliation) and presents it as a few visual views. It never writes to GitHub: every action still happens on github.com, and Radar deep-links there.

**Constraints.** Personal use only, not a SaaS. Zero cost: Cloudflare free tier only. Must respect GitHub's rate limits and polling guidance.

An accepted ADR overrides this document wherever they disagree. See the [ADR index](adr/README.md).

## Goals, non-goals and success metrics

Radar succeeds if I can answer "what needs me right now?" in under 10 seconds, without opening github.com.

**Goals**

1. One screen that shows everything waiting on me across all repos (mentions, review requests, changes requested, CI failures on my PRs).
2. A pipeline view of every PR I (or my agents) have open, with its current stage.
3. Visibility into PRs stalled on maintainers, so I know when a nudge is fair.
4. Contribution trends that show whether my profile is actually getting stronger.
5. Event-first ingestion that stays well inside GitHub's rate limits and Cloudflare's free tier.

**Non-goals**

- Writing to GitHub in any form: no comments, merges, labels, or marking notifications read. Actions happen on github.com.
- Multi-user or SaaS features: sign-ups, billing, teams.
- Private repositories (v1). They would need the full `repo` scope, which includes write access.
- Replacing GitHub's notification inbox. Radar is a glance layer on top.
- Running or controlling agents. Radar only observes what they produce.

**Success metrics**

| Metric | Target |
| --- | --- |
| Time from a GitHub mention to it showing in Radar | Under 2 minutes |
| Items needing me that I miss for more than 48 hours | Zero |
| GitHub API usage | Under 10% of the hourly limit at all times |
| Running cost | ₹0/month |
| Dashboard first load | Under 1.5 seconds |

## User and use cases

There is exactly one user: me, a developer contributing to several open-source projects in parallel, often through AI agents, to build a public track record.

**User stories**

1. When I open Radar in the morning, I see every item where someone is waiting on me, oldest first, so I can clear them before starting new work.
2. When a maintainer requests changes on a PR one of my agents opened, it appears in "Your turn" within minutes, with a link straight to the review.
3. When CI fails on one of my PRs, I see it without opening each PR.
4. When a PR has had no maintainer response for over 7 days, it is flagged so I can decide whether to nudge.
5. When I look at a repo card, I see my open and merged PRs there and how quickly that maintainer usually responds, so I can decide where to spend effort.
6. Once a week, I check my contribution trend (PRs merged, repos contributed to) to see if my profile is growing.
7. When I start an agent on an issue, I can see it as "in progress" before any PR exists, so two agents never take the same issue.
8. On my phone, I can glance at the "Your turn" count and the top items.

## Functional requirements

V1 ships three views (Your turn, Pipeline, Their turn); repo cards, trends and agent activity follow. Every item links to its page on github.com; nothing is actionable inside Radar.

| ID | Requirement | Priority |
| --- | --- | --- |
| F1 | **Your turn** view: every open PR or issue I am part of, mine or someone else's, whose `turn` is `me`, for one of these reasons (mention, team mention, review requested, changes requested on my PR, CI failed on my PR, assigned, replied: a maintainer's plain comment on my PR or issue), sorted by `since`, how long it has waited. F1 and F4 both come from each item's `turn`, as [ADR 0002](adr/0002-architecture.md) describes | P0 |
| F2 | Each item shows repo, title, who acted, what happened, age, and a deep link to the exact comment or review | P0 |
| F3 | **Pipeline** kanban of my open PRs across all repos: Draft, CI running, CI failing, Awaiting review, Changes requested, Approved; plus Merged/Closed in the last 14 days | P0 |
| F4 | **Their turn**: my PRs and issues whose `turn` is `them`, with days since; flagged amber after 7 days and red after 14 (configurable). F1 and F4 both come from each item's `turn`, as [ADR 0002](adr/0002-architecture.md) describes | P0 |
| F5 | Header counters: Your turn, Open PRs, Their turn, Merged this month | P0 |
| F6 | Activity feed: chronological stream of all ingested events, filterable by repo and event type | P1 |
| F7 | **Repo cards**: per repo, my open/merged/closed PR counts, merge rate, median time to first maintainer response, last activity | P1 |
| F8 | **Trends**: PRs opened vs merged per week, distinct repos contributed to, contribution heatmap | P1 |
| F9 | Local "seen" state: I can dismiss or snooze an item in Radar. Stored only in Radar; GitHub's read state is never changed | P0 |
| F10 | Data freshness indicator: last sync time per source, current API budget used | P1 |
| F11 | **Agent activity** lane: work in progress before a PR exists, from pushes to agent branches on my forks (branch prefix `agent/`) and optional agent check-ins to Radar's own API | P2 |
| F12 | Mobile-friendly layout: Your turn first, readable at 375 px width | P1 |
| F13 | Optional phone alert for mentions and review requests only (Telegram bot or ntfy) | P2 |

**Read-only rule.** Radar's GitHub client allows only GET requests and read-only GraphQL queries, enforced in code, so a bug cannot change anything on GitHub. Agent check-ins (F11) write only to Radar's own database.

## Authentication and access

A classic GitHub OAuth App is the right choice, and not only for login: GitHub's notifications endpoint does not accept GitHub App user tokens, GitHub App installation tokens, or fine-grained personal access tokens ([GitHub docs](https://docs.github.com/en/enterprise-server@3.18/rest/activity/notifications)). An OAuth App token with the `notifications` scope is one of the few ways to read them.

**Scopes requested:** `notifications read:user`, nothing else.

| Scope | Why | Note |
| --- | --- | --- |
| (no scope) | Read public repos, PRs, issues, reviews, check runs | Read-only by design ([scopes reference](https://docs.github.com/en/enterprise-cloud@latest/developers/apps/scopes-for-oauth-apps)) |
| `notifications` | Read my notification threads | Also allows marking read and changing subscriptions. GitHub has no read-only variant, so Radar enforces read-only in code |
| `read:user` | Profile and user ID for the login allowlist | Read-only |
| `repo` / `public_repo` | Not requested | Both grant write access to code; out of scope for v1 |

**Requirements**

1. A1. Sign-in with GitHub OAuth (authorization code flow) with a random `state` value checked on callback.
2. A2. Single-user allowlist: only my numeric GitHub user ID may sign in (IDs don't change; usernames can). Anyone else gets a 403 and no session.
3. A3. Session in a signed, `HttpOnly`, `Secure`, `SameSite=Lax` cookie, valid 30 days.
4. A4. The OAuth token is stored encrypted (AES-GCM, key held as a Worker secret) so background jobs can use it when I'm not signed in.
5. A5. On each API response, check the `X-OAuth-Scopes` header. If `notifications` is missing, show a "re-authorize" banner instead of failing silently.
6. A6. Logout clears the session. A "disconnect" action deletes the stored token (revoking the app is done on github.com).
7. A7. Optional second layer: Cloudflare Access (Zero Trust free tier) in front of radar.karthik.nz, with OAuth kept as the app-level check. This is optional operator setup item O5 in the [implementation plan](plan/implementation-plan.md).

## Data ingestion and rate limits

GitHub offers no webhooks on upstream repos I don't own, so Radar uses GitHub's notification emails as its push signal and the API as its source of truth. An email never becomes data by itself: it only triggers a conditional API sync.

**Sources**

| Source | Type | Covers | Cost |
| --- | --- | --- | --- |
| Notification email to `radar@karthik.nz` | Push (trigger only) | Mentions, reviews, comments, CI, state changes on any repo | Free inbound via Cloudflare Email Routing |
| `GET /notifications` with `If-Modified-Since` | Reconcile on trigger + every 10 min | All notification threads | A 304 doesn't count against the rate limit |
| GraphQL enrichment | On change | PR state, draft, review decision, latest reviews, CI rollup, last commenter | \~1 point per batched query |
| GraphQL search `is:pr author:@me` | First sign-in (90-day backfill) + nightly | My PRs that produced no notification | 5,000 points/hour shared |
| Repo webhooks on my own repos and forks | Push (P2) | Pushes to `agent/*` branches, activity on my projects | Free; created by hand in repo settings, no extra scope |

**Email trigger (E)**

1. E1. Set my GitHub notification email to `radar@karthik.nz`. The Email Worker forwards every message to my personal inbox (forwarding to a verified address is free), so I lose nothing.
2. E2. The worker reads headers only, never the body: `X-GitHub-Reason` (mention, review\_requested, author, ci\_activity, state\_change…), `List-Id` (owner/repo) and `X-GitHub-Sender`. GitHub documents these headers as consistent across every notification email ([GitHub docs](https://docs.github.com/en/enterprise-server@3.22/subscriptions-and-notifications/reference/email-notification-headers)).
3. E3. Headers-only keeps it inside the free tier: on Workers Free, Email Workers can fail if they exceed CPU or memory ([Cloudflare docs](https://developers.cloudflare.com/email-routing/limits)). The Email Worker forwards the message, kicks the Syncer, and returns.
4. E4. A spoofed email can only cause one extra sync that returns 304, so email content is never trusted.

**Polling rules (P)**

1. P1. Notifications are polled with `If-Modified-Since`. GitHub says a 304 leaves the rate limit untouched and that the `X-Poll-Interval` header must be obeyed ([GitHub docs](https://docs.GitHub.com/en/rest/activity/notifications)). Radar stores `Last-Modified` and the next-allowed time in the Syncer's storage, not in KV ([ADR 0002](adr/0002-architecture.md)).
2. P2. A burst of emails coalesces into one sync: a trigger arriving before the next-allowed time is deferred, not dropped. A kick makes the poll due at the later of now and the next-allowed time, and only ever moves the Syncer's alarm earlier, so kicks before then share one poll and an earlier enrichment alarm still runs on time. [ADR 0002](adr/0002-architecture.md) replaces the earlier drop rule, because a dropped trigger could wait for the next 10-minute cron.
3. P3. Enrichment fetches only threads whose `updated_at` changed, batched into one GraphQL query per 20 PRs.
4. P4. Every response's `x-ratelimit-remaining` is recorded. Below 20% remaining, Radar stops enrichment until reset; on 403/429 it honors `Retry-After` and backs off exponentially.
5. P5. Requests run sequentially per token (no fan-out), staying far below the secondary limits.

**Rate-limit budget**

OAuth user tokens get 5,000 GraphQL points per hour; secondary limits cap REST at 900 points per minute and GraphQL at 2,000 per minute, with at most 100 concurrent requests ([GitHub docs](https://docs.github.com/graphql/overview/resource-limitations)). Radar caps itself at 500 calls/hour, 10% of the primary limit. A realistic day (about 40 notification events, 144 timed polls mostly returning 304, 40 enrichment batches, one nightly search) is roughly 250 counted calls, about 0.2% of the daily allowance.

## Architecture and data model

Radar runs entirely on Cloudflare's free tier: Email Routing, Workers, Cron Triggers, one Durable Object class named `Syncer` with SQLite storage and alarms, D1, with the frontend served as Workers static assets on radar.karthik.nz.

The diagram of the three triggers and the Syncer is at the top of [ADR 0002](adr/0002-architecture.md).

The email handler, the 10-minute cron, and the webhook endpoint each call `kick(reason)` on the Syncer. The Syncer runs one unit of GitHub work per alarm and is the only part that fetches data from GitHub. The dashboard reads only from D1, so page loads never spend API budget.

**Data model (D1 unless noted)**

| Table | Key fields | Feeds |
| --- | --- | --- |
| `notification_threads` | GitHub thread ID, repo, subject kind, subject title, subject number, reason, `updated_at` | Your turn, feed |
| `items` | node ID, repo, number, `kind`, title, state, draft, review decision, CI state, last human actor, `authored_by_me`, derived `turn`, `reason`, `since`, `stage`, `updated_at`, `closed_at` | Pipeline, Your turn, Their turn |
| `events` | ID = thread ID + `updated_at`, source (email / poll / webhook), kind, repo, actor, `occurred_at` | Feed, idempotency |
| `repos` | full name, my open / merged / closed counts, median first-response hours | Repo cards |
| `rollups_weekly` | week, PRs opened, PRs merged, repos touched | Trends |
| `local_state` | item ID, dismissed-until | Dismiss / snooze |
| `auth` | GitHub user ID, encrypted token, granted scopes | Background sync |

Poll state is not in D1 or KV. It lives in the Syncer's storage, with the `work` rows, the `Last-Modified` header, the next-allowed poll time, and the rate-limit headers. See [ADR 0002](adr/0002-architecture.md).

## Non-functional requirements

Radar must cost ₹0, keep my GitHub token safe, and fail visibly rather than silently.

**Free-tier budget (Cloudflare Workers Free)**

| Resource | Free allowance | Radar's expected use |
| --- | --- | --- |
| Cron Triggers | 5 per account | 2 (10-min reconcile, nightly rollup) |
| Durable Objects | Not listed here. The R02 spike tests the Free-plan fit, see [ADR 0002](adr/0002-architecture.md) | One `Syncer` instance, one unit of GitHub work per alarm |
| D1 | 5 GB storage; 5M rows read, 100k rows written per day | <50 MB; \~50k reads, \~2k writes/day |
| Email Routing inbound | Unlimited | \~40–100 emails/day |
| Workers requests | 100k/day | <2k/day |

Allowances from [Cloudflare plans](https://www.cloudflare.com/plans.md), [Workers limits](https://developers.cloudflare.com/workers/platform/limits/index.md) and [Email Service pricing](https://developers.cloudflare.com/email-service/platform/pricing/). Free-plan limits reset daily; on overrun, operations fail rather than bill, so overrun = stale data, never cost.

**Security and privacy**

- N1. GitHub token encrypted at rest; OAuth client secret, encryption key and session key held only as Worker secrets.
- N2. GitHub client is GET-only and query-only (no GraphQL mutations), enforced by a wrapper and covered by a test.
- N3. Webhook payloads verified with `X-Hub-Signature-256` before processing; unsigned or mismatched requests get 401.
- N4. No third-party analytics or scripts; strict Content-Security-Policy.
- N5. Stored data is limited to metadata Radar displays (titles, states, timestamps, logins), not comment bodies. Snippets are fetched live when needed.
- N6. Retention: events older than 180 days roll up into weekly aggregates and are deleted.

**Performance and reliability**

- N7. Dashboard reads from D1 only, never calls GitHub on page load; first load under 1.5 s.
- N8. The Syncer is idempotent: events are keyed by GitHub thread ID + `updated_at`, so retries and duplicate emails are harmless.
- N9. Failed units of work retry. After 3 failed attempts the Syncer marks the `work` row dead. Dead `work` rows replace the dead-letter queue and show on the freshness panel. [ADR 0002](adr/0002-architecture.md) makes this change.
- N10. If email ingestion stops (no email for 24 h while polls find changes), Radar shows a warning.

**Maintainability**

- N11. Rust throughout, in one Cargo workspace, deployed with Wrangler, with D1 schema in versioned migrations. See [ADR 0005](adr/0005-rust.md).
- N12. The code can live in a public repo as a portfolio piece; no secrets in the repo.

## Milestones, risks and open questions

V1 ("Your turn" + pipeline, live from email triggers) is four milestones away; each ends with something usable.

**Milestones**

1. **M0: Skeleton.** Worker + static frontend on radar.karthik.nz, GitHub OAuth with user-ID allowlist, encrypted token storage, D1 migrations. *Done when* only I can sign in.
2. **M1: Polling core.** The Syncer runs notifications sync with `If-Modified-Since`/`X-Poll-Interval`, 10-min cron, 90-day PR backfill, GraphQL enrichment, rate-limit tracking. *Done when* the DB matches my GitHub state.
3. **M2: Your turn + pipeline (v1).** F1-F5, local dismiss/snooze (F9), freshness panel, mobile layout. *Done when* I use it daily instead of github.com/notifications.
4. **M3: Email push.** Email Routing to the Email Worker, header parsing, forwarding, Syncer kicks, coalescing. *Done when* a test mention appears within 2 minutes.
5. **M4: Insight.** Activity feed, repo cards, trends, nightly rollup. *Done when* the feed, repo cards, and trends show my real data on the preview deploy.
6. **M5: Agent era (optional).** Webhooks on my forks, `agent/*` branch lane, agent check-in API, phone alerts. *Done when* a push to an `agent/` branch on a fork shows in the Agents view within 2 minutes and a mention reaches my phone through Telegram.

**Risks**

| Risk | Impact | Mitigation |
| --- | --- | --- |
| `notifications` scope can also mark threads read and change subscriptions | A bug could alter GitHub state | GET-only client wrapper (N2) plus a test that fails on any non-GET call |
| Notification email turned off or delayed for some event types | Missed push triggers | 10-min poll still catches everything; N10 warns if email goes quiet |
| Changing my GitHub notification email affects all GitHub mail | Personal inbox misses mail if the worker breaks | Worker forwards before anything else; Cloudflare Email Routing can fall back to a plain forward rule |
| Email Worker exceeds free-tier CPU on large emails | Dropped triggers | Headers-only parsing; poll covers gaps |
| `reviewDecision` is empty on repos without required reviews | Wrong pipeline stage | Derive stage from the latest review per reviewer when it is empty |
| Cloudflare free-tier limits change | Possible stale data | Usage far below limits; free plan fails rather than bills |

**Setup notes**

- `radar@karthik.nz` must be added and verified as an email on GitHub before it can be the notification address. The verification mail passes through the worker's forward, so it reaches my inbox.
- In GitHub notification settings, email delivery must be on for participating and @mentions; watching is optional.

**Open questions**

- Should issues I opened or commented on get their own lane, or appear only inside "Your turn" and "Their turn"?
- Should my own repos (where I'm the maintainer) get a separate "Maintainer" view, or stay out of v1?
- Frontend, closed: the options were React + Vite served as Workers static assets, or Next.js on Vercel. The UI is Leptos, built with Trunk and served as Workers static assets. See [ADR 0005](adr/0005-rust.md).
- Alerts channel for F13: decided, Telegram. See [R24](plan/implementation-plan.md#send-phone-alerts-through-telegram-r24) in the implementation plan.
