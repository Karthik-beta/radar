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
      data[alias] = { issueOrPullRequest: wholeSeconds(items[key]) };
    }
  }
  const { remaining, reset } = scenario.rate;
  const resetAt = new Date(typeof reset === "number" ? reset * 1000 : Date.parse(reset)).toISOString();
  data.rateLimit = { cost: 1, remaining, resetAt: wholeSeconds(resetAt) };
  return { count, body: errors.length ? { data, errors } : { data } };
}
