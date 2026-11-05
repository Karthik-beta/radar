# Agent guide

Radar is a single-user, read-only dashboard for one person's open-source work, running on Cloudflare's free tier. Read [README.md](README.md) for the overview and [docs/prd.md](docs/prd.md) for the requirements.

## Read before you change anything

- [docs/adr/](docs/adr/README.md) holds every accepted decision. When an ADR disagrees with the PRD, the ADR wins. Each ADR lists the PRD sections it replaces.
- [design/tokens.json](design/tokens.json) holds every colour, size, spacing, radius, and motion value. UI code uses these tokens and never hard-codes a value.

## Rules that must not break

- **Radar never writes to GitHub.** The GitHub client sends only GET requests and GraphQL queries, never mutations. Do not add Octokit or any client that can write. The rule is enforced in code and covered by a test.
- **Radar costs nothing to run.** Use only Cloudflare free-tier products. Do not use Workers KV for state that changes on every sync, because the free plan allows 1,000 writes a day. Each Worker invocation gets 10 ms of CPU, so each Syncer alarm runs one unit of GitHub work.
- **Only the Syncer Durable Object calls GitHub.** Page loads read from D1 only.
- **No secrets in the repository.** The repository is public. Secrets live in Wrangler secrets, and local values go in `.dev.vars`, which git ignores.
- **Store metadata, not content.** Keep titles, states, timestamps, and logins. Do not store comment bodies.

## Code

- TypeScript in strict mode.
- `src/domain/` holds the pure rules (whose turn it is, pipeline stage). It imports nothing from Cloudflare, Hono, or React.
- Validate external data once, at the boundary where it enters (GitHub responses, email headers, request bodies). Trust the types inside.
- Write a comment only to explain a reason the code cannot show.

The project scaffold arrives in milestone M0. Until then there are no build, test, or lint commands.

## Decisions

A new decision gets a new ADR. Copy [docs/adr/template.md](docs/adr/template.md) and follow the steps in [docs/adr/README.md](docs/adr/README.md). Do not rewrite an accepted ADR. Supersede it.

## Design tokens

To change a token, edit `design/tokens.json`, then regenerate the wiki page with `node scripts/tokens-to-wiki.mjs <wiki-checkout>` and push the wiki. The README has the full steps.

## Writing

Docs, ADRs, and commit messages use short, plain sentences in the present tense. Name the real file, flag, or command, not a description of it.
