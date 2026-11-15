// Answers the `Enrich` operation from `scenario.items`, which maps "owner/repo#number" to the
// `issueOrPullRequest` node. A key with no entry answers a null alias, and a key set to null answers
// an alias whose item is null, the two shapes GitHub uses for a missing repository and a missing number.

// GitHub sends whole seconds, and Radar parses only that form.
function wholeSeconds(value) {
  if (typeof value === "string") return value.replace(/^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)\.\d+Z$/, "$1Z");
  if (Array.isArray(value)) return value.map(wholeSeconds);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, wholeSeconds(v)]));
  }
  return value;
}

// The R26 fact fields. A scenario that leaves them out gets a small pull request with no closing issues, and
// `reviews.totalCount` counts its CHANGES_REQUESTED reviews in the timeline and latest reviews.
export function withFactFields(node) {
  if (!node || node.__typename !== "PullRequest") return node;
  const reviews = [...(node.timelineItems?.nodes ?? []), ...(node.latestReviews?.nodes ?? [])].filter(
    (n) => n && n.state === "CHANGES_REQUESTED",
  );
  const changesRequested = new Set(reviews.map((n) => n.id)).size;
  const filled = {
    additions: 10,
    deletions: 2,
    changedFiles: 1,
    closingIssuesReferences: { nodes: [] },
    reviews: { totalCount: changesRequested },
    ...node,
  };
  if (node.timelineItems) {
    filled.timelineItems = { ...node.timelineItems, nodes: node.timelineItems.nodes.map(withSubmittedAt) };
  }
  if (node.latestReviews) {
    filled.latestReviews = { ...node.latestReviews, nodes: node.latestReviews.nodes.map(withSubmittedAt) };
  }
  return filled;
}

// A review the scenario leaves without `submittedAt` was submitted when it was created.
function withSubmittedAt(review) {
  if (!review || !("state" in review) || "submittedAt" in review) return review;
  return { ...review, submittedAt: review.at };
}

function aliasCount(variables) {
  let count = 0;
  while (`o${count}` in variables) count += 1;
  return count;
}

export function answerEnrich(scenario, variables) {
  const vars = variables ?? {};
  const items = scenario.items ?? {};
  const data = {};
  const errors = [];
  const count = aliasCount(vars);
  for (let i = 0; i < count; i += 1) {
    const key = `${vars[`o${i}`]}/${vars[`n${i}`]}#${vars[`i${i}`]}`;
    const alias = `a${i}`;
    if (!Object.hasOwn(items, key)) {
      data[alias] = null;
      errors.push({ type: "NOT_FOUND", path: [alias], message: `Could not resolve to a Repository for ${key}.` });
    } else if (items[key] === null) {
      data[alias] = { issueOrPullRequest: null };
      errors.push({
        type: "NOT_FOUND",
        path: [alias, "issueOrPullRequest"],
        message: `Could not resolve to an issue or pull request for ${key}.`,
      });
    } else {
      data[alias] = { issueOrPullRequest: wholeSeconds(withFactFields(items[key])) };
    }
  }
  const { remaining, reset } = scenario.rate;
  const resetAt = new Date(typeof reset === "number" ? reset * 1000 : Date.parse(reset)).toISOString();
  data.rateLimit = { cost: 1, remaining, resetAt: wholeSeconds(resetAt) };
  return { count, body: errors.length ? { data, errors } : { data } };
}
