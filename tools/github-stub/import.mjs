// Answers the five history import operations (R27) from `scenario.import`:
//
// - `repos` maps `owner/name` to `{id, stars, language}`, for search nodes and `RepoMeta`.
// - `prs` lists my pull requests, and `generate_prs` adds `count` of them spread from `from` to `to`.
// - `reviewed` lists pull requests by others with my `reviews`, and `generate_reviewed` adds more.
//
// A timeline entry is `{login, type, at, review, repeat}`. `repeat` makes that many entries, one minute apart.
// Each cursor is `base64("<kind>:<offset>")`, so a repeated cursor returns the same page. Search, like
// GitHub, counts every match in `issueCount` but returns at most the first 1,000.

const SEARCH_LIMIT = 1000;
const TIMELINE_PAGE = 20;
const REVIEWED_PAGE = 100;
const REVIEWS_PAGE = 100;
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export function importCursor(kind, offset) {
  return Buffer.from(`${kind}:${offset}`).toString("base64");
}

function offsetOf(kind, after) {
  if (after === null || after === undefined) return 0;
  const match = new RegExp(`^${kind}:(\\d+)$`).exec(Buffer.from(String(after), "base64").toString());
  return match ? Number(match[1]) : null;
}

function iso(ms) {
  return new Date(ms).toISOString().replace(/\.\d+Z$/, "Z");
}

function whole(value) {
  return value ? iso(Date.parse(value)) : null;
}

function spread(from, to, count, i) {
  const start = Date.parse(from);
  const span = Date.parse(to) - start;
  return start + (count > 1 ? Math.floor((span * i) / (count - 1)) : 0);
}

function generatedPrs(spec) {
  return Array.from({ length: spec.count }, (_, i) => {
    const created = spread(spec.from, spec.to, spec.count, i);
    const timeline = spec.responder
      ? [{ login: spec.responder, type: "User", at: iso(created + 2 * HOUR), review: true }]
      : [];
    return {
      id: `${spec.prefix}${i + 1}`,
      repo: spec.repo,
      number: (spec.first_number ?? 1) + i,
      created_at: iso(created),
      merged_at: i % 4 === 3 ? null : iso(created + DAY),
      closed_at: i % 4 === 3 ? null : iso(created + DAY),
      additions: 10 + (i % 90),
      deletions: i % 15,
      changed_files: 1 + (i % 6),
      head_ref: `feature/${i + 1}`,
      changes_requested: i % 3 === 0 ? 1 : 0,
      closing: [],
      timeline,
    };
  });
}

function generatedReviewed(spec) {
  return Array.from({ length: spec.count }, (_, i) => ({
    id: `${spec.prefix}${i + 1}`,
    repo: spec.repo,
    number: (spec.first_number ?? 1) + i,
    author: spec.author,
    reviews: [{ id: `${spec.prefix}review_${i + 1}`, state: "APPROVED", submitted_at: iso(spread(spec.from, spec.to, spec.count, i)) }],
  }));
}

function expandTimeline(entries) {
  return (entries ?? []).flatMap((entry) =>
    Array.from({ length: entry.repeat ?? 1 }, (_, i) => ({
      login: entry.login,
      type: entry.type ?? "User",
      at: iso(Date.parse(entry.at) + i * MINUTE),
      review: Boolean(entry.review),
    })),
  );
}

const MODELS = new WeakMap();

export function importModel(scenario) {
  if (MODELS.has(scenario)) return MODELS.get(scenario);
  const spec = scenario.import ?? {};
  const prs = [...(spec.prs ?? []), ...(spec.generate_prs ?? []).flatMap(generatedPrs)]
    .map((pr) => ({ ...pr, timeline: expandTimeline(pr.timeline) }))
    .sort((a, b) => Date.parse(a.created_at) - Date.parse(b.created_at) || a.id.localeCompare(b.id));
  const reviewed = [...(spec.reviewed ?? []), ...(spec.generate_reviewed ?? []).flatMap(generatedReviewed)];
  const model = {
    login: spec.login ?? scenario.user?.login ?? "karthik",
    repos: spec.repos ?? {},
    prs,
    reviewed,
  };
  MODELS.set(scenario, model);
  return model;
}

// Every pull request and review the stub would import, for lanes that compare counts.
export function importTotals(scenario) {
  const model = importModel(scenario);
  const reviews = model.reviewed.reduce((n, pr) => n + pr.reviews.filter((r) => r.state !== "PENDING").length, 0);
  return { prs: model.prs.length, reviews };
}

function rateLimit(scenario) {
  const { remaining, reset } = scenario.rate;
  return { cost: 1, remaining, resetAt: iso(typeof reset === "number" ? reset * 1000 : Date.parse(reset)) };
}

function badCursor() {
  return { count: 0, body: { data: null, errors: [{ type: "INVALID", message: "bad cursor" }] } };
}

function repoNode(model, name) {
  const repo = model.repos[name] ?? {};
  return {
    id: repo.id ?? `R_${name.replace(/[^A-Za-z0-9]/g, "_")}`,
    nameWithOwner: name,
    stargazerCount: repo.stars ?? 0,
    primaryLanguage: repo.language ? { name: repo.language } : null,
  };
}

function timelinePage(pr, offset) {
  const slice = pr.timeline.slice(offset, offset + TIMELINE_PAGE);
  const end = offset + slice.length;
  return {
    nodes: slice.map((entry) => {
      const node = { createdAt: entry.at, author: { __typename: entry.type, login: entry.login } };
      if (entry.review) node.submittedAt = entry.at;
      return node;
    }),
    pageInfo: {
      endCursor: slice.length ? importCursor("timeline", end) : null,
      hasNextPage: end < pr.timeline.length,
    },
  };
}

export function parseImportSearch(q) {
  const text = String(q ?? "");
  const author = /author:(\S+)/.exec(text)?.[1] ?? null;
  const created = /created:(\d{4}-\d\d-\d\d)\.\.(\d{4}-\d\d-\d\d)/.exec(text);
  return {
    pr: text.includes("is:pr"),
    author,
    from: created ? Date.parse(`${created[1]}T00:00:00Z`) : null,
    to: created ? Date.parse(`${created[2]}T00:00:00Z`) + DAY : null,
  };
}

export function answerImportPrs(scenario, variables) {
  const vars = variables ?? {};
  const offset = offsetOf("search", vars.after);
  if (offset === null) return badCursor();
  const model = importModel(scenario);
  const { pr, author, from, to } = parseImportSearch(vars.q);
  const all = model.prs.filter((p) => {
    const at = Date.parse(p.created_at);
    return pr && author?.toLowerCase() === model.login.toLowerCase() && from !== null && at >= from && at < to;
  });
  const visible = all.slice(0, SEARCH_LIMIT);
  const first = Math.max(Number(vars.first) || 0, 0);
  const slice = visible.slice(offset, offset + first);
  const end = offset + slice.length;
  return {
    count: slice.length,
    body: {
      data: {
        search: {
          issueCount: all.length,
          nodes: slice.map((p) => ({
            id: p.id,
            number: p.number,
            createdAt: whole(p.created_at),
            mergedAt: whole(p.merged_at),
            closedAt: whole(p.closed_at),
            additions: p.additions ?? 0,
            deletions: p.deletions ?? 0,
            changedFiles: p.changed_files ?? 0,
            headRefName: p.head_ref ?? "main",
            repository: repoNode(model, p.repo),
            closingIssuesReferences: {
              nodes: (p.closing ?? []).map((c) => ({ number: c.number, repository: { nameWithOwner: c.repo } })),
            },
            reviews: { totalCount: p.changes_requested ?? 0 },
            timelineItems: timelinePage(p, 0),
          })),
          pageInfo: { endCursor: slice.length ? importCursor("search", end) : null, hasNextPage: end < visible.length },
        },
        rateLimit: rateLimit(scenario),
      },
    },
  };
}

export function answerImportTimeline(scenario, variables) {
  const vars = variables ?? {};
  const offset = offsetOf("timeline", vars.after);
  if (offset === null) return badCursor();
  const pr = importModel(scenario).prs.find((p) => p.id === vars.id);
  if (!pr) return { count: 0, body: { data: { node: null, rateLimit: rateLimit(scenario) } } };
  const page = timelinePage(pr, offset);
  return {
    count: page.nodes.length,
    body: {
      data: {
        node: { createdAt: whole(pr.created_at), repository: { nameWithOwner: pr.repo }, timelineItems: page },
        rateLimit: rateLimit(scenario),
      },
    },
  };
}

function latestReview(pr) {
  return Math.max(...pr.reviews.map((r) => Date.parse(r.submitted_at)));
}

export function answerImportReviewedPrs(scenario, variables) {
  const vars = variables ?? {};
  const offset = offsetOf("reviewed", vars.after);
  if (offset === null) return badCursor();
  const from = Date.parse(vars.from);
  const to = Date.parse(vars.to);
  const all = importModel(scenario).reviewed.filter((pr) => {
    const at = latestReview(pr);
    return at >= from && at <= to;
  });
  const slice = all.slice(offset, offset + REVIEWED_PAGE);
  const end = offset + slice.length;
  return {
    count: slice.length,
    body: {
      data: {
        viewer: {
          contributionsCollection: {
            pullRequestReviewContributions: {
              nodes: slice.map((pr) => ({ pullRequest: { id: pr.id } })),
              pageInfo: { endCursor: slice.length ? importCursor("reviewed", end) : null, hasNextPage: end < all.length },
            },
          },
        },
        rateLimit: rateLimit(scenario),
      },
    },
  };
}

export function answerImportReviews(scenario, variables) {
  const vars = variables ?? {};
  const offset = offsetOf("reviews", vars.after);
  if (offset === null) return badCursor();
  const model = importModel(scenario);
  const mine = String(vars.login ?? "").toLowerCase() === model.login.toLowerCase();
  let count = 0;
  const nodes = (vars.ids ?? []).map((id) => {
    const pr = model.reviewed.find((p) => p.id === id);
    if (!pr) return null;
    const reviews = mine ? pr.reviews : [];
    const slice = reviews.slice(offset, offset + REVIEWS_PAGE);
    const end = offset + slice.length;
    count += slice.length;
    return {
      id: pr.id,
      number: pr.number,
      repository: { nameWithOwner: pr.repo },
      author: pr.author ? { __typename: "User", login: pr.author } : null,
      reviews: {
        nodes: slice.map((r) => ({
          id: r.id,
          state: r.state,
          createdAt: whole(r.created_at ?? r.submitted_at),
          submittedAt: r.state === "PENDING" ? null : whole(r.submitted_at),
        })),
        pageInfo: { endCursor: slice.length ? importCursor("reviews", end) : null, hasNextPage: end < reviews.length },
      },
    };
  });
  return { count, body: { data: { nodes, rateLimit: rateLimit(scenario) } } };
}

export function answerRepoMeta(scenario, variables) {
  const model = importModel(scenario);
  const byId = new Map(Object.keys(model.repos).map((name) => [repoNode(model, name).id, name]));
  const nodes = (variables?.ids ?? []).map((id) => (byId.has(id) ? repoNode(model, byId.get(id)) : null));
  return { count: nodes.filter(Boolean).length, body: { data: { nodes, rateLimit: rateLimit(scenario) } } };
}

export const IMPORT_ANSWERS = {
  ImportPrs: answerImportPrs,
  ImportTimeline: answerImportTimeline,
  ImportReviewedPrs: answerImportReviewedPrs,
  ImportReviews: answerImportReviews,
  RepoMeta: answerRepoMeta,
};
