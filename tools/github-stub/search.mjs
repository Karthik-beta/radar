// Answers the Search operation from `scenario.search.authored` and `scenario.search.involves`, two lists of
// `issueOrPullRequest` nodes. The query text picks the list (`involves:@me` or authored), the type
// (`is:pr` or `is:issue`) and the `updated:>=DATE` cutoff. A cursor names the offset in the filtered list,
// so a repeated cursor returns the same page.

export function cursorFor(offset) {
  return Buffer.from(`cursor:${offset}`).toString("base64");
}

function offsetOf(after) {
  if (after === null || after === undefined) return 0;
  const match = /^cursor:(\d+)$/.exec(Buffer.from(String(after), "base64").toString());
  return match ? Number(match[1]) : null;
}

export function parseSearch(q) {
  const text = String(q ?? "");
  const since = /updated:>=(\d{4}-\d\d-\d\d)/.exec(text);
  let type = null;
  if (text.includes("is:pr")) type = "PullRequest";
  else if (text.includes("is:issue")) type = "Issue";
  return {
    list: text.includes("involves:@me") ? "involves" : "authored",
    type,
    since: since ? Date.parse(`${since[1]}T00:00:00Z`) : null,
  };
}

function wholeSeconds(value) {
  return JSON.parse(JSON.stringify(value).replace(/(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)\.\d+Z/g, "$1Z"));
}

export function answerSearch(scenario, variables) {
  const vars = variables ?? {};
  const { list, type, since } = parseSearch(vars.q);
  const first = Math.max(Number(vars.first) || 0, 0);
  const offset = offsetOf(vars.after);
  if (offset === null) return { count: 0, body: { data: null, errors: [{ type: "INVALID", message: "bad cursor" }] } };
  const { remaining, reset } = scenario.rate;
  const resetAt = new Date(typeof reset === "number" ? reset * 1000 : Date.parse(reset)).toISOString();
  const all = (scenario.search?.[list] ?? [])
    .filter((node) => type === null || node.__typename === type)
    .filter((node) => since === null || Date.parse(node.updatedAt) >= since);
  const nodes = all.slice(offset, offset + first);
  const end = offset + nodes.length;
  return {
    count: nodes.length,
    body: {
      data: {
        search: {
          nodes: wholeSeconds(nodes),
          pageInfo: { endCursor: nodes.length ? cursorFor(end) : null, hasNextPage: end < all.length },
        },
        rateLimit: wholeSeconds({ cost: 1, remaining, resetAt }),
      },
    },
  };
}
