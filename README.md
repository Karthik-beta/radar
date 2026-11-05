# Radar

Radar is a private, read-only dashboard for one person's open-source work. It answers one question in under ten seconds: what needs me right now?

I contribute to several open-source projects at once, often through AI agents working in parallel. GitHub shows one repository and one thread at a time, so review requests, failing CI, and stalled pull requests get missed. Radar pulls them onto one screen:

- **Your turn.** Every thread where someone is waiting on me, oldest first.
- **Their turn.** My pull requests and issues waiting on maintainers, flagged after 7 days so I know when a nudge is fair.
- **Pipeline.** Every open pull request by stage, from draft to approved.

Radar never writes to GitHub. Every item links to its page on github.com, and every action happens there.

## Status

Planning is done and the code has not started. The next milestone is M0: a Worker on Cloudflare, GitHub sign-in limited to one user ID, and encrypted token storage. The milestones are listed in the [PRD](docs/prd.md).

## How it works

Radar runs entirely on Cloudflare's free tier, so it costs nothing to run.

- GitHub notification emails arrive through Cloudflare Email Routing. They only trigger a sync, and Radar never trusts their content.
- A single Durable Object, the Syncer, is the only code that calls GitHub. It polls notifications with conditional requests, enriches pull requests with GraphQL, and stays under 10% of GitHub's rate limit.
- The dashboard reads from D1 only, so loading the page never spends GitHub API budget.

The stack is TypeScript, Hono, React with Vite, Drizzle, and D1. The reasons for each choice are in the decision records.

## Documentation

- [Product requirements](docs/prd.md)
- [Architecture decision records](docs/adr/README.md)
- [Design tokens](https://github.com/Karthik-beta/radar/wiki/Design-tokens), on the wiki, generated from [`design/tokens.json`](design/tokens.json)
- [Design prototype](prototypes/direction-ac.html). Open the file in a browser.

## Updating the design tokens

1. Edit [`design/tokens.json`](design/tokens.json).
2. Clone the wiki next to this repository:

   ```bash
   git clone https://github.com/Karthik-beta/radar.wiki.git ../radar.wiki
   ```

3. Regenerate the tokens page:

   ```bash
   node scripts/tokens-to-wiki.mjs ../radar.wiki
   ```

4. Commit and push the wiki checkout.

## License

[MIT](LICENSE)
