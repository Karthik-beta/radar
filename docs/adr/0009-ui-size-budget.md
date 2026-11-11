# 0009. UI size budget

Status: accepted, 2026-11-11.

`node scripts/check-size.mjs` fails the build when the gzipped UI passes 200 KB. The four triage views at `origin/ui-wave` measure 189.4 KB. Adding the Feed view takes the UI to 214.8 KB. With Feed, Repos, Trends, and Agents, the UI measures 252.3 KB. The 200 KB budget cannot hold every view in one bundle.

## Decision

The UI budget is 300 KB gzipped. `scripts/check-size.mjs` defaults `--ui-budget-kb` to `300`. The Worker budget stays at 400 KB.

## Alternatives

| Option | Why not |
| --- | --- |
| Keep 200 KB and lazy-load the More views (Feed, Repos, Trends, Agents) from a second wasm module | It needs a second trunk build, a loader, and a split of shared code. Deferred for speed. It stays the next step if the UI nears 300 KB. |
| Drop views until the UI fits 200 KB | Every view is a PRD requirement. |

## Consequences

Every view ships in one bundle, so the first load downloads views the user may not open. The 1.5 s first-load target in the PRD still applies and must be checked. Each new view still adds to one budget, so the next large view should start with the lazy-loading split.

## PRD sections this record replaces

The PRD names no UI size. This record replaces the 200 KB figure in these places:

- [0007. Client routing](0007-client-routing.md), the first paragraph.
- [Implementation plan](../plan/implementation-plan.md), the R01 `scripts/check-size.mjs` task and rule, and the UI size rules of the later plan items that say "passes 200 KB".
