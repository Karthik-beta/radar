// Answers the ContributionHistory operation from `scenario.history.pull_requests`, a list of
// `{occurred_at, merged_at, repo}`. The window is `variables.from` to `variables.to`, and a cursor is
// `base64("history:<offset>")` into the filtered list, so a repeated cursor returns the same page.

const PAGE = 100;

export function historyCursor(offset) {
  return Buffer.from(`history:${offset}`).toString("base64");
}

function offsetOf(after) {
  if (after === null || after === undefined) return 0;
  const match = /^history:(\d+)$/.exec(Buffer.from(String(after), "base64").toString());
  return match ? Number(match[1]) : null;
}

export function answerHistory(scenario, variables) {
  const vars = variables ?? {};
  const offset = offsetOf(vars.after);
  if (offset === null) return { count: 0, body: { data: null, errors: [{ type: "INVALID", message: "bad cursor" }] } };
  const from = Date.parse(vars.from);
  const to = Date.parse(vars.to);
  const { remaining, reset } = scenario.rate;
  const resetAt = new Date(typeof reset === "number" ? reset * 1000 : Date.parse(reset)).toISOString();
  const all = (scenario.history?.pull_requests ?? []).filter((pr) => {
    const at = Date.parse(pr.occurred_at);
    return at >= from && at < to;
  });
  const slice = all.slice(offset, offset + PAGE);
  const end = offset + slice.length;
  const whole = (value) => (value ? new Date(Date.parse(value)).toISOString().replace(/\.\d+Z$/, "Z") : null);
  return {
    count: slice.length,
    body: {
      data: {
        viewer: {
          contributionsCollection: {
            pullRequestContributions: {
              nodes: slice.map((pr) => ({
                occurredAt: whole(pr.occurred_at),
                pullRequest: { mergedAt: whole(pr.merged_at), repository: { nameWithOwner: pr.repo } },
              })),
              pageInfo: { endCursor: slice.length ? historyCursor(end) : null, hasNextPage: end < all.length },
            },
          },
        },
        rateLimit: { cost: 1, remaining, resetAt },
      },
    },
  };
}
