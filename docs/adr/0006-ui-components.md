# 0006. UI components and Tailwind

Status: accepted, 2026-11-06. Supersedes the styling choice in [0005. Rust everywhere](0005-rust.md).

ADR 0005 chose plain CSS on custom properties and left every component to be written by hand. The owner wants Radar to use the Rust ecosystem as fully as it can, and the UI needs a few components that are easy to get wrong: a dropdown menu for snooze, a sheet for the phone menu, and accessible tabs. A component library for Leptos 0.8 saves that work, so the decision is needed before the web shell (R05) is built.

## Decision

| Part | Choice |
| --- | --- |
| Components | [rust-ui](https://leptos.rust-ui.com), the Leptos edition. Its `ui-cli` copies each component into `crates/web/src/components/ui/` as Leptos source that Radar owns. Radar's own components stay in `crates/web/src/components/` |
| Styling | Tailwind CSS v4, which rust-ui needs. Trunk runs Tailwind's standalone binary, so no Node package is added. The Tailwind version is pinned in `crates/web/Trunk.toml` |
| Tokens | `scripts/tokens-to-css.mjs` writes `crates/web/style/tokens.css` as a Tailwind `@theme` block that first clears Tailwind's default palette. Every utility colour, size, and spacing comes from `design/tokens.json`, and `tokens.json` stays the one source |
| Charts | Hand-drawn SVG in Rust, as ADR 0005 says. rust-ui's charts use ApexCharts, a JavaScript library, so Radar does not copy them |
| Browser helpers | `leptos-use` when a PR needs a hook such as a media query or click-outside |
| Icons | `icondata` with `leptos_icons`, compiled into the app as inline SVG |

Rules for copied components:

- Copy only the components a PR uses, with `ui-cli` or by hand from the rust-ui source. Record the rust-ui commit in the PR body.
- Rewrite each copied component's colour classes to Radar's token names, such as `bg-surface` and `text-muted`. Do not add shadcn aliases such as `--color-background`.
- A copied component must work under Radar's CSP, which has no `'unsafe-inline'`. Change it if it sets a `style` attribute or ships a script.
- Radar's design rules win over the copied code. Dark only, colour only for waiting time and failing CI, and the motion tokens from ADR 0003.

## Alternatives

| Option | Why not |
| --- | --- |
| Hand-written components and plain CSS (ADR 0005) | Works, but rebuilds menus, sheets, and focus handling that rust-ui already has |
| Thaw | Pure Rust with no Tailwind, but its latest release, 0.4.8, targets Leptos 0.7. Its Fluent look also fights the Linear and Vercel feel in ADR 0003 |
| leptos-shadcn-ui | Supports Leptos 0.8, but it is young and also needs Tailwind |
| leptonic, leptix | No recent releases |
| rust-ui's charts | Pull in ApexCharts, a JavaScript library |
| leptos-chartistry | Pure Rust charts, but last pushed in June 2024, before Leptos 0.8 |

## Consequences

- Building the UI needs the Tailwind standalone binary. Trunk downloads it on the first build. R05 proves this works with Tailwind v4 and pins the version. If Trunk cannot run Tailwind v4, `scripts/build.mjs` runs the standalone binary itself.
- Node still runs only Wrangler and `scripts/`, as ADR 0005 says.
- Tailwind writes one static stylesheet, so the CSP keeps `style-src 'self'`.
- Copied components are Radar's code. They get no upstream fixes for free, and nothing breaks when rust-ui changes. The Leptos edition of rust-ui is small and young, so this is the safer trade.
- A class that names a colour outside the tokens, or an arbitrary value such as `bg-[#123456]`, fails a test from R05 on.
- The `faint` contrast record that R17 writes becomes ADR 0007.

## PRD sections this record replaces

None. The PRD names no styling tool.
