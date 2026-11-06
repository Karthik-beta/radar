# Radar implementation plan

This plan builds Radar from an empty Cargo workspace to every feature in the [PRD](../prd.md), following ADRs [0001](../adr/0001-tech-stack.md) to [0005](../adr/0005-rust.md).
It is for one operator, Karthik, and the subagents that work for him.
The rule the program enforces is that Radar never writes to GitHub and never costs money, and that every PR proves its behavior on the real surface before it is handed over.
PRs merge one at a time in the order R01, R02, R03, R04, R06, R05, R07, then R08 to R24 in number order. The next owner may start early work on a branch from a PR that waits on review, but never merges first. Each goes through its own pull request, an Opus owner, CodeRabbit review, and a squash merge to `main`. M0 is R01 to R07. M1 is R08 to R11. M2, the v1 release, is R12 to R17. M3 is R18 and R19. M4 is R20 to R22. M5 is R23 and R24. R25 is a docs fix that can run at any time.

## How to read this

One box is one unit of work. Every box names the evidence that checks it. A nested box is a sub-step of the box above it. Check a box only when its evidence exists, a file, a log line, a screenshot, a test run, or a SHA. The body is a how-to. The appendices explain and record.

The program runs `playbooks/autopilot-full.md` from the pstack plugin (`~/.claude/plugins/cache/cursor-plugins/pstack/<version>/skills/poteto-mode/playbooks/autopilot-full.md`). Every R goes through a pull request. An Opus owner is responsible for each PR from branch to merge. When every review passes, the owner marks every changed file as Viewed, submits a review with a comment, and squash-merges the PR to `main`. Every deploy to Cloudflare stays an operator item, because a deploy is an irreversible write.

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

## Program checklist

### Arm the program

- [ ] State the protocol and this plan to the operator, then stop. Start execution only on the operator's explicit go.
- [ ] Finish the operator's setup items before R02 and R07 need them. Each is done by the operator, not an agent.
  - [ ] O1. The `karthik.nz` zone is active on the operator's Cloudflare account. Evidence is the zone listed by `npx wrangler whoami` and the Cloudflare dashboard.
  - [ ] O2. Two GitHub OAuth Apps exist. "Radar" has callback `https://radar.karthik.nz/auth/callback`. "Radar dev" has callback `http://localhost:8787/auth/callback`. Evidence is both client IDs recorded in `.dev.vars` and in Wrangler secrets.
  - [ ] O3. `npx wrangler login` has run, the D1 database exists from `npx wrangler d1 create radar`, and each secret is set with `npx wrangler secret put` before the PR that needs it. `OAUTH_CLIENT_ID`, `OAUTH_CLIENT_SECRET`, `TOKEN_KEY`, `SESSION_KEY`, and `ALLOWED_USER_ID` are for R07. `FORWARD_TO` is for R18. `WEBHOOK_SECRET` and `AGENT_TOKEN` are for R23, each at least 32 random bytes. `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` are for R24. Evidence is `npx wrangler secret list`.
  - [ ] O3b. Before R07's perf box, the `radar-preview` Worker from R01's `[env.preview]` has its own D1 database, the same secrets set with `--env preview` as each PR needs them, and a third OAuth App "Radar preview" whose callback is the preview Worker's `/auth/callback`. Evidence is `npx wrangler secret list --env preview`.
  - [ ] O4. Before R18, Email Routing sends `radar@karthik.nz` to the Worker, the address is verified on GitHub, and it is the GitHub notification email. The `FORWARD_TO` address is a verified destination address in Email Routing. Evidence is a GitHub test notification arriving in the operator's inbox through the forward, and the destination listed as verified in the Cloudflare dashboard.
  - [ ] O5. Optional, any time after R07. Cloudflare Access protects `radar.karthik.nz` except `/hooks/*` (PRD A7).
  - [ ] O6. Upload `design/brand/social-preview.png` in the repository settings.
- [ ] Read these from trunk at program start. Re-read them at every tick.
  - [ ] `git show origin/main:AGENTS.md`
  - [ ] `git show origin/main:docs/plan/implementation-plan.md`
  - [ ] `git show origin/main:docs/adr/0002-architecture.md`
  - [ ] `git show origin/main:docs/adr/0005-rust.md`
  - [ ] The pstack files `playbooks/autopilot-full.md`, `playbooks/opening-a-pr.md`, and `skills/swarm/SKILL.md`, read from the installed plugin.
- [ ] On the operator's go, arm the audit tick as `/loop 1h` with the tick prompt below. Never leave the cadence to memory.
- [ ] Use this tick prompt, verbatim. "Re-read the execution playbook from trunk. Audit the operation against it and fix drift in this tick. Probe every active lane and judge progress by side effects only. Stand down a stuck lane and dispatch its replacement now. Then post a short status message to the operator in chat only when the audit found a tracked change that no earlier status message reported, such as a PR opened, a code-ready head, a round launched or closed, a verdict, a merge, a stuck agent and the action taken, a blocker added or cleared, or a decision only the operator can make. Name every such change and nothing else. Do not repeat a table, the merged list, or an unchanged blocker. If the audit found none, end the turn with no reply text. Either way, log this tick's row in your decision trail. The row names the items reported, or none."
- [ ] On the operator's hold or stand-down, send every owner a zero-writes order at once.

### Spawn owners

- [ ] Spawn one Opus owner per PR, at high effort, with `subagent_type` `pstack:poteto-agent`. The owner is responsible for the PR from branch to merge. It reviews every diff its writers produce, answers every review comment, and performs the merge. The models follow `~/.claude/rules/pstack-models.md`.
- [ ] The owner delegates code writing by difficulty and reviews the result before it is pushed.
  - [ ] Opus writes the code where a subtle mistake is expensive. These are R02, R03, R07, R08, R12, and R23. R12 and R23 hold API auth, CSRF checks, and webhook verification.
  - [ ] Sonnet, high effort, writes the feature PRs. These are R01, R04, R05, R06, R09 to R11, R13 to R18, R20 to R22, and R24.
  - [ ] Haiku, high effort, writes the small mechanical PRs. These are R19 and R25.
  - [ ] Explorers that only read code run on Haiku. Swarm verification lanes run on Sonnet. Audits and verdicts run on Opus.
- [ ] Follow this order. Each PR branches from `main` after the PR before it has merged.
  - [ ] The order is R01, R02, R03, R04, R06, R05, R07, R08, R09, R10, R11, R12, R13, R14, R15, R16, R17, R18, R19, R20, R21, R22, R23, R24.
  - [ ] While a PR waits on review, the next owner may start on a branch from the waiting PR. It rebases onto `main` and retargets `main` as soon as that PR merges, and it never merges first.
  - [ ] R25 is a docs fix that branches from `main` and can run at any time.
  - [ ] R08 starts only after R02 has merged with its result in the ADRs.
- [ ] Tooling PRs outside R01 to R25, such as the CodeRabbit and CI setup, follow the same review and merge rules. They need no plan section, and their title has no R id.
  - [ ] Their verification is what can run for the change. CI's `docs` job passes, CodeRabbit approves, and one verifier runs the changed script or workflow and records the output in the PR. The Cargo checks apply once `Cargo.toml` exists. The ten live lanes and the perf lane apply only to R PRs. If R02 finds that Durable Object alarms do not run on Workers Free, R08 builds the Queue fallback from ADR 0002 instead.
- [ ] Hold the file boundaries. Two owners never edit the same file in parallel.
  - [ ] `crates/domain/**` belongs to R03. Later PRs add to it only in PR order, never in parallel.
  - [ ] `migrations/**` gains one new numbered file per PR, with the next free number on `main`. No PR edits an earlier migration.
  - [ ] `crates/worker/src/sync/**` belongs to R08 to R11, which run in sequence. R20 edits the `Rollup` arm, R22 adds `WorkKind::SeedHistory`, and R24 adds `WorkKind::SendAlerts`, each only in PR order, and each owner tells the root before editing.
  - [ ] `crates/web/src/views/**` gains one file per view PR. Shared components in `crates/web/src/components/**` change in R13, or later in PR order. R20 to R23 each add one `Tab` variant in `crates/web/src/components/tabs.rs`, in stack order.
  - [ ] `tools/github-stub/**` belongs to R06. Later PRs add scenarios, extend `tools/github-stub/scenarios/make.mjs` (from R10), and teach `server.mjs` new queries, only in PR order.
- [ ] Hold the review gate. R05, R07, R13, R14, R15, R16, R17, R19, R20, R21, R22, and R23 change what the operator sees or does. They wait for the operator's review in chat with screenshots and a video before merge.

### PR mechanics, for every PR

- [ ] Resolve the forge once. Default to `gh`; if `command -v origin` succeeds and Origin can resolve the repository, use `origin pr` for every PR operation. Record any fallback to `gh`. Never require `gt`.
- [ ] Open the PR ready, never draft, per **Opening a PR**. Use `gh pr create --base main`. The title starts with the plan id, for example `R07: Sign in with GitHub as the one allowed user`. The body links the plan section and lists the evidence for every checked box.
- [ ] Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` once before the PR-facing push. Push with hooks on.
- [ ] Run `/deslop` before each commit and `/no-comments` before review. If a skill is not installed, record the skip in the PR body.
- [ ] Wait for CodeRabbit's review. `.coderabbit.yaml` configures it, and it reviews every PR to `main` on its own. If no review appears within 15 minutes, comment `@coderabbitai review` on the PR.
- [ ] Triage every CodeRabbit comment per `references/bugbot-triage.md` in the pstack plugin. Fix what is real. Reply with a concrete reason to what is noise. Resolve each thread once it is answered. Push fixes until CodeRabbit approves the PR and its commit status is green. Its `Read-only GitHub` and `No secrets` checks are blocking.
- [ ] Rebase onto `main` before the code-ready report and babysit. Keep that merge base in fix rounds. Rebase again only at merge prep, on a `git merge-tree` conflict with `main`, on a CI failure that comes from a change on `main`, or when a known concurrent change to the same files lands.

### Verdict and merge, for every PR

- [ ] A full swarm runs once, at the code-ready head. Another full swarm runs only for new behavior, a redesign, or a large new diff. A rebase, a merge-conflict resolution, or a minor fix gets a single verifier at the new head, and a wording-only change gets none. Run each full swarm per `skills/swarm/SKILL.md` in the pstack plugin. One gates lane. The ten live lanes from the PR's **Verify, live** block. The perf lane from its **Verify, perf** block. Two or more audit lanes, each with its own focus, that read the diff and the receipts and distrust the PR body. The root audits the receipts in the code-ready report before the verdict.
- [ ] Judge every finding by the execution playbook's blocking rule. The verdict is clean when none blocks. A failure on trunk behavior in a lane marked `Regression lane against trunk.` is recorded against trunk and does not block the PR. A new head gets a fresh verdict, except for results that stay valid under the patch-id rule in `playbooks/shipping.md`.
- [ ] Merge only when every review has passed at the exact head SHA. CI is green. CodeRabbit has approved, and every thread is resolved. The root's swarm verdict is clean. For a review-gated PR, the operator has approved in chat. A rebase that leaves the PR's `git patch-id` unchanged keeps every verdict.
- [ ] Then the Opus owner does three things, in this order.
  - [ ] Mark every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>`. The script prints one line per file and fails if any file stays unviewed.
  - [ ] Submit a review with `gh pr review <number> --comment --body "<verdict, CodeRabbit result, and the evidence links>"`. GitHub does not let an account approve its own PR, so the review is a comment.
  - [ ] Squash-merge with `gh pr merge <number> --squash --delete-branch`. Record the merge SHA in the plan box.

### Boot recipe, for every live lane

Each live lane runs in its own git worktree at the PR head, on the operator's machine. There are no cloud VMs. Web lanes drive the browser through the Playwright MCP server or the Claude desktop browser pane. CLI lanes drive `curl`, `wrangler`, and `cargo` through the shell.

- [ ] `git fetch origin <head-branch> && git worktree add ../radar-<pr-id>-<n> <head SHA>`.
- [ ] Start the GitHub stub with `node tools/github-stub/server.mjs --port <8900 + n> --scenario <scenario>`. From R06 on, it serves the GitHub REST, GraphQL, and OAuth endpoints Radar calls, from fixtures.
- [ ] Apply migrations with `npx wrangler d1 migrations apply radar --local --persist-to .wrangler/lane-<n>`.
- [ ] Start Radar with `npx wrangler dev --port <8800 + n> --persist-to .wrangler/lane-<n> --var GITHUB_API_BASE:http://127.0.0.1:<8900 + n> --var GITHUB_WEB_BASE:http://127.0.0.1:<8900 + n>`. Wait for the `Ready on` log line. For R06 to R08 lanes, build with `RADAR_FEATURES=probe` and add `--var PROBE_TOKEN:gho_stubtoken0000`, so lanes can call the client before the UI exists. Any other stub, such as R24's Telegram stub, uses port `<9000 + n>`. Add `--test-scheduled` when a lane fires a cron through `/__scheduled`. Port 8787 stays free for the operator's own `wrangler dev`.
- [ ] Deliver input only through the browser or HTTP requests. Read-only diagnostics are `npx wrangler d1 execute radar --local --command "<select>"`, the `wrangler dev` log, and the stub's request log.
- [ ] Save every screenshot to `/tmp/swarm-<pr-id>/worker-<n>/<slug>.png` and return the paths with the report.

## Scaffold the Cargo workspace and CI (R01)

**Depends on.** None.

**Files.**

- [ ] Create `Cargo.toml` (workspace root), `rust-toolchain.toml`, and `clippy.toml`.
- [ ] Create `crates/domain/Cargo.toml`, `crates/domain/src/lib.rs`, `crates/domain/src/health.rs`, and `crates/domain/src/error.rs`.
- [ ] Create `crates/worker/Cargo.toml`, `crates/worker/src/lib.rs`, `crates/worker/src/api.rs`, and `crates/worker/src/sync/mod.rs`.
- [ ] Create `crates/web/Cargo.toml`, `crates/web/Trunk.toml`, `crates/web/index.html`, `crates/web/src/main.rs`, and `crates/web/src/app.rs`.
- [ ] Create `wrangler.toml`, `package.json`, and `package-lock.json`.
- [ ] Create `scripts/build.mjs`, `scripts/check-size.mjs`, and `scripts/check-size.test.mjs`.
- [ ] Create `.github/workflows/ci.yml`.
- [ ] Edit `.gitignore` to add `artifacts/`, the folder for review media.
- [ ] Edit `AGENTS.md` to replace the sentence "The project scaffold arrives in milestone M0. Until then there are no build, test, or lint commands." with the real commands.

**Build.**

- [ ] Name the data shape first. `domain::health::Health { status: HealthStatus, version: String }` with `enum HealthStatus { Ok }`, and `domain::error::ErrorBody { error: ErrorCode }` with `enum ErrorCode { NotFound, Internal }`. Both use serde with `rename_all = "snake_case"`. They live in `crates/domain` so the Worker and the UI parse the same types (ADR 0005). `domain` depends on `serde` and `serde_json` only.
- [ ] Root `Cargo.toml` is a workspace with members `crates/domain`, `crates/worker`, `crates/web`, `exclude = ["spikes"]`, `resolver = "2"`, and shared `[workspace.dependencies]`. It carries the release profile from `spikes/rust-fit/Cargo.toml` (`opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = "debuginfo"`).
- [ ] `rust-toolchain.toml` pins `channel = "stable"`, `targets = ["wasm32-unknown-unknown"]`, and `components = ["rustfmt", "clippy"]`.
- [ ] `crates/worker` uses `worker` 0.8 and has `crate-type = ["cdylib", "rlib"]`. The `rlib` lets later examples and integration tests link the crate. `#[event(fetch)]` routes `GET /api/health` to a `Health` JSON body. Any other `/api/*` or `/__*` path returns a JSON 404 with `ErrorBody { error: NotFound }`, never the single page application fallback. Every other request goes to `env.assets("ASSETS")?.fetch_request(req)`.
- [ ] `#[event(scheduled)]` kicks the Syncer with the reason `reconcile` and logs `scheduled reconcile`. `src/sync/mod.rs` holds a stub `#[durable_object] Syncer` whose `fetch` logs `syncer kick` and returns 200. R08 replaces the stub.
- [ ] Clippy denies panics in the Worker. `crates/worker/Cargo.toml` sets `[lints.clippy] unwrap_used = "deny"` and `expect_used = "deny"`. Root `clippy.toml` sets `allow-unwrap-in-tests = true` and `allow-expect-in-tests = true`.
- [ ] `crates/web` is a Leptos 0.8 client-side app built by Trunk. `Trunk.toml` sets `target = "index.html"` and `dist = "dist"`. `App` shows the word "Radar" and the result of `GET /api/health`, either "API ok" or "API unreachable", parsed with `domain::health::Health`.
- [ ] `wrangler.toml` sets `name = "radar"`, `main = "crates/worker/build/worker/shim.mjs"`, a `compatibility_date`, and `[build] command = "node scripts/build.mjs"` with `watch_dir = ["crates/domain/src", "crates/worker/src", "crates/web/src", "crates/web/style"]`, plus `crates/web/index.html` if Wrangler accepts a file path there. It never lists `crates`, `crates/web/dist`, or `crates/worker/build`, because a watched build output rebuilds forever. It sets `[assets]` with `directory = "crates/web/dist"`, `binding = "ASSETS"`, `not_found_handling = "single-page-application"`, and `run_worker_first = ["/api/*", "/auth/*", "/hooks/*", "/__*"]`. R05 changes that to `true`, which still covers those four prefixes.
- [ ] `wrangler.toml` declares D1 binding `DB` (database `radar`, `migrations_dir = "migrations"`, placeholder `database_id` the operator replaces at the first deploy), Durable Object binding `SYNCER` for class `Syncer` with `[[migrations]] tag = "v1"` and `new_sqlite_classes = ["Syncer"]`, `[triggers] crons = ["*/10 * * * *", "0 3 * * *"]`, and `routes` with the custom domain `radar.karthik.nz`. Production sets `workers_dev = false`.
- [ ] `wrangler.toml` has `[env.preview]` for a Worker named `radar-preview` with its own D1 database `radar-preview`, the same Durable Object, assets and crons, `workers_dev = true`, and no routes. Every later PR that needs real Cloudflare CPU numbers uses it. The operator deploys it with `npx wrangler deploy --env preview`.
- [ ] `scripts/build.mjs` runs `trunk build --release` in `crates/web`, then `worker-build --release` in `crates/worker`, and exits non-zero when either fails. It always builds the UI first, because Wrangler needs `crates/web/dist` to exist at start.
- [ ] `scripts/check-size.mjs` gzips (level 9) every `.wasm`, `.js`, `.mjs`, and `.css` file under `crates/worker/build/worker` and `crates/web/dist`, prints one line per bundle with its budget, and exits 1 above 400 KB for the Worker or 200 KB for the UI. It exits 2 when a directory is missing or holds no files, so an empty build never passes. Flags are `--worker-budget-kb`, `--ui-budget-kb`, `--worker-dir`, and `--ui-dir`.
- [ ] `.github/workflows/ci.yml` runs on push and pull request with `permissions: contents: read`. Jobs are `fmt` (`cargo fmt --all --check`), `clippy` (`cargo clippy --workspace --all-targets -- -D warnings`), `test` (`cargo test --workspace` and `node --test scripts`), `wasm` (`node scripts/build.mjs`), and `size` (`node scripts/check-size.mjs` after `wasm`). Pin `trunk` and the `worker-build` release that matches `worker` 0.8 with `cargo install --locked`, cache with `Swatinem/rust-cache`, and pin every third-party action to a full commit SHA.
- [ ] `AGENTS.md` lists `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `node scripts/build.mjs`, `node scripts/check-size.mjs`, and `npx wrangler dev`. It says lane worktrees may share `CARGO_TARGET_DIR` to reuse compiled dependencies.

**You see.**

- [ ] `npm ci && npx wrangler dev` prints a `Ready on http://localhost:8787` line, and `curl -s http://localhost:8787/api/health` prints `{"status":"ok","version":"0.0.0"}`.
- [ ] Opening `http://localhost:8787/` shows the word "Radar" and "API ok" on the page.
- [ ] `curl -s -o /dev/null -w "%{http_code}" http://localhost:8787/api/nope` prints `404`, and the body is `{"error":"not_found"}`.
- [ ] `node scripts/check-size.mjs` prints one `worker` line and one `ui` line, both marked `ok`.
- [ ] The CI run for the head SHA shows the five jobs `fmt`, `clippy`, `test`, `wasm`, and `size` green.
- [ ] `npx wrangler deploy --env preview --dry-run --outdir /tmp/radar-preview` lists the bindings `DB`, `SYNCER`, and `ASSETS` and the two crons.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/domain/src/health.rs` gains `health_serializes_status_and_version` and `unknown_status_is_rejected`. `crates/domain/src/error.rs` gains `error_body_uses_snake_case_codes`. Run `cargo test -p domain`.
- [ ] `crates/worker/src/api.rs` gains `unknown_api_path_maps_to_not_found` and `unknown_double_underscore_path_maps_to_not_found`, which check the status and the `ErrorBody` the router builds for `/api/nope` and `/__nope`. Run `cargo test -p worker`.
- [ ] `scripts/check-size.test.mjs` gains `under_budget_exits_0`, `over_budget_exits_1`, and `missing_dir_exits_2`, each on temporary directories. Run `node --test scripts`.
- [ ] Every crate compiles for Wasm. Run `cargo build --workspace --target wasm32-unknown-unknown --release` and `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe. The GitHub stub and the migrations do not exist yet, so lanes skip those two boot steps.

- [ ] Lane 1. Regression lane against trunk. Run a clean build at trunk and at head. Trunk has no workspace, so record that and gate the behavior the diff adds plus the end state the user waits for, which is `npx wrangler dev` serving `/` and `/api/health` from a clean checkout. Save `r01-health.png`. Pass when trunk reports no `Cargo.toml`, and head serves the page with "API ok" and the JSON body from `/api/health`.
- [ ] Lane 2. Open `http://127.0.0.1:<8800 + n>/` in the browser at 1280 px width. Save `r01-hello.png`. Pass when the page shows "Radar" and "API ok" and the browser console has no errors.
- [ ] Lane 3. Open the page at 375 px width. Save `r01-phone.png`. Pass when `document.documentElement.scrollWidth` is at most 375 and the text is readable.
- [ ] Lane 4. Request unknown paths with `curl -i http://127.0.0.1:<8800 + n>/api/nope` and `curl -i http://127.0.0.1:<8800 + n>/__nope`. Save `r01-api-404.png`. Pass when both statuses are 404, both content types are JSON, and both bodies are `{"error":"not_found"}`, not the hello page.
- [ ] Lane 5. Open the deep link `http://127.0.0.1:<8800 + n>/some/deep/link` in the browser. Save `r01-spa-fallback.png`. Pass when the status is 200 and the hello page renders, which proves the single page application fallback.
- [ ] Lane 6. Start Radar per the boot recipe with `--test-scheduled` added and run `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*%2F10+*+*+*+*"`. Save `r01-cron.png` as a terminal capture of the request and the `wrangler dev` log. Pass when the log shows `scheduled reconcile` followed by `syncer kick`, which proves the cron wiring and the Durable Object binding.
- [ ] Lane 7. Plant `let _ = Some(1).unwrap();` in `crates/worker/src/api.rs` and run `cargo clippy -p worker -- -D warnings`. Revert the change. Save `r01-clippy-gate.png` as a terminal capture. Pass when clippy fails with `clippy::unwrap_used` and the revert leaves `git diff` empty.
- [ ] Lane 8. Read the CI run for the head SHA with `gh run list --commit <sha>` and `gh run view <id>`. Save `r01-ci.png` as a terminal capture. Pass when `fmt`, `clippy`, `test`, `wasm`, and `size` all conclude `success`.
- [ ] Lane 9. Run `node scripts/check-size.mjs`, then `node scripts/check-size.mjs --worker-budget-kb 1`. Save `r01-size-gate.png` as a terminal capture. Pass when the first run exits 0 and the second exits 1 with the Worker line marked over budget.
- [ ] Lane 10. Run `node scripts/build.mjs` twice, then `git status --porcelain` and `git check-ignore .dev.vars artifacts/review/x.mp4 crates/web/dist crates/worker/build target`. Save `r01-clean-tree.png` as a terminal capture. Pass when the second build succeeds, `git status --porcelain` prints nothing, and every named path is ignored.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Gzipped Worker size and gzipped UI size from `node scripts/check-size.mjs`. Trunk has no build, so the metric exists only at head. The work the diff adds is the whole scaffold. The end state the user waits for is a bundle that leaves room for the rest of the program.
- [ ] Probe. Run `node scripts/build.mjs` then `node scripts/check-size.mjs` three times at head and take the median line for each bundle.
- [ ] Baseline. Trunk has no workspace, so there is no trunk number. Record the head sizes in the PR body. Later PRs compare against them. The reference is ADR 0005, which measured 134 KB for a full Worker and 66 KB for a Leptos list screen.
- [ ] Rule. Fails if the Worker is above 400 KB or the UI is above 200 KB. It also fails if the Worker is above 268 KB or the UI is above 132 KB, which is twice the ADR 0005 spike figures, because a hello world that large means the scaffold carries dead weight.

**Review gate.** None. R01 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R01 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R01 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R01 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Prove Durable Object alarms and CPU on Workers Free (R02)

**Depends on.** R01.

**Files.**

- [ ] Create `spikes/do-alarm/Cargo.toml` (its own `[workspace]`, crate type `cdylib`), `spikes/do-alarm/src/lib.rs`, and `spikes/do-alarm/wrangler.toml` (Worker name `radar-spike`, no D1, no assets, no routes).
- [ ] Create `spikes/do-alarm/make-fixture.mjs` and `spikes/do-alarm/fixture.json` (the generated 20-PR response) and `spikes/do-alarm/expected.json`.
- [ ] Create `spikes/do-alarm/drive.mjs`, `spikes/do-alarm/summarize.mjs`, `spikes/do-alarm/summarize.test.mjs`, `spikes/do-alarm/testdata/tail-sample.jsonl`, and `spikes/do-alarm/package.json`.
- [ ] Create `spikes/do-alarm/results/tail-<run date>.jsonl` from the operator's run, reduced to the fields `summarize.mjs` keeps.
- [ ] Edit `docs/adr/0002-architecture.md` and `docs/adr/0005-rust.md` to append a dated section at the end. Change no earlier line.

**Build.**

- [ ] Name the data shape first. In the spike, each alarm writes one row `AlarmRun { seq, scheduled_for, started_at, kind: Cold | Warm, attempt, parsed, failing, drafts, outcome: Ok | Error }` to the Durable Object's SQLite table `runs`. The summarizer ends in `enum Decision { Syncer { batch: 20 | 10 }, QueueFallback, Inconclusive }`.
- [ ] The spike Worker is Rust on `worker` 0.8, the same stack as R01. It has a `#[durable_object] Probe` with SQLite storage and `new_sqlite_classes = ["Probe"]`. `kind` is `Cold` when a module level flag is unset, which means a fresh Wasm instance, and the first alarm sets it.
- [ ] Routes are `POST /arm?chain=<n>&gap_ms=<ms>&fixture=ok|throw`, `GET /log`, and `POST /reset`. Every route needs `Authorization: Bearer <SPIKE_KEY>`. The key is a Wrangler secret. A missing secret makes every route answer 500, never open.
- [ ] `/arm` calls `set_alarm`. `alarm()` parses the embedded fixture with serde, counts failing and draft PRs with the structs and rules copied from `spikes/rust-fit/parse-bench/src/lib.rs`, writes an `AlarmRun`, and re-arms itself `gap_ms` later until `chain` runs are done. `fixture=throw` makes the first attempt of each alarm return an error, to show how the platform retries.
- [ ] `make-fixture.mjs` copies the `pr()` generator from `spikes/rust-fit/measure.mjs`. It takes `--prs <n>` (default 20) and writes `fixture.json` and `expected.json` (`prs`, `failing`, `drafts`) computed in JavaScript from the same data.
- [ ] `drive.mjs --url <worker url> --rounds 3 --chain 10 --gap-ms 2000 --idle-s 900` reads the key from the `SPIKE_KEY` environment variable, never from an argument. Each round arms a chain, polls `/log` until the chain is complete or 120 s pass, then idles so the next round starts cold.
- [ ] `summarize.mjs` reads `wrangler tail --format json` lines on stdin. It keeps `cpuTime`, `wallTime`, `outcome`, `eventTimestamp`, and the log line `alarm seq=<n> kind=cold|warm`, and drops every other field. It prints cold and warm count, median, p99, and max `cpuTime`, then one `Decision:` line. Rules, in order. `QueueFallback` when no alarm fired. `Inconclusive` when fewer fired than `--expect` (default 30). With the 20-PR fixture (`--prs 20`, the default), `Syncer { batch: 20 }` when the cold maximum is at or under 7 ms, `Syncer { batch: 10 }` when it is over 7 ms and at or under 10 ms, and `Inconclusive` above 10 ms with the instruction to rerun on the 10-PR fixture. With `--prs 10`, `Syncer { batch: 10 }` when the cold maximum is at or under 10 ms, and `QueueFallback` above 10 ms. The perf rule below uses the same bands.
- [ ] `package.json` scripts are `build`, `deploy` (`wrangler deploy`), `tail` (`wrangler tail radar-spike --format json`), `drive`, and `summarize`. Each runs from `spikes/do-alarm`.
- [ ] Operator step. The operator runs `npx wrangler secret put SPIKE_KEY` and `npm run deploy` in `spikes/do-alarm` on the Free account. The operator keeps `npm run tail > raw.jsonl` open, runs `npm run drive`, then runs `node summarize.mjs < raw.jsonl`. The owner pastes the deploy output into the PR body with the account subdomain replaced by `<account>`.
- [ ] The owner commits `spikes/do-alarm/results/tail-<run date>.jsonl` (reduced fields only) and appends a section `## Spike result, <run date>` to both ADRs. It states the Free plan deploy result, alarms fired out of expected, warm and cold `cpuTime`, the alarm retry behavior seen with `fixture=throw`, the `Decision:` line, and the Enrich batch size R08 uses (20 or 10).
- [ ] If the decision is `QueueFallback` or `Inconclusive`, the owner stops, tells the operator in chat, and R08 is rewritten before any sync code exists.
- [ ] Operator step, after the results are committed. The operator runs `npx wrangler delete --name radar-spike`, which also removes the spike's Durable Object data.

**You see.**

- [ ] `npx wrangler deploy` in `spikes/do-alarm` succeeds on the Free plan and prints a `radar-spike` Worker URL.
- [ ] `node summarize.mjs < raw.jsonl` prints cold and warm `cpuTime` rows and a final line that starts with `Decision:`.
- [ ] `git diff docs/adr/` shows only added lines, in a section headed `## Spike result, <run date>` at the end of each of the two ADRs.
- [ ] `npx wrangler deployments list --name radar-spike` fails with a not found error after the operator deletes the Worker.
- [ ] The owner's chat message states the decision and the Enrich batch size in one sentence each.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `spikes/do-alarm/summarize.test.mjs` gains `medians_match_the_sample`, `no_alarm_gives_queue_fallback`, `short_run_is_inconclusive`, `slow_cold_start_is_inconclusive`, `passing_run_gives_syncer_batch_20`, `cold_between_7_and_10_ms_gives_batch_10`, `slow_ten_pr_rerun_gives_queue_fallback`, `malformed_lines_are_counted_and_skipped`, and `empty_input_exits_nonzero`. Run `node --test spikes/do-alarm`.
- [ ] `cargo test --workspace` at the repository root does not compile the spike. Run it and check that `cargo metadata --no-deps --format-version 1` lists no `do-alarm` package.
- [ ] The spike builds for Wasm. Run `cargo build --release --target wasm32-unknown-unknown` in `spikes/do-alarm`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe. Lanes boot the spike, not the root Worker, with `npx wrangler dev --config spikes/do-alarm/wrangler.toml --port <8800 + n> --persist-to .wrangler/lane-<n> --var SPIKE_KEY:lane-key`. There is no stub and no D1, so skip those boot steps. Every request carries `Authorization: Bearer lane-key` unless the lane says otherwise. Alarms run in the local runtime here, so Cloudflare itself is only checked in the perf boxes.

- [ ] Lane 1. Regression lane against trunk. Trunk has no spike, so record that and gate the behavior the diff adds plus the end state the user waits for, which is a dated decision in both ADRs. Boot the spike, run `curl -X POST "http://localhost:<port>/arm?chain=1"`, and read `/log`. Save `r02-boot.png` as a terminal capture. Pass when `/log` holds one row with `outcome` `ok` and both ADRs end with a `## Spike result` section.
- [ ] Lane 2. Arm one alarm with `chain=1` on a fresh persist directory. Save `r02-single-alarm.png` as a terminal capture of `/log`. Pass when exactly one row exists, `kind` is `cold`, and `parsed` is 20.
- [ ] Lane 3. Arm `chain=10&gap_ms=500`. Save `r02-chain.png` as a terminal capture of `/log`. Pass when ten rows exist in `seq` order, the first is `cold`, the other nine are `warm`, and each `started_at` is within 2 s of its `scheduled_for`.
- [ ] Lane 4. Compare the parsed counts with the fixture. Run `node spikes/do-alarm/make-fixture.mjs --check` against `expected.json`. Save `r02-counts.png` as a terminal capture. Pass when `failing` and `drafts` in every `/log` row equal the values in `expected.json`.
- [ ] Lane 5. Send two `POST /arm?chain=1` requests within 100 ms. Save `r02-idempotent-arm.png` as a terminal capture. Pass when `/log` holds exactly one new row, because the second `set_alarm` replaced the first.
- [ ] Lane 6. Arm with `fixture=throw` and wait 60 s. Save `r02-retry.png` as a terminal capture of `/log`. Pass when `/log` shows an `error` row followed by an `ok` row with `attempt` 2, or shows no second row and the PR body says the local runtime does not retry. Either way the retry behavior is written in the ADR section.
- [ ] Lane 7. Arm a single alarm with `gap_ms=20000`, stop the dev server after 5 s, then restart it with the same `--persist-to`. Save `r02-restart.png` as a terminal capture. Pass when earlier rows are still in `/log` and the pending alarm fires after the restart.
- [ ] Lane 8. Call `/arm` with no header, a wrong key, and the right key, then restart with no `SPIKE_KEY` and call `/log`. Save `r02-auth.png` as a terminal capture. Pass when the first two return 401, the third returns 200, and the keyless start returns 500 for every route.
- [ ] Lane 9. Feed `testdata/tail-sample.jsonl` to `node spikes/do-alarm/summarize.mjs`, then an empty file, then the sample with one cold `cpuTime` edited to 11. Save `r02-summarize.png` as a terminal capture. Pass when the table matches the medians computed by hand in the PR body, the empty input exits non-zero, and the edited sample prints `Decision: Inconclusive`.
- [ ] Lane 10. Check repository hygiene. Run `cargo test --workspace`, `git ls-files spikes/do-alarm`, and `git check-ignore spikes/do-alarm/build spikes/do-alarm/raw.jsonl`. Save `r02-hygiene.png` as a terminal capture. Pass when the workspace tests pass without compiling the spike, no tracked file holds a key or an account subdomain, and the build output and `raw.jsonl` are ignored.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `cpuTime` per alarm invocation on Cloudflare Workers Free, split into cold (first alarm in a fresh Wasm instance) and warm, while the alarm parses the 20-PR fixture and writes one SQLite row. Trunk has no spike, so there is no trunk metric. The work the diff adds is the alarm. The end state the user waits for is a Syncer decision backed by these numbers.
- [ ] Probe. The operator runs `npm run deploy`, `npm run tail > raw.jsonl`, and `npm run drive` with three rounds of ten alarms and a 15 minute idle between rounds. Then `node summarize.mjs < raw.jsonl`. If `tail` shows no `cpuTime` for alarm events, the operator reads the Workers metrics page for `radar-spike` and the owner records that instead.
- [ ] Baseline. Trunk has no spike. The reference is ADR 0005, measured locally on V8, 0.60 ms median and 4.8 ms for the first call. Record those as a reference, not a baseline.
- [ ] Rule. Fails if fewer than 30 of 30 alarms fire, or if any alarm starts more than 30 s after its due time. The batch follows the cold maximum on the 20-PR fixture. At or under 7 ms it is 20. Over 7 ms and at or under 10 ms it is 10. Above 10 ms the operator reruns on the 10-PR fixture, which gives batch 10 at or under 10 ms and the Queue fallback above it. These are the bands `summarize.mjs` applies, so the `Decision:` line and this rule always agree.

**Review gate.** None. R02 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R02 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R02 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R02 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Encode whose turn it is in the domain crate (R03)

**Depends on.** R02.

**Files.**

- [ ] Edit `crates/domain/src/lib.rs` to declare the new modules.
- [ ] Create `crates/domain/src/ids.rs` (`Login`, `RepoName`, `ItemRef`, `ItemKind`, `Timestamp`), `crates/domain/src/activity.rs`, `crates/domain/src/item.rs`, `crates/domain/src/turn.rs`, `crates/domain/src/stage.rs`, and `crates/domain/src/classify.rs`.
- [ ] Create `crates/domain/tests/classify.rs` and `crates/domain/tests/fixtures/*.json`, one file per named case.
- [ ] Create `crates/domain/examples/classify.rs`, the command line surface the live lanes drive.
- [ ] Edit `crates/domain/Cargo.toml` to add `serde_json` as a dev dependency.

**Build.**

- [ ] Name the data shape first. `Item { item_ref: ItemRef, author: Actor, state: ItemState, created_at, updated_at, closed_at: Option<Timestamp>, review_decision: Option<ReviewDecision>, review_requests: Vec<Login>, assignees: Vec<Login>, ci: Option<Ci>, activity: Vec<Activity> }` with `ItemState { Open, Draft, Merged, Closed }`, `ReviewDecision { Approved, ChangesRequested, ReviewRequired }`, and `Ci { state: CiState { Pending, Passing, Failing }, at: Timestamp }`. `Item` has no read state, so reading a thread on github.com cannot change a result (ADR 0002). Every type derives serde with `snake_case`.
- [ ] Identity types. `Login` compares ASCII case-insensitively and keeps its original case for display. `RepoName` parses `owner/name` and rejects anything else. `ItemRef { repo: RepoName, number: u32, kind: ItemKind { Pr, Issue } }` displays as `owner/name#123`. `Timestamp(i64)` holds epoch seconds and has `parse_rfc3339`, a small handwritten parser for the `YYYY-MM-DDTHH:MM:SSZ` form GitHub sends, so the crate takes no date dependency and no `SystemTime`.
- [ ] Activity. `Activity { at: Timestamp, actor: Actor, kind: ActivityKind }` with `Actor { login: Login, kind: ActorKind { User, Bot } }` and `ActivityKind { Comment { mentions_me: bool }, Review { state: ReviewState { Approved, ChangesRequested, Commented, Dismissed, Pending } }, ReviewRequested { reviewer: Login }, Assigned { assignee: Login }, Pushed }`. A doc comment says R10 sets `mentions_me` from the notification reasons `mention` and `team_mention` (F1), on the newest human comment after my last activity. Radar never scans a comment body (N5). `ActivityKind::tag()` returns a stable snake_case word for storage.
- [ ] Result types. `Turn { Me(Reason), Them }` and `Reason { ReviewRequested, ChangesRequested, CiFailed, Mentioned, Assigned, Replied }` with `Reason::PRECEDENCE` set to ChangesRequested, CiFailed, ReviewRequested, Mentioned, Assigned, Replied. `Stage { Draft, CiRunning, CiFailing, AwaitingReview, ChangesRequested, Approved, Merged, Closed }` with `Stage::ALL`. `Classification { turn: Turn, since: Timestamp, stage: Option<Stage>, last_human: Option<Activity> }`. An open issue has no stage.
- [ ] `classify(item: &Item, me: &Login) -> Classification` is a pure function. A merged or closed item is `Them`, with `since` set to `closed_at`, and the dashboard queries filter it out by stage. For an open item, collect the triggers still outstanding. The displayed reason is the highest in `Reason::PRECEDENCE`. `since` is the earliest outstanding trigger time, because that is how long something has waited on me.
- [ ] Trigger rules (F1). `ChangesRequested` is outstanding on my PR while a human reviewer's latest opinionated review is `ChangesRequested` and I have not pushed since. A comment from me does not answer it. `CiFailed` is outstanding on my non-draft PR while `ci.state` is `Failing`. `ReviewRequested` is outstanding on someone else's open, non-draft PR while I am in `review_requests`. `Mentioned` is a human comment with `mentions_me` after my last activity. `Assigned` is outstanding on someone else's item while I am in `assignees` and have not acted since the assignment, or since `created_at` when no assignment event exists. `Replied` is a human comment or `Commented` review by someone else on my item after my last activity.
- [ ] Answering. My `Comment`, `Review`, or `Pushed` after a trigger answers `Mentioned`, `Assigned`, and `Replied`. An activity at the same second as the trigger does not answer it, because the order is unknowable. An approval does not raise `Replied`.
- [ ] Bots never flip the turn (ADR 0002). Activity by an `ActorKind::Bot` is ignored for triggers, answers, reviews, and `last_human`.
- [ ] Stage. The first match wins in this order. `Merged`, `Closed`, `Draft`, `ChangesRequested` (only while outstanding), `CiFailing`, `CiRunning`, `Approved`, `AwaitingReview`. When `review_decision` is `None`, decide `Approved` and `ChangesRequested` from the latest opinionated review per human reviewer, where a later `Dismissed` removes that reviewer's opinion and a `Commented` review replaces nothing.
- [ ] `Them` and `since`. For an open item that is not `Me`, `since` is the time of the latest human activity, or `created_at` when there is none. A CI failure on a draft PR stays `Them` by design, and the test says so.
- [ ] `examples/classify.rs` reads one or more fixture paths and prints the `Classification` as JSON with the fixture name. `classify --bench <runs> <dir>` classifies every fixture `runs` times and prints microseconds per fixture. Invalid input exits non-zero and names the field.

**You see.**

- [ ] `cargo run -p domain --example classify -- crates/domain/tests/fixtures/reason_ci_failed.json` prints `"turn":{"me":"ci_failed"}` and a `since` equal to the CI time in the fixture.
- [ ] Running the example over `stage_*.json` prints each of the eight stage names once.
- [ ] `cargo test -p domain` ends with `test result: ok` and lists every fixture name in the test output.
- [ ] `cargo tree -p domain` shows no `worker` and no `leptos`.
- [ ] `cargo build -p domain --target wasm32-unknown-unknown --release` succeeds.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/domain/tests/classify.rs` runs every file in `tests/fixtures/` as one named case, each with `me`, `item`, and `expect` (`turn`, `stage`, `since`). It fails with the file name. It also asserts that every `Reason` and every `Stage` is expected by at least one fixture. Fixture names are `reason_review_requested`, `reason_changes_requested`, `reason_ci_failed`, `reason_mentioned`, `reason_assigned`, `reason_replied`, `stage_draft`, `stage_ci_running`, `stage_ci_failing`, `stage_awaiting_review`, `stage_changes_requested`, `stage_approved`, `stage_merged`, and `stage_closed`.
- [ ] Rule fixtures in the same directory are `rule_bot_comment_after_mine`, `rule_bot_review_ignored`, `rule_bot_mention_ignored`, `rule_empty_decision_latest_per_reviewer`, `rule_empty_decision_second_reviewer_blocks`, `rule_dismissed_review_ignored`, `rule_commented_review_keeps_approval`, `rule_push_answers_changes_requested`, `rule_comment_does_not_answer_changes_requested`, `rule_reply_answers_mention`, `rule_same_second_does_not_answer`, `rule_draft_ci_failure_stays_them`, `rule_third_party_thread_stays_them`, `rule_author_case_insensitive`, `rule_new_own_pr_waits_on_them`, `rule_issue_replied`, `rule_issue_assigned_to_me`, `rule_precedence_changes_over_ci`, `rule_since_is_oldest_trigger`, and `rule_review_request_on_draft_ignored`.
- [ ] A metamorphic test `bot_activity_never_changes_classification` appends a bot comment, review, and mention, newer than everything, to every fixture and asserts the `Classification` is unchanged.
- [ ] Unit tests in `ids.rs` cover `login_compares_case_insensitively`, `repo_name_rejects_missing_owner`, `item_ref_displays_as_owner_name_hash_number`, and `timestamp_parses_github_format_and_rejects_garbage`. A test `item_has_no_read_state` serializes an `Item` and asserts no key is named `unread` or `read`.
- [ ] Run `cargo test -p domain`, then `cargo clippy -p domain --all-targets -- -D warnings`, then `cargo build -p domain --target wasm32-unknown-unknown`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe. This PR has no Worker route, so lanes drive the command line surface `cargo run -p domain --example classify`. They skip the stub, migration, and `wrangler dev` boot steps and edit copies of the fixtures in a scratch directory, never the committed files.

- [ ] Lane 1. Regression lane against trunk. Run `cargo test -p domain` at trunk and at head. Trunk has only the `Health` types, so record that and gate the behavior the diff adds plus the end state the user waits for, which is a correct classification for every named fixture. Save `r03-regression.png` as a terminal capture. Pass when trunk's health tests still pass at head and the example prints a classification for every fixture without an error.
- [ ] Lane 2. Run the example over the six `reason_*` fixtures. Save `r03-reasons.png` as a terminal capture. Pass when each output turn equals `me` with the reason in the file name, and a second run prints byte-identical output.
- [ ] Lane 3. Run the example over the eight `stage_*` fixtures. Save `r03-stages.png` as a terminal capture. Pass when each output stage equals the name in the file name, and the issue fixtures print `"stage":null` while open.
- [ ] Lane 4. Copy `reason_changes_requested.json` to scratch and append a bot comment dated later than everything. Run the example on both files. Save `r03-bots.png` as a terminal capture. Pass when the two outputs are identical.
- [ ] Lane 5. Copy a PR fixture, set `review_decision` to `null`, and give reviewer A a `changes_requested` then an `approved` review. Then add a `changes_requested` review from reviewer B. Save `r03-empty-decision.png` as a terminal capture. Pass when the first edit prints stage `approved` and the second prints `changes_requested`.
- [ ] Lane 6. Copy `reason_changes_requested.json`. Add a comment by me after the review, then run the example, then replace it with a push by me. Save `r03-push-vs-comment.png` as a terminal capture. Pass when the comment leaves `{"me":"changes_requested"}` and the push gives `them` with stage `awaiting_review`.
- [ ] Lane 7. Copy a fixture and change the author login to `KARTHIK-beta` while `me` is `karthik-beta`. Save `r03-case.png` as a terminal capture. Pass when the item is treated as mine, which `rule_author_case_insensitive` expects.
- [ ] Lane 8. Feed malformed input. Use a timestamp of `yesterday`, an unknown stage word, and a missing `item_ref`. Save `r03-bad-input.png` as a terminal capture. Pass when each run exits non-zero, names the offending field, and prints no panic or backtrace.
- [ ] Lane 9. Test boundaries. Set a reply from me to the same second as a mention, then one second later, and run a new own PR with an empty `activity` list. Save `r03-boundaries.png` as a terminal capture. Pass when the same second stays `me`, one second later flips to `them`, and the empty PR prints `them` with `since` equal to `created_at`.
- [ ] Lane 10. Check the crate boundary. Run `cargo tree -p domain`, `cargo build -p domain --target wasm32-unknown-unknown --release`, and `grep -rn "SystemTime\|tokio" crates/domain`. Save `r03-boundary.png` as a terminal capture. Pass when the tree has no `worker` or `leptos`, the Wasm build succeeds, and the grep prints nothing.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Microseconds to classify one 20-fixture batch, a stand-in for one Enrich batch, from `classify --bench`. Trunk has no `classify`, so there is no trunk metric. The work the diff adds is `classify`. The end state the user waits for is an Enrich unit that stays under 10 ms of CPU (N7), of which classify is only a small part.
- [ ] Probe. Run `cargo run -p domain --release --example classify -- --bench 10000 crates/domain/tests/fixtures` three times and take the median.
- [ ] Baseline. Trunk has no `classify`. The reference is ADR 0005, which measured parsing and classifying 20 PRs at 0.60 ms median in Wasm on V8. Record it as a reference.
- [ ] Rule. Fails if one 20-fixture batch takes more than 50 microseconds natively. That leaves a 20 times Wasm slowdown inside 1 ms, a tenth of the CPU budget. It also fails if `cargo build -p domain --target wasm32-unknown-unknown --release` is not clean.

**Review gate.** None. R03 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R03 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R03 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R03 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Create the D1 schema and row access (R04)

**Depends on.** R03.

**Files.**

- [ ] Create `migrations/0001_init.sql`.
- [ ] Create `crates/worker/src/db/mod.rs`, `sql.rs`, `rows.rs`, `items.rs`, `threads.rs`, `events.rs`, `repos.rs`, `rollups.rs`, `local_state.rs`, `auth.rs`, and `queries.rs`.
- [ ] Create `crates/worker/examples/emit_sql.rs` and `crates/worker/examples/fixtures/*.json`.
- [ ] Edit `crates/worker/Cargo.toml` to add `rusqlite` with the `bundled` feature as a native-only dev dependency and a `sql-dump` feature.
- [ ] Edit `crates/worker/src/lib.rs` to declare `mod db`.

**Build.**

- [ ] Name the data shape first. `ItemRow` mirrors the `items` table. Its `turn` and `reason` columns convert to and from `domain::Turn` through `TryFrom`, so an invalid pair never becomes a value. A `Statement { sql: &'static str, params: Vec<Param> }` with `enum Param { Text, Int, Null, Blob }` is the only thing the upsert functions return. Statements are pure, so tests run them natively. A thin `run(&D1Database, Vec<Statement>)` sends them with `db.batch`.
- [ ] `migrations/0001_init.sql` creates `notification_threads` (`id` primary key, `repo`, `subject_kind` of `pr`, `issue`, or `other`, `subject_title`, `subject_number`, `item_node_id`, `reason`, `updated_at`, `synced_at`). It does not store an unread flag, because reading never changes anything in Radar.
- [ ] `items` has `node_id` primary key, `repo`, `number` (unique with `repo`), `kind`, `title`, `url`, `head_ref`, `author_login`, `author_kind`, `authored_by_me`, `state` of `open`, `draft`, `merged`, or `closed`, `created_at`, `updated_at`, `closed_at`, the derived `turn`, `reason`, `since`, `stage`, `ci_state`, `review_decision`, and the last human activity as `last_human_at`, `last_human_login`, `last_human_kind`, `last_human_url`, plus `synced_at`. A `CHECK` allows `turn = 'me'` only with a reason, and `turn = 'them'` only without one. `head_ref` lets the UI mark agent work by the `agent/` prefix without extra tables.
- [ ] Indexes serve the four dashboard queries and nothing else. These are partial indexes on `since` for open items with `turn = 'me'` and for `turn = 'them' AND authored_by_me = 1`, one on `stage` for my open PRs, and one on `closed_at` for merged and closed items. The `repo` column gets one plain index.
- [ ] `events` holds one row per GitHub timeline item, the model R10 writes and R20 to R22 read. Columns are `id` primary key, `item_node_id`, `repo`, `kind`, `review_state`, `actor_login`, `actor_is_me`, `actor_is_bot`, `occurred_at`, `source` of `poll`, `backfill`, `webhook`, or `agent`, and `ingested_at`, with indexes on `occurred_at` and on `(repo, occurred_at)`. A `CHECK` limits `kind` to `opened`, `commented`, `reviewed`, `review_requested`, `assigned`, `mentioned`, `committed`, `merged`, `closed`, `reopened`, `ci_failed`, and `ci_passed`, and allows `review_state` only with `reviewed`. The id is the timeline item's GitHub node id, so a re-read writes nothing (N8). `EventId::for_node(node_id)` builds it, and `EventId::for_ci(item_node_id, commit_oid, state)` builds the id of a CI transition, which has no node of its own.
- [ ] `repos` has `full_name` primary key, `open_prs`, `merged_prs`, `closed_prs`, `median_first_response_hours`, `last_activity_at`, and `updated_at`. `rollups_weekly` has `week_start` (epoch of Monday 00:00 UTC) as primary key plus `prs_opened`, `prs_merged`, `repos_touched`, and `events`.
- [ ] `local_state` has `item_id` primary key, `kind` of `dismissed` or `snoozed`, and `value`. For `dismissed`, `value` is the item's `last_human_at` when I dismissed it, so a newer human activity brings the item back. For `snoozed`, `value` is the wake time (F9). The Rust type is `LocalState { Dismissed { seen_human_at }, Snoozed { until } }`.
- [ ] `auth` is one row, enforced by `id INTEGER PRIMARY KEY CHECK (id = 1)`, with `github_user_id`, `login`, `token_cipher` and `token_nonce` (both nullable, because R07's disconnect clears them and keeps the row), `scopes`, `scopes_checked_at`, `session_epoch INTEGER NOT NULL DEFAULT 0` (R07 bumps it to revoke every session), and `created_at`.
- [ ] Every upsert is `INSERT ... ON CONFLICT DO UPDATE`. `items` and `notification_threads` only overwrite when `excluded.updated_at >= existing.updated_at`, so an older response never replaces a newer one. `events` uses `ON CONFLICT DO NOTHING`. All timestamps are epoch seconds in `INTEGER` columns.
- [ ] `queries.rs` holds the four canonical read statements as constants for R12 to call. They are Your turn (oldest `since` first, hiding rows `local_state` hides), Their turn, the pipeline by `stage`, and merged or closed in the last 14 days.
- [ ] `examples/emit_sql.rs` (feature `sql-dump`) prints the statements for a fixture as literal SQL, with each parameter escaped. `--synthetic <n>` prints `n` deterministic items across all turns and stages. Live lanes run that SQL against local D1 with `wrangler d1 execute --file`.

**You see.**

- [ ] `npx wrangler d1 migrations apply radar --local --persist-to .wrangler/dev` prints `0001_init.sql` as applied, and a second run prints that there is nothing to apply.
- [ ] `npx wrangler d1 execute radar --local --persist-to .wrangler/dev --command "select name from sqlite_master where type='table' order by name"` lists `auth`, `events`, `items`, `local_state`, `notification_threads`, `repos`, and `rollups_weekly`.
- [ ] Inserting an item fixture through `emit_sql` and selecting `turn, reason, since, stage` prints the same values that `cargo run -p domain --example classify` prints for that fixture.
- [ ] `EXPLAIN QUERY PLAN` for each of the four dashboard queries names an `idx_items_` index and never `SCAN items`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/worker/src/db/items.rs` tests, run against an in-memory SQLite built from `migrations/0001_init.sql`, are `upsert_twice_keeps_one_row`, `older_update_does_not_overwrite_newer`, `turn_me_without_reason_is_rejected`, `turn_them_with_reason_is_rejected`, `unknown_stage_is_rejected`, and `hostile_title_is_stored_verbatim` (quotes, emoji, and `'); DROP TABLE items;--`). Run `cargo test -p worker db::`.
- [ ] `events.rs` tests are `same_timeline_node_is_one_event`, `new_timeline_node_is_a_new_event`, `same_ci_transition_is_one_event`, and `unknown_kind_is_rejected` (N8).
- [ ] `local_state.rs` tests are `dismissed_item_hidden_until_newer_human_activity`, `snoozed_item_returns_at_wake_time`, and `your_turn_query_orders_oldest_first`.
- [ ] `queries.rs` tests run `EXPLAIN QUERY PLAN` on each constant and assert it uses an index and does not scan `items`. `auth.rs` has `second_auth_row_is_rejected`.
- [ ] A round trip test builds an `ItemRow` from every `domain` classification fixture and reads it back as the same `Turn`, `Stage`, and `since`. Run `cargo test -p worker` and `cargo clippy -p worker --all-targets -- -D warnings`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe. This PR has no HTTP caller of the row layer yet, so lanes apply the migration to local D1, run the exact SQL from `emit_sql` with `npx wrangler d1 execute radar --local --persist-to .wrangler/lane-<n> --file <sql>`, and read the result. The stub is not needed, so skip its boot step.

- [ ] Lane 1. Regression lane against trunk. Apply nothing at trunk and apply `0001_init.sql` at head, then request `/api/health` on both. Trunk has no tables, so record that and gate the behavior the diff adds plus the end state the user waits for, which is the seven tables and four indexed queries. Save `r04-migrate.png` as a terminal capture. Pass when `/api/health` is identical on both and head lists the seven tables.
- [ ] Lane 2. Insert the `pr_ci_failing` fixture through `emit_sql` and select its row. Save `r04-insert.png` as a terminal capture. Pass when `turn`, `reason`, `since`, and `stage` equal the output of the `domain` classify example for the same fixture.
- [ ] Lane 3. Run the same insert SQL twice. Save `r04-idempotent.png` as a terminal capture. Pass when `select count(*) from items` is 1 after both runs and only `synced_at` differs.
- [ ] Lane 4. Insert `pr_ci_passing`, the same node with a newer `updated_at`, then re-run the older `pr_ci_failing` SQL. Save `r04-monotonic.png` as a terminal capture. Pass when the row shows the newer state after both runs and no duplicate exists.
- [ ] Lane 5. Insert two events with the same timeline node id, then one with a new node id, then one with `kind = 'pushed'`. Save `r04-events.png` as a terminal capture. Pass when the table holds two rows, the duplicate id was ignored without an error, and the unknown kind failed its `CHECK`.
- [ ] Lane 6. Break the constraints by hand with `wrangler d1 execute --command`. Insert `turn='me'` with a null reason, `turn='them'` with a reason, `stage='bogus'`, and a second `auth` row with `id = 2`. Save `r04-checks.png` as a terminal capture. Pass when each statement fails with a constraint error and the tables are unchanged.
- [ ] Lane 7. Seed 5,000 items with `emit_sql --synthetic 5000`, then run `EXPLAIN QUERY PLAN` on the four dashboard queries. Save `r04-plans.png` as a terminal capture. Pass when every plan uses an `idx_items_` index and none says `SCAN items`.
- [ ] Lane 8. Insert the `hostile_title` fixture, then count tables and read the title back. Save `r04-hostile.png` as a terminal capture. Pass when the title is returned byte for byte, all seven tables still exist, and no statement beyond the insert ran.
- [ ] Lane 9. Dismiss and snooze through `local_state`. Dismiss an item at its `last_human_at`, run the Your turn query, bump the item's `last_human_at`, and run it again, then snooze one item to a past and a future time. Save `r04-local-state.png` as a terminal capture. Pass when the dismissed item is hidden, returns after the newer activity, and the snoozed item hides only while its wake time is in the future.
- [ ] Lane 10. Check migration discipline. Run `npx wrangler d1 migrations apply radar --local` twice on a fresh directory and once after editing nothing, then list `migrations/`. Save `r04-migrations.png` as a terminal capture. Pass when the second run applies nothing and `migrations/` holds exactly `0001_init.sql`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. D1 `rows_read` for each of the four dashboard queries on 5,000 seeded items with about 40 waiting on me, and `rows_written` for one item upsert, both from the `meta` block of `wrangler d1 execute --json`. Trunk has no tables, so there is no trunk metric. The work the diff adds is the schema. The end state the user waits for is one dashboard load that reads few rows (N7).
- [ ] Probe. Seed with `emit_sql --synthetic 5000`, then run each query and one upsert with `npx wrangler d1 execute radar --local --persist-to .wrangler/lane-<n> --json --command "<sql>"` three times. If local `meta` reports zero rows, the `EXPLAIN QUERY PLAN` output is the proof and the owner says so.
- [ ] Baseline. Trunk has no tables. Record head's numbers per query in the PR body as the figures R12 compares against.
- [ ] Rule. Fails if the Your turn query reads more than 120 rows, if the four queries together read more than 500 rows, or if one item upsert writes more than 7 rows. The budget keeps a day of page loads far under the 5 million rows read and 100,000 rows written that D1 allows.

**Review gate.** None. R04 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R04 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R04 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R04 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Build the web shell with tokens, fonts, icons, and CSP (R05)

**Depends on.** R06.

**Files.**

- [ ] Create `scripts/lib/tokens.mjs` (token walk and value formatters), `scripts/tokens-to-css.mjs`, `scripts/tokens-to-css.test.mjs`, and `scripts/check-dist.mjs`.
- [ ] Edit `scripts/tokens-to-wiki.mjs` to import its walk and formatters from `scripts/lib/tokens.mjs`.
- [ ] Create `crates/web/style/tokens.css` (generated) and `crates/web/style/base.css`.
- [ ] Edit `crates/web/index.html`, `crates/web/Trunk.toml`, and `crates/web/Cargo.toml` (add `leptos_router`).
- [ ] Edit `crates/web/src/main.rs` and `crates/web/src/app.rs`. Create `crates/web/src/components/mod.rs`, `crates/web/src/components/tabs.rs`, `crates/web/src/components/header.rs`, and `crates/web/src/views/mod.rs`, `crates/web/src/views/not_found.rs`.
- [ ] Create `crates/worker/src/security.rs`. Edit `crates/worker/src/lib.rs` and `wrangler.toml`.
- [ ] Edit `.github/workflows/ci.yml` and `AGENTS.md`.

**Build.**

- [ ] Name the data shapes first. `Tab { YourTurn, TheirTurn, Pipeline, Merged }` in `crates/web/src/components/tabs.rs` with `Tab::ALL` (the ADR 0003 order), `label()`, `href()` (`/`, `/theirs`, `/pipeline`, and `/pipeline` for Merged, which then scrolls to the recent list), and `from_path(&str) -> Option<Tab>`, which never returns `Merged`. Routes are paths, never `#` hashes. In the Worker, `Csp` is a const table of directive and source list pairs, and `HEADERS` is a const list of four name and value pairs built from it. Later view PRs add a variant and a row, never a string edit.
- [ ] `scripts/lib/tokens.mjs` holds the `collect` walk and the formatters now in `scripts/tokens-to-wiki.mjs`. Both generators import it. Before the edit, run `node scripts/tokens-to-wiki.mjs /tmp/wiki-before`. After it, run the script into `/tmp/wiki-after`. `diff -r` must print nothing, and the owner records that in the PR body.
- [ ] `scripts/tokens-to-css.mjs` writes `crates/web/style/tokens.css` as one `:root` block with a "Generated, do not edit" header. Names follow the wiki script, such as `--color-bg`, `--font-family-ui`, `--space-4`, and `--motion-duration-dismiss`. `--check` exits 1 when the file differs from the generated text. An unknown `$type` exits non-zero. The output is sorted and byte-stable.
- [ ] `crates/web/style/base.css` sets `color-scheme: dark`, the page background `var(--color-bg)`, text `var(--color-text)`, the UI font at `var(--font-size-body)`, and tabular figures. It sets a visible `:focus-visible` ring and removes motion under `prefers-reduced-motion`. It has one `@media (max-width: 600px)` block, which the test compares with `--breakpoint-phone`. It holds no hex colors.
- [ ] `crates/web/index.html` has the head from ADR 0004 (icons, manifest, `theme-color`, the font preload, `/fonts/fonts.css`) and Trunk links for the Rust app, `style/tokens.css`, `style/base.css`, and a copy of `public/` into the dist root. After `trunk build`, the files must sit at `/fonts/schibsted-grotesk.woff2`, `/icon.svg`, and `/manifest.webmanifest`. If `copy-dir` nests them, use `data-target-path` or a `post_build` hook.
- [ ] Trunk's default init script is inline, and the CSP forbids inline scripts. Make Trunk emit an external script instead. Use `pattern_script` in `Trunk.toml`, or `filehash = false` with a static `boot.js`, whichever Trunk supports. The owner writes the choice in the PR body.
- [ ] `scripts/check-dist.mjs` fails unless `crates/web/dist` holds `index.html`, both `.woff2` fonts, `fonts/fonts.css`, `manifest.webmanifest`, the three PNG icons, `icon.svg`, and `favicon.ico`, and unless `index.html` has no inline `<script>` body.
- [ ] `App` renders `Header` (the text wordmark "Radar" and a `nav` of the four tabs with `aria-current="page"` on the active one) over an empty main area, using `leptos_router`. A path that matches no `Tab` shows `not_found` inside the shell. Tabs show their label only, with no counts yet. The phone layout follows `prototypes/direction-ac.html` at 600 px and below, where the tabs take their own row and scroll sideways.
- [ ] `security.rs` adds the four headers to every response, including 404s, redirects, and asset responses. The CSP is `default-src 'none'`, `script-src 'self' 'wasm-unsafe-eval'`, `style-src 'self'`, `font-src 'self'`, `img-src 'self'`, `connect-src 'self'`, `manifest-src 'self'`, `base-uri 'none'`, `form-action 'self'`, `frame-ancestors 'none'`, and `object-src 'none'` (N4). `Referrer-Policy` is `no-referrer`, `X-Content-Type-Options` is `nosniff`, and `Strict-Transport-Security` is `max-age=31536000; includeSubDomains`.
- [ ] `wrangler.toml` sets `run_worker_first = true`, so the Worker sees every request and serves static files through the `ASSETS` binding. Responses from `ASSETS` have immutable headers, so `security.rs` rebuilds the response with copied headers.
- [ ] CI runs `node scripts/tokens-to-css.mjs --check` and `node scripts/check-dist.mjs` after the `wasm` job. `AGENTS.md` adds one sentence, which says to run `node scripts/tokens-to-css.mjs` after a token change.

**You see.**

- [ ] `curl -sI http://localhost:8787/` prints `content-security-policy`, `referrer-policy`, `x-content-type-options`, and `strict-transport-security` with the values above.
- [ ] The page at `http://localhost:8787/` has a graphite background (`rgb(20, 21, 25)`), the wordmark "Radar", four tabs, and an empty main area, set in Schibsted Grotesk.
- [ ] The browser console shows no CSP violation, and the network list shows every request on `localhost` with the fonts served as `font/woff2`.
- [ ] `node scripts/check-dist.mjs` prints `dist ok` with a file count.
- [ ] Changing a color in `design/tokens.json`, then running `node scripts/tokens-to-css.mjs` and a rebuild, changes the page color.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scripts/tokens-to-css.test.mjs` gains `every_token_becomes_a_custom_property`, `output_is_byte_stable`, `check_flags_a_stale_file`, `unknown_type_exits_nonzero`, `no_hex_literal_outside_tokens_css`, and `breakpoint_matches_the_base_css_media_query`. Run `node --test scripts`.
- [ ] `crates/web/src/components/tabs.rs` gains `from_path_round_trips_every_routed_tab`, `merged_links_to_the_pipeline`, `unknown_path_is_none`, and `all_follows_the_adr_order`. Run `cargo test -p web`.
- [ ] `crates/worker/src/security.rs` gains `csp_denies_by_default`, `csp_never_allows_unsafe_inline_or_eval`, `all_four_headers_are_set`, and `a_weaker_upstream_csp_is_overwritten`. Run `cargo test -p worker security::`.
- [ ] Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `node scripts/build.mjs`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe. The stub is not needed for this PR, so skip its boot step. Use a Chromium browser, which allows `Secure` cookies and service features on `localhost`.

- [ ] Lane 1. Regression lane against trunk. Load `/` and `/api/health` on trunk and on head. Trunk serves the R01 hello page, so record that, and gate the behavior the diff adds plus the end state the user waits for, which is the shell with fonts and headers on every response. Save `r05-regression.png`. Pass when `/api/health` returns the same JSON on both, trunk shows the hello page, and head shows the shell.
- [ ] Lane 2. Open `/` at 1280 px width. Save `r05-laptop.png`. Pass when the page shows the wordmark and four tabs in order Your turn, Their turn, Pipeline, Merged, the body background computes to `rgb(20, 21, 25)`, and there is no light theme even with `prefers-color-scheme: light` emulated.
- [ ] Lane 3. Open `/` at 375 px width. Save `r05-phone.png`. Pass when the tabs sit in their own row, scroll sideways inside it, `document.documentElement.scrollWidth` is at most 375, and the page gutter is 16 px.
- [ ] Lane 4. Inspect fonts. Read `document.fonts` and the network list after a hard reload. Save `r05-fonts.png`. Pass when `Schibsted Grotesk` is loaded from `/fonts/schibsted-grotesk.woff2` with status 200, `getComputedStyle(document.body).fontFamily` starts with `"Schibsted Grotesk"`, and no request leaves `localhost`.
- [ ] Lane 5. Request `/`, `/index.html`, `/fonts/fonts.css`, `/manifest.webmanifest`, `/api/health`, `/api/nope`, and `/does/not/exist` with `curl -sI`. Save `r05-headers.png` as a terminal capture. Pass when all seven responses carry the four security headers with the exact values from the Build list.
- [ ] Lane 6. Prove the CSP bites. In the page, add a `securitypolicyviolation` listener, then append an inline `<script>` that sets `window.__x`, and call `fetch("https://example.com")`. Save `r05-csp.png`. Pass when `window.__x` stays undefined, the fetch is rejected, and the listener saw both `script-src-elem` and `connect-src` violations.
- [ ] Lane 7. Use only the keyboard. Press Tab from a fresh load until the first tab has focus, move through all four, and press Enter on Pipeline. Save `r05-keyboard.png`. Pass when each tab shows a visible focus ring, Enter changes the URL to `/pipeline`, and `aria-current="page"` moves to Pipeline.
- [ ] Lane 8. Check routing. Load `/pipeline` directly, press Back and Forward, then load `/zzz`. Save `r05-routing.png`. Pass when `/pipeline` marks Pipeline current, history navigation keeps the tab and URL in step, and `/zzz` shows "Not found" inside the shell.
- [ ] Lane 9. Check the brand assets. Request `/manifest.webmanifest`, the three PNG icons, `/icon.svg`, `/favicon.ico`, and `/apple-touch-icon.png`. Save `r05-assets.png`. Pass when every status is 200, the manifest parses and names the icons, the `link` tags resolve in the page, and the tab shows the favicon.
- [ ] Lane 10. Prove tokens drive the page. In a scratch worktree, change `color.bg` in `design/tokens.json`, run `node scripts/tokens-to-css.mjs`, rebuild, and reload. Save `r05-token-flow.png`. Pass when the body background changes to the new value, `tokens-to-css.mjs --check` fails before the regenerate step, and the committed worktree is unchanged afterwards.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Gzipped UI size from `node scripts/check-size.mjs`, median `/api/health` latency on `wrangler dev`, and first contentful paint of `/` from the Performance API. UI size and `/api/health` latency exist at trunk and head. The shell does not exist at trunk, so for paint the work the diff adds is the shell and the end state the user waits for is a first load under 1.5 s.
- [ ] Probe. Interleave trunk and head. Build both, then run `node scripts/check-size.mjs` on each, then request `/api/health` 100 times alternately against each server with `curl` and take the medians. Load `/` 10 times at head with the cache disabled and read `performance.getEntriesByName("first-contentful-paint")[0].startTime`.
- [ ] Baseline. Record the trunk UI size and trunk `/api/health` median first.
- [ ] Rule. Fails if the head UI is above 200 KB or more than 60 KB above trunk, if the head `/api/health` median is more than 5 ms above trunk, or if the median first contentful paint at head is above 1.5 s.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, lane 3, and lane 6 screenshots into `artifacts/review/R05-review-laptop.png`, `artifacts/review/R05-review-phone.png`, and `artifacts/review/R05-review-csp.png`.
- [ ] Record a 30 to 60 second video of the shell at laptop and phone width, tabbing through the four tabs. Save it as `artifacts/review/R05-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's go.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R05 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R05 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R05 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Build the read-only GitHub client and the GitHub stub (R06)

**Depends on.** R04.

**Files.**

- [ ] Create `crates/worker/src/github/mod.rs`, `client.rs`, `path.rs`, `meta.rs`, `error.rs`, and `query.rs`.
- [ ] Create `crates/worker/src/github/queries/viewer.graphql`.
- [ ] Create `crates/worker/src/probe.rs`, compiled only with the `probe` feature.
- [ ] Create `crates/worker/tests/read_only.rs`.
- [ ] Edit `crates/worker/Cargo.toml` (feature `probe`, default empty), `crates/worker/src/lib.rs`, `wrangler.toml` (vars `GITHUB_API_BASE` and `GITHUB_WEB_BASE`), `scripts/build.mjs`, and `.github/workflows/ci.yml`.
- [ ] Create `tools/github-stub/server.mjs`, `tools/github-stub/server.test.mjs`, and `tools/github-stub/README.md`.
- [ ] Create `tools/github-stub/scenarios/basic.json`, `empty.json`, `fast-poll.json`, `low-budget.json`, `rate-limited.json`, `server-error.json`, `graphql-errors.json`, `no-notifications-scope.json`, `oauth-other-user.json`, `oauth-deny.json`, and `oauth-bad-code.json`.

**Build.**

- [ ] Name the data shapes first. `Conditional { None, IfModifiedSince(String), IfNoneMatch(String) }` is the only way to make a request conditional. `Fetched<T> { Modified { body: T, validators: Validators, meta: RateMeta }, NotModified { meta: RateMeta } }` is the success type. `RateMeta { limit, remaining, reset: Option<Timestamp>, poll_interval: Option<u32>, scopes: Option<Vec<String>> }` holds the headers Radar records (P1, P4, A5). `GithubError { Unauthorized, Forbidden, NotFound, RateLimited { retry_after_secs }, Server(u16), Transport, Parse { path }, Graphql(Vec<String>), BadConfig }` covers every failure.
- [ ] `ApiPath::new(&str)` is the boundary for paths. It accepts a string that starts with one `/` and rejects `//`, `..`, a scheme, `@`, backslashes, and control characters. `Client::get<T>(&self, path: ApiPath, cond: Conditional)` takes no method.
- [ ] `QueryDoc { name: &'static str, text: &'static str }` is built only by a `query_doc!` macro that includes a file from `src/github/queries/`. `validate_document(&str)` accepts only documents whose every operation starts with the `query` keyword and rejects `mutation` and `subscription`, and it skips comments and string values so a field such as `mutationCount` or a `# mutation` comment does not trip it (N2). `Client::query<T>(&self, doc: &QueryDoc, variables: impl Serialize)` posts `{query, variables}` to `<base>/graphql`. That is the only POST in the client.
- [ ] `Client::new(base, token)` refuses a base that is neither `https://api.github.com` nor an `http` URL on `127.0.0.1`, `localhost`, or `[::1]`, so a bad variable can never send a token over cleartext or to another host. `Token` redacts itself in `Debug`.
- [ ] Every request sends `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2022-11-28`, `User-Agent: radar`, and `Authorization: Bearer <token>`. Status 304 gives `NotModified`. Status 403 or 429 with `Retry-After` gives `RateLimited` with those seconds. Status 403 with `x-ratelimit-remaining: 0` gives `RateLimited` with the seconds until `x-ratelimit-reset`. Any other 403 is `Forbidden`. A 200 GraphQL body with `errors` gives `Graphql`.
- [ ] `meta.rs` parses `x-ratelimit-limit`, `x-ratelimit-remaining`, `x-ratelimit-reset`, `X-Poll-Interval`, and `X-OAuth-Scopes` from a header lookup closure, so the parser runs natively in tests. Missing or non-numeric values become `None`.
- [ ] `crates/worker/tests/read_only.rs` scans `crates/worker/src` and fails on `Method::Post`, `Method::Put`, `Method::Patch`, `Method::Delete`, or a quoted HTTP verb other than GET outside an allow-list const that names `github/client.rs` only. It also fails when a file in `github/queries/` is not registered through `query_doc!`, and when `default` in the `[features]` table of `crates/worker/Cargo.toml` is not empty.
- [ ] `wrangler.toml` `[vars]` sets `GITHUB_API_BASE = "https://api.github.com"` and `GITHUB_WEB_BASE = "https://github.com"`. `scripts/build.mjs` passes `--features $RADAR_FEATURES` to `worker-build` when that variable is set.
- [ ] `probe.rs` (feature `probe`) adds `GET /__probe/get?path=&ims=&etag=` and `GET /__probe/query?doc=Viewer`. They call the client with the bearer value from the `PROBE_TOKEN` variable and return `{outcome, status, meta, body or error}`. Lanes build with `RADAR_FEATURES=probe`. Normal builds and CI do not, so there every `/__probe/*` path gets R01's JSON 404 and never the single page application fallback.
- [ ] `tools/github-stub/server.mjs` is Node with no dependencies. It takes `--port` and `--scenario <name>`, binds `127.0.0.1`, and serves `GET /user`, `GET /rate_limit`, `GET /notifications`, `POST /graphql`, `GET /login/oauth/authorize`, and `POST /login/oauth/access_token`. `/user`, `/rate_limit`, `/notifications`, and `/graphql` need `Authorization` to carry the scenario token, else 401.
- [ ] Stub behavior. `/notifications` honors `If-Modified-Since` and `If-None-Match` with 304, and sends `Last-Modified`, `ETag`, and `X-Poll-Interval`. Every API response carries `x-ratelimit-*` and `X-OAuth-Scopes`. `/graphql` finds the operation name in the query and answers from `graphql[<name>]`, paging by the `after` variable. The authorize endpoint redirects to `redirect_uri` with `code` and the same `state`, or with `error=access_denied` when the scenario denies. The token endpoint checks `client_id`, `client_secret`, and `code`, and answers `bad_verification_code` otherwise.
- [ ] Stub controls. A scenario `faults` list, such as `{ "match": "GET /rate_limit", "status": 500, "times": 3 }` or a 403 with `retry_after`, injects failures. `GET /__log` returns every request with method, path, `If-Modified-Since`, `If-None-Match`, whether `Authorization` was present (never its value), GraphQL operation kind, and a `violation` field. `POST /__scenario` with `{name}` swaps the scenario and `POST /__log/clear` empties the log.
- [ ] The stub defends itself. Any method other than GET, or other than POST on `/graphql` and the token endpoint, and any GraphQL document that is not a `query`, gets 405 or 400 and a `violation` in the log. `tools/github-stub/README.md` documents flags, endpoints, the scenario format, and the log fields. CI runs `node --test scripts tools`.

**You see.**

- [ ] `node tools/github-stub/server.mjs --port 8900 --scenario basic` prints `stub listening on 127.0.0.1:8900 scenario=basic`.
- [ ] With `RADAR_FEATURES=probe`, `curl "http://localhost:8787/__probe/get?path=/user"` returns the stub user with `meta.scopes` listing `notifications` and `read:user`.
- [ ] A second `/__probe/get?path=/notifications&ims=<Last-Modified>` returns `"outcome":"not_modified"`, and `curl http://127.0.0.1:8900/__log` shows the `If-Modified-Since` header was sent.
- [ ] Under `rate-limited`, the probe returns `"error":"rate_limited"` with `retry_after_secs` equal to the stub's `Retry-After`.
- [ ] `cargo test -p worker github::` and `cargo test -p worker --test read_only` pass, and `node --test tools` passes.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `path.rs` gains `accepts_plain_paths` and `rejects_double_slash_dotdot_scheme_at_sign_and_backslash`. `meta.rs` gains `parses_every_header`, `missing_headers_are_none`, and `garbage_numbers_are_none`.
- [ ] `error.rs` or `client.rs` gains `status_304_is_not_modified`, `retry_after_on_403_is_rate_limited`, `retry_after_on_429_is_rate_limited`, `exhausted_budget_on_403_uses_reset`, `plain_403_is_forbidden`, `graphql_errors_in_a_200_are_errors`, and `server_error_carries_status`.
- [ ] `query.rs` gains `rejects_mutation_subscription_and_mixed_documents`, `accepts_queries_with_mutation_in_comments_strings_and_field_names`, and `every_registered_document_is_a_query`. `client.rs` gains `refuses_non_loopback_http_base`, `accepts_https_api_github_com`, and `token_debug_is_redacted`.
- [ ] `tests/read_only.rs` gains `no_write_methods_outside_the_client`, `every_graphql_file_is_registered`, and `probe_is_not_a_default_feature`. A deliberate bad fixture directory proves the scanner fails, in `scanner_fails_on_a_planted_post`.
- [ ] `tools/github-stub/server.test.mjs` gains one case per endpoint, plus `conditional_get_returns_304`, `mutation_is_rejected_and_logged`, `put_is_rejected_and_logged`, `fault_injection_counts_down`, and `oauth_round_trip`. Run `node --test tools`.
- [ ] Run `cargo test -p worker`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build -p worker --target wasm32-unknown-unknown --release`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe. Set `RADAR_FEATURES=probe` before the `npx wrangler dev` step and add `--var PROBE_TOKEN:gho_stubtoken0000`, which matches the `basic` scenario. Migrations are not needed.

- [ ] Lane 1. Regression lane against trunk. Request `/api/health` and load `/` at trunk and at head. Trunk has no client or stub, so record that and gate the behavior the diff adds plus the end state the user waits for, which is a GET through the client to the stub with the stub's log as proof. Save `r06-regression.png`. Pass when `/api/health` is identical on both and the probe returns the stub user at head.
- [ ] Lane 2. Probe `/__probe/get?path=/user` under `basic`, then read the stub log. Save `r06-get.png` as a terminal capture. Pass when the body names `karthik`, `meta.scopes` holds both scopes, the log shows `GET /user` with `hasAuth` true, and the token value appears nowhere in the log or the `wrangler dev` output.
- [ ] Lane 3. Probe `/notifications`, then probe it again with `ims` set to the `Last-Modified` value. Save `r06-conditional.png` as a terminal capture. Pass when the first call is `modified` with a recorded `poll_interval` of 60, the second is `not_modified`, and the stub log shows the `If-Modified-Since` header.
- [ ] Lane 4. Repeat lane 3 with `etag` in place of `ims`. Save `r06-etag.png` as a terminal capture. Pass when the second call is `not_modified` and the log shows `If-None-Match`.
- [ ] Lane 5. Probe `/__probe/query?doc=Viewer`, then switch to `graphql-errors` with `POST /__scenario`. Save `r06-graphql.png` as a terminal capture. Pass when the first call returns parsed data, the stub log shows an operation of kind `query` on `POST /graphql`, and the second returns a `graphql` error with the stub's message.
- [ ] Lane 6. Switch to `rate-limited` and probe `/rate_limit` repeatedly. Save `r06-rate-limit.png` as a terminal capture. Pass when a 403 with `Retry-After` and a 429 both return `rate_limited` with the stub's seconds, a 403 with `x-ratelimit-remaining: 0` and only a reset time returns seconds derived from it, and a plain 403 returns `forbidden`.
- [ ] Lane 7. Send hostile paths. Probe `path=//evil.example/x`, `path=@evil.example/x`, `path=/../x`, `path=https://evil.example`, and `path=/a%5Cb` with a second stub on port `<9000 + n>` playing the other host. Save `r06-paths.png` as a terminal capture. Pass when every call is rejected before any request is made, and the second stub's log is empty.
- [ ] Lane 8. Prove the read-only guard fails closed. In the lane worktree add a `.graphql` file that holds a `mutation`, then run `cargo test -p worker`. Revert, then add a `Method::Put` use outside `github/client.rs` and run it again. Save `r06-guard.png` as a terminal capture. Pass when each run fails and names the file, and each revert returns to green.
- [ ] Lane 9. Break the transport. Stop the stub and probe, then restart it under `server-error` and probe, then under a scenario that returns a body that is not JSON. Save `r06-failures.png` as a terminal capture. Pass when the three calls return `transport`, `server` with 500, and `parse` naming the path, each within 5 s, with no panic, and `/api/health` still returns 200 afterwards.
- [ ] Lane 10. Check secrets and the probe boundary. Grep the `wrangler dev` output for `gho_stubtoken0000`, request `/__probe/get` on a build made without `RADAR_FEATURES`, and start with `--var GITHUB_API_BASE:http://example.com`. Save `r06-boundary.png` as a terminal capture. Pass when the grep finds nothing, the plain build answers a JSON 404 `{"error":"not_found"}` for the probe and not the HTML page, and the bad base is refused with `BadConfig` and no outbound request.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Gzipped Worker size from `node scripts/check-size.mjs` on a default build, and median latency of `/__probe/get?path=/user` against the stub on `wrangler dev`. Worker size exists at trunk and head. The probe does not exist at trunk, so the work the diff adds is one client call and the end state the user waits for is a client that fits the size budget.
- [ ] Probe. Build trunk and head without `RADAR_FEATURES`, run `node scripts/check-size.mjs` on each, interleaved. Build head with `RADAR_FEATURES=probe` and send 50 requests to `/__probe/get?path=/user`, then take the median.
- [ ] Baseline. Record the trunk Worker size first.
- [ ] Rule. Fails if the head Worker is above 400 KB or more than 40 KB above trunk, or if the probe median is above 100 ms on `wrangler dev` with the stub. A default build that contains the string `__probe` also fails.

**Review gate.** None. R06 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R06 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R06 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R06 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Sign in with GitHub as the one allowed user (R07)

**Depends on.** R05.

**Files.**

- [ ] Create `crates/worker/src/auth/mod.rs`, `keys.rs`, `cookie.rs`, `token_box.rs`, `flow.rs`, `session.rs`, `exchange.rs`, and `pages.rs`.
- [ ] Create `.cargo/config.toml` and `.dev.vars.example`.
- [ ] Edit `crates/worker/Cargo.toml` (`aes-gcm`, `hmac`, `sha2`, `base64`, `subtle`, and `getrandom` 0.3 with the `wasm_js` feature on `wasm32`), `crates/worker/src/lib.rs`, `crates/worker/src/db/auth.rs`, and `crates/worker/tests/read_only.rs`.
- [ ] Edit `crates/domain/src/error.rs` and create `crates/domain/src/me.rs`.
- [ ] Create `crates/web/src/auth.rs`, `crates/web/src/views/sign_in.rs`, and `crates/web/style/sign-in.css`. Edit `crates/web/src/app.rs`, `crates/web/src/views/mod.rs`, and `crates/web/index.html`.
- [ ] Edit `AGENTS.md` to add the local sign-in steps.

**Build.**

- [ ] Name the data shapes first. `Session { user_id: GithubUserId(u64), login: Login, epoch: u32, expires_at: Timestamp }` and `SessionError { Missing, BadSignature, Expired, WrongUser, StaleEpoch }`. The callback is one state machine, `enum CallbackOutcome { Signed(Session), Denied { user_id }, BadState, ProviderDenied, ExchangeFailed }`, produced by the pure `flow::decide`. `Me { login, user_id }` and the error codes `Unauthorized` and `Forbidden` go in `crates/domain` (A2).
- [ ] `keys.rs` parses `TOKEN_KEY` and `SESSION_KEY` at the boundary. Each is base64 and must decode to at least 32 bytes, and they must differ. `read_setting(env, name)` tries a secret, then a variable, so `.dev.vars` and Wrangler secrets both work. `ALLOWED_USER_ID`, `OAUTH_CLIENT_ID`, and `OAUTH_CLIENT_SECRET` are read the same way.
- [ ] `cookie.rs` signs `<purpose>|<payload>` with HMAC-SHA256 under `SESSION_KEY`, where purpose is `state` or `session`, and verifies with a constant-time check. A state cookie cannot pass as a session cookie.
- [ ] `GET /auth/login` makes 32 random bytes as the state with `getrandom` and its `wasm_js` backend, sets `radar_oauth_state` (signed, `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/auth`, 10 minutes), and redirects to `<GITHUB_WEB_BASE>/login/oauth/authorize` with `client_id`, `redirect_uri` set to `<origin>/auth/callback`, the `state`, and `scope=notifications read:user` and nothing else (A1).
- [ ] `GET /auth/callback` runs in this order. Check the state cookie signature and expiry and compare it with the `state` parameter in constant time, then clear the cookie, so a replay fails. Stop on `error=access_denied`. Exchange the code at `<GITHUB_WEB_BASE>/login/oauth/access_token` through `exchange.rs`. Call `GET /user` through the R06 client. Compare the numeric `id` with `ALLOWED_USER_ID` (A2). Only then encrypt the token and store it.
- [ ] A user who is not allowed gets a 403 page, no session cookie, and no `auth` row. The page is a short HTML document with `<meta name="color-scheme" content="dark">` and no inline style. Other failures redirect to `/signin?error=state`, `cancelled`, or `exchange`.
- [ ] `token_box.rs` seals the token with AES-256-GCM under `TOKEN_KEY`, a fresh 12 byte nonce from `getrandom`, and the additional data `radar-token-v1:<user id>`, so a ciphertext cannot be moved to another row (A4, N1). `db/auth.rs` stores `token_cipher`, `token_nonce`, and the scopes from the `X-OAuth-Scopes` header. `auth::load_token(env)` returns `Option<Token>` for R08.
- [ ] On success, set `radar_session` (signed, `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/`, 30 days) and redirect to `/` (A3). The cookie payload carries the user id, the login, the `auth` row's current `session_epoch`, and the expiry. A sign-in upserts the `auth` row and never resets `session_epoch`.
- [ ] `require_session(req, env) -> Result<Session, SessionError>` verifies the cookie signature and expiry, then rechecks the user id against `ALLOWED_USER_ID` on every request, so changing that secret locks out an old cookie (`WrongUser`). It then reads `session_epoch` from the `auth` row and rejects a cookie whose epoch differs, or any cookie when no row exists (`StaleEpoch`). `GET /api/me` returns `Me` or 401 `{"error":"unauthorized"}`. R12 reuses `require_session`.
- [ ] `POST /auth/logout` clears the session cookie only (A6). It does not touch `session_epoch`. `POST /auth/disconnect` sets `token_cipher` and `token_nonce` to NULL, increments `session_epoch` so every existing session cookie stops working, and clears the cookie. Both reject a request whose `Origin` is not Radar's own origin with 403.
- [ ] `exchange.rs` applies the base URL rule of R06's `Client::new` to `GITHUB_WEB_BASE`. It accepts only `https://github.com`, or an `http` URL on `127.0.0.1`, `localhost`, or `[::1]` for `wrangler dev` lanes, and answers any other base with `BadConfig` before it builds the authorize redirect or sends a code. Production therefore only ever sends the code and client secret to `https://github.com`.
- [ ] `exchange.rs` makes the one `POST` that is not the GitHub client, the OAuth code exchange. Add `auth/exchange.rs` to the allow-list in `tests/read_only.rs` with a comment that says it exchanges a login code and never calls the GitHub API. Add `getrandom_backend="wasm_js"` to `.cargo/config.toml` for the `wasm32-unknown-unknown` target.
- [ ] `.dev.vars.example` holds obviously fake development values for `OAUTH_CLIENT_ID` (`stub-client`), `OAUTH_CLIENT_SECRET` (`stub-secret`), `TOKEN_KEY`, `SESSION_KEY`, and `ALLOWED_USER_ID` (`1001`, the stub user). Lanes copy it to `.dev.vars`. The operator sets the real values as Wrangler secrets, including `ALLOWED_USER_ID`.
- [ ] The web app asks `GET /api/me` on load. `AuthState { Loading, SignedOut, SignedIn(Me) }` decides the screen. `SignedOut` shows the `sign_in` view, a heading, one sentence, and a full-width "Sign in with GitHub" link to `/auth/login` that is at least 44 px tall, with the message for `?error=`. `SignedIn` shows the R05 shell.

**You see.**

- [ ] With the stub running under `basic`, `/` shows the sign-in page. Clicking "Sign in with GitHub" lands on `/` with the shell and four tabs.
- [ ] The `Set-Cookie` header on the callback carries `HttpOnly`, `Secure`, `SameSite=Lax`, and `Max-Age=2592000`.
- [ ] Signing in as the user in `oauth-other-user` shows a 403 page, and `select count(*) from auth` prints 0.
- [ ] `npx wrangler d1 execute radar --local --command "select length(token_nonce), length(token_cipher), scopes from auth"` prints a 12 byte nonce, a ciphertext longer than the plain token, and the stub scopes.
- [ ] `curl -i http://localhost:8787/api/me` without a cookie returns 401.
- [ ] After the operator's real sign-in on the preview Worker, `/api/me` returns the operator's login. A different GitHub account gets the 403 page.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `cookie.rs` gains `round_trips`, `a_flipped_character_fails`, `a_state_cookie_is_not_a_session`, `expiry_is_exclusive_at_the_boundary`, and `a_different_key_fails`.
- [ ] `token_box.rs` gains `round_trips`, `wrong_key_fails`, `wrong_user_id_in_the_additional_data_fails`, `tampered_ciphertext_fails`, and `a_thousand_seals_use_a_thousand_nonces`.
- [ ] `flow.rs` gains `allowed_id_signs_in`, `other_id_is_denied`, `id_is_compared_as_a_number`, `missing_code_is_bad_state`, `state_mismatch_is_bad_state`, and `provider_error_is_provider_denied`. `keys.rs` gains `short_keys_are_rejected`, `equal_keys_are_rejected`, and `bad_base64_is_rejected`.
- [ ] `session.rs` gains `missing_cookie_is_missing`, `same_origin_guard_rejects_a_foreign_origin_and_a_missing_one`, `a_cookie_for_another_user_id_is_wrong_user`, `a_cookie_with_an_old_epoch_is_stale`, `logout_leaves_the_epoch_unchanged`, and `disconnect_bumps_the_epoch_and_clears_the_token`. An authorize URL test asserts the scope is exactly `notifications read:user` and the state is URL safe.
- [ ] `exchange.rs` gains `accepts_https_github_com_and_loopback_http` and `refuses_any_other_base`, including `https://github.com.evil.example` and `http://example.com`.
- [ ] Run `cargo test -p worker auth::`, `cargo test -p worker --test read_only`, and `cargo build -p worker --target wasm32-unknown-unknown --release`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe. Copy `.dev.vars.example` to `.dev.vars` in each worktree, run the stub with `--scenario basic` unless the lane names another, and use a Chromium browser. Apply migrations with the boot recipe command before starting Radar.

- [ ] Lane 1. Regression lane against trunk. Load `/` and request `/api/health` at trunk and at head. Trunk has no sign-in, so record that and gate the behavior the diff adds plus the end state the user waits for, which is the allowed user reaching the shell with a session. Save `r07-signed-in.png`. Pass when `/api/health` is identical on both and head shows the four tabs after sign-in.
- [ ] Lane 2. Sign in as the allowed user through the browser, then replay the callback with `curl -i` and read `Set-Cookie`. Save `r07-happy-path.png`. Pass when the browser ends on `/` with the shell, the session cookie has `HttpOnly`, `Secure`, `SameSite=Lax`, and `Max-Age=2592000`, and the stub log shows one authorize, one token exchange, and one `GET /user`.
- [ ] Lane 3. Switch to `oauth-other-user` with `POST /__scenario` and sign in. Save `r07-denied.png`. Pass when the page is a 403, no `radar_session` cookie exists, and `select count(*) from auth` is 0.
- [ ] Lane 4. Tamper with the state. Use `curl` with a correct code and a changed `state`, and again with no state cookie. Save `r07-bad-state.png` as a terminal capture. Pass when both redirect to `/signin?error=state` and the stub log shows no `access_token` request for either.
- [ ] Lane 5. Replay a callback. Complete one sign-in, then send the same callback URL again with the old state cookie. Save `r07-replay.png` as a terminal capture. Pass when the second request redirects to `/signin?error=state` and the stub log shows only one token exchange.
- [ ] Lane 6. Inspect the token at rest. Sign in twice, then read `hex(token_cipher)` and `hex(token_nonce)` with `wrangler d1 execute`. Save `r07-at-rest.png` as a terminal capture. Pass when the nonce is 12 bytes and differs between the two sign-ins, the ciphertext does not contain the stub token `gho_stubtoken0000` as bytes, and `scopes` equals the stub scopes.
- [ ] Lane 7. Attack the session. Call `/api/me` with no cookie, a cookie with one changed character, the state cookie in place of the session cookie, a cookie signed with the lane's `SESSION_KEY` for user id `1002`, and the valid cookie. Then restart Radar with `ALLOWED_USER_ID` set to `1002` and replay the valid cookie. Save `r07-session.png` as a terminal capture. Pass when the first four return 401, the valid cookie returns 200 with the login, and the same cookie returns 401 after the restart.
- [ ] Lane 8. Check logout and disconnect. Save the session cookie, log out, and call `/api/me` without it. Sign in again, save that cookie, post a disconnect with `Origin: https://evil.example`, then with the real origin, then replay the saved cookie. Save `r07-logout.png` as a terminal capture. Pass when logout clears the cookie and keeps `session_epoch` and the token, the foreign origin gets 403 and changes nothing, the real origin sets `token_cipher` to NULL and raises `session_epoch` by 1, and the replayed cookie gets 401.
- [ ] Lane 9. Use the sign-in page at 375 px with only the keyboard. Tab to the button and press Enter. Save `r07-phone-keyboard.png`. Pass when the button is full width, at least 44 px tall, shows a focus ring, `scrollWidth` is at most 375, and Enter starts the flow.
- [ ] Lane 10. Fail the provider. Run `oauth-deny` and cancel, then `oauth-bad-code`, then a `server-error` scenario that fails `/user`, then restart Radar with `--var GITHUB_WEB_BASE:http://example.com` and open `/auth/login`. Save `r07-failures.png`. Pass when the first three end on `/signin?error=cancelled`, `/signin?error=exchange`, and `/signin?error=exchange`, each message is readable, the bad base is refused with no redirect to `example.com` and no outbound request, and no session cookie or `auth` row exists after any of them.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `cpuTime` for `GET /auth/callback` and for `GET /api/me`, gzipped Worker size, and local `/api/me` latency. Trunk has no sign-in, so the work the diff adds is the callback and the session check, and the end state the user waits for is one sign-in under the CPU limit. Worker size exists at trunk and head.
- [ ] Probe. Size comes from `node scripts/check-size.mjs` at trunk and head, interleaved. For CPU, the operator approves a deploy with `npx wrangler deploy --env preview`, uses O3b's "Radar preview" OAuth App, whose callback is the preview Worker's `/auth/callback`, runs `npx wrangler tail radar-preview --format json`, then signs in once and calls `/api/me` ten times. The owner reads `cpuTime`. Local `/api/me` latency is the median of 100 `curl` requests.
- [ ] Baseline. Record the trunk Worker size first. CPU has no trunk value.
- [ ] Rule. Fails if the callback `cpuTime` is above 10 ms or the `/api/me` median is above 2 ms. The target is 7 ms for the callback. It also fails if the head Worker is above 400 KB or more than 60 KB above trunk.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 9, lane 3, and lane 1 screenshots into `artifacts/review/R07-review-signin-phone.png`, `artifacts/review/R07-review-denied.png`, and `artifacts/review/R07-review-signed-in.png`.
- [ ] Record a 30 to 60 second video of the full sign-in flow at laptop width, then the 403 page for the other user. Save it as `artifacts/review/R07-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's go.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R07 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R07 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R07 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Run the Syncer Durable Object with a work table and alarms (R08)

**Depends on.** R07.

**Files.**

- [ ] Edit `crates/worker/src/sync/mod.rs` to replace the R01 stub with the real `Syncer`.
- [ ] Create `crates/worker/src/sync/work.rs`, `scheduler.rs`, `gate.rs`, `state.rs`, `store.rs`, `handlers.rs`, and `config.rs`.
- [ ] Create `crates/domain/src/sync_status.rs` and edit `crates/domain/src/lib.rs`.
- [ ] Edit `crates/worker/src/lib.rs` (the `scheduled` handler and the Syncer stub helper) and `crates/worker/src/probe.rs`.
- [ ] Edit `crates/worker/Cargo.toml` to add `toml` as a native dev dependency.

**Build.**

- [ ] Read the R02 result first. Open the section `## Spike result` at the end of `docs/adr/0002-architecture.md`. If its `Decision:` line says `QueueFallback` or `Inconclusive`, stop and tell the operator. The root rewrites this section for the Queue consumer before any sync code exists. Otherwise take the Enrich batch size from that section and set `pub const ENRICH_BATCH: usize` in `crates/worker/src/sync/config.rs` to it, with a doc comment that names the ADR note. R09, R10, and R11 read this constant and never write a literal batch size.
- [ ] Name the data shapes first. `WorkKind { PollNotifications, Enrich(EnrichBatch), BackfillPage(Cursor), Rollup }` maps to the `work` table columns `kind` and `key`. The keys are `main`, a hash of the sorted node ids for `Enrich`, the cursor text or `start` for `BackfillPage`, and `nightly` for `Rollup`, so enqueueing the same unit twice is one row. A row is `WorkRow { kind, due_at, attempts, state: RowState { Pending, Dead }, last_error }`. The table stores `dead` as `0` or `1` under a `CHECK`.
- [ ] `KickReason { Reconcile, Email, Webhook, SignedIn, Manual, Rollup }` is the argument to `kick`. `Gate { Open, WaitUntil(Timestamp) }` is the rate-limit decision. `UnitOutcome { Done { follow_ups: Vec<WorkKind> }, Retry { error }, Deferred { until, cause: DeferCause { RateLimited, NoToken } } }` is what a unit returns.
- [ ] `scheduler.rs` is pure. `plan(rows, state, now) -> Decision { Run(WorkKind), Sleep(Timestamp), Idle }` picks the earliest due, non-dead row whose gate is open. `PollNotifications` waits for `state.next_poll_at`. `Enrich` and `BackfillPage` wait for the rate-limit gate. `Rollup` waits for nothing. `apply(row, outcome, now) -> RowEffect { Delete, Update(row), MarkDead }` is a pure function too. Natively tested code, not the Durable Object, holds every rule.
- [ ] Retry rules (N9). A failed unit increments `attempts`. After the first failure the row is due 10 s later, after the second 60 s later, and after the third it is `Dead` with `last_error` set. A success deletes the row. `plan` never runs a dead row and a dead row never blocks other rows. Only a kick brings one back, as the `kick` box below says, so a GitHub outage cannot stop polling for good.
- [ ] Rate-limit rules (P4). `gate.rs` returns `WaitUntil(reset)` for `Enrich` and `BackfillPage` when `remaining` is below 20% of `limit`. `PollNotifications` still runs, because a 304 costs nothing. A `RateLimited { retry_after_secs }` outcome does not add an attempt. It sets `WaitUntil` to the larger of `Retry-After` and `60 s * 2^strikes` (at most 1 hour), raises `strikes`, and the next success resets `strikes` to 0.
- [ ] `state.rs` is the single-row `sync_state` table in the Durable Object's SQLite (`CHECK (id = 1)`). It holds `last_modified`, `next_poll_at`, `rl_limit`, `rl_remaining`, `rl_reset`, `scopes`, `strikes`, and `last_ok_at` for each `WorkKind`. R09 to R11 use these columns and add none to the shape.
- [ ] `kick(reason)` follows ADR 0002, deferred, not dropped. `Reconcile`, `Email`, `Webhook`, `SignedIn`, and `Manual` insert the `main` poll row if absent, due at the later of now and `state.next_poll_at`. When the `main` row is dead, they reset it to `Pending` with zero attempts and that due time. They set the alarm to the earlier of the current alarm and that time, so many kicks make one alarm. `Rollup`, the nightly kick, inserts the `nightly` row due now and re-arms every dead `BackfillPage` row as `Pending` with zero attempts, due now, so a backfill resumes from its stored cursor.
- [ ] P5. GitHub requests run one at a time per token, never in parallel. There is one `Syncer` instance, `alarm()` runs one unit, and each handler awaits its GitHub calls in sequence and never joins futures over the client, so no two requests overlap.
- [ ] `alarm()` loads the small `work` table, calls `plan`, runs exactly one unit through `handlers.rs`, applies the outcome, writes the results, and sets the next alarm from `plan`. It catches every error and records it in the row, so it never returns `Err` to the platform. It logs `unit start kind=<kind> key=<key>` and `unit done kind=<kind> ms=<n>`.
- [ ] `handlers.rs` holds the `UnitHandler` trait and the placeholder handlers R09 to R11 replace. Each GitHub kind loads the token with `auth::load_token`, makes one `GET /rate_limit` through the R06 client, and records `RateMeta` in `sync_state`. With no token the unit returns `Deferred { cause: NoToken }`, sets no alarm, and waits for a kick. `Rollup` returns `Done`.
- [ ] Cron wiring. `scheduled` maps the cron `*/10 * * * *` to `kick(Reconcile)` and `0 3 * * *` to `kick(Rollup)`. The cron strings are constants, and a native test parses `wrangler.toml` and asserts they match it.
- [ ] Status RPC. The Syncer answers `https://syncer/status` with `domain::sync_status::SyncStatus { pending_by_kind, dead_by_kind, next_alarm, next_poll_at, rate: { limit, remaining, reset }, last_ok_by_kind, blocked: Option<DeferCause> }`. The Syncer is reachable only through the `SYNCER` binding, never through a public route. R12 reads it for `GET /api/freshness`.
- [ ] Probe routes (feature `probe`, from R06). Add `GET /__probe/syncer/status`, `GET /__probe/syncer/kick?reason=`, and `GET /__probe/syncer/enqueue?kind=&key=` so lanes can drive the Syncer.

**You see.**

- [ ] With the stub under `basic` and a signed-in session, `curl "http://localhost:8787/__scheduled?cron=*%2F10+*+*+*+*"` makes the `wrangler dev` log print `unit start kind=poll_notifications key=main` and `unit done kind=poll_notifications`.
- [ ] `curl http://localhost:8787/__probe/syncer/status` prints a `SyncStatus` with `pending_by_kind` and `rate.remaining` from the stub.
- [ ] After a unit fails three times, the status shows `dead_by_kind` with one entry, and the log shows no further attempt for that row until a kick re-arms it.
- [ ] With no token, the status shows `blocked` as `no_token`, and the stub log shows no request.
- [ ] `cargo test -p worker sync::` ends with `test result: ok`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scheduler.rs` gains `picks_the_earliest_due_row`, `skips_dead_rows`, `poll_waits_for_next_poll_at`, `enrich_waits_while_the_gate_is_closed`, `rollup_ignores_the_gate`, and `idle_when_nothing_is_pending`.
- [ ] `apply` tests are `success_deletes_the_row`, `first_failure_retries_in_10_seconds`, `second_failure_retries_in_60_seconds`, `third_failure_marks_dead`, `rate_limited_adds_no_attempt`, and `strikes_double_the_wait_and_reset_on_success`.
- [ ] `gate.rs` gains `below_20_percent_closes_enrich_but_not_poll`, `exactly_20_percent_is_open`, and `reopens_at_reset`. `work.rs` gains `same_unit_twice_is_one_key`, `batch_key_ignores_id_order`, and `kind_round_trips_through_columns`.
- [ ] `kick` tests are `many_kicks_make_one_row`, `kick_during_cooldown_is_deferred_not_dropped`, `rollup_kick_inserts_nightly_once`, `kick_revives_a_dead_poll_row_with_zero_attempts`, `nightly_kick_rearms_a_dead_backfill_row_at_its_cursor`, and `reconcile_kick_leaves_a_dead_enrich_row_dead`. A test `cron_strings_match_wrangler_toml` reads `wrangler.toml`. `sync_status.rs` in `domain` gains `status_round_trips_json`.
- [ ] Run `cargo test -p worker sync::`, `cargo test -p domain`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build -p worker --target wasm32-unknown-unknown --release`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe. Build with `RADAR_FEATURES=probe`, copy `.dev.vars.example` to `.dev.vars`, start Radar with `--test-scheduled`, and sign in once through the browser so the `auth` table holds the stub token, unless the lane says otherwise. Use the `fast-poll` scenario, which sets `X-Poll-Interval` to 5 s, unless the lane names another. Lane captures are terminal captures.

- [ ] Lane 1. Regression lane against trunk. At trunk, call the cron route and read the stub log. Trunk's Syncer is the R01 stub that only logs, so record that and gate the behavior the diff adds plus the end state the user waits for, which is one cron firing producing exactly one GitHub request and a visible status. Save `r08-regression.png`. Pass when trunk logs `syncer kick` with no request, and head logs one `unit done` with exactly one request in the stub log.
- [ ] Lane 2. Fire the 10-minute cron once. Save `r08-cron.png`. Pass when the status shows the `main` poll row consumed, the stub log has one `GET /rate_limit`, and `next_poll_at` is about 5 s after the unit.
- [ ] Lane 3. Send 100 `kick?reason=email` requests in 2 s. Save `r08-coalesce.png`. Pass when the stub log shows at most one poll per 5 s window over the next 30 s, and a kick sent inside a cooldown still produces one poll at the next allowed time rather than none.
- [ ] Lane 4. Enqueue five `enrich` rows with distinct keys. Save `r08-one-unit.png`. Pass when the log shows five separate `unit start` and `unit done` pairs, never two units in one alarm, in `due_at` order, and the table ends empty.
- [ ] Lane 5. Under a scenario whose `faults` fail `GET /rate_limit` with 500 three times, enqueue one `enrich` row. Save `r08-dead.png`. Pass when the row shows `attempts` 1 then 2 then 3 with the gaps close to 10 s and 60 s, ends `dead` with `last_error` set, and later rows still run.
- [ ] Lane 6. Switch to `low-budget`, where 8% remains and the reset is 20 s away, and enqueue one `enrich`, one `backfill_page`, and one `rollup`. Save `r08-gate.png`. Pass when the poll and the rollup run at once, the `enrich` and `backfill_page` rows wait with the gate open time equal to the reset, and they run within 5 s after it.
- [ ] Lane 7. Under `rate-limited`, where the first `GET /rate_limit` gets a 403 with `Retry-After: 15`, enqueue one `enrich`. Save `r08-retry-after.png`. Pass when no attempt is added, the next request comes at least 15 s later, the strike count rises then returns to 0 after a success, and the row ends done.
- [ ] Lane 8. Enqueue ten `enrich` rows, kill `wrangler dev` after three complete, then restart with the same `--persist-to`. Save `r08-restart.png`. Pass when the remaining rows complete after the restart, no completed row runs again, the table ends empty, and the stub log holds ten requests plus at most one for the interrupted unit.
- [ ] Lane 9. Fire the nightly cron `0 3 * * *` twice before the first unit ends. Save `r08-rollup.png`. Pass when the log shows one `unit done kind=rollup`, the `nightly` key never has two rows, and no GitHub request is made.
- [ ] Lane 10. Boundaries. Empty the `auth` table, kick, and watch for 10 s. Then request `curl -i http://127.0.0.1:<8800 + n>/__syncer/status`, `curl -i http://127.0.0.1:<8800 + n>/api/syncer/status`, and `curl -i http://127.0.0.1:<8800 + n>/syncer/status`. Save `r08-boundaries.png`. Pass when the status shows `blocked` as `no_token`, the stub log is empty, the alarm fired at most twice in 10 s, no row is dead, the first two requests return a JSON 404 `{"error":"not_found"}`, and the third returns the HTML page with no `SyncStatus` field in it.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `cpuTime` per Syncer alarm on Cloudflare, cold and warm, for each unit kind, from `wrangler tail`. Also GitHub requests per hour projected from the stub log of a 10-minute run, and gzipped Worker size. Trunk's Syncer is a stub, so the work the diff adds is one alarm unit. The end state the user waits for is a poll cycle that stays under 10 ms of CPU and well under 500 calls per hour.
- [ ] Probe. The operator approves a deploy with `npx wrangler deploy --env preview` after signing in on the preview, runs `npx wrangler tail radar-preview --format json`, and lets the 10-minute cron fire at least three times, then the owner runs the spike's `summarize.mjs` on the output. If the preview has no token, the owner reports the `NoToken` path only and asks for a sign-in. For calls per hour, run a 10-minute local session under `fast-poll` with 100 kicks and the cron, then count `GET` and `POST` lines in the stub log and scale to an hour. Size comes from `node scripts/check-size.mjs` at trunk and head, interleaved.
- [ ] Baseline. Record the trunk Worker size first. CPU has no trunk value, so the reference is the R02 result in the ADR note.
- [ ] Rule. Fails if any alarm `cpuTime` is above 10 ms, if a unit kind averages above 7 ms warm, if the projected calls per hour are above 500, or if the head Worker is above 400 KB or more than 60 KB above trunk.

**Review gate.** None. R08 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R08 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R08 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R08 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Poll notifications without spending rate limit (R09)

**Depends on.** R08.

**Files.**

- [ ] Create `crates/worker/src/sync/poll.rs`. It holds `PollPhase`, `PollPlan`, the pure `plan_poll`, and `run`, the handler for `WorkKind::PollNotifications`.
- [ ] Edit `crates/worker/src/sync/mod.rs` so the Syncer alarm routes `PollNotifications` to `poll::run` and routes `Enrich` rows to R08's placeholder handler, which logs and succeeds until R10.
- [ ] Edit `crates/worker/src/db/threads.rs` (R04) to add `known_updated_at(ids) -> HashMap<ThreadId, String>`.
- [ ] Create `crates/worker/tests/fixtures/notifications_page1.json` and `notifications_page2.json`, each taken from a real `GET /notifications` shape with three threads (a pull request, an issue, and a `CheckSuite`).
- [ ] Create `tools/github-stub/scenarios/poll-basic.json`, `poll-slow.json`, `poll-pages.json`, `poll-pages-fail-once.json`, `poll-429.json`, `poll-ignores-ims.json`, and `poll-500.json`, in the format R06 defined.
- [ ] Edit `tools/github-stub/server.mjs` only for what R06's stub lacks among `Link` pagination, a scripted failure sequence, `Retry-After`, and an `If-Modified-Since` check against `Last-Modified`. Make each addition its own commit.

**Build.**

- [ ] Name the data shape first. In `sync/poll.rs` define `enum PollPhase { Idle { last_modified: Option<String> }, Scanning { committed: Option<String>, incoming: String, next_page: u32 } }`, stored in the Syncer's storage under `poll_phase`. A scan only commits its new `Last-Modified` when the last page succeeds, so a failed page 2 can never hide the threads on it behind a later 304.
- [ ] Define the boundary struct `NotificationThread { id, unread, reason, updated_at, subject: Subject { title, url, kind }, repository: Repo { full_name } }` and `enum SubjectKind { PullRequest, Issue, Other }` parsed from `subject.type`. A page that lacks a required field fails as a whole and plans nothing. Unknown fields are ignored (N5, store metadata only).
- [ ] Write the pure `plan_poll(phase, page, known, now) -> PollPlan`. `PollPlan` holds `upserts`, `enrich_batches` (thread ids, at most `ENRICH_BATCH` from `crates/worker/src/sync/config.rs` per batch, only `PullRequest` and `Issue` subjects whose `updated_at` differs from `known` or is new, P3), `next_phase`, `next_poll_at`, and `follow_up: FollowUp { None, NextPage(u32) }`.
- [ ] Request `Client::get("/notifications?all=true&per_page=50&page=N")`. Send `If-Modified-Since` only on page 1 of an `Idle` phase. `all=true` keeps read threads, because reading does not clear an item (ADR 0002).
- [ ] Set `next_poll_at = now + X-Poll-Interval` seconds, or 60 s when the header is absent (P1). Write it where R08's `kick` reads it, so a kick before that time is deferred, not dropped (P2). A 304 plans no upserts and no enrich rows.
- [ ] Map a 429, or a 403 with `Retry-After` (which R06's client reports as a limit), to `PollOutcome::Limited { retry_at }`. Move the same work row to `retry_at` and leave `attempts` unchanged, because a limit is not a failed unit (P4).
- [ ] In `poll::run`, upsert all threads of the page in one D1 batch, enqueue one `Enrich` row per batch keyed by its sorted thread ids (so enqueueing twice leaves one row), and enqueue the next page as the same `PollNotifications` kind when `FollowUp::NextPage` is set. Delete the row on success.
- [ ] Log one structured line per outcome. `sync.poll page=1 status=200 threads=3 changed=3 next_poll_at=<iso>`, `sync.poll status=304 next_poll_at=<iso>`, `sync.poll status=429 retry_at=<iso>`, and `sync.enqueue kind=enrich threads=<n>`.
- [ ] Use only `Client::get`. Nothing in this PR can send a non-GET request (N2).

**You see.**

- [ ] After one reconcile kick against `poll-basic`, the `wrangler dev` log shows `sync.poll page=1 status=200 threads=3 changed=3` and `sync.enqueue kind=enrich threads=2`.
- [ ] `npx wrangler d1 execute radar --local --command "select id, reason, unread, updated_at from notification_threads"` lists the three threads, including one with `unread = 0`.
- [ ] A second kick after the poll interval logs `sync.poll status=304`, and the stub's request log shows that request carried the `Last-Modified` value of the first response in `If-Modified-Since`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/worker/src/sync/poll.rs` gains table-driven cases for `plan_poll`. A first page of three threads yields three upserts and one enrich batch of two ids. A thread with an unchanged `updated_at` yields no enrich entry. 45 changed threads yield full batches of `ENRICH_BATCH` and the remainder last, which is 20, 20, and 5 when the constant is 20. The test reads the constant and never hard-codes 20. Run `cargo test -p worker poll`.
- [ ] Phase cases. A page with a `rel="next"` link yields `Scanning` with `next_page = 2` and does not commit the new `Last-Modified`. The last page yields `Idle` holding the page-1 value. A 304 keeps the phase and sets `next_poll_at = now + interval`. Run `cargo test -p worker poll`.
- [ ] Header and boundary cases. A missing `X-Poll-Interval` yields 60 s and a value of 120 yields 120 s. A page missing `updated_at` returns an error with an empty plan. An unknown subject type parses as `Other`. A `Link` header parser test covers one, two, and no `next` relations. Run `cargo test -p worker poll`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`. Sign in once with `curl -sL -c jar.txt http://127.0.0.1:<8800 + n>/auth/login`, which runs R07's flow against the stub and stores the token the Syncer reads. Kick the reconcile cron with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"`, using the cron string in `wrangler.toml`. Read the stub's request log the way `tools/github-stub/README.md` says. Each saved capture is a terminal screenshot.

- [ ] Lane 1. Regression lane against trunk. Run `curl http://127.0.0.1:<8800 + n>/api/health` at trunk and at head and compare the JSON. Trunk has no polling, so record that and gate the behavior the diff adds plus the end state the user waits for. At head, boot `poll-basic` (three threads, `Last-Modified` set, `X-Poll-Interval: 5`), kick once, and read D1. Save `regression-first-poll.png`. Pass when the health JSON is identical at both SHAs, `select count(*) from notification_threads` returns 3, and the stub log holds exactly one `GET /notifications` with no `If-Modified-Since`.
- [ ] Lane 2. A second poll costs nothing. Boot `poll-basic`, kick, wait 6 s, kick again. Save `second-poll-304.png`. Pass when the second request carries `If-Modified-Since` equal to the first `Last-Modified`, the dev log shows `status=304`, and D1 rows are byte-identical to the first poll (compare `select * from notification_threads order by id`).
- [ ] Lane 3. Only changed threads are enriched. After lane 2's state, switch the stub to the step of `poll-basic` where one pull request thread has a newer `updated_at` and the `CheckSuite` thread is also newer. Kick after 6 s. Save `changed-thread-enqueue.png`. Pass when the log shows `changed=2` and `sync.enqueue kind=enrich threads=1`, because the `CheckSuite` is stored but never enriched.
- [ ] Lane 4. The poll interval is obeyed. Boot `poll-slow` (`X-Poll-Interval: 30`). Kick at 0 s, 10 s, and 20 s, then wait until 40 s. Save `poll-interval-obeyed.png`. Pass when the stub log holds exactly two `GET /notifications` requests, the second 30 to 35 s after the first and with no third kick needed, so the deferred alarm fired by itself (P1, P2).
- [ ] Lane 5. A burst coalesces into one poll. On a fresh database, run `for i in 1 2 3 4 5; do curl -s "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*" & done; wait`. Save `burst-coalesces.png`. Pass when the stub log holds exactly one `GET /notifications` and the dev log holds exactly one `sync.poll` line (P2).
- [ ] Lane 6. Pagination. Boot `poll-pages` (120 threads over three pages with `Link` headers). Kick once and wait for the scan to end. Save `three-page-scan.png`. Pass when D1 holds 120 rows, the stub log shows `page=1`, `page=2`, `page=3` in order as separate requests, and only page 1 could carry `If-Modified-Since`.
- [ ] Lane 7. A failed page cannot hide threads. Boot `poll-pages-fail-once`, where page 2 answers 500 once. Wait for the scan to complete. Save `mid-scan-failure.png`. Pass when D1 ends with 120 rows, page 2 was requested twice, no request between the failure and the completion carried a new `If-Modified-Since`, and the next poll after completion carries the page-1 `Last-Modified`.
- [ ] Lane 8. Rate limit backoff. Boot `poll-429`, which answers 429 with `Retry-After: 15` and then 200. Save `retry-after-honored.png`. Pass when no second request arrives before 15 s, exactly one arrives after, the dev log shows `status=429 retry_at=`, there is no `sync.dead` line, and D1 fills after the retry (P4).
- [ ] Lane 9. Idempotency and read-only. Boot `poll-ignores-ims`, which always answers 200 with the same data. Run three polls. Save `idempotent-and-read-only.png`. Pass when `select count(*), max(updated_at) from notification_threads` is identical after each poll, `sync.enqueue kind=enrich` appears only after the first, the thread with `unread = 0` stays in D1, and every request in the stub log is a `GET` (N2).
- [ ] Lane 10. Dead after three failures, then recovery. Boot `poll-500`, which always answers 500. Wait for the work row to exhaust its attempts. Save `poll-dead-then-recover.png`. Pass when the stub saw exactly three poll requests for that row, the dev log shows `sync.dead kind=poll_notifications` (N9), and after the stub is switched to `poll-basic` the next cron kick resets the dead row to zero attempts, as R08's `kick` does, then polls successfully and D1 fills.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. GitHub requests in a 10-minute window while the cron is kicked every 10 s, counted from the stub's request log, and the gzipped Worker size from `node scripts/check-size.mjs`. Trunk has no polling, so the request count is an absolute budget for the work the diff adds. The size is measured at both sides.
- [ ] Probe. Boot `poll-basic` with `X-Poll-Interval: 60`. Run `for i in $(seq 60); do curl -s "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*" > /dev/null; sleep 10; done`, then count `GET /notifications` lines in the stub log. Run `node scripts/check-size.mjs` at trunk and at head, interleaved with the probe.
- [ ] Baseline. Trunk lacks polling, so there is no trunk request count. Record the trunk gzipped Worker size first.
- [ ] Rule. Fail if the window holds more than 11 requests or any two requests are under 60 s apart. That extrapolates to at most 66 requests an hour against the budget of 500. Fail if the Worker grows more than 15 KB gzipped over trunk or passes 400 KB.

**Review gate.** None. R09 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R09 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R09 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R09 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Enrich pull requests and issues with GraphQL (R10)

**Depends on.** R09.

**Files.**

- [ ] Create `crates/worker/src/sync/enrich.rs`. It holds `EnrichBatch`, `Outcome`, the mapping to the domain input, and `run`, the handler for `WorkKind::Enrich`.
- [ ] Create `crates/worker/src/github/enrich_response.rs` with the serde structs for the GraphQL response.
- [ ] Edit the query constants in `crates/worker/src/github/` to add `ENRICH_FRAGMENT` and `build_enrich_query(n)`, built through R06's `QueryDoc`.
- [ ] Edit `crates/worker/src/db/items.rs` and `crates/worker/src/db/events.rs` (R04) so upserts write the new columns and `events` rows are insert-or-ignore.
- [ ] Create `crates/domain/src/event.rs` with the event model below, and export it from `crates/domain/src/lib.rs`. R20, R21, R22, and R24 read it.
- [ ] Create `migrations/<next number>_item_extras.sql` adding `items.requested_reviewers` (JSON array of logins, default `'[]'`), `items.stage_since`, and `items.latest_url`. `items.head_ref` already exists from R04's `0001_init.sql`, so this migration does not add it. Use the next free number on `main`, and never edit an earlier migration.
- [ ] Create `crates/worker/tests/fixtures/enrich_20.json`, the 20-item response shape of `spikes/rust-fit/measure.mjs` extended with the new fields, and `enrich_edge.json` (ghost author, null alias with `NOT_FOUND`, a bot comment).
- [ ] Create `tools/github-stub/scenarios/make.mjs`. It writes a scenario file for a named kind, with timestamps relative to now, to stdout. Each file carries an `_expect` object that the stub ignores. Kinds are `enrich-basic`, `enrich-bot`, `enrich-read`, `enrich-45`, `enrich-gone`, `enrich-limited`, `enrich-502`, and `enrich-body-marker`.
- [ ] Edit `tools/github-stub/server.mjs` so `/graphql` answers the enrich query. It reads the `$o<i>`, `$n<i>`, `$i<i>` variables and answers each alias from `scenario.items["owner/repo#number"]`, with `null` plus a `NOT_FOUND` error for unknown keys.
- [ ] Edit `.gitignore` to ignore `tools/github-stub/scenarios/generated/`.

**Build.**

- [ ] Name the data shape first. `EnrichBatch { threads: Vec<ThreadId> }` has at most `ENRICH_BATCH` entries, the constant R08 set from R02's result. The response is `enum ItemNode { PullRequest(PrNode), Issue(IssueNode) }`, tagged by `__typename`, and `Actor { login, kind: ActorKind { User, Bot } }` comes from `author.__typename == "Bot"`. An alias that comes back `null` is `Outcome::Gone`, never an error for the batch.
- [ ] A REST notification carries no node id, so address each item by repository and number. `build_enrich_query(n)` emits `a<i>: repository(owner: $o<i>, name: $n<i>) { issueOrPullRequest(number: $i<i>) { __typename ...ItemFields } }` with typed variables, plus `rateLimit { cost remaining resetAt }`. The returned node id becomes the item key.
- [ ] `ItemFields` selects state, draft flag, dates, author, `reviewDecision`, `reviewRequests(first: 10)`, `latestReviews(first: 10)`, the last commit's `oid` and `statusCheckRollup { state }`, `headRefName`, assignees, and `timelineItems(since: $s<i>, first: 50, itemTypes: [...])` with each node's `id`, `__typename`, actor, time, review state, and `url`, where `$s<i>` is the item's stored `updated_at`. It never selects a `body` field (N5).
- [ ] Parse once at the boundary into `ItemNode`, then map to the input struct of `domain::classify` from R03. A mention comes only from the notification reason R09 stored. When it is `mention` or `team_mention`, the mapper sets `mentions_me` on the newest human comment after my last activity, so that comment is the mention R03 classifies. Radar never scans comment bodies (N5). The reason `assign` enters as the assign signal. A `null` author becomes a `User` named `ghost`. The signed-in `Login` comes from the `auth` row.
- [ ] Write `run(batch)` as one unit. Load the threads from D1, build the query, call `Client::query`, parse, classify, then write all `items` and `events` rows in one D1 batch. Record `rateLimit.remaining` for the gate (P4).
- [ ] Set the derived columns with `domain::classify`. Upsert `stage_since` in SQL as `CASE WHEN items.stage = excluded.stage THEN items.stage_since ELSE excluded.stage_since END`, so the stage clock restarts only when the stage changes. A newly seen item starts its clock at its latest known event time.
- [ ] Set R04's `head_ref` column from `headRefName`, and let the Pipeline and rows treat a `head_ref` that starts with `agent/` as an agent item (F11). Set `requested_reviewers` from `reviewRequests`. Set `latest_url` from the newest timeline node's `url`, and fall back to the item URL.
- [ ] Record one `events` row per GitHub timeline item since the last sync, never one per thread update. The shape in `crates/domain/src/event.rs` is `Event { id, item, repo, kind: EventKind, actor: EventActor { login, is_me, is_bot }, occurred_at }` with `EventKind { Opened, Commented, Reviewed(ReviewState), ReviewRequested, Assigned, Mentioned, Committed, Merged, Closed, Reopened, CiFailed, CiPassed }`, stored in R04's columns. `id` is the timeline item's GitHub node id (N8). `Opened` comes from the item's `createdAt` the first time Radar sees it. `Mentioned` is the comment the mapper marked, with id `mention:<comment node id>`.
- [ ] CI events come from the status rollup change between syncs. When the stored `ci_state` and the new rollup state differ and the new state is `FAILURE` or `SUCCESS`, write `CiFailed` or `CiPassed` with `EventId::for_ci(item, commit oid, state)`. Insert every event with `ON CONFLICT DO NOTHING`, so a retried batch writes nothing new.
- [ ] Map failures. A 5xx or network error returns `Err` so R08 retries and, after 3 attempts, marks the row dead. A GraphQL `RATE_LIMITED` error in a 200 body delays the row until `resetAt` with no write. A `NOT_FOUND` alias logs `enrich.gone` and the rest of the batch still writes.
- [ ] Log `sync.enrich batch=<n> enriched=<n> gone=<n> events=<n> remaining=<n>`.

**You see.**

- [ ] After a kick against `enrich-basic`, the log shows `sync.poll ... changed=6`, then `sync.enrich batch=6 enriched=6 gone=0 events=<n>`, where `n` equals the scenario's `_expect.events`, one per timeline item.
- [ ] `npx wrangler d1 execute radar --local --command "select repo, number, turn, reason, stage from items order by since"` lists six rows that match the scenario's `_expect` values.
- [ ] The stub's request log shows one `POST /graphql` for the whole batch, and its body starts with `query`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/worker/src/github/enrich_response.rs` parses `enrich_20.json` into 20 nodes, and `enrich_edge.json` into one `Gone`, one ghost author, and one `Bot` actor. A response with a missing required field returns an error. Run `cargo test -p worker enrich`.
- [ ] `build_enrich_query(20)` and `build_enrich_query(1)` start with `query`, contain no `mutation`, never contain `body`, and the existing N2 test over every query constant and builder output still passes. Run `cargo test -p worker`.
- [ ] Event ids are deterministic. Building events twice from the same fixture yields the same ids, one per timeline node, and a new timeline node yields one new id. A rollup change from failing to passing yields one `CiPassed` event, and an unchanged rollup yields none. A `mention` reason yields one `Mentioned` event on the newest human comment after mine, and a bot comment never does. Run `cargo test -p worker enrich`.
- [ ] The mapping test feeds `enrich_edge.json` through the mapper and `domain::classify`. A bot comment after my comment leaves `Turn::Them`. An empty `reviewDecision` takes the stage from the latest review per reviewer. Run `cargo test -p worker enrich`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Generate the scenario with `node tools/github-stub/scenarios/make.mjs <kind> > tools/github-stub/scenarios/generated/<kind>.json` and boot the stub with `--scenario generated/<kind>`. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`. Sign in once with `curl -sL -c jar.txt http://127.0.0.1:<8800 + n>/auth/login`. Kick with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"` and wait for the `sync.enrich` log line. Read the stub's request log the way `tools/github-stub/README.md` says.

- [ ] Lane 1. Regression lane against trunk. Run `curl http://127.0.0.1:<8800 + n>/api/health` at trunk and head. Trunk has no enrichment, so record that and gate the behavior the diff adds plus the end state the user waits for. At head, boot `enrich-basic` and kick once. Save `regression-enrich-basic.png`. Pass when the health JSON matches at both SHAs, `select count(*) from items` returns 6, and `select count(*) from events` equals the scenario's `_expect.events`.
- [ ] Lane 2. Every turn and stage is classified. Boot `enrich-basic`, which holds a pull request that asks for my review, my pull request with changes requested, my pull request with failing CI, my pull request awaiting review where I commented last, an issue where a maintainer replied after me, and my draft pull request. Save `classified-turns.png`. Pass when `select turn, reason, stage from items` matches `_expect` for each row, including `me / review_requested`, `me / changes_requested`, `me / ci_failed`, `them`, `me / replied`, and stage `Draft`.
- [ ] Lane 3. Bots never flip the turn. Boot `enrich-bot`, where `dependabot[bot]` comments after my comment. Save `bot-does-not-flip.png`. Pass when the item stays `turn = them`, its `since` is the time of my comment, and the bot's comment is its own `commented` event row with `actor_is_bot = 1`.
- [ ] Lane 4. Reading does not clear. Boot `enrich-read`, where a thread has `unread: false` and its newest activity is aimed at me. Save `read-stays-mine.png`. Pass when the item is `turn = me` and no code path changed `unread` (the stub log shows only `GET` and `POST /graphql`).
- [ ] Lane 5. Batching. Boot `enrich-45` (45 changed pull request threads). Save `enrich-batches.png`. Pass when the stub log shows exactly ceil(45 / `ENRICH_BATCH`) `POST /graphql` requests, each carrying at most `ENRICH_BATCH` aliases (20, 20, and 5 when the constant is 20), D1 holds 45 items, and every request body starts with `query` (N2).
- [ ] Lane 6. Idempotency. Boot `enrich-basic`, kick, record `select count(*) from items` and `from events`, then force a second full enrich by switching the stub to an `If-Modified-Since` ignoring step and kicking twice more. Save `enrich-idempotent.png`. Pass when both counts are unchanged, no event id repeats (`select id, count(*) from events group by id having count(*) > 1` is empty), and `stage_since` is unchanged for rows whose stage did not change (N8).
- [ ] Lane 7. Stage clock. Boot `enrich-basic`, kick, then switch the stub to the step where one pull request moves from CI running to CI failing and kick after the poll interval. Save `stage-since-restarts.png`. Pass when only that row's `stage_since` moved to the new event time and every other `stage_since` is unchanged.
- [ ] Lane 8. A missing item does not poison the batch. Boot `enrich-gone`, where one of 20 aliases returns `null` with `NOT_FOUND`. Save `gone-alias.png`. Pass when 19 items are written, the log shows `gone=1`, and no row is dead.
- [ ] Lane 9. Failure paths. Boot `enrich-limited` (a 200 body with a `RATE_LIMITED` error) and then `enrich-502` (three 502 responses). Save `enrich-failures.png`. Pass when `RATE_LIMITED` writes no rows and delays the row until `resetAt`, and three 502 responses yield `sync.dead kind=enrich` with no partial rows (N9).
- [ ] Lane 10. No content is stored. Boot `enrich-body-marker`, whose comment bodies contain the string `BODYMARK-7f3a` in the stub's responses even though the query never asks for them. Save `no-bodies-stored.png`. Pass when `select * from items`, `select * from events`, and the `wrangler dev` log all lack `BODYMARK-7f3a` (N5), and the request body never contains the word `body`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. CPU per Enrich alarm for a full batch of `ENRICH_BATCH` items, as `cpuTime` from `npx wrangler tail --format json` on a preview deploy the operator approves. GraphQL requests for 45 changed threads, from the stub log. Gzipped Worker size. Trunk lacks enrichment, so these are absolute budgets for the work the diff adds and for the end state of a full sync.
- [ ] Probe. The operator deploys the PR head to a preview Worker with its own D1 database and signs in with the real account. Run `npx wrangler tail <preview name> --format json` and record `cpuTime` for ten consecutive Enrich alarms, the first of which is a cold instance. Run `enrich-45` locally and count requests. Run `node scripts/check-size.mjs`, interleaved with the probe.
- [ ] Baseline. Trunk has no enrichment, so there is no trunk CPU number. Record the trunk gzipped Worker size first.
- [ ] Rule. Fail if any warm Enrich alarm reports `cpuTime` above 7 ms, if the cold one reports above 10 ms, or if 45 threads need more than ceil(45 / `ENRICH_BATCH`) GraphQL requests. Fail if the Worker grows more than 40 KB gzipped over trunk or passes 400 KB. If a unit nears 7 ms, the owner lowers `ENRICH_BATCH` in `crates/worker/src/sync/config.rs` before handover and says so in the PR body.

**Review gate.** None. R10 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R10 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R10 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R10 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Backfill 90 days and run the nightly search (R11)

**Depends on.** R10.

**Files.**

- [ ] Create `crates/worker/src/sync/backfill.rs`. It holds `SearchWindow`, `BackfillCursor`, the pure `next_step`, and `run`, the handler for `WorkKind::BackfillPage`.
- [ ] Edit `crates/worker/src/sync/mod.rs` to route `BackfillPage`, to enqueue the first page when sign-in stores a token, and to enqueue the nightly search when the nightly kick fires.
- [ ] Edit the query constants in `crates/worker/src/github/` to add `SEARCH_QUERY`, built from R10's `ItemFields`.
- [ ] Edit `crates/worker/src/sync/enrich.rs` to expose the item and event writers for reuse. Do not copy them.
- [ ] Create `crates/worker/tests/fixtures/search_page.json` and `search_last_page.json`.
- [ ] Create `scripts/compare-sync.mjs`, which compares the item set in D1 with an expected set and prints missing and extra items.
- [ ] Edit `tools/github-stub/server.mjs` so `/graphql` answers `SEARCH_QUERY` from `scenario.search.authored` and `scenario.search.involves` by cursor, and edit `tools/github-stub/scenarios/make.mjs` to add kinds `backfill-150`, `backfill-empty`, `backfill-overlap`, `backfill-old`, `nightly-new`, and `low-budget`.

**Build.**

- [ ] Name the data shape first. `enum SearchWindow { Backfill90d, Nightly7d }` sets the `updated:>=` date. `enum SearchKind { Authored, AuthoredIssues, Involves }` picks the query text, `is:pr author:@me`, `is:issue author:@me`, or `involves:@me`. `BackfillCursor { window, kind, after: Option<String> }` lives inside the work row, so a restart resumes from it. `enum Step { Page(BackfillCursor), Done }` is the pure result of `next_step`.
- [ ] Fetch one page of `ENRICH_BATCH` items per alarm with `search(type: ISSUE, first: $first, after: $after, query: $q)`, where `$first` is `ENRICH_BATCH` from `crates/worker/src/sync/config.rs`, selecting `ItemFields` plus `pageInfo { endCursor hasNextPage }` and `rateLimit`. One page is one unit, so the CPU cost matches an Enrich batch.
- [ ] Chain the work. A first sign-in enqueues `Authored`, then `AuthoredIssues`, then `Involves` for `Backfill90d`. Each page that reports `hasNextPage` enqueues the same kind with the new cursor. `Done` for all three kinds records `backfill_done_at` in the Syncer's storage, so a second sign-in does not backfill again.
- [ ] Reuse R10's mapping, classification, and writers for every search node. Events carry the timeline node ids from R10, so an item found by both a notification and a search writes each timeline item once.
- [ ] The nightly kick enqueues `Nightly7d` for all three kinds. R08's nightly kick also re-arms a dead `BackfillPage` row at its stored cursor, so a backfill that died resumes instead of stopping for good. It catches my pull requests that produced no notification (PRD data sources).
- [ ] Respect R08's rate-limit gate. Below 20% remaining, no backfill row runs until `resetAt`.
- [ ] Build `scripts/compare-sync.mjs --db local --persist-to .wrangler/lane-<n> --expect generated/<kind>` for the stub and `--db remote --database <preview database> --source gh` for the real account. The `gh` mode uses `gh search prs --author @me`, `gh search issues --author @me`, and `gh search issues --involves @me`, which only read.
- [ ] Log `sync.backfill window=<w> kind=<k> page=<n> items=<n> next=<cursor|done>`.

**You see.**

- [ ] After sign-in against `backfill-150`, the log shows `sync.backfill window=Backfill90d kind=Authored page=1 items=<ENRICH_BATCH> next=<cursor>` on its own alarm, one line per page, until `next=done` for all three kinds.
- [ ] `node scripts/compare-sync.mjs --db local --persist-to .wrangler/lane-<n> --expect generated/backfill-150` prints `0 missing, 0 extra`.
- [ ] Operator item, checked by the operator. On the preview deploy with the operator's real account, the operator runs `node scripts/compare-sync.mjs --db remote --database <preview database> --source gh`, sees `0 missing, 0 extra`, posts the output in chat, and checks this box. M1 is done only when this box is checked. It does not gate the verdict.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `next_step` cases in `sync/backfill.rs`. A page with `hasNextPage` yields the same kind with the new cursor. The last `Authored` page yields `AuthoredIssues` at `after: None`, whose last page yields `Involves` at `after: None`. The last `Involves` page yields `Done`. Run `cargo test -p worker backfill`.
- [ ] Window cases. `Backfill90d` and `Nightly7d` produce the expected `updated:>=` date for a fixed `now`, and the query strings contain `is:pr author:@me`, `is:issue author:@me`, and `involves:@me` and nothing else but the date. Run `cargo test -p worker backfill`.
- [ ] `SEARCH_QUERY` starts with `query`, has no `mutation` and no `body`, and passes R06's N2 test. `search_page.json` and `search_last_page.json` parse. Run `cargo test -p worker`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Generate each scenario with `node tools/github-stub/scenarios/make.mjs <kind> > tools/github-stub/scenarios/generated/<kind>.json` and boot the stub with `--scenario generated/<kind>`. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`. Sign in with `curl -sL -c jar.txt http://127.0.0.1:<8800 + n>/auth/login`. The nightly kick uses the nightly cron string from `wrangler.toml` at `/__scheduled?cron=<nightly cron, + for spaces>`. Read the stub's request log the way `tools/github-stub/README.md` says.

- [ ] Lane 1. Regression lane against trunk. Run `curl http://127.0.0.1:<8800 + n>/api/health` at trunk and head. Trunk has no backfill, so record that and gate the behavior the diff adds plus the end state the user waits for. At head, boot `backfill-150` (150 authored pull requests, 20 authored issues, and 60 involved items, some shared) and sign in. Save `regression-backfill-end-state.png`. Pass when health JSON matches at both SHAs and `node scripts/compare-sync.mjs --db local --persist-to .wrangler/lane-<n> --expect generated/backfill-150` prints `0 missing, 0 extra`.
- [ ] Lane 2. One page per alarm. Boot `backfill-150` and watch the log and the stub during the run. Save `one-page-per-alarm.png`. Pass when each `sync.backfill` line has `items` of at most `ENRICH_BATCH`, the stub log shows one GraphQL `POST` at a time with `first` equal to `ENRICH_BATCH` and a cursor chain, and no two requests overlap (P5).
- [ ] Lane 3. Restart resumes the cursor. Boot `backfill-150`, stop `wrangler dev` after the log shows page 3, start it again with the same `--persist-to`, and kick. Save `resume-from-cursor.png`. Pass when the stub log never repeats the cursors of pages 1 to 3 after the restart, and the run still ends with `0 missing, 0 extra`.
- [ ] Lane 4. Overlap and idempotency. Boot `backfill-overlap`, where items appear in both searches and as notification threads. Run the sign-in backfill, then kick the notification poll. Save `overlap-one-row-each.png`. Pass when `select count(*), count(distinct id) from items` shows equal numbers and `select id, count(*) from events group by id having count(*) > 1` is empty.
- [ ] Lane 5. The 90-day window. Boot `backfill-old`, whose items are 20, 89, 91, and 400 days old. Save `window-90-days.png`. Pass when the stub log's search string contains `updated:>=` and the date 90 days before today, the 20 and 89 day items are in D1, and the 91 and 400 day items are absent.
- [ ] Lane 6. The nightly search. Boot `nightly-new`, where one open pull request of mine has no notification and appears only in search. Kick the nightly cron. Save `nightly-finds-silent-pr.png`. Pass when the item appears in `items` with a `turn`, the log shows `window=Nightly7d` for all three kinds, and no `Backfill90d` line appears.
- [ ] Lane 7. An empty account. Boot `backfill-empty` and sign in. Save `empty-account.png`. Pass when the run ends after one page per kind with `items=0 next=done`, `select count(*) from items` returns 0, and a second sign-in starts no new backfill.
- [ ] Lane 8. A failing page resumes in place. Boot `backfill-150` with a step where page 3 answers 502 twice and then 200. Save `failed-page-retried.png`. Pass when page 3 is requested three times, no page is skipped or repeated otherwise, and the end state has `0 missing, 0 extra`. Then boot a step with three failures and pass when the log shows `sync.dead kind=backfill_page`, the backfill is not marked done, and the next nightly kick re-arms the dead row with zero attempts and resumes from the page 3 cursor, as R08's `kick` does.
- [ ] Lane 9. The budget gate. Boot `low-budget`, where responses report `x-ratelimit-remaining: 400` of 5000 and a reset 20 s away. Save `budget-gate.png`. Pass when no GraphQL request reaches the stub until after the reset time and exactly one request follows it (P4).
- [ ] Lane 10. Read-only and no content. Boot `backfill-150` with comment bodies containing `BODYMARK-7f3a` in the stub's responses. Save `read-only-no-bodies.png`. Pass when every `POST` in the stub log goes to `/graphql` with a body that starts with `query`, no `GET` or `POST` goes elsewhere except `/user` and `/notifications`, and `BODYMARK-7f3a` appears in no D1 table and no log (N2, N5).

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. CPU per backfill alarm for a page of `ENRICH_BATCH` nodes, as `cpuTime` from `npx wrangler tail --format json` on a preview deploy the operator approves. GraphQL requests for the whole `backfill-150` run, from the stub log. Gzipped Worker size. Trunk has no backfill, so these are absolute budgets for the work the diff adds and for the end state of a first sign-in.
- [ ] Probe. The operator deploys the head to the preview Worker and signs in with the real account. Run `npx wrangler tail <preview name> --format json` through the whole backfill and record `cpuTime` for every `BackfillPage` alarm. Locally, boot `backfill-150` and count requests. Run `node scripts/check-size.mjs`, interleaved with the probe.
- [ ] Baseline. Trunk lacks backfill, so there is no trunk request or CPU number. Record the trunk gzipped Worker size first.
- [ ] Rule. Fail if any warm backfill alarm reports `cpuTime` above 7 ms or the cold one above 10 ms. Fail if `backfill-150` takes more than ceil(150 / b) + ceil(20 / b) + ceil(60 / b) + 1 GraphQL requests, where `b` is `ENRICH_BATCH` (13 when it is 20), or the real-account backfill exceeds 100 requests, which is 20% of the 500 an hour budget. Fail if the Worker grows more than 15 KB gzipped over trunk or passes 400 KB.

**Review gate.** None. R11 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R11 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R11 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R11 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.
- [ ] M1 is done only when the operator has checked the real-account compare box under **You see**.

## Serve the dashboard API (R12)

**Depends on.** R11.

**Files.**

- [ ] Create `crates/domain/src/api.rs` with the response types shared by the Worker and the UI, and `crates/domain/src/settings.rs` with `NudgeThresholds`. Edit `crates/domain/src/lib.rs` to export both. These land in PR order, never in parallel.
- [ ] Create `crates/domain/tests/api_contract.rs` and the golden file `crates/domain/tests/golden/dashboard.json`.
- [ ] Create `crates/worker/src/api/mod.rs`, `dashboard.rs`, `items.rs`, and `freshness.rs`. Edit the router in `crates/worker/src/lib.rs` to mount `/api/dashboard`, `/api/items/:id/dismiss`, `/api/items/:id/snooze`, and `/api/freshness`.
- [ ] Create `crates/worker/src/db/dashboard.rs` with the one batch of selects. Edit `crates/worker/src/db/local_state.rs` (R04) to add `dismiss` and `snooze`.
- [ ] Edit `tools/github-stub/scenarios/make.mjs` to add kinds `dashboard-mixed`, `dashboard-large`, `dashboard-empty`, `missing-scope`, `no-scope-header`, `dead-work`, and `reply-after-dismiss`. Edit `tools/github-stub/server.mjs` only to add the scope header per scenario.

**Build.**

- [ ] Name the data shape first, in `crates/domain/src/api.rs`. `Dashboard { generated_at, counters: Counters, your_turn: Vec<TurnRow>, their_turn: Vec<TheirRow>, pipeline: Pipeline, recent: Vec<RecentRow>, nudge: NudgeThresholds }`. `Counters { your_turn, their_turn, open_prs, merged_this_month }` is F5. `Pipeline { columns: [PipelineColumn; 6] }` always holds all six stages in the order Draft, CiRunning, CiFailing, AwaitingReview, ChangesRequested, Approved, each with its cards, possibly none. Every type in `api.rs` derives serde with `rename_all = "snake_case"`, so enum values reach the UI as words such as `missing_notifications`.
- [ ] `TurnRow { id, kind, repo, number, title, url, reason, actor, what, since, agent }` carries F2. `url` is the item's `latest_url`, or the item URL plus `/checks` for `CiFailed`. `agent` is true when `head_ref` starts with `agent/`. `TheirRow` adds `last_action` and `waiting_on`. `PipelineCard` carries `stage_since`. `RecentRow` carries `state: Merged or Closed`, `ended_at`.
- [ ] `NudgeThresholds { warm_days, hot_days }` has a smart constructor that rejects zero and any pair where `hot_days <= warm_days`, and `Default` returns 7 and 14 (F4). `Dashboard.nudge` returns the default until R14 reads the `settings` row.
- [ ] Use R04's local state model and add no other. `LocalState { Dismissed { seen_human_at }, Snoozed { until } }` maps to the `local_state` columns `kind` and `value`, and an item with no row is visible. A dismissed item is hidden while its `last_human_at` is at or before `seen_human_at`, so a newer human activity brings it back, which is R04's rule. A snoozed item is hidden while `until > now`. Put the rule in one pure function, `visible(local: Option<LocalState>, last_human_at, now)`.
- [ ] `GET /api/dashboard` runs exactly one `db.batch` of indexed selects and assembles the response with a pure `assemble(rows, now)`. Your turn and Their turn sort by `since` ascending. `recent` holds merged or closed items from the last 14 days, newest first. `merged_this_month` counts calendar-month merges.
- [ ] `POST /api/items/:id/dismiss` upserts `kind = 'dismissed'` with `value` set to the item's current `last_human_at`. `POST /api/items/:id/snooze` takes `{ "for": "1d" | "1w" }`, parsed as `enum SnoozeFor { Day, Week }`, and upserts `kind = 'snoozed'` with `value` set to the wake time. Both return 204, are idempotent, return 404 for an unknown id, and return 400 for a bad body (F9, P0).
- [ ] Every route calls R07's session check first and returns 401 with an empty JSON error and no data when it fails. Every POST also requires an `Origin` header equal to the request's own origin, else 403. Every response sets `Cache-Control: no-store`.
- [ ] `GET /api/freshness` calls the Syncer's status RPC and maps it to `Freshness { sources: Vec<SourceFreshness>, budget: Budget { limit, remaining, reset_at }, dead: Vec<DeadWork { kind, key, attempts, since }>, scope: ScopeState { Ok, MissingNotifications, Unknown }, next_poll_at }`. Parse `X-OAuth-Scopes` with `ScopeState::parse`, where an absent header is `Unknown` and a list without the exact token `notifications` is `MissingNotifications` (A5).
- [ ] The dashboard and freshness routes never call GitHub (N7). Only the Syncer does.

**You see.**

- [ ] After a sync of `dashboard-mixed`, `curl -s -b jar.txt http://127.0.0.1:<8800 + n>/api/dashboard` returns JSON whose `counters` equal the lengths of its lists and the `pipeline.columns` array has six entries.
- [ ] `curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:<8800 + n>/api/dashboard` without a cookie prints `401`.
- [ ] After `curl -s -b jar.txt -X POST -H "Origin: http://127.0.0.1:<8800 + n>" .../api/items/<id>/dismiss`, the item is gone from `your_turn` and `select kind, value from local_state` holds `dismissed` and the item's `last_human_at`.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/domain/tests/api_contract.rs` serializes a fixed `Dashboard` and compares it with `golden/dashboard.json`, so a field rename breaks the test. Run `cargo test -p domain`.
- [ ] `visible()` cases. A dismissed item with a newer `last_human_at` is visible again. A snoozed item is hidden until its time and visible after. A dismissed item with an equal `last_human_at` stays hidden. A dismissed item whose `since` moves but whose `last_human_at` does not stays hidden. Run `cargo test -p domain`.
- [ ] `assemble()` cases with fixed rows. Counters equal list lengths, lists sort oldest first, a merge 13 days ago is in `recent` and 15 days ago is not, and empty input yields six empty pipeline columns. Run `cargo test -p worker api`.
- [ ] `ScopeState::parse` cases. `"notifications, read:user"` is `Ok`. `"read:user"` is `MissingNotifications`. An absent header is `Unknown`. `"notifications-extra"` does not count as `notifications`. `NudgeThresholds::new(0, 5)` and `new(7, 7)` fail. Run `cargo test -p domain`.
- [ ] Request validation cases. A snooze body of `{ "for": "1y" }` is rejected, and a POST with a foreign `Origin` is rejected before any D1 call. Run `cargo test -p worker api`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Generate each scenario with `node tools/github-stub/scenarios/make.mjs <kind> > tools/github-stub/scenarios/generated/<kind>.json` and boot the stub with `--scenario generated/<kind>`. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`. Sign in with `curl -sL -c jar.txt http://127.0.0.1:<8800 + n>/auth/login`. Seed data only by syncing. Kick with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"` and wait for `sync.enrich` and `sync.backfill` to finish. Read the stub's request log the way `tools/github-stub/README.md` says.

- [ ] Lane 1. Regression lane against trunk. Run `curl -s -o /dev/null -w "%{http_code}\n" -b jar.txt http://127.0.0.1:<8800 + n>/api/health` and `/api/dashboard` at trunk and head. Trunk has no dashboard route, so record its 404 and gate the behavior the diff adds plus the end state the user waits for. At head, boot `dashboard-mixed`, sync, and fetch. Save `regression-dashboard-json.png`. Pass when health is 200 at both SHAs and the head dashboard answers 200 with non-empty `your_turn` and `their_turn`.
- [ ] Lane 2. Counters match the data. Boot `dashboard-mixed`, sync, and compare `counters` with D1 counts of the same predicates. Save `counters-match-d1.png`. Pass when each of the four counters equals its `wrangler d1 execute` count and equals the length of its list.
- [ ] Lane 3. Ordering. Fetch the dashboard. Save `ordering.png`. Pass when `your_turn[*].since` and `their_turn[*].since` are each ascending, and `recent[*].ended_at` is descending.
- [ ] Lane 4. Pipeline and recent window. Boot `dashboard-mixed`, which has pull requests in all six stages, one merged 13 days ago, one closed 3 days ago, and one merged 15 days ago. Save `pipeline-and-recent.png`. Pass when the six columns appear in the fixed order with the expected card counts (a stage with none is an empty array), `recent` includes the 13 and 3 day items and excludes the 15 day one, and `merged_this_month` matches `_expect`.
- [ ] Lane 5. Dismiss and idempotency. POST dismiss for one item twice with the right `Origin`. Save `dismiss-idempotent.png`. Pass when both calls return 204, the item leaves `your_turn`, `counters.your_turn` drops by exactly 1, `local_state` holds one row for it, and the stub log shows no request except `GET` and `POST /graphql` (read-only).
- [ ] Lane 6. New activity revives a dismissed item. Dismiss an item, then boot the `reply-after-dismiss` step where a maintainer replies on it and sync. Save `revive-after-reply.png`. Pass when the item is back in `your_turn` and its `last_human_at` is later than the `value` stored in its `dismissed` row.
- [ ] Lane 7. Snooze. POST snooze with `{ "for": "1w" }` on one item and `{ "for": "1d" }` on another, then a bad body, then an unknown id. Save `snooze.png`. Pass when the week item's `snoozed` row has a `value` within 5 s of now plus 7 days, the day item's within 5 s of now plus 1 day, both leave `your_turn`, the bad body returns 400, and the unknown id returns 404.
- [ ] Lane 8. Security boundaries. Call all four routes with no cookie, with a cookie whose last character is changed, with a cookie whose payload is edited, and POST with `Origin: https://evil.example`. Save `auth-boundaries.png`. Pass when every cookie case returns 401 with no data, the cross-origin POST returns 403, and `local_state` is unchanged.
- [ ] Lane 9. Freshness in three states. Boot `dashboard-mixed`, `missing-scope` (`X-OAuth-Scopes: read:user`), and `dead-work` (every GraphQL call answers 500) in turn. Save `freshness-states.png`. Pass when the first shows `"scope":"ok"` and a recent `last_ok_at` for the notifications source, the second shows `"scope":"missing_notifications"`, and the third lists one `dead` entry with `attempts: 3` once R08's backoff has run out.
- [ ] Lane 10. Page loads never spend API budget, and an empty account works. Boot `dashboard-mixed`, sync, note the stub log length, then fetch the dashboard and freshness 30 times at 1 per second. Then boot `dashboard-empty` and fetch. Save `no-github-on-load.png`. Pass when the stub log length did not change during the 60 fetches, every response has `Cache-Control: no-store`, and the empty account returns 200 with zero counters, empty lists, and six empty columns (N7).

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `/api/dashboard` latency on `wrangler dev` with 500 items, the response size gzipped, the number of D1 statements per request, and the gzipped Worker size. Trunk has no dashboard route, so latency and size are absolute budgets for the work the diff adds, which is also the end state the user waits for.
- [ ] Probe. Boot `dashboard-large` (500 items), sync, then run `for i in $(seq 100); do curl -s -o /dev/null -b jar.txt -w '%{time_total}\n' http://127.0.0.1:<8800 + n>/api/dashboard; done | sort -n | sed -n 95p` for the p95. Measure `curl -s -b jar.txt --compressed -w '%{size_download}\n' -o /dev/null` for the size. Run `node scripts/check-size.mjs`, interleaved with the probe.
- [ ] Baseline. Trunk lacks the route, so there is no trunk latency. Record the trunk gzipped Worker size first.
- [ ] Rule. Fail if p95 is above 100 ms, if the compressed body is above 40 KB, or if one request runs more than one `db.batch`. Fail if the Worker grows more than 20 KB gzipped over trunk or passes 400 KB.

**Review gate.** None. R12 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R12 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R12 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R12 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Show Your turn with keyboard triage (R13)

**Depends on.** R12.

**Files.**

- [ ] Create `crates/web/src/api.rs` (fetch client and `ApiError`), `crates/web/src/store.rs` (the load state machine), `crates/web/src/keys.rs` (the pure key reducer), `crates/web/src/heat.rs` (age heat and age words), and `crates/web/src/url.rs` (`is_github_url`).
- [ ] Create the shared components in `crates/web/src/components/`. They are `triage_row.rs`, `age.rs`, `key_hints.rs`, `count_block.rs`, `empty_state.rs`, and `snooze_menu.rs`. Later view PRs reuse these and change them only in PR order.
- [ ] Create `crates/web/src/views/your_turn.rs` and `crates/web/style/your-turn.css`. Edit `crates/web/src/app.rs` for the tab counters and the sync time, and `crates/web/index.html` to link the stylesheet.
- [ ] Edit `crates/web/Cargo.toml` to add the fetch dependency (`gloo-net`) and the `web-sys` features the view uses.
- [ ] Edit `tools/github-stub/scenarios/make.mjs` to add kinds `your-turn-edge` (ages of 23, 25, 71, and 73 hours, a CI failure, an agent item), `your-turn-hostile`, and `your-turn-theirs-only`.

**Build.**

- [ ] Name the data shapes first. `enum Load { Loading, Ready { data: Dashboard, fetched_at: f64 }, Stale { data: Dashboard, fetched_at: f64, error: ApiError }, SignedOut, Failed(ApiError) }` in `store.rs`. `enum Heat { Fresh, Warm, Hot }` in `heat.rs`. In `keys.rs`, `enum Mode { Browsing, Snoozing { item: ItemId } }`, `enum Selection { None, At(usize) }`, and `enum Effect { Open(Url), Dismiss(ItemId), Snooze(ItemId, SnoozeFor) }`. The reducer `step(state, key, len) -> (State, Option<Effect>)` is pure.
- [ ] Heat on Your turn. `heat(age, Thresholds { warm: 1 d, hot: 3 d })`, so under 24 h is `Fresh`, from 24 h is `Warm`, and from 72 h is `Hot`. Rows carry `data-heat`. The `warm` and `hot` tokens colour only the age (ADR 0003).
- [ ] Render the view per ADR 0003, as the Your turn list of F1. The `count_block` shows the count in Funnel Display with the heading "waiting on you" and the line "The oldest has waited <age words>". Laptop rows are one line (reason, repo and number, title with a faint "agent" mark, who did what, age). Rows carry `data-row`, `data-reason`, and `data-heat`. Colour `red` only the reason and the who-line of `CiFailed` rows, and nothing else (F2).
- [ ] At or below 600 px (token `breakpoint.phone`), rows become three lines (title with age, who did what, repo) and the count uses `font.size.count-phone` (F12). Read every value from the generated custom properties, and hard-code none.
- [ ] Keys. `j` and `k` move the selection and clamp at the ends. `enter` opens the row's URL in a new tab with `noopener,noreferrer`. `e` dismisses. `s` enters `Snoozing`, which offers "1 day" (`d`) and "1 week" (`w`), and `escape` returns to `Browsing`. While `Snoozing`, `j`, `k`, `e`, and `enter` do nothing. The handler ignores keys with a modifier and keys typed in an input, and the view removes the handler when it unmounts.
- [ ] Dismiss and snooze (F9) are optimistic. The row slides out over `motion.duration.dismiss` with `motion.easing.standard`, the counters drop, and the selection stays at the same index, clamped to the new last row. If the POST fails, the row returns and an inline message reads "Could not dismiss. Try again." or "Could not snooze. Try again."
- [ ] `is_github_url` accepts only `https://github.com/` URLs. Any other link renders as plain text without an `href`. Titles render as text nodes, never as HTML.
- [ ] Refetch `GET /api/dashboard` on window focus and every 60 s, only while the tab is visible, and never start a second request while one runs. A failed refetch moves `Ready` to `Stale`, keeps the rows, and the header reads "Refresh failed" in `muted`. A 401 moves to `SignedOut`, which shows R07's sign-in view.
- [ ] The header shows all four tab counters from `counters`. Tabs for views that do not exist yet render as `aria-disabled="true"`. The sync time reads "Synced <age> ago" from the last successful fetch.
- [ ] The empty state replaces the count block and reads "Nothing is waiting on you. N threads are waiting on others." with "1 thread is" for one (ADR 0003).

**You see.**

- [ ] With `dashboard-mixed` synced, `/` shows the count in large type, "waiting on you", the oldest-wait line, and the rows oldest first.
- [ ] Pressing `j` moves the surface-coloured selection down one row. Pressing `e` slides the row out and the Your turn tab counter drops by one.
- [ ] At 375 px each row has three lines, the count is 104 px, and the page does not scroll sideways.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `heat.rs` cases. Ages of 23 h 59 m, 24 h, 71 h 59 m, and 72 h give `Fresh`, `Warm`, `Warm`, `Hot`. Age words give "2 hours", "1 day", and "4 days". Run `cargo test -p web heat`.
- [ ] `keys.rs` cases. `j` at the last row stays, `k` at the first stays, an empty list never selects, dismissing the last row selects the new last row, `s` then `w` yields `Snooze(id, Week)`, `escape` cancels, and `e` while `Snoozing` does nothing. Run `cargo test -p web keys`.
- [ ] `url.rs` cases. `https://github.com/o/r/pull/1#issuecomment-2` passes. `javascript:alert(1)`, `http://github.com/x`, `https://github.com.evil.example/x`, and an empty string fail. Run `cargo test -p web url`.
- [ ] Empty-state wording for 0, 1, and 7 waiting threads. Run `cargo test -p web`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Generate each scenario with `node tools/github-stub/scenarios/make.mjs <kind> > tools/github-stub/scenarios/generated/<kind>.json` and boot the stub with `--scenario generated/<kind>`. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`, sync with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"`, and wait for `sync.enrich`. Drive the browser through the Playwright MCP server or the Claude browser pane. Open `/`, press the sign-in button, and let the stub's OAuth page return. Read the stub's request log the way `tools/github-stub/README.md` says.

- [ ] Lane 1. Regression lane against trunk. Open `/` signed in at trunk and at head at 1280 px. Trunk shows the shell of R05 (wordmark and empty tabs), so record that and gate the behavior the diff adds plus the end state the user waits for. At head, boot `dashboard-mixed`. Save `regression-your-turn.png` and `regression-shell-trunk.png`. Pass when the header and background render identically at both SHAs and head shows the count and the rows.
- [ ] Lane 2. Happy path. Boot `dashboard-mixed` and open `/`. Save `happy-your-turn.png`. Pass when the count equals the number of `[data-row]` elements, rows run oldest first, the oldest-wait line equals the first row's age, each row shows reason, repo and number, title, who did what, and age, tab counters match `GET /api/dashboard`, and the console holds no errors and no CSP violation.
- [ ] Lane 3. Colour has two jobs. Boot `your-turn-edge`. Save `colour-two-jobs.png`. Pass when the 23 h row has `data-heat="fresh"`, the 25 h and 71 h rows `warm` with the `warm` colour, the 73 h row `hot` with the `hot` colour, and a script over `getComputedStyle` finds the `red` colour only on the reason and who-line of the failing-CI row.
- [ ] Lane 4. Hostile data. Boot `your-turn-hostile`, whose titles hold `<img src=x onerror=alert(1)>` and `<script>`, and whose one URL is `javascript:alert(1)`. Save `hostile-titles.png`. Pass when titles show as literal text, no dialog opens, no `onerror` ran (a page flag stays unset), the bad URL row has no `href`, and the console holds no CSP violation.
- [ ] Lane 5. Keyboard navigation and open. Press `j` three times, `k` once, then `enter`. Save `keyboard-nav.png`. Pass when the selection is on row 3, `aria-current="true"` follows it, the selected row is scrolled into view, `j` on the last row stays, and `enter` opens a new tab at that row's `https://github.com/` URL while the Radar tab stays.
- [ ] Lane 6. Dismiss. Press `e` on row 2, wait 300 ms, reload. Save `dismiss-row.png`. Pass when the row slid out (its computed `transition` or `animation` lasted 180 ms), the count and the Your turn tab dropped by one, the selection moved to the next row, `select kind, value from local_state` holds `dismissed` and the item's `last_human_at`, and the row is absent after reload.
- [ ] Lane 7. Snooze and failure. Press `s`, check that "1 day" and "1 week" show, press `escape` and see nothing change, press `s` then `w` and see the row leave with a `snoozed` row whose `value` is near now plus 7 days. Then block `POST /api/items/*` in the browser (return 500) and press `e` again. Save `snooze-and-failure.png`. Pass when the failed dismiss restores the row and shows "Could not dismiss. Try again.", and D1 has no row for it.
- [ ] Lane 8. Empty state. Boot `your-turn-theirs-only` (nothing for me, 7 threads for others). Save `empty-state.png`. Pass when the page reads "Nothing is waiting on you. 7 threads are waiting on others.", no count block shows, and pressing `j`, `k`, `e`, `s`, and `enter` changes nothing and logs no error.
- [ ] Lane 9. Phone width. Resize to 375 by 812. Save `phone-375.png`. Pass when every row has three lines (title with age, who did what, repo), the count's computed font size is 104 px, the heading is 24 px, and `document.documentElement.scrollWidth` is at most 375.
- [ ] Lane 10. Refetch and sign-out. Open `/`, then add a mention through the next stub step and kick. Hide and show the tab. Save `refetch-and-401.png`. Pass when focus triggers a `GET /api/dashboard` (network log), the new row appears within 65 s without reload, no request is sent while the tab is hidden, and after the session cookie is removed the next refetch shows R07's sign-in view.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. First load in the browser through the Performance API, as the largest contentful paint start time with rows on screen, plus the gzipped UI size from `node scripts/check-size.mjs`. Trunk shows only the shell, so the load number is an absolute budget for the end state the user waits for, and the size is measured at both sides.
- [ ] Probe. Boot `dashboard-large` (500 items), sync, and open `/` in a fresh browser context with an empty cache, signed in. Read `new PerformanceObserver(...)` with `{ type: 'largest-contentful-paint', buffered: true }` and take the last entry's `startTime`. Repeat five times at trunk and at head, interleaved. Run `node scripts/check-size.mjs` at both.
- [ ] Baseline. Record the trunk shell LCP median and the trunk gzipped UI size first.
- [ ] Rule. Fail if the head median LCP with rows is above 1.5 s. Fail if the UI grows more than 40 KB gzipped over trunk or passes 200 KB.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, 3, 5, 8, and 9 screenshots into `artifacts/review/R13-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of triage with `j`, `k`, `e`, `s`, and `enter` at laptop width, then the same list at 375 px. Save it as `artifacts/review/R13-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's go.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R13 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R13 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R13 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Show Their turn with nudge timing (R14)

**Depends on.** R13.

**Files.**

- [ ] Create `migrations/<next number>_settings.sql` with a `settings` table that holds one row.
- [ ] Edit `crates/worker/src/db/dashboard.rs` and `crates/worker/src/api/dashboard.rs` (R12) to read the row into `Dashboard.nudge`.
- [ ] Edit `crates/domain/src/settings.rs` (R12) to add `NudgeThresholds::from_row`. Land it in PR order.
- [ ] Create `crates/web/src/views/their_turn.rs` and `crates/web/style/their-turn.css`. Edit `crates/web/src/app.rs` to enable the Their turn tab and its route, and `crates/web/index.html` to link the stylesheet.
- [ ] Edit `crates/web/src/heat.rs` (R13) to add `days_phrase`. Reuse `triage_row.rs`, `age.rs`, `key_hints.rs`, and `keys.rs` from R13 without changing them.
- [ ] Edit `tools/github-stub/scenarios/make.mjs` to add kinds `their-turn-mixed` and `their-turn-none`.

**Build.**

- [ ] Name the data shape first. `settings(id INTEGER PRIMARY KEY CHECK (id = 1), nudge_warm_days INTEGER NOT NULL DEFAULT 7, nudge_hot_days INTEGER NOT NULL DEFAULT 14, CHECK (nudge_warm_days >= 1 AND nudge_hot_days > nudge_warm_days))`, seeded with `insert into settings (id) values (1)`. The domain value is `NudgeThresholds`.
- [ ] `NudgeThresholds::from_row(Option<Row>)` returns the stored pair when it is valid and the 7 and 14 default when the row is missing or invalid, and logs the fallback. It never fails the request (F4).
- [ ] There is no settings endpoint and no settings screen. The operator changes the row with `npx wrangler d1 execute radar --command "update settings set ..."`. The CHECK constraint rejects a bad pair.
- [ ] Render `their_turn` per the prototype. Columns are repo and number, title with the agent mark, "<last action>, waiting on <who>", the usual reply time, and the age in days. Sort by longest wait first.
- [ ] `waiting_on` reads `requested_reviewers` from the item, and shows the logins joined with commas, or "maintainers" when the list is empty.
- [ ] Heat uses R13's `heat(age, thresholds)` with `dashboard.nudge`, so under `warm_days` is `Fresh`, from `warm_days` is `Warm`, and from `hot_days` is `Hot`.
- [ ] The usual reply time column renders a dash (`-`) until R21 fills it, and it is hidden at or below 600 px.
- [ ] The summary reads "<n> have waited over <days phrase>", where `days_phrase(7)` is "a week", `days_phrase(14)` is "two weeks", and any other value is "<n> days". With none past the warm threshold it reads "None has waited over <days phrase>". The second line reads "Nudging is fair after <days phrase>."
- [ ] Keys on this view are `j`, `k`, `enter`, and `s`. `e` does nothing here. Snooze reuses `snooze_menu.rs` and the same POST. The empty state reads "Nothing is waiting on others. N are waiting on you."
- [ ] Refetch on focus and every 60 s through R13's store. Enable the Their turn tab and route `/theirs`, with `aria-current="page"` on the active tab.

**You see.**

- [ ] `/theirs` shows a summary line such as "2 have waited over a week", then rows longest wait first.
- [ ] A row waiting 8 days has an amber age and one waiting 15 days has a coral age. After `update settings set nudge_warm_days = 3, nudge_hot_days = 5`, a reload recolours them and the summary says "over 3 days".
- [ ] The usual reply column shows `-` on every row.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `NudgeThresholds::from_row` cases. A valid row, a missing row, and a row with `warm >= hot` (built by hand, since D1 would reject it) return the stored pair, the default, and the default. Run `cargo test -p domain settings`.
- [ ] `days_phrase` cases for 7, 14, 3, and 30. Heat cases at 6 d 23 h, 7 d, 13 d 23 h, and 14 d with the default pair, and at 3 d and 5 d with a custom pair. Run `cargo test -p web heat`.
- [ ] Summary wording for 0, 1, and 5 stale items. Run `cargo test -p web`.
- [ ] `waiting_on` wording for zero, one, and three reviewers. Run `cargo test -p web`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Generate each scenario with `node tools/github-stub/scenarios/make.mjs <kind> > tools/github-stub/scenarios/generated/<kind>.json` and boot the stub with `--scenario generated/<kind>`. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`, sync with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"`, and wait for `sync.enrich`. Drive the browser through the Playwright MCP server or the Claude browser pane, signed in through the stub's OAuth page. Read the stub's request log the way `tools/github-stub/README.md` says.

- [ ] Lane 1. Regression lane against trunk. Open `/` signed in at trunk and head with `their-turn-mixed`. Trunk shows only Your turn, with the Their turn tab disabled, so record that and gate the behavior the diff adds plus the end state the user waits for. Save `regression-your-turn-intact.png`. Pass when the Your turn rows and counters are identical at both SHAs and head's Their turn tab opens `/theirs`.
- [ ] Lane 2. Happy path. Boot `their-turn-mixed` and open `/theirs`. Save `their-turn-happy.png`. Pass when the row count equals the tab counter, rows run longest wait first, each row shows repo and number, title, "<action>, waiting on <who>", and age, and the Their turn tab has `aria-current="page"`.
- [ ] Lane 3. Default thresholds. The scenario holds PRs waiting 6, 8, 13, and 15 days. Save `default-thresholds.png`. Pass when `data-heat` reads `fresh`, `warm`, `warm`, `hot`, and the summary reads "3 have waited over a week".
- [ ] Lane 4. Configurable thresholds. Run `npx wrangler d1 execute radar --local --persist-to .wrangler/lane-<n> --command "update settings set nudge_warm_days = 3, nudge_hot_days = 5"` and reload. Save `custom-thresholds.png`. Pass when the 6, 8, 13, and 15 day rows are all `hot`, and the summary says "over 3 days".
- [ ] Lane 5. A bad or missing settings row. Try `update settings set nudge_warm_days = 9, nudge_hot_days = 9`, then `delete from settings`, and reload. Save `bad-settings.png`. Pass when the update fails with a CHECK error and changes nothing, and after the delete the page still loads with the 7 and 14 day thresholds and no error banner.
- [ ] Lane 6. Waiting on. The scenario holds a PR with reviewers `alice` and `bob`, a PR with none, and an issue I commented on. Save `waiting-on.png`. Pass when the rows read "waiting on alice, bob", "waiting on maintainers", and "waiting on maintainers".
- [ ] Lane 7. Usual reply placeholder. Save `usual-reply-dash.png`. Pass when the usual reply cell on every row is exactly `-`, and `select count(*) from items` shows the row source is unchanged.
- [ ] Lane 8. Keyboard. Press `j`, `j`, `k`, `s`, `w`. Then press `e` and `enter`. Save `their-turn-keys.png`. Pass when the selection moves, the snoozed row leaves with a `snoozed` row in `local_state` whose `value` is near now plus 7 days and the tab counter drops by one, `e` sends no request, and `enter` opens the GitHub URL in a new tab.
- [ ] Lane 9. Phone width. Resize to 375 by 812 and open `/theirs`. Save `their-turn-phone-375.png`. Pass when each row is three lines (title with age, who it waits on, repo), the usual reply cell is hidden, and `scrollWidth` is at most 375.
- [ ] Lane 10. Empty and hostile. Boot `their-turn-none` (nothing waiting on others, 4 waiting on me), then a row whose title is `<script>alert(1)</script>`. Save `their-turn-empty.png`. Pass when the page reads "Nothing is waiting on others. 4 are waiting on you.", keys produce no error, and the hostile title shows as literal text with no script run.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Time from clicking the Their turn tab to the last row painted, and the first load of `/theirs` as largest contentful paint start time, with 200 rows, plus the gzipped UI size. Trunk has no Their turn view, so these are absolute budgets for the work the diff adds and for the end state the user waits for.
- [ ] Probe. Boot `dashboard-large`, sync, open `/`, and wrap the tab click in `performance.mark` calls, then wait two animation frames and measure. Repeat ten times. Load `/theirs` cold five times and read the LCP entry. Run `node scripts/check-size.mjs` at trunk and head, interleaved.
- [ ] Baseline. Trunk lacks the view. Record the trunk gzipped UI size first.
- [ ] Rule. Fail if the median tab switch is above 100 ms, the cold LCP is above 1.5 s, or the UI grows more than 15 KB gzipped over trunk or passes 200 KB.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, 3, 4, 6, and 9 screenshots into `artifacts/review/R14-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of switching to Their turn, changing the thresholds with the `d1 execute` command, reloading, and snoozing a row. Save it as `artifacts/review/R14-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's go.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R14 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R14 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R14 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Show the pull request pipeline (R15)

**Depends on.** R14.

**Files.**

- [ ] Create `crates/web/src/views/pipeline.rs` and `crates/web/style/pipeline.css`. Edit `crates/web/src/app.rs` to enable the Pipeline and Merged tabs and the `/pipeline` route, and `crates/web/index.html` to link the stylesheet.
- [ ] Create `crates/web/src/components/pipeline_card.rs`, `crates/web/src/components/recent_list.rs`, and `crates/web/src/stage_label.rs`.
- [ ] Edit `tools/github-stub/scenarios/make.mjs` to add kinds `pipeline-all-stages`, `pipeline-two-stages`, `pipeline-stage-move`, `pipeline-recent`, and `pipeline-hostile`.

**Build.**

- [ ] Name the data shape first. The view renders `Dashboard.pipeline.columns`, which is always the six `Stage` values in order, and `Dashboard.recent`. `stage_label(Stage) -> &'static str` is one match over `Stage`, so a missing stage fails to compile. The card shows `PipelineCard { id, repo, number, title, url, agent, stage_since }`.
- [ ] Render a column per stage with its heading and count, then its cards, or the word "None" when the column is empty. Draw the heading of `CiFailing` in `red`, and no other element in red (ADR 0003).
- [ ] A card shows the title, then "repo #number" with ", agent" for agent items, then the days in stage from `stage_since`, as "today", "1d", "3d". Cards link to GitHub in a new tab with `noopener,noreferrer`, through `is_github_url` from R13.
- [ ] Apply heat to the age of cards in `AwaitingReview` only, with `heat(age, dashboard.nudge)`, because that is the column where a nudge is fair (F4).
- [ ] Render "Merged or closed in the last 14 days" from `recent`, newest first, with the state word, repo and number, title, and age. Closed rows use `muted`. With none it reads "Nothing merged or closed in the last 14 days."
- [ ] Layout. At 601 px and above, six columns in a grid with a minimum width of 160 px, and the board scrolls inside its container at narrow laptop widths without scrolling the page. At or below 600 px (F12), each column becomes a stacked section in the same order, with its heading and count, cards at full width, and "None" kept for empty sections.
- [ ] Enable the Pipeline tab with the open pull request count and the Merged tab with the merged-this-month count. The Merged counter links to `/pipeline` and scrolls to the recent list, and the URL stays `/pipeline`.
- [ ] This view registers no `j`, `k`, `e`, `s`, or `enter` handler. Tab order follows the visual order, left to right and top to bottom.

**You see.**

- [ ] `/pipeline` shows six columns headed Draft, CI running, CI failing, Awaiting review, Changes requested, and Approved, each with a count.
- [ ] A column with no pull requests shows "None". The recent list sits under the board.
- [ ] At 375 px the columns are stacked sections in the same order.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `stage_label` returns a distinct non-empty label for every `Stage`, and the six board stages appear in the documented order. Run `cargo test -p web stage`.
- [ ] Days-in-stage wording for 0 h, 23 h, 24 h, and 3 d 2 h gives "today", "today", "1d", "3d". Run `cargo test -p web`.
- [ ] Heat for `AwaitingReview` cards at 6 d, 8 d, and 15 d with the default pair gives `Fresh`, `Warm`, `Hot`, and the same ages in `Draft` give `Fresh`. Run `cargo test -p web heat`.
- [ ] Recent-list wording for zero rows, and the "Closed" style flag for closed rows. Run `cargo test -p web`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Generate each scenario with `node tools/github-stub/scenarios/make.mjs <kind> > tools/github-stub/scenarios/generated/<kind>.json` and boot the stub with `--scenario generated/<kind>`. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`, sync with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"`, and wait for `sync.enrich`. Drive the browser through the Playwright MCP server or the Claude browser pane, signed in through the stub's OAuth page. Read the stub's request log the way `tools/github-stub/README.md` says.

- [ ] Lane 1. Regression lane against trunk. Open `/` signed in at trunk and head with `pipeline-all-stages`. Trunk has no pipeline view, with the tab disabled, so record that and gate the behavior the diff adds plus the end state the user waits for. Save `regression-your-turn-intact.png`. Pass when the Your turn rows are identical at both SHAs and head's Pipeline tab opens the board.
- [ ] Lane 2. Six columns with true counts. Open `/pipeline` with `pipeline-all-stages`. Save `six-columns.png`. Pass when the headings appear in order, and each count equals `select count(*) from items where stage = '<stage>' and <mine and open>` for that stage.
- [ ] Lane 3. Empty columns. Boot `pipeline-two-stages` (only Draft and Approved populated). Save `empty-columns.png`. Pass when the other four columns each show the word "None" and the Pipeline tab counter equals the total cards.
- [ ] Lane 4. Days in stage and the stage clock. The scenario holds cards in a stage for 0, 1, and 3 days. Then run the `pipeline-stage-move` step, where one PR goes from CI running to CI failing, sync, and reload. Save `days-in-stage.png`. Pass when cards read "today", "1d", and "3d", and after the move the moved card reads "today" in CI failing while the other cards' days are unchanged.
- [ ] Lane 5. Heat only where a nudge is fair. The scenario holds PRs of 6, 8, and 15 days in Awaiting review and an 8-day PR in Draft. Save `heat-awaiting-review.png`. Pass when the 8 and 15 day Awaiting review cards have `warm` and `hot` ages, the 6 day card and the Draft card have none, and `red` appears only on the CI failing heading.
- [ ] Lane 6. Merged or closed window. Boot `pipeline-recent` (merged 13 days ago, closed 3 days ago, merged 15 days ago). Save `recent-14-days.png`. Pass when the list shows the 3 and 13 day items newest first, hides the 15 day one, shows "Closed" in `muted`, and the Merged tab counter equals `merged_this_month`.
- [ ] Lane 7. Phone stacking. Resize to 375 by 812. Save `pipeline-phone-375.png`. Pass when the six sections stack in order, each heading shows its count, empty sections show "None", cards span the width, and `scrollWidth` is at most 375.
- [ ] Lane 8. Narrow laptop. Resize to 768 by 900. Save `pipeline-768.png`. Pass when the board scrolls horizontally inside its own container, `document.documentElement.scrollWidth` is at most 768, and the Pipeline tab stays in the header.
- [ ] Lane 9. Keyboard. Press `Tab` from the header through every card, then `enter` on a focused card, and press `j`, `k`, `e`, and `s`. Save `pipeline-keyboard.png`. Pass when focus visits columns left to right and cards top to bottom, each focus ring is visible, `enter` opens the card's GitHub URL in a new tab, and the other four keys send no request and move nothing.
- [ ] Lane 10. Hostile data. Boot `pipeline-hostile`, where one title is 300 characters with no spaces, one holds `<img src=x onerror=alert(1)>`, and one URL is `javascript:alert(1)`. Save `pipeline-hostile.png`. Pass when the long title wraps or clips inside its card with no page overflow, the HTML shows as text, no script runs, and the bad URL card has no `href`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Time from clicking the Pipeline tab to the last card painted, and the first load of `/pipeline` as largest contentful paint start time, with 120 open pull requests, plus the gzipped UI size. Trunk has no pipeline view, so these are absolute budgets for the work the diff adds and for the end state the user waits for.
- [ ] Probe. Boot `dashboard-large`, sync, open `/`, wrap the tab click in `performance.mark` calls, wait two animation frames, and measure, ten times. Load `/pipeline` cold five times and read the LCP entry. Run `node scripts/check-size.mjs` at trunk and head, interleaved.
- [ ] Baseline. Trunk lacks the view. Record the trunk gzipped UI size first.
- [ ] Rule. Fail if the median tab switch is above 150 ms, the cold LCP is above 1.5 s, or the UI grows more than 15 KB gzipped over trunk or passes 200 KB.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, 3, 5, 6, and 7 screenshots into `artifacts/review/R15-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the board at laptop width, then the stacked sections at 375 px, and the Merged tab scrolling to the recent list. Save it as `artifacts/review/R15-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's go.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R15 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R15 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R15 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Show freshness and the re-authorize banner (R16)

**Depends on.** R15.

**Files.**

- [ ] Create `crates/web/src/views/freshness.rs` (the panel) and `crates/web/src/components/reauth_banner.rs`, `crates/web/src/components/sync_status.rs`, and `crates/web/style/freshness.css`.
- [ ] Create `crates/web/src/sync_label.rs` with the pure label logic.
- [ ] Edit `crates/web/src/api.rs` and `crates/web/src/store.rs` (R13) to fetch `GET /api/freshness` alongside the dashboard. Edit `crates/web/src/app.rs` to place the banner above every view and to replace the plain sync time with the status button.
- [ ] Edit `crates/web/index.html` to link the stylesheet.
- [ ] Edit `tools/github-stub/scenarios/make.mjs` to add kinds `fresh-ok`, `fresh-budget`, and `scope-fixed-step`, and reuse `missing-scope`, `no-scope-header`, and `dead-work` from R12.

**Build.**

- [ ] Name the data shapes first. The panel renders the `Freshness` type from R12. In `sync_label.rs`, `enum SyncHealth { Fresh { age }, Quiet { age }, Failed { age, dead: usize }, Unknown }` and `sync_label(health, now) -> String`. `Fresh` is a last good poll within 30 minutes, `Quiet` is older than that, and `Unknown` means the freshness request failed.
- [ ] The header status button reads "Synced <age> ago", "Not synced for <age>" when `Quiet`, "Synced <age> ago, <n> failed" when dead rows exist, and "Sync status unavailable" when `Unknown`. It uses `muted` and adds no colour (ADR 0003).
- [ ] The button opens the panel as a `section` labelled "Data freshness". Enter, Space, and click open it, `escape` closes it, and focus returns to the button.
- [ ] The panel lists each source with its last good time (notifications poll, enrichment, and the search backfill and nightly run). It shows API budget used as "<used> of <limit> (<percent>%), resets at <local time>" from `budget`, and the dead work rows as kind, key, attempts, and since (F10, N9). It has no button that changes anything.
- [ ] The banner shows when `scope == MissingNotifications` on every view, above the content, with `role="alert"`. It reads "Radar cannot read your notifications. Authorize again to grant the notifications scope." and links "Authorize again" to `/auth/login` as a plain same-origin link (A5). It is not dismissible and uses `surface` and `text`, no colour. `Unknown` shows no banner.
- [ ] Fetch `/api/freshness` on load, on window focus, and every 60 s while visible, on the same timer as R13's dashboard fetch. A failed fetch gives `Unknown` and keeps the last panel contents.
- [ ] At or below 600 px (F12), the panel becomes a full-width sheet, the banner text wraps, and the banner link and status button each have a hit area of at least 44 px.
- [ ] Render all text from the API as text nodes, never as HTML.

**You see.**

- [ ] The header reads "Synced 40s ago". Clicking it opens a panel listing last sync per source, "250 of 5,000 (5%), resets at 14:05", and "No failed syncs".
- [ ] With `missing-scope`, a banner with an "Authorize again" link sits above the content on all four views.
- [ ] With `dead-work`, the header reads "Synced 2m ago, 1 failed" and the panel lists the dead row.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `sync_label` cases. 29 minutes is `Fresh`, 31 minutes is `Quiet` and reads "Not synced for 31 min", one dead row appends ", 1 failed", and `Unknown` reads "Sync status unavailable". Run `cargo test -p web sync_label`.
- [ ] Budget wording for `remaining` 4750 of 5000 gives "250 of 5,000 (5%)", and for 0 remaining gives "5,000 of 5,000 (100%)". Run `cargo test -p web`.
- [ ] Banner rule cases. `MissingNotifications` shows it, and `Ok` and `Unknown` do not. Run `cargo test -p web`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Generate each scenario with `node tools/github-stub/scenarios/make.mjs <kind> > tools/github-stub/scenarios/generated/<kind>.json` and boot the stub with `--scenario generated/<kind>`. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`, sync with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"`, and wait for `sync.enrich`. Drive the browser through the Playwright MCP server or the Claude browser pane, signed in through the stub's OAuth page. Read the stub's request log the way `tools/github-stub/README.md` says.

- [ ] Lane 1. Regression lane against trunk. Open `/` signed in at trunk and head with `fresh-ok`. Trunk shows "Synced <age> ago" from the last dashboard fetch and no panel, so record that and gate the behavior the diff adds plus the end state the user waits for. Save `regression-header.png`. Pass when the Your turn rows and counters are identical at both SHAs, and head's header time matches `last_ok_at` of `GET /api/freshness` within 5 s.
- [ ] Lane 2. Panel by mouse and keyboard. Click the status button, close with `escape`, then open with `enter` and `space`. Save `panel-open.png`. Pass when the panel opens every time, focus returns to the button after `escape`, and each source's time equals the matching field of `GET /api/freshness`.
- [ ] Lane 3. Budget numbers. Boot `fresh-budget` (limit 5000, remaining 4750, reset in 40 minutes). Save `budget-used.png`. Pass when the panel reads "250 of 5,000 (5%)" with the reset time shown in local time, matching the stub's `x-ratelimit-*` headers.
- [ ] Lane 4. Dead work rows. Boot `dead-work` and wait until R08's backoff has exhausted three attempts. Save `dead-work.png`. Pass when the header reads "Synced <age> ago, 1 failed", the panel lists one row with its kind and `attempts` of 3, and `GET /api/freshness` agrees.
- [ ] Lane 5. Re-authorize banner. Boot `missing-scope` and visit each of the four tabs. Save `banner-missing-scope.png`. Pass when the banner shows above the content on all four views with `role="alert"`, "Authorize again" links to `/auth/login`, and after the click the stub's OAuth page returns, the `scope-fixed-step` grants `notifications`, and the banner disappears on the next refetch.
- [ ] Lane 6. No false banner. Boot `fresh-ok`, then `no-scope-header`. Save `no-banner.png`. Pass when neither shows a banner, and `GET /api/freshness` reports `"scope":"ok"` and `"scope":"unknown"` respectively.
- [ ] Lane 7. Stale and unavailable. Intercept `GET /api/freshness` in the browser to return a `last_ok_at` 45 minutes old, then a 500. Save `stale-and-unavailable.png`. Pass when the header reads "Not synced for 45 min" and then "Sync status unavailable", and the last panel contents stay visible after the failure.
- [ ] Lane 8. Phone width. Resize to 375 by 812 with `missing-scope` and `dead-work` state, and open the panel. Save `freshness-phone-375.png`. Pass when the panel is a full-width sheet, the banner wraps without clipping, the banner link and status button measure at least 44 px high, and `scrollWidth` is at most 375.
- [ ] Lane 9. Live refresh. Open the panel, then advance the stub to a step with a newer poll and kick. Switch the tab hidden and visible. Save `panel-refreshes.png`. Pass when the panel updates within 65 s without reload, a focus event triggers `GET /api/freshness`, and no request is sent while the tab is hidden.
- [ ] Lane 10. Read-only and safe. Boot `dead-work` with a dead row whose key is `<img src=x onerror=alert(1)>`, then exercise the panel and banner. Save `freshness-read-only.png`. Pass when the key shows as literal text and no script runs, the page holds no `form`, the network log shows only `GET /api/*` requests, and the stub log does not grow from opening the panel twenty times (N7).

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Time from clicking the status button to the panel painted, `GET /api/freshness` p95 latency on `wrangler dev`, GitHub requests caused by opening the panel, and the gzipped UI size. Trunk has no panel, so panel time and latency are absolute budgets for the work the diff adds, and the request count is an absolute budget of zero.
- [ ] Probe. Open the panel ten times with `performance.mark` around the click and two animation frames. Run `for i in $(seq 100); do curl -s -o /dev/null -b jar.txt -w '%{time_total}\n' http://127.0.0.1:<8800 + n>/api/freshness; done | sort -n | sed -n 95p`. Count stub log lines before and after twenty openings. Run `node scripts/check-size.mjs` at trunk and head, interleaved.
- [ ] Baseline. Trunk lacks the panel. Record the trunk gzipped UI size first.
- [ ] Rule. Fail if the median panel time is above 100 ms, the endpoint p95 is above 50 ms, any GitHub request is caused by a panel open, or the UI grows more than 15 KB gzipped over trunk or passes 200 KB.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, 3, 4, 5, and 8 screenshots into `artifacts/review/R16-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of opening the panel, the dead-work state, and the banner appearing and clearing after the re-authorize round trip. Save it as `artifacts/review/R16-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's go.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R16 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R16 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R16 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Pass phone, keyboard, and accessibility checks for v1 (R17)

**Depends on.** R16.

**Files.**

- [ ] Create `scripts/check-contrast.mjs`, which reads `design/tokens.json` and checks every text and background pair the UI uses.
- [ ] Create `scripts/audit-a11y.mjs`, which runs Lighthouse's accessibility category on each view with the session cookie from a curl jar.
- [ ] Edit the stylesheets in `crates/web/style/` for focus rings, tap targets, reflow, and `prefers-reduced-motion`. Edit `design/tokens.json` and regenerate `crates/web/style/tokens.css` with `node scripts/tokens-to-css.mjs`, only if the contrast script fails a pair.
- [ ] Create `docs/adr/0006-faint-contrast.md` from `docs/adr/template.md`. Edit `docs/adr/README.md` to add its row, and edit the status line and row of ADR 0003 to read "Superseded in part by 0006", as the ADR README's "Change a decision" section says.
- [ ] Edit `crates/web/src/app.rs`, `crates/web/src/components/triage_row.rs`, and `crates/web/src/components/snooze_menu.rs` for landmarks, the live region, and per-view document titles. Edit `crates/web/index.html` for `lang` and the viewport meta.
- [ ] Edit `scripts/build-assets.mjs` only if the manifest needs a change. Never edit `public/manifest.webmanifest` by hand (ADR 0004).
- [ ] Edit `.github/workflows/ci.yml` to run `node scripts/check-contrast.mjs`.
- [ ] Edit `tools/github-stub/scenarios/make.mjs` to add kind `a11y-all-views` (every view populated, a missing scope, and one dead row).

**Build.**

- [ ] Name the data shape first. `scripts/check-contrast.mjs` holds a table `PAIRS = [{ fg, bg, min, use }]` of token names, for example `text` on `bg` at 4.5, `muted` on `surface` at 4.5, and `faint` on `surface` at 4.5, with large text and focus rings at 3. It computes WCAG contrast from `design/tokens.json`, prints one line per pair, and exits 1 on any failure.
- [ ] Run it first. Measured by hand on 2026-11-05, `faint` is 2.90 on `bg` and 2.68 on `surface`, against 4.5 needed for the sync time, key hints, and agent mark. `muted`, `warm`, `hot`, and `red` all pass. Lighten `faint` to the darkest value that passes 4.5 on `surface`, keep it below `muted` in lightness, regenerate the CSS, and show before and after at the review gate. The operator may veto the value there.
- [ ] Record the change in `docs/adr/0006-faint-contrast.md`, which supersedes the `faint` value in ADR 0003 instead of silently changing a recorded value. It states the old value `#5d6069`, the new value, the measured ratios on `bg` and `surface` before and after, and the text that uses `faint`. ADR 0003 keeps its text and gains only the status that points to 0006.
- [ ] Tell the operator in the handover that the wiki page needs `node scripts/tokens-to-wiki.mjs <wiki-checkout>` and a push, because that is a public write the owner does not make (AGENTS.md).
- [ ] Focus. One rule, `:focus-visible { outline: 2px solid var(--color-text); outline-offset: 2px; }`, on every interactive element. No rule removes an outline without replacing it.
- [ ] Reduced motion. Under `@media (prefers-reduced-motion: reduce)`, the dismiss slide and every transition and animation get a duration of 0, and the row simply disappears.
- [ ] Phone. Every view works at 375 px and at 320 px width with no horizontal page scroll. Every button and link outside running text has a hit area of at least 44 by 44 px at or below 600 px.
- [ ] Semantics. `lang="en"`, one `header`, one `nav` labelled "Views", one `main`. Tabs expose `aria-current="page"`. Each route sets `document.title` to "<tab> (<count>) · Radar". A visually hidden `aria-live="polite"` region announces "Dismissed. N left." and "Snoozed for 1 week. N left." after those actions.
- [ ] PWA. The manifest and icons from ADR 0004 must pass Chrome's installability check with no service worker. Radar never caches private data offline, so if the only error asks for a service worker, the owner stops and asks the operator.
- [ ] `scripts/audit-a11y.mjs --jar jar.txt --base <url>` loads the session cookie name and value from the jar, passes them as extra headers to `npx lighthouse@12 --only-categories=accessibility` for Your turn, Their turn, Pipeline, and the panel, at the mobile and desktop presets, and prints each score. The operator approves the Lighthouse download in chat first.

**You see.**

- [ ] `node scripts/check-contrast.mjs` prints `0 failing pairs` and exits 0. CI runs it.
- [ ] Pressing `Tab` through any view shows a 2 px ring on every stop. With the OS set to reduce motion, `e` removes a row with no slide.
- [ ] Chrome offers to install Radar, and the installed app opens at `/` in its own window with the graphite theme colour.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scripts/check-contrast.mjs` has a self-test mode, `node scripts/check-contrast.mjs --self-test`. It checks a known pair, black on white at 21:1 and `#777777` on white at 4.48:1, and it fails when a pair table entry names a missing token. Run it.
- [ ] `crates/web` gains tests for `document_title(tab, count)` and the live region text for dismiss, snooze, and a count of 0 or 1. Run `cargo test -p web`.
- [ ] A CSS check in `scripts/check-contrast.mjs --css crates/web/style` fails on any `outline: none` or `outline: 0` without a `:focus-visible` replacement in the same file. Run it with a fixture that fails and with the real CSS that passes.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

Lane setup. Generate `a11y-all-views` with `node tools/github-stub/scenarios/make.mjs a11y-all-views > tools/github-stub/scenarios/generated/a11y-all-views.json` and boot the stub with `--scenario generated/a11y-all-views`. Start `wrangler dev` with the recipe's flags plus `--test-scheduled`, sync with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"`, and wait for `sync.enrich`. Drive the browser through the Playwright MCP server or the Claude browser pane, signed in through the stub's OAuth page. Script lanes use `curl -sL -c jar.txt http://127.0.0.1:<8800 + n>/auth/login`.

- [ ] Lane 1. Regression lane against trunk. Open Your turn, Their turn, Pipeline, and the panel at 1280 px at trunk and head. Trunk lacks the accessibility work, so record that and gate the behavior the diff adds plus the end state the user waits for. Save `your-turn-laptop.png`. Pass when the visible `innerText` of every view is identical at both SHAs, apart from the document title, and no view lost a feature.
- [ ] Lane 2. Phone width on every view. Resize to 375 by 812 and visit Your turn, Their turn, Pipeline, the open panel, the banner, and the sign-in view. Save `phone-all-views.png`. Pass when `document.documentElement.scrollWidth` is at most 375 on every view and no element's bounding box extends past the viewport.
- [ ] Lane 3. Reflow at 320 px and at 200 percent zoom. Resize to 320 by 700, then to 640 wide at a 200 percent zoom factor. Save `reflow-320.png`. Pass when no view scrolls horizontally and no text is clipped or overlapped.
- [ ] Lane 4. Tap targets. At 375 px, list every button and link outside running text with its bounding box. Save `tap-targets.png`. Pass when none is under 44 by 44 px, and the list of exceptions is empty.
- [ ] Lane 5. Keyboard only. Starting from the page's first element, press `Tab` through the tabs, the status button, the banner link, and the rows, then use `j`, `k`, `e`, `s`, `enter`, and `escape`. Save `keyboard-walk.png`. Pass when every stop shows an outline of at least 2 px with contrast of at least 3:1 against its background, the order is logical, `escape` leaves the panel and the snooze menu, and no step traps focus.
- [ ] Lane 6. Screen reader semantics. Read the accessibility tree of each view. Save `a11y-tree.png`. Pass when it shows one `banner`, one `navigation` named "Views", one `main`, the active tab marked current, the selected row marked current, and after pressing `e` a status text "Dismissed. N left." appears in the live region.
- [ ] Lane 7. Reduced motion. Emulate `prefers-reduced-motion: reduce` and press `e`, then repeat with no preference as a control. Save `reduced-motion.png`. Pass when every element's computed `transition-duration` and `animation-duration` is 0 under the preference, the row is gone within one frame, and the control still slides over 180 ms.
- [ ] Lane 8. Contrast in the rendered page. Run `node scripts/check-contrast.mjs`, then walk every visible text node in every view and compute its colour against its effective background. Save `contrast.png`. Pass when the script reports `0 failing pairs` and the lowest rendered ratio is at least 4.5, with the `faint` text included.
- [ ] Lane 9. Installability. Through the Chrome DevTools protocol, call `Page.getInstallabilityErrors` and fetch the manifest and each icon. Save `pwa-installability.png`. Pass when the error list is empty, the manifest has `name`, `short_name`, `start_url` of `/`, `display` of `standalone`, `theme_color` of `#141519`, 192 and 512 icons and a maskable icon that all return 200 at their stated sizes, and `apple-touch-icon.png` returns 200.
- [ ] Lane 10. Lighthouse accessibility. Run `node scripts/audit-a11y.mjs --jar jar.txt --base http://127.0.0.1:<8800 + n>` after the operator approves the download. Save `lighthouse-a11y.png`. Pass when all eight runs (four views at mobile and desktop) score at least 95 and none reports a failed contrast, name, label, or tap target audit.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. First load in the browser through the Performance API, as the largest contentful paint start time with rows on screen, in a phone emulation with a 4x CPU slowdown, and the gzipped UI size. The same scenario runs at trunk and head, so the load number compares directly.
- [ ] Probe. Boot `dashboard-large`, sync, and open `/` in a fresh context with an empty cache at 375 by 812 with `Emulation.setCPUThrottlingRate` set to 4. Read the last `largest-contentful-paint` entry. Repeat five times at trunk and head, interleaved. Run `node scripts/check-size.mjs` at both.
- [ ] Baseline. Record the trunk median LCP under the same throttle and the trunk gzipped UI size first.
- [ ] Rule. Fail if head's median LCP is above 1.5 s or more than 10 percent above trunk's. Fail if the UI grows more than 5 KB gzipped over trunk or passes 200 KB.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, 5, 7, 8, and 10 screenshots into `artifacts/review/R17-review-<slug>.png`, with before and after captures of the `faint` text.
- [ ] Record a 30 to 60 second video of a keyboard-only pass through the four views, then the same views at 375 px, then the installed app opening. Save it as `artifacts/review/R17-review.mp4`.
- [ ] Post the screenshots and the video in chat with the proposed `faint` value and the wiki command. Tell the operator that M2 is done when he uses Radar daily instead of github.com/notifications. Stop at merge-ready. Wait for the operator's go.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R17 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R17 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R17 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Trigger syncs from GitHub notification emails (R18)

**Depends on.** R17.

**Files.**

- [ ] Create `crates/domain/src/email.rs` with the header types and the parser, and export it from `crates/domain/src/lib.rs`.
- [ ] Create `crates/worker/src/email.rs` with the `#[event(email)]` handler, and declare `mod email;` in `crates/worker/src/lib.rs`.
- [ ] Create `tools/github-stub/emails/` with `review-requested.eml`, `mention.eml`, `team-mention.eml`, `ci-activity.eml`, `unknown-reason.eml`, `not-github.eml`, `hostile-headers.eml`, `big-body.eml`, and `forged-mention.eml` (a `mention` email with GitHub's headers sent by someone who is not GitHub).
- [ ] Create `tools/github-stub/scenarios/email-forged.json`, whose `/notifications` answers 304 to a request that carries the `Last-Modified` it sent before.
- [ ] Create `.dev.vars.example` with the line `FORWARD_TO=`, or add that line if the file exists. The forward address is a secret because the repository is public (N12).

**Build.**

- [ ] Name the data shape first. `EmailHeaders` holds the three header values as `Option<&str>` and nothing else. `EmailTrigger { reason: EmailReason, repo: Option<RepoName>, sender: Option<Login> }` comes from `EmailTrigger::from_headers(&EmailHeaders) -> Result<EmailTrigger, EmailRejected>`. Because the parser never receives a body, it cannot read one (E2).
- [ ] Define `EmailReason` as an enum with `Assign`, `Author`, `CiActivity`, `Comment`, `Mention`, `ReviewRequested`, `StateChange`, `Subscribed`, `TeamMention`, and `Other`. `Other` keeps a value GitHub adds later from breaking the handler. Define `EmailRejected` as an enum with the one variant `NotGithub`, returned when `X-GitHub-Reason` is missing or empty.
- [ ] Parse `X-GitHub-Reason` case-insensitively after trimming. Parse `List-Id`, which GitHub sends as `owner/repo <repo.owner.github.com>`, by taking the text before `<` and accepting it only when `RepoName::parse` accepts it. Anything else gives `repo: None`. Parse `X-GitHub-Sender` through `Login::parse`, and give `None` on failure.
- [ ] Treat any header value longer than 256 bytes as missing, so a hostile header costs no CPU (E3).
- [ ] Write the handler in `crates/worker/src/email.rs` in a fixed order. Forward the message to the `FORWARD_TO` secret first, so a parse bug never loses mail (PRD risk table). Then parse the three headers. Then call the Syncer kick helper that R08 uses for cron, with `KickReason::Email`. A forward error is logged and does not stop the kick. The handler never calls `message.raw()` or reads the body.
- [ ] When `FORWARD_TO` is unset or empty, the handler calls `message.setReject("Radar has no forward address configured")` instead of forwarding, so the sender gets a bounce and the mail never vanishes. It still parses the headers and kicks. O3 sets the secret before R18 deploys, and O4 verifies the address in Email Routing.
- [ ] E4. Email content is never trusted. A forged email can only cause a kick, a kick can only cause the poll that would run anyway, and that poll sends `If-Modified-Since`, so a forged mention costs exactly one poll that returns 304 and changes nothing.
- [ ] Use the single `Syncer` instance that cron uses, so a burst of emails collapses into one alarm at the later of now and the next allowed poll (ADR 0002 "Triggers are deferred, not dropped"). If R08's `KickReason` has no `Email` variant, stop and report to the root, because `crates/worker/src/sync/**` belongs to R08.
- [ ] Log exactly one line per email with `console_log!` in the form `email trigger reason=<reason> repo=<owner/name or none> sender=<login or none> forward=<ok or failed or rejected> outcome=<kicked or not_github>`. The line carries no body text and no address.
- [ ] Return `Result` everywhere. Clippy denies `unwrap_used` in `crates/worker`, so the handler has no `unwrap` and no `expect`.
- [ ] Define the lane command used below as `curl -sS -w "%{http_code}\n" -X POST "http://127.0.0.1:<8800 + n>/cdn-cgi/handler/email?from=notifications@github.com&to=radar@karthik.nz" -H "Content-Type: application/octet-stream" --data-binary @tools/github-stub/emails/<name>.eml`, the email post. Add a table-driven test in `crates/domain/src/email.rs` with one named case per reason, one per malformed `List-Id`, and one per missing header.

**You see.**

- [ ] The email post of `mention.eml` returns `200`, and the `wrangler dev` log shows `email trigger reason=mention repo=octo/radar-demo sender=maintainer forward=ok outcome=kicked`.
- [ ] The Syncer status RPC from R08 shows one `PollNotifications` work row that is due now or at the next allowed poll time.
- [ ] After 50 posts in 10 seconds the same status still shows one `PollNotifications` row.
- [ ] Operator item, checked only when the operator posts the evidence after a deploy. A real test mention from another account reaches Radar within 2 minutes (M3). This does not gate the verdict.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/domain/src/email.rs` maps each of the nine known `X-GitHub-Reason` values, and an unknown value, to the right `EmailReason`. Run `cargo test -p domain email`.
- [ ] The same table returns `NotGithub` for a missing header, an empty header, and a header of only spaces. Run `cargo test -p domain email`.
- [ ] The same table returns `repo: None` for `List-Id` values `../../x <y>`, `owner <y>`, `a/b/c <y>`, and an empty string, and returns `octo/radar-demo` for the real GitHub form. Run `cargo test -p domain email`.
- [ ] A header value of 257 bytes behaves as missing, and one of 256 bytes is accepted. Run `cargo test -p domain email`.
- [ ] The whole workspace still builds for the Worker target. Run `cargo build -p worker --target wasm32-unknown-unknown`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run the cron with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=*/10+*+*+*+*"` and `GET /api/health` at trunk (`main`) and at head, then run the email post of `mention.eml` at both. Trunk has no email handler, so record its status code for the post and gate the behavior the diff adds, which is a `200` and a due `PollNotifications` row. Save `email-regression.png` as a terminal capture. Pass when the cron leaves the same Syncer status at both, `/api/health` returns `200` JSON at both, and head returns `200` for the post.
- [ ] Lane 2. Forged mention (E4). Sign in, boot the stub on `email-forged`, let one poll complete, and wait past the poll interval. Run the email post of `forged-mention.eml` and wait for the next `unit done`. Save `email-forged.png` as a terminal capture. Pass when the stub log shows exactly one new `GET /notifications` after the post, it carries `If-Modified-Since` and answers 304, no GraphQL request follows, and `select count(*) from items` and `select count(*) from events` are unchanged.
- [ ] Lane 3. Reason table. Run the email post of `review-requested.eml`, `mention.eml`, `team-mention.eml`, `ci-activity.eml`, and `unknown-reason.eml` in turn. Save `email-reasons.png` as a terminal capture. Pass when the five log lines show `review_requested` with `repo=octo/radar-demo outcome=kicked`, then `mention`, `team_mention`, `ci_activity`, and `other`, every post returns `200`, and the Syncer status shows one due `PollNotifications` row.
- [ ] Lane 4. Burst. Post `mention.eml` 50 times in 10 seconds with a shell loop. Save `email-burst.png` as a terminal capture. Pass when the Syncer status shows exactly one `PollNotifications` row and one pending alarm, and `wrangler dev` shows no error.
- [ ] Lane 5. Not a GitHub email. Run the email post of `not-github.eml`, which has no `X-GitHub-Reason`. Save `email-not-github.png` as a terminal capture. Pass when the log shows `forward=ok outcome=not_github`, the post returns `200`, and the Syncer work table gains no row.
- [ ] Lane 6. Body is never read. Run the email post of `big-body.eml`, a 2 MB body that contains the marker `RADAR-BODY-MARKER-7f3a`. Save `email-big-body.png` as a terminal capture. Pass when the post returns `200` in under 1 second, the marker appears nowhere in the `wrangler dev` log, and `wrangler d1 execute radar --local --command "select count(*) from events"` is unchanged.
- [ ] Lane 7. No forward address. Restart Radar without `FORWARD_TO`, then with `--var FORWARD_TO:` set empty, and run the email post of `mention.eml` each time. Save `email-forward-rejected.png` as a terminal capture. Pass when each response or log shows the reject reason "Radar has no forward address configured", the log shows `forward=rejected outcome=kicked`, the work row still exists, and the process serves `/api/health` afterwards.
- [ ] Lane 8. Idempotency. Run the email post of `mention.eml` twice, with `wrangler d1 execute radar --local --command "select (select count(*) from items), (select count(*) from events)"` before and after. Save `email-idempotent.png` as a terminal capture. Pass when the two counts are identical and the Syncer status still shows one `PollNotifications` row.
- [ ] Lane 9. Hostile headers. Run the email post of `hostile-headers.eml`, which has a 10 KB `X-GitHub-Reason`, a `List-Id` of `../../etc <x>`, and a `X-GitHub-Sender` with spaces and a non-ASCII name. Save `email-hostile.png` as a terminal capture. Pass when the log shows `repo=none sender=none`, the log contains no `panicked`, and `/api/health` still returns `200` afterwards.
- [ ] Lane 10. Read-only boundary. After lanes 2 to 9, read the GitHub stub's request log and run `rg -n "raw\(|\.text\(|github::Client" crates/worker/src/email.rs`. Save `email-read-only.png` as a terminal capture. Pass when every request in the stub log is a `GET` or a GraphQL `query` that follows a Syncer `unit start` line, none comes from the email handler itself, and the `rg` search prints nothing.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. The work the diff adds is the email handler. Measure handler wall time per email on `wrangler dev`, `PollNotifications` rows left by a burst of 50 emails, and gzipped Worker size from `node scripts/check-size.mjs`. The end state the user waits for is a due sync after an email. On an operator-approved preview deploy, also read `cpuTime` per email.
- [ ] Probe. Alternate trunk and head three times each for `node scripts/check-size.mjs`. At head, post `mention.eml` 50 times with `curl -w "%{time_total}\n"` and take the p95. On the preview deploy, the operator runs `npx wrangler tail --format json` while 20 emails arrive, and the owner reads `cpuTime`.
- [ ] Baseline. Trunk has no email handler, so no trunk handler number exists and none is invented. Record the trunk Worker gzipped size first.
- [ ] Rule. Fail when the head Worker is more than 15 KB gzipped above trunk or above 400 KB, when p95 handler time on `wrangler dev` is 100 ms or more, when 50 emails leave more than one `PollNotifications` row, or when preview `cpuTime` per email is 5 ms or more. A value of 10 ms is the hard limit.

**Review gate.** None. R18 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R18 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R18 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R18 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Warn when notification emails stop (R19)

**Depends on.** R18.

**Files.**

- [ ] Create `migrations/00NN_email_state.sql`, where `NN` is the next free number on `main`.
- [ ] Create `crates/domain/src/email_health.rs` and export it from `crates/domain/src/lib.rs`.
- [ ] Create `crates/worker/src/db/email_state.rs`.
- [ ] Edit `crates/worker/src/email.rs` to record each GitHub email after the forward and the kick.
- [ ] Edit the `GET /api/freshness` handler under `crates/worker/src/api/` and the freshness response type in `crates/domain/`, to add the `email` field.
- [ ] Edit the freshness view that R16 created in `crates/web/src/views/` to show the email row and the warning.
- [ ] Create `tools/github-stub/scenarios/email-silent.json` with one notification thread whose `updated_at` is the moment the stub serves it.

**Build.**

- [ ] Name the data shape first. `EmailHealth` is an enum with `Flowing { last_email_at }`, `Idle { last_email_at: Option<Time> }`, and `Silent { last_email_at: Option<Time>, threads_missed: u32 }`. It replaces a boolean warning flag and keeps the three states apart. `Time` is the time type `domain` already uses for `Classification.since`.
- [ ] Write the pure function `email_health(now, last_email_at, threads_after, window) -> EmailHealth` in `crates/domain/src/email_health.rs`. The result is `Flowing` when the last email is younger than the window. Past the window it is `Silent` when at least one notification thread changed after the last email, and `Idle` otherwise (N10). `window` is a `QuietWindow` newtype that defaults to 24 hours.
- [ ] Add the table in the new migration. It has one row, `id INTEGER PRIMARY KEY CHECK (id = 1)`, with `last_email_at` and `last_reason`. Write it with an upsert that keeps `MAX(last_email_at, excluded.last_email_at)`, so a repeated or late write never moves the time backwards.
- [ ] Write `record_email`, `read`, and `threads_after` in `crates/worker/src/db/email_state.rs`. `threads_after(at)` counts `notification_threads` rows whose `updated_at` is later than `at`, or all rows when there is no email yet.
- [ ] Record an email only when `EmailTrigger::from_headers` returned `Ok`. A message without `X-GitHub-Reason` must not reset the clock. A D1 failure is logged and never fails the email, because the forward has already happened.
- [ ] Read the window from the var `EMAIL_QUIET_SECS`, parsed once at the boundary. A missing, non-numeric, or non-positive value gives 24 hours and logs `ignored EMAIL_QUIET_SECS` once. Lanes set it to 5 so the 24-hour rule is testable in seconds.
- [ ] Add `email: EmailHealth` to the freshness response type in `crates/domain/` and fill it in `GET /api/freshness`. The endpoint still requires a session.
- [ ] Show one row in the freshness panel with the last email age, or "No email yet". When the state is `Silent`, also show the banner component that R16 added in `crates/web/src/components/`. Its text says how long email has been quiet, how many threads changed meanwhile, and links to `https://github.com/settings/notifications` in a new tab with `rel="noopener noreferrer"`. The banner uses `text` colour and weight 600, because colour is reserved for waiting time and CI (ADR 0003).

**You see.**

- [ ] After an email, the freshness panel row reads "Notification email, last received 3s ago", and no banner shows.
- [ ] With `EMAIL_QUIET_SECS` set to 5, six seconds after the email and a poll that finds a newer thread, the banner shows "No notification email for 6 seconds while 1 thread changed" and the settings link.
- [ ] A new email clears the banner on the next refetch.
- [ ] A message that is not a GitHub notification email leaves the row time unchanged.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `email_health` returns `Flowing` for an email 1 hour ago with a newer thread. Run `cargo test -p domain email_health`.
- [ ] It returns `Silent { threads_missed: 1 }` for an email 25 hours ago with one thread after it, and `Idle` for the same email with no thread after it. Run `cargo test -p domain email_health`.
- [ ] It returns `Idle { last_email_at: None }` for no email and no threads, and `Silent { last_email_at: None, threads_missed: 3 }` for no email and three threads. Run `cargo test -p domain email_health`.
- [ ] It is `Flowing` one second before the window ends, `Silent` at exactly the window, and `Flowing` when `last_email_at` is later than `now`. Run `cargo test -p domain email_health`.
- [ ] The `EMAIL_QUIET_SECS` parser returns 24 hours for `abc`, `0`, `-5`, and an empty string, and 5 seconds for `5`. Run `cargo test -p domain email_health`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Sign in through the stub, open the freshness panel at trunk (`main`) and at head with `EMAIL_QUIET_SECS` unset and no email sent. Trunk has no email row, so gate the rows both sides share plus the new row. Save `freshness-regression.png`. Pass when last sync, API budget, and dead work rows are identical at both, no banner shows at head, and the new row reads "No email yet".
- [ ] Lane 2. Healthy and duplicate. Start with `EMAIL_QUIET_SECS:5`, run the email post of `mention.eml` twice, and reload the panel. Save `freshness-healthy.png`. Pass when the row shows an age under 5 seconds, no banner shows, and `wrangler d1 execute radar --local --command "select count(*) from email_state"` returns 1.
- [ ] Lane 3. Silent. With the stub on scenario `email-silent`, run the email post of `mention.eml`, wait 6 seconds, trigger a poll with the cron URL, and focus the tab. Save `freshness-silent.png`. Pass when the banner shows with a thread count of at least 1 and the settings link.
- [ ] Lane 4. Idle. Run the email post of `mention.eml` against a stub scenario with no notification threads, and wait 6 seconds. Save `freshness-idle.png`. Pass when no banner shows and the row says the last email age with no newer GitHub activity.
- [ ] Lane 5. No email ever. On a fresh `--persist-to` directory with scenario `email-silent`, poll once and wait 6 seconds. Save `freshness-never.png`. Pass when the banner reads "No notification email has arrived yet" and shows a thread count of at least 1.
- [ ] Lane 6. Recovery. From the lane 3 state, run the email post of `review-requested.eml` and refocus the tab. Save `freshness-recovered.png`. Pass when the banner disappears within one refetch and the row age restarts under 5 seconds.
- [ ] Lane 7. Spoof does not reset. From the lane 3 state, run the email post of `not-github.eml`. Save `freshness-spoof.png`. Pass when `select last_email_at from email_state` is unchanged and the banner stays.
- [ ] Lane 8. Session boundary. Run `curl -i http://127.0.0.1:<8800 + n>/api/freshness` with no cookie. Save `freshness-401.png` as a terminal capture. Pass when the status is `401` and the body contains neither `last_email_at` nor `email`.
- [ ] Lane 9. Phone width. At 375 px width, repeat the lane 3 state. Save `freshness-phone.png`. Pass when the banner text wraps with no clipped words and `document.documentElement.scrollWidth` is at most 375.
- [ ] Lane 10. Keyboard. From the lane 3 state, reach the banner link using only Tab and press enter. Save `freshness-keyboard.png`. Pass when focus lands on the link in tab order with a visible focus ring, enter opens a new tab on `github.com/settings/notifications`, and Radar keeps its place.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `GET /api/freshness` latency on `wrangler dev` at trunk and head, since the diff adds two D1 reads to it. Also the work added to the email handler, which is one D1 write, and gzipped Worker and UI size from `node scripts/check-size.mjs`.
- [ ] Probe. Alternate trunk and head three times. Each run sends 200 requests to `/api/freshness` with a valid session cookie and takes the p95, then runs `node scripts/check-size.mjs`. At head, post `mention.eml` 50 times and take the p95 of handler time.
- [ ] Baseline. Record the trunk `/api/freshness` p95 and both gzipped sizes first. Trunk has no email write, so the email handler gets an absolute budget instead of a ratio.
- [ ] Rule. Fail when head `/api/freshness` p95 is more than 10 ms or 25 percent above trunk, when the Worker is above 400 KB or the UI above 200 KB gzipped, or when p95 email handler time on `wrangler dev` is 100 ms or more.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, lane 3, and lane 9 screenshots into `artifacts/review/R19-review-healthy.png`, `artifacts/review/R19-review-silent.png`, and `artifacts/review/R19-review-phone.png`.
- [ ] Record a 30 to 60 second video of an email arriving, the quiet window passing, and the banner appearing and clearing, from a lane's `wrangler dev` session. Save it as `artifacts/review/R19-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R19 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R19 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R19 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Show the activity feed and roll up old events (R20)

**Depends on.** R19.

**Files.**

- [ ] Create `migrations/00NN_feed_index.sql` with an index on `events (occurred_at DESC, id DESC)` and one on `events (repo, occurred_at DESC)`. Use the next free number on `main`.
- [ ] Create `crates/domain/src/feed.rs` and `crates/domain/src/rollup.rs`, and export both from `crates/domain/src/lib.rs`.
- [ ] Create `crates/worker/src/db/feed.rs`, `crates/worker/src/api/feed.rs`, and `crates/worker/src/rollup.rs`.
- [ ] Edit `crates/worker/src/sync/work.rs` so `WorkKind::Rollup` carries `RollupKey { Nightly, Week(WeekStart) }`, and edit the `Rollup` arm in `crates/worker/src/sync/handlers.rs` to call `rollup::run`. These files belong to R08 to R11, so the owner tells the root before the code-ready report.
- [ ] Create `crates/web/src/views/feed.rs`. Add the `Feed` variant to the `Tab` enum in `crates/web/src/components/tabs.rs`, with `NavGroup { Counters, More }` and `Tab::group()`. Edit `crates/web/src/components/header.rs` for the second nav group. R21, R22, and R23 add their variants after this PR in the stack.
- [ ] Create `tools/github-stub/scenarios/feed-basic.json`, `feed-old.json`, and `feed-many.json`, in the shape of the existing scenarios and in the stub's relative-time form from R06.
- [ ] Create `scripts/gen-feed-scenario.mjs`, which writes `feed-many.json` with 100 pull requests of 20 events each.

**Build.**

- [ ] Name the data shapes first. The feed reads R10's event model from `crates/domain/src/event.rs` and adds no event type of its own. `FeedEvent { event: Event, title }` pairs an `Event { id, item, repo, kind: EventKind, actor: EventActor { login, is_me, is_bot }, occurred_at }` with its item title. The kind filter is `FeedKind`, the tag words of `EventKind` (`opened`, `commented`, `reviewed`, `review_requested`, `assigned`, `mentioned`, `committed`, `merged`, `closed`, `reopened`, `ci_failed`, `ci_passed`). `FeedQuery { repo: Option<RepoName>, kind: Option<FeedKind>, before: Option<FeedCursor> }`. `FeedCursor { at, id }` is a keyset cursor, never an offset. Put all of them in `crates/domain/src/feed.rs`.
- [ ] Write `GET /api/feed?repo=<owner/name>&kind=<kind>&before=<cursor>` in `crates/worker/src/api/feed.rs`. It requires a session, parses the query into `FeedQuery` at the boundary and returns `400` for an unknown `kind`, a bad `repo`, or a malformed cursor. It returns `FeedPage { events, next: Option<FeedCursor>, repos }`, with 50 events per page, where `repos` fills the filter list (F6).
- [ ] Write the SQL in `crates/worker/src/db/feed.rs` with bound parameters only. Order by `occurred_at DESC, id DESC` and use the keyset condition `(occurred_at, id) < (?, ?)`.
- [ ] Write `rollup_cutoff(now) -> WeekStart` in `crates/domain/src/rollup.rs`. It returns the Monday, in UTC, of the week that contains the instant 180 days before `now`. Only whole weeks that end before the cutoff roll up, so each week rolls exactly once and its distinct repo count is never split across runs (N6).
- [ ] Write `rollup::run` in `crates/worker/src/rollup.rs`. One `WorkKind::Rollup` unit handles the oldest eligible week. In a single D1 `batch` it upserts that week into `rollups_weekly` with PRs opened, PRs merged, and repos touched, then deletes the week's `events` rows. The batch is atomic, so a retry never double counts. If another eligible week remains, the unit returns a follow-up `Rollup(RollupKey::Week(<that monday>))` row, keyed `week:<monday>` and never `nightly`, so it cannot merge with the `nightly` row that is finishing. That keeps CPU per alarm bounded (ADR 0002).
- [ ] Navigation. The four counter tabs stay as ADR 0003 has them. Feed sits in a second, quieter nav group at the end of the header, with no counter, in `muted`. At or below 600 px the quiet group collapses into one "More" button that opens a menu of its tabs, with `aria-expanded` and `escape` to close. R21 to R23 add their tabs to this group.
- [ ] Build the Feed view in `crates/web/src/views/feed.rs`. It reuses the list components from R13. A row shows age, repo and number, a grey word for the kind, the actor, and the title, which links to the item on GitHub in a new tab. A bot actor carries a faint "bot" mark after the title. `CiFailed` is the only kind that uses `red`.
- [ ] Add a repo `<select>` and a kind `<select>` above the list. Store the choice in the URL query as `/feed?repo=<owner/name>&kind=<kind>`, so a reload keeps it. A "Load older" button fetches the next page with `next`.
- [ ] Write the empty state as "Nothing matches this filter. Clear it to see all N events." when a filter is set, and "No events yet." when there are none.
- [ ] Keep `j`, `k`, and enter working in the feed through the shared list components.

**You see.**

- [ ] A Feed tab shows events newest first with repo and kind filters, and the first page holds 50 events when more exist.
- [ ] Choosing a repo and a kind that match nothing shows "Nothing matches this filter. Clear it to see all 12 events."
- [ ] After the nightly cron runs on `feed-old`, no row in `events` is older than the cutoff, and `rollups_weekly` holds the rolled weeks.
- [ ] The wrangler dev log shows `rollup week=<monday> events=<n> opened=<n> merged=<n> repos=<n>` for each unit.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `rollup_cutoff` returns the right Monday for day 179, day 180, and day 181 before `now`, for a `now` on a Monday and on a Sunday, and across a year boundary. Run `cargo test -p domain rollup`.
- [ ] The week range helper rolls only weeks that end before the cutoff, so a week that straddles the cutoff stays. Run `cargo test -p domain rollup`.
- [ ] `FeedQuery::parse` rejects an unknown kind, a repo with a quote or a space, and a cursor that is not `<at>_<id>`, and accepts the empty query. Run `cargo test -p domain feed`.
- [ ] Keyset paging over 120 sorted events returns pages of 50, 50, and 20 with no duplicate and no gap, including events with equal `at`. Run `cargo test -p domain feed`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Sign in through the stub and open Your turn at trunk (`main`) and at head on the same scenario. Trunk has no Feed tab, so gate Your turn and `GET /api/dashboard` at both plus the Feed tab at head. Save `feed-regression.png`. Pass when the two `/api/dashboard` bodies are identical, Your turn looks the same at both, and only head has a Feed tab.
- [ ] Lane 2. Happy path. Run scenario `feed-basic`, 12 events across 3 repos, and open the Feed tab. Save `feed-basic.png`. Pass when 12 rows show newest first and the row count equals `wrangler d1 execute radar --local --command "select count(*) from events"`.
- [ ] Lane 3. Filters and empty state. Choose one repo, then add a kind that no event of that repo has. Save `feed-filter-empty.png`. Pass when the repo filter shows only that repo's rows, the URL query holds both values and survives a reload, and the empty state names the full event count.
- [ ] Lane 4. Pagination. Run scenario `feed-many` generated by `node scripts/gen-feed-scenario.mjs`, then click "Load older" until it disappears. Save `feed-paging.png`. Pass when every id appears once, times never increase down the list, and the final count equals the D1 count.
- [ ] Lane 5. Bots and CI colour. Use events from a bot actor and a failing CI event. Save `feed-bot-ci.png`. Pass when the bot row carries the faint "bot" mark with no colour, the CI failed row is `red`, and no other row has a colour.
- [ ] Lane 6. Nightly rollup. Run scenario `feed-old` with events 200, 190, and 170 days old, then run the nightly cron with `curl "http://127.0.0.1:<8800 + n>/__scheduled?cron=<nightly cron from wrangler.toml>"`. Save `feed-rollup.png` as a terminal capture. Pass when `events` holds no row older than the cutoff week, `rollups_weekly` counts equal the pre-run counts from a saved `select`, and the 170 day events remain.
- [ ] Lane 7. Rollup is idempotent. Run the nightly cron three more times. Save `feed-rollup-rerun.png` as a terminal capture. Pass when `rollups_weekly` and `events` are byte-for-byte equal to their state after lane 6, and the log shows no unit with `events=0` writing a row.
- [ ] Lane 8. Boundary and injection. Run `curl` against `/api/feed` with no cookie, with `kind=nope`, with `repo=x'+OR+1=1--`, and with `before=abc`. Save `feed-security.png` as a terminal capture. Pass when the first is `401` and the other three are `400` with no event data in the bodies.
- [ ] Lane 9. Phone width. At 375 px width, open Feed through the "More" menu with `feed-basic`. Save `feed-phone.png`. Pass when the four counter tabs stay in their row, Feed is reachable only through "More", rows show as stacked lines with the age, the filters fit the width, and `document.documentElement.scrollWidth` is at most 375.
- [ ] Lane 10. Keyboard. Use only the keyboard to open Feed from the tabs, change the repo filter, move with `j` and `k`, and open a row with enter. Save `feed-keyboard.png`. Pass when focus rings are visible, the selection follows `j` and `k`, enter opens GitHub in a new tab, and no key press triggers `e` or `s` actions on the feed.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Trunk lacks the feed and the rollup. Measure `GET /api/feed` p95 on `wrangler dev` with the `feed-many` data set of 2,000 events. Measure `cpuTime` per `Rollup` alarm on an operator-approved preview deploy. Measure gzipped UI and Worker size at trunk and head with `node scripts/check-size.mjs`.
- [ ] Probe. Alternate trunk and head three times for `node scripts/check-size.mjs`. At head, send 200 requests each to the first page and to a deep page of `/api/feed`, and take each p95. On the preview deploy, the operator runs `npx wrangler tail --format json` through one nightly run, and the owner reads `cpuTime` for each `Rollup` alarm.
- [ ] Baseline. Record trunk gzipped sizes first. No trunk feed number exists, so none is invented.
- [ ] Rule. Fail when the Worker is above 400 KB or the UI above 200 KB gzipped, when either `/api/feed` p95 is 100 ms or more, or when any `Rollup` alarm shows `cpuTime` of 7 ms or more as a target miss and 10 ms or more as a hard fail.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, lane 3, and lane 9 screenshots into `artifacts/review/R20-review-feed.png`, `artifacts/review/R20-review-filter.png`, and `artifacts/review/R20-review-phone.png`.
- [ ] Record a 30 to 60 second video of the feed, a filter change, and loading older events, from a lane's `wrangler dev` session. Save it as `artifacts/review/R20-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R20 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R20 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R20 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Show repo cards with maintainer reply times (R21)

**Depends on.** R20.

**Files.**

- [ ] Create `crates/domain/src/repo_stats.rs` and export it from `crates/domain/src/lib.rs`.
- [ ] Create `crates/worker/src/db/repo_stats.rs` and `crates/worker/src/api/repos.rs`.
- [ ] Edit the dashboard handler under `crates/worker/src/api/` and the shared response types in `crates/domain/`, to add `usual_reply` to each Their turn item.
- [ ] Edit the usual reply cell in the Their turn view that R14 created under `crates/web/src/views/`. R14 is an ancestor in the stack.
- [ ] Create `crates/web/src/views/repos.rs`, and add the `Repos` variant to the `Tab` enum in `crates/web/src/components/tabs.rs`, in R20's quiet `More` nav group with no counter.
- [ ] Create `tools/github-stub/scenarios/repo-cards.json` with 4 repos and 60 pull requests, in the shape of the existing scenarios.

**Build.**

- [ ] Name the data shape first. `RepoCard { repo, open: u32, merged: u32, closed: u32, merge_rate: Option<Percent>, median_first_response: Option<Duration>, last_activity: Time }`. `merge_rate` is `None` when `merged + closed` is 0, so the card never shows a false 0 percent. Put it in `crates/domain/src/repo_stats.rs`.
- [ ] Define `merge_rate` as merged divided by merged plus closed without merge. Open pull requests do not count. Count only pull requests that I authored.
- [ ] Define `first_response(opened_at, events: &[Event]) -> Option<Duration>` over R10's event model as the time from opening to the first maintainer reply, which is the first `Commented` or `Reviewed` event after opening with `actor.is_me` and `actor.is_bot` both false. A bot event and my own event never count. A pull request with no such event is left out of the median.
- [ ] Define `median(durations) -> Option<Duration>` so an empty list gives `None` and an even count gives the mean of the two middle values.
- [ ] Write the SQL in `crates/worker/src/db/repo_stats.rs` as one D1 batch of indexed selects over `items` and `events`. It returns per repo counts and one `(repo, first_response_secs)` row per pull request that has a response, found from `events` rows with `kind` in `commented` or `reviewed`, `actor_is_me = 0`, and `actor_is_bot = 0`. Compute the medians in Rust, and never store them, so the numbers cannot drift from the events. Use the `items` column that marks a pull request as mine, as R04 defined it.
- [ ] Write `GET /api/repos` in `crates/worker/src/api/repos.rs`. It requires a session and returns `Vec<RepoCard>` ordered by `last_activity` descending (F7). It reads D1 only (N7).
- [ ] Fill `usual_reply: Option<Duration>` on each Their turn item from the same repo medians, in the dashboard handler. The Their turn cell shows "usually replies in 4d" when `Some`, and keeps the dash when `None`.
- [ ] Build the Repos view in `crates/web/src/views/repos.rs`. Cards sit on `surface` with the `md` radius, three per row on a laptop and one per row at 600 px and below. A card shows the repo name as a link to the repo on GitHub in a new tab, "3 open, 12 merged, 2 closed", "86% merged", "Maintainers reply in 2d", and "Active 3d ago". It uses no colour. `None` values read "No merged or closed yet" and "No reply yet".
- [ ] Write the empty state as "No repositories yet. They appear after the first sync."

**You see.**

- [ ] A Repos tab shows one card per repo with counts, a merge rate, a median reply time, and the last activity age.
- [ ] A repo with 3 merged, 1 closed, and 2 open pull requests shows "75% merged".
- [ ] A repo with four responses of 1, 2, 4, and 9 days shows "Maintainers reply in 3d".
- [ ] On Their turn, a row from that repo shows "usually replies in 3d" where trunk shows a dash.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `merge_rate` returns 75 percent for 3 merged and 1 closed, `None` for 0 and 0, and 0 percent for 0 merged and 2 closed. Run `cargo test -p domain repo_stats`.
- [ ] `median` returns `None` for an empty list, the middle value for an odd count, and the mean of the two middle values for 1, 2, 4, and 9 days, which is 3 days. Run `cargo test -p domain repo_stats`.
- [ ] `first_response` ignores a bot review and the author's own comment, picks the first human event after opening, and returns `None` when none exists. Run `cargo test -p domain repo_stats`.
- [ ] `first_response` ignores events that are dated before the pull request opened. Run `cargo test -p domain repo_stats`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Sign in through the stub and open Their turn at trunk (`main`) and at head on `repo-cards`. Trunk has no Repos tab and shows a dash for usual reply, so gate row order, ages, and links at both plus the filled cell at head. Save `repos-regression.png`. Pass when row order, ages, and links are identical at both, trunk shows a dash in every row, and head shows a value for every repo that has a reply.
- [ ] Lane 2. Happy path and idempotency. Open the Repos tab on `repo-cards`, then run the cron URL twice to sync again. Save `repos-basic.png`. Pass when each card's counts equal a `wrangler d1 execute radar --local` select over `items`, and the cards are unchanged after the two syncs.
- [ ] Lane 3. Merge rate. Open the cards for a repo with 3 merged, 1 closed, and 2 open pull requests, and for a repo with only open ones. Save `repos-merge-rate.png`. Pass when the first reads "75% merged" and the second reads "No merged or closed yet", not "0%".
- [ ] Lane 4. Median reply. Open the card whose responses took 1, 2, 4, and 9 days. Save `repos-median.png`. Pass when it reads "Maintainers reply in 3d".
- [ ] Lane 5. Bots, self, and no reply. Open the cards for a repo whose first event is a bot review followed by a maintainer comment 5 days later, a repo where only I commented, and one with no events. Save `repos-excluded.png`. Pass when the first reads "Maintainers reply in 5d" and the other two read "No reply yet".
- [ ] Lane 6. Their turn cell. Open Their turn on `repo-cards`. Save `repos-their-turn.png`. Pass when each row's "usually replies in" value equals the median on that repo's card, and a repo with no reply keeps the dash.
- [ ] Lane 7. Empty state. On a fresh `--persist-to` directory, open the Repos tab before any sync. Save `repos-empty.png`. Pass when it reads "No repositories yet. They appear after the first sync." and the text contains no `NaN`, `undefined`, or `null`.
- [ ] Lane 8. Session and read-only boundary. Run `curl -i http://127.0.0.1:<8800 + n>/api/repos` with no cookie, then read the GitHub stub's request log after the lanes. Save `repos-security.png` as a terminal capture. Pass when the status is `401`, and the stub log shows only GET requests and GraphQL queries that came from the Syncer.
- [ ] Lane 9. Phone width. At 375 px width, open the Repos tab. Save `repos-phone.png`. Pass when cards stack in one column, no text is clipped, and `document.documentElement.scrollWidth` is at most 375.
- [ ] Lane 10. Keyboard. Use only the keyboard to open Repos and move through the cards. Save `repos-keyboard.png`. Pass when Tab visits each card link in order with a visible focus ring, enter opens the repo on GitHub in a new tab, and the link has `rel="noopener noreferrer"`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `GET /api/dashboard` p95 on `wrangler dev` at trunk and head, since the diff adds the reply medians to it. `GET /api/repos` p95 at head, because trunk lacks it. Gzipped UI and Worker size at both.
- [ ] Probe. Alternate trunk and head three times. Each run loads `repo-cards` through the Syncer, sends 200 requests to `/api/dashboard` with a valid session, and takes the p95, then runs `node scripts/check-size.mjs`. At head, send 200 requests to `/api/repos`.
- [ ] Baseline. Record the trunk `/api/dashboard` p95 and both gzipped sizes first.
- [ ] Rule. Fail when the head `/api/dashboard` p95 is more than 10 ms or 25 percent above trunk, when `/api/repos` p95 is 100 ms or more, or when the Worker is above 400 KB or the UI above 200 KB gzipped. On the operator-approved preview, a `cpuTime` of 7 ms or more for `/api/repos` is a target miss and 10 ms or more is a hard fail.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, lane 6, and lane 9 screenshots into `artifacts/review/R21-review-cards.png`, `artifacts/review/R21-review-their-turn.png`, and `artifacts/review/R21-review-phone.png`.
- [ ] Record a 30 to 60 second video of the Repos tab and the filled usual reply column, from a lane's `wrangler dev` session. Save it as `artifacts/review/R21-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R21 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R21 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R21 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Show contribution trends as SVG charts (R22)

**Depends on.** R21.

**Files.**

- [ ] Create `crates/domain/src/trends.rs` and export it from `crates/domain/src/lib.rs`. It reuses `week_start` from `crates/domain/src/rollup.rs`.
- [ ] Create `crates/worker/src/db/trends.rs` and `crates/worker/src/api/trends.rs`.
- [ ] Create `crates/web/src/views/trends.rs`, and add the `Trends` variant to the `Tab` enum in `crates/web/src/components/tabs.rs`, in R20's quiet `More` nav group with no counter.
- [ ] Create `migrations/00NN_rollup_source.sql`, with `NN` the next free number on `main`, adding `rollups_weekly.source TEXT NOT NULL DEFAULT 'rolled' CHECK (source IN ('rolled', 'seeded'))`.
- [ ] Create `crates/worker/src/github/queries/contribution_history.graphql`, registered through `query_doc!`, and `crates/worker/src/sync/history.rs`, the handler for `WorkKind::SeedHistory`. Edit `crates/worker/src/sync/work.rs`, `handlers.rs`, and `mod.rs` to add the kind and enqueue it. These files belong to R08 to R11, so the owner tells the root before editing.
- [ ] Edit `tools/github-stub/server.mjs` so `/graphql` answers `ContributionHistory` from `scenario.history`, and create `tools/github-stub/scenarios/trends-history.json` with two years of pull request contributions.
- [ ] Create `crates/web/src/charts/mod.rs`, `crates/web/src/charts/bars.rs`, and `crates/web/src/charts/heatmap.rs`.
- [ ] Create `crates/web/style/trends.css`, which uses only the custom properties generated from `design/tokens.json`.
- [ ] Create `tools/github-stub/scenarios/trends-26w.json` with pull requests opened and merged across 26 weeks, in the stub's relative-time form.

**Build.**

- [ ] Name the data shapes first. `Trends { weeks: Vec<WeekBucket>, heatmap: Vec<DayCell>, merged_total: u32, repos_total: u32 }`. `WeekBucket { week_start: Date, opened: u32, merged: u32, repos: u32, source: WeekSource }`, where `WeekSource` is an enum with `Live`, `Rolled`, and `Seeded`. `DayCell { date: Date, count: u32, level: HeatLevel }`, where `HeatLevel` is an enum with `None`, `Low`, `Mid`, and `High`. Put them in `crates/domain/src/trends.rs`.
- [ ] Write `build_trends(items, events, rollups, now) -> Trends` as a pure function. It returns 52 weekly buckets that end in the current week, starting each on Monday in UTC. A week that has a row in `rollups_weekly` takes its numbers from the row, with `source` `Rolled` or `Seeded` from the row's `source` column. Any other week counts from live `items` and `events` with `source: Live`, so a boundary week is never counted twice (F8, N6).
- [ ] Count opened and merged from `items` for pull requests I authored. Count `repos` per week as the distinct repos with an event where `actor_is_me = 1` that week, from R10's event model. The heatmap covers the last 26 weeks, which is the span daily events survive before the rollup deletes them.
- [ ] Seed weekly history so 52 weeks show real data despite the 90-day backfill. While `history_seeded_at` is unset in the Syncer's storage, the nightly kick enqueues one `WorkKind::SeedHistory` row per year of history, keyed by the year's start. Each unit makes one read-only GraphQL call, `query ContributionHistory`, over `viewer { contributionsCollection(from: $from, to: $to) { pullRequestContributions(first: 100) { nodes { occurredAt pullRequest { mergedAt repository { nameWithOwner } } } } } }`, and a page past 100 nodes is a follow-up unit with `after`. It writes opened, merged, and distinct repos per week into `rollups_weekly` with `source = 'seeded'`, only for weeks older than the backfill window, with `ON CONFLICT DO NOTHING` so a rolled row always wins. The last unit sets `history_seeded_at`. It passes R06's N2 test and respects R08's rate-limit gate.
- [ ] Write `heat_level(count, max) -> HeatLevel`. It returns `None` for 0, `Low` when three times the count is at most the max, `Mid` when it is at most twice the max, and `High` above that.
- [ ] Write `nice_max(n) -> u32`, which rounds a chart ceiling up to 1, 2, 5, or 10 times a power of ten, with a minimum of 4.
- [ ] Write `GET /api/trends` in `crates/worker/src/api/trends.rs`. It requires a session and reads D1 only, in one batch of indexed selects (N7).
- [ ] Draw the weekly bars in `crates/web/src/charts/bars.rs` as hand-written SVG in Leptos with a `viewBox`. Opened uses `faint` and merged uses `text`. Gridlines use `line` and labels use `muted` at the caption size. The chart uses no other colour, because colour is reserved for waiting time and CI (ADR 0003).
- [ ] Draw the heatmap in `crates/web/src/charts/heatmap.rs` as 26 columns by 7 rows of 10 px cells with a 3 px gap, so it is 335 px wide and fits a 375 px screen with 16 px gutters. Levels use `line`, `faint`, `muted`, and `text`. Each cell has a `<title>` such as "3 events on 2026-10-14".
- [ ] Give each chart `role="img"` and an `aria-label` that states the totals. Add a visually hidden `<table>` per chart with the same numbers, so the data is readable without sight.
- [ ] Open the Trends view with a summary line at the title size, such as "14 pull requests merged in 26 weeks across 6 repos". Write "Nothing merged yet." when the totals are 0, and show empty axes with no `NaN`.

**You see.**

- [ ] A Trends tab shows a weekly bar chart of opened and merged pull requests, a distinct repos count, and a 26 week heatmap, all in grey on graphite.
- [ ] The summary line matches the numbers a `wrangler d1 execute radar --local` select returns.
- [ ] Hovering a heatmap cell shows its date and count.
- [ ] A fresh database shows "Nothing merged yet." with empty charts and no error.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `build_trends` with no data returns 52 buckets with the right Monday starts and zero counts. Run `cargo test -p domain trends`.
- [ ] A week that exists in both live data and `rollups_weekly` takes the rolled numbers once. A `seeded` row fills a week before the backfill window and is marked `Seeded`. Run `cargo test -p domain trends`.
- [ ] `sync/history.rs` gains `seed_skips_weeks_inside_the_backfill_window`, `seed_never_overwrites_a_rolled_week`, and `two_years_make_two_units`, and `contribution_history.graphql` passes R06's `every_registered_document_is_a_query`. Run `cargo test -p worker history`.
- [ ] Two events by me in one repo in one week give `repos: 1`, and events in two repos give 2. Run `cargo test -p domain trends`.
- [ ] A pull request merged on a Sunday and one opened on a Monday land in different weeks, and a year boundary week is placed correctly. Run `cargo test -p domain trends`.
- [ ] `heat_level` returns the right level at 0, 1 of 9, 3 of 9, 4 of 9, 6 of 9, and 9 of 9. `nice_max` returns 4 for 0, 5 for 5, 10 for 7, and 20 for 11. Run `cargo test -p domain trends`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Sign in through the stub and open Your turn, Their turn, and Pipeline at trunk (`main`) and at head on the same scenario. Trunk has no Trends tab, so gate the three views plus the Trends tab at head. Save `trends-regression.png`. Pass when the three views and `/api/dashboard` are identical at both and only head has Trends.
- [ ] Lane 2. Happy path and session. First run `curl -i http://127.0.0.1:<8800 + n>/api/trends` with no cookie, then sign in and open Trends on `trends-26w`. Save `trends-basic.png`. Pass when the unauthenticated call returns `401` with no data, and each bar height matches the opened and merged counts from `select` queries on `items`.
- [ ] Lane 3. Seeded, rolled, and live weeks. Run scenario `trends-history`, sign in, let the backfill finish, and fire the nightly cron twice. Then run scenario `feed-old` from R20 with the nightly cron and open Trends. Save `trends-rolled.png`. Pass when the stub log shows one `ContributionHistory` query per year of history and none on the second nightly run, weeks older than 90 days show the stub's numbers marked `Seeded`, rolled weeks come from `rollups_weekly`, recent weeks come from `items`, all 52 weeks have data, and the summed `opened` equals the sum of the pre-rollup counts plus the seeded counts.
- [ ] Lane 4. Empty state. On a fresh `--persist-to` directory, open Trends. Save `trends-empty.png`. Pass when the summary reads "Nothing merged yet.", the charts show empty axes, and the page text has no `NaN`, `undefined`, or `null`.
- [ ] Lane 5. Heatmap. Use events by me on known days. Save `trends-heatmap.png`. Pass when 182 cells show, today's cell sits in the last column on the right row, and each `<title>` count equals a `select count(*)` per day.
- [ ] Lane 6. Distinct repos. Use a week with five events in one repo and one in another. Save `trends-repos.png`. Pass when that week reports 2 repos in the hidden table and the summary repo total equals the distinct repos in D1.
- [ ] Lane 7. Tokens only. Read `getComputedStyle` for every `fill` and `stroke` in both charts. Save `trends-tokens.png`. Pass when each value equals one of `bg`, `surface`, `line`, `text`, `muted`, or `faint` from `design/tokens.json`, and none equals `warm`, `hot`, or `red`.
- [ ] Lane 8. Accessibility. Read the page with the accessibility tree. Save `trends-a11y.png`. Pass when each chart has `role="img"` with a label that states totals, the hidden tables have 52 and 26 week rows that equal the chart values, and the heatmap cells are not tab stops.
- [ ] Lane 9. Phone width. At 375 px width, open Trends on `trends-26w`. Save `trends-phone.png`. Pass when the heatmap is at most 343 px wide, the bars stay legible, and `document.documentElement.scrollWidth` is at most 375.
- [ ] Lane 10. Keyboard. Use only the keyboard to open Trends from the quiet nav group, and through the "More" menu at 375 px, then leave it again. Save `trends-keyboard.png`. Pass when the tab has a visible focus ring, enter opens the view, Tab leaves the charts without a trap, and `prefers-reduced-motion` shows no chart animation.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Gzipped UI size at trunk and head from `node scripts/check-size.mjs`, since the charts add UI code. Trunk lacks `/api/trends`, so measure its p95 on `wrangler dev` at head. Count GitHub requests the history seed makes, from the stub log. The end state the user waits for is time from opening Trends to the heatmap cells being in the DOM, from the Performance API.
- [ ] Probe. Alternate trunk and head three times for `node scripts/check-size.mjs`. At head, send 200 requests to `/api/trends` on `trends-26w` and take the p95. In the browser, record `performance.now()` at the route change and again when the 182nd cell exists, 10 times.
- [ ] Baseline. Record trunk gzipped UI and Worker size first. Trunk has no trends, so no trunk latency is invented.
- [ ] Rule. Fail when the head UI is more than 15 KB gzipped above trunk or above 200 KB, when the Worker is above 400 KB, when `/api/trends` p95 is 100 ms or more, when the median time to the heatmap is 1.5 s or more, or when the history seed makes more than one request per year of history plus one per 100 contributions past the first 100.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, lane 5, and lane 9 screenshots into `artifacts/review/R22-review-trends.png`, `artifacts/review/R22-review-heatmap.png`, and `artifacts/review/R22-review-phone.png`.
- [ ] Record a 30 to 60 second video of the Trends tab at laptop and phone width, with a heatmap hover, from a lane's `wrangler dev` session. Save it as `artifacts/review/R22-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R22 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R22 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R22 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Show agent work before a PR exists (R23)

**Depends on.** R22.

**Files.**

- [ ] Create `migrations/00NN_agent_work.sql`, with `NN` the next free number on `main`.
- [ ] Create `crates/domain/src/agents.rs` and export it from `crates/domain/src/lib.rs`.
- [ ] Create `crates/worker/src/hooks/mod.rs`, `crates/worker/src/hooks/secret.rs`, `crates/worker/src/hooks/github.rs`, `crates/worker/src/hooks/checkin.rs`, `crates/worker/src/api/agents.rs`, and `crates/worker/src/db/agents.rs`. Add the three routes `POST /hooks/github`, `POST /hooks/checkin`, and `GET /api/agents` in `crates/worker/src/lib.rs`.
- [ ] Create `crates/web/src/views/agents.rs`, and add the `Agents` variant to the `Tab` enum in `crates/web/src/components/tabs.rs`, in R20's quiet `More` nav group with no counter.
- [ ] Create `scripts/send-webhook.mjs`, which signs a fixture with a secret and posts it with the right headers.
- [ ] Create `tools/github-stub/webhooks/` with `push-agent-branch.json`, `push-main.json`, `push-delete.json`, `push-foreign-owner.json`, and `ping.json`.
- [ ] Edit `.dev.vars.example` to add `WEBHOOK_SECRET=` and `AGENT_TOKEN=`.

**Build.**

- [ ] Name the data shapes first. `AgentKey` is an enum with `Branch { repo, branch }` and `Issue { repo, number, agent }`, stored as one text primary key. `Reported` is an enum with `Working`, `Blocked`, and `Finished`. `AgentRow { key, repo, branch, issue, agent, reported, title, first_seen_at, last_signal_at }`. The derived `AgentState` is an enum with `Working`, `Quiet`, `Blocked`, and `Finished`. Put them in `crates/domain/src/agents.rs`.
- [ ] Write `agent_state(reported, last_signal_at, now) -> AgentState`. `Finished` and `Blocked` pass through. `Working` becomes `Quiet` after 2 hours without a signal. A row leaves the lane when it is `Finished` or its last signal is older than 7 days.
- [ ] Write `flag_overlaps(rows) -> Vec<Overlap>` in the same file. It marks every pair of unfinished rows that name the same `repo` and issue number, so the lane shows when two agents hold one issue (user story 7).
- [ ] Both write routes fail closed. `hooks/secret.rs` parses `WEBHOOK_SECRET` and `AGENT_TOKEN` at the boundary into `HookSecret`, which only exists for a value of at least 32 bytes. When the secret a route needs is missing, empty, or shorter than 32 bytes, that route returns `500` with an empty body before it reads the request, writes nothing, and logs `hook secret missing name=<name>` without the value. O3 sets both secrets, and lanes use values of at least 32 bytes.
- [ ] Write `POST /hooks/github` in `crates/worker/src/hooks/github.rs`. It rejects a body above 256 KB with `413` before reading it. It reads the raw bytes, computes HMAC-SHA256 with the `WEBHOOK_SECRET` secret, and compares to `X-Hub-Signature-256` in constant time. A missing, malformed, or wrong signature gets `401` with an empty body and writes nothing (N3). The legacy `X-Hub-Signature` header is not accepted.
- [ ] After the signature check, route on `X-GitHub-Event`. `ping` returns `{"outcome":"pong"}`. `push` is handled below. Any other event returns `{"outcome":"ignored_event"}`. `HookOutcome` is an enum with `Recorded`, `Removed`, `Pong`, `IgnoredEvent`, `IgnoredRef`, and `IgnoredOwner`, serialised in snake case. A verified delivery always returns `200`, and malformed JSON after a valid signature returns `400`.
- [ ] Handle `push` only when `repository.owner.id` equals `ALLOWED_USER_ID`, so the hook tracks the operator's own forks. Accept only refs that start with `refs/heads/agent/`. A push upserts the `Branch` row with `Reported::Working`, the first line of `head_commit.message` cut to 120 characters as `title`, and `last_signal_at` set to now. A `deleted` push sets `Reported::Finished`. Replaying a delivery sets the same values, so it is idempotent (N8). Store no comment bodies (N5).
- [ ] Write `POST /hooks/checkin` in `crates/worker/src/hooks/checkin.rs`. It lives under `/hooks/*`, so O5's Cloudflare Access exemption for `/hooks/*` covers it and agents can reach it without an Access login. It requires `Authorization: Bearer <token>` that equals `AGENT_TOKEN` by a constant-time compare of SHA-256 digests. It ignores cookies, so a browser session cannot call it. A missing or wrong token gets `401`. The token is never accepted in the query string.
- [ ] Parse the check-in body once into `AgentCheckin { agent: AgentName, repo: RepoName, branch: Option<BranchName>, issue: Option<IssueNumber>, state: Reported, note: Option<ShortNote> }` with `deny_unknown_fields`. Cap the body at 4 KB with `413`, the agent name at 40 characters, and the note at 140. A bad body gets `400`. The check-in upserts by key, so a repeat only moves `last_signal_at` (F11).
- [ ] Add `GET /api/agents` in `crates/worker/src/api/agents.rs`, which requires a session and returns the visible rows with `AgentState` and overlaps. Both write paths touch only Radar's D1 tables, and neither module calls the GitHub client (the read-only rule).
- [ ] Build the Agents view in `crates/web/src/views/agents.rs` from the shared list components. A row shows the agent name, repo and branch, the issue as a link, a grey state word, and the age of the last signal. A row that shares an issue says "also taken by <agent or branch>" in grey. Write the empty state as "No agents are working. They appear here when they push to an agent branch on your forks or check in."
- [ ] Define `scripts/send-webhook.mjs <port> <fixture> [--secret <value>] [--event <name>]` so lanes sign payloads the way GitHub does with `X-Hub-Signature-256`.

**You see.**

- [ ] A signed push to `refs/heads/agent/fix-123` on the operator's fork returns `200` with `{"outcome":"recorded"}` and shows a Working row in the Agents tab.
- [ ] A push with a wrong signature returns `401` and the Agents tab does not change.
- [ ] A check-in naming an issue shows a row with that issue, and a second agent on the same issue makes both rows say "also taken by".
- [ ] The `wrangler dev` log shows `hook push outcome=recorded repo=<owner/name> branch=<branch>` and never the secret or token.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `agent_state` returns `Working` at 119 minutes, `Quiet` at 121 minutes, `Blocked` and `Finished` unchanged at 3 days, and the visibility rule drops a row at 8 days. Run `cargo test -p domain agents`.
- [ ] `flag_overlaps` marks two unfinished rows on one issue, ignores a `Finished` row, and ignores the same issue number in two different repos. Run `cargo test -p domain agents`.
- [ ] The signature check accepts a known good HMAC vector and rejects a flipped bit, an empty header, `sha1=` form, uppercase hex of a wrong digest, and a missing `sha256=` prefix. Run `cargo test -p domain agents`.
- [ ] `AgentCheckin` parsing rejects an unknown field, a 41 character agent name, a 141 character note, and `repo` of `a/b/c`. Run `cargo test -p domain agents`.
- [ ] `hooks/secret.rs` gains `missing_secret_fails_closed`, `empty_secret_fails_closed`, `thirty_one_byte_secret_fails_closed`, and `thirty_two_byte_secret_is_accepted`. Run `cargo test -p worker hooks`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Sign in through the stub and open Your turn at trunk (`main`) and at head on the same scenario. Trunk has no Agents tab, so gate Your turn and `/api/dashboard` at both plus the Agents tab at head. Save `agents-regression.png`. Pass when the two `/api/dashboard` bodies are identical and only head has an Agents tab.
- [ ] Lane 2. Signed push, then branch deleted. Start Radar with `--var WEBHOOK_SECRET:lane-webhook-secret-0123456789abcdef` (36 bytes) and run `node scripts/send-webhook.mjs <port> tools/github-stub/webhooks/push-agent-branch.json --secret lane-webhook-secret-0123456789abcdef`. Then post `push-delete.json` the same way. Save `agents-push.png`. Pass when the first output is `200` with `recorded` and the Agents tab shows a Working row for the branch, and after the delete the row leaves the Agents tab on the next refetch and the stored `reported` value is `finished`.
- [ ] Lane 3. Bad signatures. Post `push-agent-branch.json` with a wrong secret, with no signature header, with only `X-Hub-Signature`, and as a `GET`. Save `agents-bad-signature.png` as a terminal capture. Pass when the first three return `401`, the `GET` returns `405`, and `select count(*) from agent_work` is unchanged.
- [ ] Lane 4. Ignored pushes. Post `push-main.json`, `push-foreign-owner.json`, and `ping.json` with the right secret. Save `agents-ignored.png` as a terminal capture. Pass when the outcomes are `ignored_ref`, `ignored_owner`, and `pong`, and no row is added.
- [ ] Lane 5. Fail closed. Restart Radar three times, with no `WEBHOOK_SECRET` or `AGENT_TOKEN`, with both set empty, and with both set to the 31 byte `short-secret-0123456789abcdefgh`. Each time, post `push-agent-branch.json` signed with the configured value (`--secret ''` for the empty case) and send a check-in to `POST /hooks/checkin` with that value as the bearer token. Save `agents-fail-closed.png` as a terminal capture. Pass when every call returns `500` with an empty body, `select count(*) from agent_work` stays 0, the log shows `hook secret missing` and never the value, and the GitHub stub log shows no request.
- [ ] Lane 6. Replay. Post the same signed delivery three times. Save `agents-replay.png` as a terminal capture. Pass when `select count(*) from agent_work` is 1 and `title` and `reported` equal their values after the first post.
- [ ] Lane 7. Check-ins and overlap. With `--var AGENT_TOKEN:lane-agent-token-0123456789abcdef` (33 bytes), send check-ins from two agents for the same issue with `curl -X POST http://127.0.0.1:<8800 + n>/hooks/checkin -H "Authorization: Bearer lane-agent-token-0123456789abcdef"`. Save `agents-overlap.png`. Pass when both rows show "also taken by" naming the other agent, and a check-in with state `finished` removes its row.
- [ ] Lane 8. Check-in boundary. Send to `POST /hooks/checkin` a check-in with no token, a wrong token, the token in the query string, a session cookie only, a 5 KB body, an unknown field, and a 41 character agent name. Save `agents-checkin-security.png` as a terminal capture. Pass when the first four return `401`, the oversized body returns `413`, the last two return `400`, no row is written, and the GitHub stub log shows no request caused by any of these calls.
- [ ] Lane 9. Phone width and empty state. At 375 px width, open the Agents tab with no rows, then after lane 7. Save `agents-phone.png`. Pass when the empty state reads as specified, rows stack without clipping, and `document.documentElement.scrollWidth` is at most 375.
- [ ] Lane 10. Keyboard. Use only the keyboard to open Agents, move with `j` and `k`, and open a row with enter. Save `agents-keyboard.png`. Pass when focus rings are visible, the selection follows `j` and `k`, enter opens the branch or issue on GitHub in a new tab, and `e` and `s` do nothing.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Trunk lacks `POST /hooks/github` and `POST /hooks/checkin`. Measure `POST /hooks/github` wall time on `wrangler dev` for a 100 KB signed push, and `cpuTime` for the same request on an operator-approved preview deploy. Measure gzipped Worker and UI size at trunk and head.
- [ ] Probe. Alternate trunk and head three times for `node scripts/check-size.mjs`. At head, post a 100 KB signed push 100 times with `node scripts/send-webhook.mjs` and take the p95. On the preview deploy, the operator runs `npx wrangler tail --format json` while 20 signed pushes arrive, and the owner reads `cpuTime`.
- [ ] Baseline. Record trunk gzipped sizes first. No trunk webhook number exists, so none is invented.
- [ ] Rule. Fail when the Worker grows by more than 20 KB gzipped over trunk or passes 400 KB, when the UI passes 200 KB, when the p95 for a 100 KB push is 100 ms or more, or when preview `cpuTime` is 5 ms or more as a target miss and 10 ms or more as a hard fail.

**Review gate.** The operator reviews before merge.

- [ ] Copy lane 2, lane 7, and lane 9 screenshots into `artifacts/review/R23-review-push.png`, `artifacts/review/R23-review-overlap.png`, and `artifacts/review/R23-review-phone.png`.
- [ ] Record a 30 to 60 second video of a signed push, a check-in, and the overlap mark appearing, from a lane's `wrangler dev` session. Save it as `artifacts/review/R23-review.mp4`.
- [ ] Post the screenshots and the video in chat. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R23 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R23 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R23 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Send phone alerts through Telegram (R24)

**Depends on.** R23.

**Files.**

- [ ] Create `migrations/00NN_alerts.sql`, with `NN` the next free number on `main`.
- [ ] Create `crates/domain/src/alert.rs` and export it from `crates/domain/src/lib.rs`.
- [ ] Create `crates/worker/src/alerts/mod.rs` and `crates/worker/src/alerts/telegram.rs`.
- [ ] Edit `crates/worker/src/sync/work.rs` to add `WorkKind::SendAlerts` with the key `main`, `crates/worker/src/sync/enrich.rs` to enqueue it after it writes events, and `crates/worker/src/sync/handlers.rs` to route it to `alerts::dispatch`. These files belong to R08 to R11, so the owner tells the root first.
- [ ] Edit `crates/worker/tests/read_only.rs` (R06) to add `alerts/telegram.rs` to the allow-list, next to `github/client.rs` and R07's `auth/exchange.rs`.
- [ ] Create `tools/telegram-stub/server.mjs`, a Node server with no dependencies that serves `POST /bot<token>/sendMessage` and `GET /requests`.
- [ ] Create `tools/github-stub/scenarios/alerts.json` with the threads listed in the lanes below.
- [ ] Edit `.dev.vars.example` to add `TELEGRAM_BOT_TOKEN=` and `TELEGRAM_CHAT_ID=`.

**Build.**

- [ ] Name the data shapes first. `AlertsMode` is an enum with `Off` and `On(TelegramConfig { token, chat_id })`, built from the two secrets. Both must be present and non-empty, otherwise the mode is `Off` (F13). `AlertDecision` is an enum with `Send` and `Skip(SkipReason)`, and `SkipReason` is an enum with `WrongReason`, `NotMyTurn`, `BotActor`, and `TooOld`. `AlertState` is an enum with `Sent` and `Failed { attempts }`. Put the decision types in `crates/domain/src/alert.rs`.
- [ ] Write `decide(event: &Event, item, now) -> AlertDecision` as a pure function over R10's event model. It sends only for an `EventKind::Mentioned` or `EventKind::ReviewRequested` event where the item's turn is `Me(Mentioned)` or `Me(ReviewRequested)`, the actor is not a bot, and the event is under 6 hours old. The age cap stops a flood when alerts are first switched on.
- [ ] Write `alert_text(&Alert) -> String`. It gives plain text of at most four lines with the reason, repo and number, who acted, the title, and the GitHub link. It has no `parse_mode`, so a title with markup is sent as inert text. It contains metadata only (N5).
- [ ] Add the `alerts` table with `event_id TEXT PRIMARY KEY`, `state`, `attempts`, and `updated_at`. `event_id` is the event id from N8, so one event is one message.
- [ ] Alerts are their own unit. Enrich never sends a message. After it writes events, it enqueues `WorkKind::SendAlerts` keyed `main`, so many Enrich batches make one row, and R08's scheduler runs it in its own alarm. `alerts::dispatch(db, env, now)` in `crates/worker/src/alerts/mod.rs` is that unit's handler.
- [ ] In `Off` mode `dispatch` returns `Done` at once and marks nothing, so enabling alerts later still sends fresh events. Otherwise it selects events with no `alerts` row, or a `Failed` row with fewer than 3 attempts, calls `decide`, sends at most 5 messages in order of time, and writes `Sent` or `Failed` for each. When eligible events remain, it returns a follow-up `SendAlerts` row. A crash between a send and its row can send one message twice, which is the cost of never losing an alert.
- [ ] Write `crates/worker/src/alerts/telegram.rs` with one function, `send(config, text)`. It posts JSON `{ chat_id, text, link_preview_options: { is_disabled: true } }` to `<TELEGRAM_API_BASE>/bot<token>/sendMessage`, with the base from the var `TELEGRAM_API_BASE` and `https://api.telegram.org` as the default. A non-2xx status, including `429`, is a failed attempt and is not retried in the same call. This client is separate from the GitHub client, which stays `get` and `query` only.
- [ ] The read-only scanner allows the Telegram POST and nothing more. `tests/read_only.rs` lists `alerts/telegram.rs` in its allow-list, still fails on any POST in a file outside the list, and gains `telegram_client_never_names_a_github_host`, which fails when `alerts/telegram.rs` contains `github.com`, `GITHUB_API_BASE`, `GITHUB_WEB_BASE`, or `github::`. A POST to a GitHub host therefore stays impossible outside R06's client and R07's code exchange (N2).
- [ ] Never log, store, or return the token. Log `alert event=<id> outcome=<sent or failed or skipped>` and the path as `/bot<redacted>/sendMessage`. The freshness response may say `alerts on` or `alerts off` and nothing more.
- [ ] Write `tools/telegram-stub/server.mjs --port <port> [--fail-first <n>] [--status <code>]`. It records each request as one JSON line on stdout and in `GET /requests`, and returns `{"ok":true}` unless told to fail. Lanes use port `9000 + n`.

**You see.**

- [ ] With both secrets set, a fresh mention on `alerts.json` makes the Telegram stub record exactly one `sendMessage` with the repo, the title, and the link.
- [ ] With no secrets, the log shows `alerts off` once and the Telegram stub records nothing.
- [ ] Running the sync again records no new message.
- [ ] The phone shows one Telegram message per mention or review request. Operator item, checked only when the operator posts a screenshot after configuring the real bot. This does not gate the verdict.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `decide` returns `Send` for a fresh mention and a fresh review request on my turn. Run `cargo test -p domain alert`.
- [ ] `decide` returns `Skip(WrongReason)` for `CiFailed`, `Commented`, and `Assigned` events, `Skip(BotActor)` for a bot mention, `Skip(NotMyTurn)` for a dismissed or closed item, and `Skip(TooOld)` at 6 hours and 1 second. Run `cargo test -p domain alert`.
- [ ] `AlertsMode::from_secrets` returns `Off` for no secrets, only a token, only a chat id, and an empty string for either, and `On` for both. Run `cargo test -p domain alert`.
- [ ] `alert_text` keeps `*`, `_`, `<`, and backticks from a title unchanged, and never contains the token. Run `cargo test -p domain alert`.
- [ ] `tests/read_only.rs` passes with the new allow-list entry, and `telegram_client_never_names_a_github_host` fails on a planted `GITHUB_API_BASE` in a scratch copy of `alerts/telegram.rs`. A `work.rs` test `send_alerts_enqueued_twice_is_one_row` passes. Run `cargo test -p worker --test read_only` and `cargo test -p worker sync::`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run scenario `alerts` with no secrets at trunk (`main`) and at head, and count the GitHub stub's requests at each. Trunk sends no alerts, so gate the GitHub request count and the Your turn rows at both plus zero Telegram calls at head. Save `alerts-regression.png` as a terminal capture. Pass when the two request logs have the same count and paths, the `items` rows match, and the Telegram stub records nothing.
- [ ] Lane 2. Happy path. Start the Telegram stub on `9000 + n`, then Radar with `--var TELEGRAM_BOT_TOKEN:lane-token --var TELEGRAM_CHAT_ID:42 --var TELEGRAM_API_BASE:http://127.0.0.1:<9000 + n>`, and run the sync on `alerts`. Save `alerts-sent.png` as a terminal capture. Pass when the stub shows two requests, one for the mention and one for the review request, each with `chat_id` 42 and the title and link in the text.
- [ ] Lane 3. Off by default. Start Radar with no Telegram vars and run the sync. Save `alerts-off.png` as a terminal capture. Pass when the log shows `alerts off` once, the stub shows no request, and `select count(*) from alerts` is 0.
- [ ] Lane 4. Partial config. Start with only `TELEGRAM_BOT_TOKEN`, then with only `TELEGRAM_CHAT_ID`. Save `alerts-partial.png` as a terminal capture. Pass when both runs behave as lane 3.
- [ ] Lane 5. Dedupe and batch cap. After lane 2, run the cron URL three times. Then add 12 fresh mentions to the scenario and sync. Save `alerts-dedupe.png` as a terminal capture. Pass when the first three runs add no request, and the 12 mentions produce 5 messages on the first dispatch and the rest on later ones, with no event sent twice.
- [ ] Lane 6. Filter. Use scenario threads for a CI failure on my PR, a plain reply on my PR, a mention by a bot, and a mention 10 hours old. Save `alerts-filtered.png` as a terminal capture. Pass when none of the four produces a request and the `alerts` table has no row for them.
- [ ] Lane 7. Telegram failure. Start the Telegram stub with `--fail-first 2`, then sync three times. Save `alerts-retry.png` as a terminal capture. Pass when the first two attempts end in `Failed` with `attempts` 1 and 2, the third ends in `Sent`, and no event is attempted more than 3 times.
- [ ] Lane 8. Rate limit and permanent failure. Start the stub with `--status 429`, then sync five times. Save `alerts-429.png` as a terminal capture. Pass when each event is attempted at most 3 times in total, the Syncer does not loop, and the other work units still run.
- [ ] Lane 9. Secret hygiene. After the lanes above, search the `wrangler dev` log, the output of `wrangler d1 execute radar --local --command ".dump"`, and the bodies of `/api/dashboard` and `/api/freshness` for `lane-token`. Also sync a title that contains `*bold* <b>x</b> [a](b)`. Save `alerts-secrets.png` as a terminal capture. Pass when `lane-token` appears in none of them, the logged path is `/bot<redacted>/sendMessage`, and the stub received the title as literal text.
- [ ] Lane 10. Read-only boundary. Read the GitHub stub's request log after all lanes and run `rg -n "reqwest|Method::Post|Method::Put|Method::Patch|Method::Delete" crates/worker/src/github`. Save `alerts-read-only.png` as a terminal capture. Pass when the GitHub log has only GET requests and GraphQL queries, the `rg` search prints nothing, `cargo test -p worker github` still passes the mutation and method tests from R06, `cargo test -p worker --test read_only` passes with `alerts/telegram.rs` as the only new allow-list entry, and the `wrangler dev` log shows every `sendMessage` under a `unit start kind=send_alerts` line and none under `kind=enrich`.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. GitHub requests per sync of scenario `alerts`, which both sides produce. Telegram requests per event at head, where trunk has none. `cpuTime` of a `SendAlerts` alarm that sends 5 alerts and of an `Enrich` alarm that enqueues it, on an operator-approved preview deploy. Gzipped Worker size at both.
- [ ] Probe. Alternate trunk and head three times. Each run syncs `alerts` and counts the GitHub stub's requests, then runs `node scripts/check-size.mjs`. At head, count the Telegram stub's requests per event. On the preview deploy, the operator runs `npx wrangler tail --format json` through one alert batch, and the owner reads `cpuTime` for the `SendAlerts` and `Enrich` alarms.
- [ ] Baseline. Record the trunk GitHub request count for the scenario and the trunk Worker gzipped size first. Record the trunk `cpuTime` of an `Enrich` alarm on the same preview. Trunk has no `SendAlerts` unit, so its budget is absolute.
- [ ] Rule. Fail when head makes any more GitHub requests than trunk, when it sends more than one Telegram request per event, when the Worker grows by more than 10 KB gzipped or passes 400 KB, when a `SendAlerts` alarm that sends 5 alerts shows `cpuTime` of 7 ms or more as a target miss and 10 ms or more as a hard fail, or when the head `Enrich` alarm is more than 1 ms above trunk's.

**Review gate.** None. R24 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R24 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R24 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R24 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Bring the PRD in line with the ADRs (R25)

**Depends on.** None.

**Files.**

- [ ] Edit `docs/prd.md`. No other file changes.

**Build.**

- [ ] Name the data shape first. The work is a table of rows, each one a stale PRD statement, the ADR that replaced it, and the new wording. The owner writes the table in the PR body and edits the PRD one row at a time. The sources are `docs/adr/0002-architecture.md`, `docs/adr/0003-design.md`, and `docs/adr/0005-rust.md`, and the ADR wins wherever it differs from the PRD.
- [ ] Add one sentence to the Overview that says an accepted ADR overrides this document where they disagree, with a link to `adr/README.md`.
- [ ] Replace the architecture paragraph that names Queues and KV, and the line "3 triggers, 1 queue, 1 consumer", with the Syncer Durable Object. Describe the three triggers calling `kick(reason)`, and link ADR 0002. Replace the embedded diagram placeholder with one sentence and a link to the ADR's diagram.
- [ ] Rewrite the data model table. `threads` and `pull_requests` become the names R04 defines, `notification_threads` and `items`, with the derived `turn`, `reason`, `since`, and `stage` columns. Remove the KV `sync:*` row and say that poll state lives in the Syncer's storage. Leave `events`, `repos`, `rollups_weekly`, `local_state`, and `auth` as they are.
- [ ] Replace the Queues row in the free-tier table with a Durable Objects row for one `Syncer` instance. Do not invent allowance numbers. Say the Free-plan fit is proven by R02 and link ADR 0002.
- [ ] Rewrite E3, P1, and P2 to match ADR 0002. E3 says the Email Worker forwards and kicks the Syncer. P1 says `Last-Modified` and the next allowed poll time live in Syncer storage. P2 says a trigger before the next allowed time is deferred, not dropped, as the ADR replaced it. Rewrite N8 to name the Syncer, and N9 to say dead `work` rows replace the dead-letter queue and show on the freshness panel.
- [ ] Update F1 to add `replied` to the list of reasons. Update F1 and F4 to say both come from the `turn` of each item. Change the F9 priority from P1 to P0. Change M1 and M3 so they no longer mention a queue.
- [ ] Rename the PRD's view names to the ones in ADR 0003 everywhere they appear. "Needs me" becomes "Your turn", "Waiting on others" becomes "Their turn", and "PR pipeline" becomes "Pipeline". This is the only edit allowed in User stories.
- [ ] Add a "Done when" line to M4 and M5 in the same form as M0 to M3. M4 is done when the feed, repo cards, and trends show my real data on the preview deploy. M5 is done when a push to an `agent/` branch on a fork shows in the Agents view within 2 minutes and a mention reaches my phone through Telegram.
- [ ] Mark A7 as optional operator setup item O5 in `docs/plan/implementation-plan.md`, with a relative link to the plan. This is the only edit allowed in the Authentication section.
- [ ] Rewrite N11 to say Rust throughout, one Cargo workspace, deployed with Wrangler, with D1 migrations, and link ADR 0005. Close the frontend open question by naming Leptos, built with Trunk and served as Workers static assets, with a link to ADR 0005. Record the alerts channel question for F13 as decided, Telegram, with a pointer to R24 in `docs/plan/implementation-plan.md`. Leave the issues lane and maintainer view questions unchanged.
- [ ] Keep the PRD's voice. Use first person where it already does, keep its tables and numbering, and add no long dashes or curly quotes. Do not touch Goals, the rate-limit budget, or Setup notes. User stories change only by the view rename, and Authentication only by the A7 note.

**You see.**

- [ ] `rg -n -i "queue|\bKV\b|dead-letter|enqueue" docs/prd.md` prints only lines that name ADR 0002 as the replacement.
- [ ] `rg -n -i "typescript|next\.js|react|vite|hono" docs/prd.md` prints only the closed frontend question and its ADR 0005 link.
- [ ] The F9 row in the functional requirements table ends in `P0`, and the F1 row lists `replied`.
- [ ] `rg -n "Needs me|Waiting on others|PR pipeline" docs/prd.md` prints nothing, M4 and M5 each end with a "Done when" line, and A7 names O5.
- [ ] `git diff --stat origin/main...HEAD` lists `docs/prd.md` and nothing else.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] This PR has no code, so the unit evidence is the stale term search. Run `rg -n -i "queue|\bKV\b|dead-letter|enqueue|typescript|next\.js" docs/prd.md` and show that each hit sits in a sentence that names the ADR that replaced it.
- [ ] Run `rg -n "\]\((?!http)[^)#]+" -P -o docs/prd.md`, and show that every relative link target exists under `docs/`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `sonnet` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `git diff -U0 origin/main...HEAD -- docs/prd.md` and compare it to trunk's `docs/prd.md`. Trunk has no corrections, so record its stale term counts and gate that the hunks at head all fall inside the sections in the PR body's table. Save `prd-regression.png` as a terminal capture. Pass when every hunk header falls inside a listed section, no hunk touches Goals, the rate-limit budget, or Setup notes, and the only hunks in User stories and Authentication are the view rename and the A7 note.
- [ ] Lane 2. Queue and KV terms. Run `rg -n -i "queue|\bKV\b|dead-letter|enqueue" docs/prd.md`. Save `prd-queue-terms.png` as a terminal capture. Pass when every remaining hit is in a sentence that names ADR 0002.
- [ ] Lane 3. Trigger and polling rules. Read E3, P1, P2, N8, and N9 at head. Save `prd-rules.png` as a terminal capture. Pass when E3 says the Email Worker kicks the Syncer, P1 puts poll state in Syncer storage, P2 says deferred, N8 names the Syncer, and N9 says dead `work` rows.
- [ ] Lane 4. Requirements table. Print the F1, F4, and F9 rows. Save `prd-requirements.png` as a terminal capture. Pass when F1 lists `replied`, F1 and F4 say they come from `turn`, and F9 has priority `P0`.
- [ ] Lane 5. Language and frontend terms. Run `rg -n -i "typescript|next\.js|react|vite|hono" docs/prd.md` and print N11. Save `prd-rust.png` as a terminal capture. Pass when N11 says Rust and links ADR 0005, and the only hits for the other words are in the closed frontend question.
- [ ] Lane 6. Voice. Run `git diff -U0 origin/main...HEAD -- docs/prd.md | rg "^\+" | rg -P "[\x{2013}\x{2014}\x{2018}\x{2019}\x{201c}\x{201d}]"`. Save `prd-voice.png` as a terminal capture. Pass when the search prints nothing, so no added line has a long dash or a curly quote, and the added lines keep the first person voice where the PRD had it.
- [ ] Lane 7. Links resolve. Extract each relative link with `rg -P -o "\]\((?!http)[^)#]+" docs/prd.md`, and test each target from `docs/` with `test -e`. Save `prd-links.png` as a terminal capture. Pass when every `test -e` succeeds, including the new links to `adr/README.md`, `adr/0002-architecture.md`, and `adr/0005-rust.md`.
- [ ] Lane 8. Data model names. Compare the table names in the PRD data model to `migrations/0001_init.sql` if it exists on trunk, and otherwise to the table names in R04's Build box. Save `prd-tables.png` as a terminal capture. Pass when every name in the PRD appears in the source, and no `sync:*` row remains.
- [ ] Lane 9. Untouched sections and new lines. Run `git diff -U0 origin/main...HEAD -- docs/prd.md | rg "^@@"` and map each hunk to its section by line number, then run `rg -n "Needs me|Waiting on others|PR pipeline" docs/prd.md` and print M4, M5, and A7. Save `prd-untouched.png` as a terminal capture. Pass when no hunk falls in Goals, the rate-limit budget, or Setup notes, the User stories and Authentication hunks hold only the view rename and the A7 note, the `rg` prints nothing, M4 and M5 carry the "Done when" lines from the Build list, and A7 names O5.
- [ ] Lane 10. Open questions. Print the Open questions section. Save `prd-open-questions.png` as a terminal capture. Pass when the frontend question is closed with a link to ADR 0005, the alerts channel question is recorded as decided for Telegram, and the issues lane and maintainer view questions are byte-identical to trunk apart from the view rename.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Docs have no runtime cost, so measure the edit itself. Count the stale terms `Queue`, `KV`, `dead-letter`, `enqueue`, `TypeScript`, and `Next.js` outside sentences that name an ADR, and count changed lines in `docs/prd.md`. Both exist at trunk and head.
- [ ] Probe. Run `rg -c -i "queue|\bKV\b|dead-letter|enqueue|typescript|next\.js" docs/prd.md` and `git diff --shortstat origin/main...HEAD`, at trunk and at head, in that order.
- [ ] Baseline. Record the trunk stale term count first.
- [ ] Rule. Fail when the head stale term count outside ADR-naming sentences is above 0, when more than 120 lines change, or when any file other than `docs/prd.md` appears in the diff.

**Review gate.** None. R25 is not review-gated.

**Merge.**

- [ ] CI is green and CodeRabbit has approved R25 at the exact head SHA, with every thread resolved.
- [ ] Root's clean swarm verdict at the exact head SHA. If R25 is review-gated, the operator has approved in chat.
- [ ] Rebased onto `main` after the verdict, patch-id unchanged.
- [ ] The Opus owner marked every changed file as Viewed with `node scripts/pr-mark-viewed.mjs <number>` and submitted a review with `gh pr review <number> --comment --body "<verdict and evidence>"`.
- [ ] The Opus owner squash-merged R25 to `main` with `gh pr merge <number> --squash --delete-branch`. The merge SHA is recorded here.

## Close the program

- [ ] Every box above is checked with its evidence.
- [ ] Every R, R01 to R25, is merged to `main` through its own PR.
- [ ] M2 is done. The operator has used Radar daily for a week instead of github.com/notifications, and checks this box.
- [ ] M4 is done. The feed, repo cards, and trends show the operator's real data on the preview deploy, and the operator checks this box.
- [ ] M5 is done. A push to an `agent/` branch on a fork shows in the Agents view within 2 minutes, a mention reaches the operator's phone through Telegram, and the operator checks this box.
- [ ] Reply to the operator with the report `playbooks/autopilot-full.md` names, each PR with its verdict and merge SHA.

## Appendix A. Prototype evidence

These questions are already answered, with the evidence in the repository.

| Question | Answer | Evidence |
| --- | --- | --- |
| Which visual direction fits a focused inbox? | Triage rows plus the large count, on graphite | `prototypes/design-directions.html` and `prototypes/direction-ac.html`, commit `a2f4d84` |
| Does a Rust Worker fit the free-plan size limit? | Yes. 134 KB gzipped against 3 MB | `spikes/rust-fit/`, commit `af5573e`, `node measure.mjs` |
| Does a Leptos UI fit the 1.5 s first-load target? | Likely. 66 KB gzipped for a list screen | Same spike |
| Can Rust parse a 20-PR GraphQL response inside 10 ms of CPU? | Yes on V8 locally. 0.60 ms median, 4.8 ms on the first call | Same spike |
| Does `workers-rs` expose every binding Radar needs? | Yes in 0.8.7. `fetch`, `scheduled`, `email`, Durable Object SQL and alarms, D1, Queues | ADR 0005 |

These stay unproven until R02 runs on Cloudflare.

- Durable Object alarms with SQLite storage run on the Workers Free plan. Two Cloudflare pages disagree.
- CPU time per Syncer alarm stays under 10 ms on Cloudflare, including the first call into a fresh Wasm instance.

## Appendix B. Alternatives rejected

| Approach | Why it lost |
| --- | --- |
| One PR per milestone | Too large to verify. A failing lane could not point at one change |
| Test against real GitHub in live lanes | Needs a real token in every lane, is not repeatable, and spends rate limit. The GitHub stub serves fixtures instead |
| A development login that skips OAuth | Code that can mint a session without GitHub would exist in production. The stub emulates GitHub's OAuth endpoints instead |
| Queues and KV for sync, as in the PRD | Replaced by the Syncer in ADR 0002. R02 decides whether the Queue fallback is needed |
| The operator lands every PR (`autopilot-stack`) | The operator wants each PR reviewed by CodeRabbit and merged by its Opus owner. Deploys stay with the operator |
| One long stack of unmerged PRs | Each PR merges before the next one starts, so `main` always holds the verified work and CodeRabbit reviews small diffs |

## Appendix C. Risks

| Risk | Lands in | What the owner watches |
| --- | --- | --- |
| Durable Object alarms are not available on Workers Free | R02 | If the spike fails, R08 switches to the Queue fallback before any sync code is written |
| A Syncer alarm exceeds 10 ms of CPU | R02, R10, R11 | `cpuTime` in `wrangler tail` output for every unit. Smaller GraphQL batches if a unit nears 7 ms |
| The `notifications` scope can mark threads read | R06 | The client has no method parameter. A test fails on any non-GET request or GraphQL mutation |
| Rust panics abort the Wasm instance | Every Worker PR | No `unwrap` or `expect` outside tests. Clippy denies `unwrap_used` in `crates/worker` |
| Wasm bundle grows past the budget | Every PR | `scripts/check-size.mjs` in CI fails above 400 KB gzipped for the Worker or 200 KB for the UI |
| `cursor-team-kit` control skills are not installed | Every live lane | Lanes drive the browser through the Playwright MCP server or the Claude browser pane instead |
| The Playwright MCP server failed to connect in the planning session | Every web lane | Check it at program start. Fall back to the Claude browser pane |
| GitHub notification email is delayed or off for some events | R18, R19 | The 10-minute poll still catches every change. R19 warns after 24 hours of silence |
| The `faint` token (`#5d6069`) has 2.90 to 1 contrast on `bg`, below the 4.5 to 1 that WCAG AA needs for text | R17 | R17 lightens `faint` in `design/tokens.json` and records it in ADR 0006, which supersedes ADR 0003's value. The operator can veto at the review gate |
| `reviewDecision` is empty on repos without required reviews | R03, R10 | Stage comes from the latest review per reviewer. R03 has a test for it |

## Appendix D. Links and reading list

- Read before editing anything. `AGENTS.md`, `docs/prd.md`, and every file in `docs/adr/`.
- Read before UI work. `docs/adr/0003-design.md`, `design/tokens.json`, and `prototypes/direction-ac.html`.
- Read before Worker work. The `workers-rs` README and examples, and `spikes/rust-fit/worker-size/src/lib.rs`.
- Run the pstack `how` skill before R07, R08, and R10, because each changes how data enters Radar.
- Run the pstack `interrogate` skill before handing over R03, R07, and R08, because a wrong rule there corrupts everything after it.
- Keep a decision trail per the pstack `show-me-your-work` skill in `docs/plan/trail.md`, one row per tick.
