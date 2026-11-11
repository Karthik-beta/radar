# 0007. Client routing without leptos_router

Status: accepted, 2026-11-10.

The UI must fit 200 KB gzipped (`node scripts/check-size.mjs`). With all four views, `origin/ui-wave` measures 234.9 KB. Radar has five fixed paths, no route parameters, no nested routes, and no lazy data per route. `leptos_router` brings nested async route matching, a path stack, and form handling that Radar never uses, and removing it takes the UI from 233.4 KB to 193.1 KB.

## Decision

`crates/web/src/route.rs` replaces `leptos_router`. It parses the path into a `Route` (a `Tab`, `SignIn`, or `NotFound`), keeps the path in one signal, and listens for clicks and `popstate`:

- A left click on a same-origin link with no `target`, no `download`, no `rel="external"`, and no modifier key pushes the link onto the history and updates the path, as `leptos_router` did. Other links load as the browser would.
- Navigation scrolls to the `#hash` target or to the top.
- `/signin` inside the signed-in shell moves to `/`.
- `route::query` reads one query value, for the sign-in error.

## Alternatives

| Option | Why not |
| --- | --- |
| Keep `leptos_router` | Costs about 40 KB gzipped, which the budget cannot hold |
| Nightly `-Zbuild-std` with `panic_immediate_abort` | Needs a toolchain change and leaves the router's cost in place |

## Consequences

Adding a route means adding a `Route` variant and a match arm in `app.rs`. A route with parameters, nested routes, or route-level data loading would need this module to grow, or `leptos_router` to come back with a new budget.
