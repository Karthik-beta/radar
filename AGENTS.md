# Agent guide

Radar is a single-user, read-only dashboard for one person's open-source work, running on Cloudflare's free tier. Read [README.md](README.md) for the overview and [docs/prd.md](docs/prd.md) for the requirements.

## Read before you change anything

- [docs/adr/](docs/adr/README.md) holds every accepted decision. When an ADR disagrees with the PRD, the ADR wins. Each ADR lists the PRD sections it replaces.
- [design/tokens.json](design/tokens.json) holds every colour, size, spacing, radius, and motion value. UI code uses these tokens through Tailwind classes or CSS custom properties and never hard-codes a value. UI components come from rust-ui, copied into the repository, as [ADR 0006](docs/adr/0006-ui-components.md) describes.

## Rules that must not break

- **Radar never writes to GitHub.** The GitHub client exposes only `get` and `query`, never mutations. Do not add a GitHub client crate that can write. The rule is enforced in code and covered by a test.
- **Radar costs nothing to run.** Use only Cloudflare free-tier products. Do not use Workers KV for state that changes on every sync, because the free plan allows 1,000 writes a day. Each Worker invocation gets 10 ms of CPU, so each Syncer alarm runs one unit of GitHub work.
- **Only the Syncer Durable Object calls GitHub.** Page loads read from D1 only.
- **No secrets in the repository.** The repository is public. Secrets live in Wrangler secrets, and local values go in `.dev.vars`, which git ignores.
- **Store metadata, not content.** Keep titles, states, timestamps, and logins. Do not store comment bodies.

## Code

- Rust, in one Cargo workspace. The layout is in [ADR 0005](docs/adr/0005-rust.md).
- `crates/domain/` holds the pure rules (whose turn it is, pipeline stage) and the types the Worker and UI share. It depends on neither `worker` nor `leptos`.
- Every crate must compile to `wasm32-unknown-unknown`. No Tokio, threads, or `std::time::SystemTime`.
- Return `Result` instead of panicking. A panic aborts the Wasm instance.
- Validate external data once, at the boundary where it enters (GitHub responses, email headers, request bodies). Trust the types inside.
- Write a comment only to explain a reason the code cannot show.

Commands, run from the repository root:

- `cargo fmt --all --check` checks formatting.
- `cargo clippy --workspace --all-targets -- -D warnings` lints. The Worker denies `unwrap` and `expect` outside tests.
- `cargo test --workspace` and `node --test "scripts/*.test.mjs"` run the tests.
- `node scripts/build.mjs` builds the UI with `trunk build --release`, then the Worker with `worker-build --release`. Install both with `cargo install --locked trunk@0.22.1 worker-build@0.8.7`.
- `node scripts/check-size.mjs` checks the gzipped bundles against their budgets, 400 KB for the Worker and 200 KB for the UI.
- `npm ci`, then `npx wrangler dev`, serves Radar on `http://localhost:8787`.

The Worker package is `radar-worker`, because a package named `worker` would clash with the `worker` crate, so run `cargo test -p radar-worker`. Lane worktrees may share one `CARGO_TARGET_DIR` to reuse compiled dependencies. `spikes/` holds throwaway measurements, not product code.

## Pull requests

Every change reaches `main` through a pull request. The full rules are in [docs/plan/implementation-plan.md](docs/plan/implementation-plan.md), under **PR mechanics** and **Verdict and merge**. In short:

1. An Opus owner is responsible for each PR from branch to merge.
2. CodeRabbit reviews every PR, configured by [.coderabbit.yaml](.coderabbit.yaml). Answer every comment. Fix what is real, and reply with a reason to what is not.
3. Merge only when CI is green, CodeRabbit is clear, and every other review has passed. CodeRabbit is clear when its `CodeRabbit` check has `conclusion: success` at the head SHA (or it skipped the PR because `path_filters` excludes every changed file, as its review-status comment says), its `Read-only GitHub` and `No secrets` checks pass, and no Critical or Major thread is unresolved. Its explicit approval is not required. Fix real Critical and Major findings. Fix a Minor finding only when it is quick, otherwise reply in one line. Do not wait for CodeRabbit to confirm a fix.
4. Then mark every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>`, submit a review with `gh pr review <number> --comment --body "<verdict, CodeRabbit result, and evidence links>"`, and squash-merge with `gh pr merge <number> --squash --delete-branch`.

These `gh` commands are developer tooling. They do not change the rule that Radar itself never writes to GitHub.

## Decisions

A new decision gets a new ADR. Copy [docs/adr/template.md](docs/adr/template.md) and follow the steps in [docs/adr/README.md](docs/adr/README.md). Do not rewrite an accepted ADR. Supersede it.

## Design tokens

To change a token, edit `design/tokens.json`, run `node scripts/tokens-to-css.mjs` to rewrite `crates/web/style/tokens.css`, then regenerate the wiki page with `node scripts/tokens-to-wiki.mjs <wiki-checkout>` and push the wiki. The README has the full steps.

## Brand assets

The mark, icons, manifest, fonts, and social preview are described in [ADR 0004](docs/adr/0004-brand-assets.md). Edit the sources in `design/brand/`, then run `node scripts/build-assets.mjs`. Do not edit the generated files in `public/` or `design/brand/social-preview.png` by hand. Serve every asset from Radar's own origin, and never load fonts or images from a third-party CDN.

## Writing

Docs, ADRs, and commit messages use short, plain sentences in the present tense. Name the real file, flag, or command, not a description of it.
