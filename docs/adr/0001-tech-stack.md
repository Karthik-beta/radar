# 0001. Tech stack

Status: accepted, 2026-11-05.

Radar must cost ₹0, run only on Cloudflare's free tier, and never write to GitHub. Those three constraints pick most of the stack. This record lists what we chose and the reasons that would have to change for a choice to be revisited.

## Choices

| Layer | Choice | Rejected |
| --- | --- | --- |
| Platform | Cloudflare Workers free plan: Workers, Email Routing, Cron Triggers, Queues or Durable Objects, D1 | Any second platform |
| Language | TypeScript, strict | |
| Frontend | React + Vite single-page app, built with `@cloudflare/vite-plugin` and served as Workers static assets | Next.js on Vercel, server-rendered Hono JSX |
| API | Hono, with its typed `hc` client shared by the frontend | |
| Database access | Drizzle ORM. `drizzle-kit` writes SQL into the folder that `wrangler d1 migrations` applies | Raw SQL, Kysely |
| Sync state | Strongly consistent storage (D1 or a Durable Object). Not KV | Workers KV |
| GitHub client | Our own `fetch` wrapper with no `method` parameter, typed GraphQL through gql.tada, responses parsed with valibot | Octokit |
| Auth | Hand-written OAuth code flow with Web Crypto, Hono signed cookies | Better Auth, Auth.js |
| Styling | Tailwind CSS v4. The component library is chosen in the design phase | |
| Tooling | pnpm, Biome, Vitest with `@cloudflare/vitest-pool-workers`, Wrangler | ESLint with Prettier |

## Reasons

**Next.js on Vercel.** D1 is reachable only from a Worker, so every Vercel page load would cross two platforms. That works against the 1.5 s first-load target. It also doubles the deploys and secret stores.

**React over server-rendered HTML.** Server rendering would load fastest. The kanban, filters, snooze, and trend charts all add client interactivity, and React is what a reader of a public portfolio repo expects.

**No KV for sync state.** The free plan allows 1,000 KV writes a day and one write a second per key. The polling rules write sync state after almost every GitHub response, which comes close to that cap. KV is also eventually consistent, so a coalescing check can read stale state and let duplicate syncs through.

**No Octokit.** It ships every write endpoint, so the read-only rule would depend on a test alone. A wrapper whose type has no `method` parameter cannot express a write. The wrapper also rejects any GraphQL document that contains a mutation.

**No auth library.** There is one user with one allowlisted GitHub ID. The flow is about 60 lines.

## Risks to check early

- The Workers free plan caps CPU time at 10 ms per invocation, for HTTP requests, Cron Triggers, queue consumers, and Durable Object alarms alike. Waiting on the network does not count, but parsing large GraphQL responses does. M1 measures CPU time for each sync step before more work builds on it.
- Queue operations count three times per message (write, read, delete). Free-plan messages are kept for 24 hours, so the dead-letter queue has to surface within a day.
