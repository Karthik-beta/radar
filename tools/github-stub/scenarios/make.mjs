// Writes a scenario file for the enrich lanes to stdout, with times relative to now.
//   node tools/github-stub/scenarios/make.mjs <kind>          the scenario JSON
//   node tools/github-stub/scenarios/make.mjs <kind> --sql    SQL that seeds notification_threads
//   node tools/github-stub/scenarios/make.mjs <kind> --ids    the thread ids, comma separated
// Until R09 polls, the SQL stands in for the poll and the ids name the Enrich unit.
import { pathToFileURL } from "node:url";

import { cursorFor } from "../search.mjs";
import { resolveTimes } from "../server.mjs";

const ME = "karthik";
const REPO = "octo/widgets";

const actor = (login, type = "User") => ({ login, __typename: type });

function context(repo, number, kind) {
  return { repo, number, path: kind === "pr" ? "pull" : "issues" };
}

const comment = (c, id, at, who, type) => ({
  __typename: "IssueComment",
  id,
  at,
  url: `https://github.com/${c.repo}/${c.path}/${c.number}#issuecomment-${id}`,
  actor: actor(who, type),
});

const review = (c, id, at, who, state) => ({
  __typename: "PullRequestReview",
  id,
  at,
  url: `https://github.com/${c.repo}/pull/${c.number}#pullrequestreview-${id}`,
  state,
  actor: actor(who),
});

const requested = (id, at, who, reviewer) => ({
  __typename: "ReviewRequestedEvent",
  id,
  at,
  actor: actor(who),
  requestedReviewer: { __typename: "User", login: reviewer },
});

const commit = (c, id, at, who, oid) => ({
  __typename: "PullRequestCommit",
  id,
  url: `https://github.com/${c.repo}/pull/${c.number}/commits/${oid}`,
  commit: { oid, at, author: { user: { login: who } } },
});

function prNode(c, o) {
  const pending = o.ci ? [{ commit: { oid: o.ci.oid, statusCheckRollup: { state: o.ci.state } } }] : [];
  return {
    __typename: "PullRequest",
    id: `PR_stub_${c.number}`,
    number: c.number,
    title: o.title ?? `Pull request ${c.number}`,
    url: `https://github.com/${c.repo}/pull/${c.number}`,
    prState: o.state ?? "OPEN",
    isDraft: o.draft ?? false,
    createdAt: o.created ?? "@-3d",
    updatedAt: o.updated ?? "@-10m",
    closedAt: null,
    mergedAt: null,
    headRefName: o.head ?? `feature/${c.number}`,
    reviewDecision: o.decision ?? null,
    author: "author" in o ? o.author : actor(o.by ?? ME),
    assignees: { nodes: (o.assignees ?? []).map((login) => ({ login })) },
    reviewRequests: {
      nodes: (o.requests ?? []).map((login) => ({ requestedReviewer: { __typename: "User", login } })),
    },
    latestReviews: { nodes: o.latest ?? [] },
    commits: { nodes: pending },
    timelineItems: { nodes: o.timeline ?? [] },
    ...o.extra,
  };
}

function issueNode(c, o) {
  return {
    __typename: "Issue",
    id: `I_stub_${c.number}`,
    number: c.number,
    title: o.title ?? `Issue ${c.number}`,
    url: `https://github.com/${c.repo}/issues/${c.number}`,
    issueState: o.state ?? "OPEN",
    createdAt: o.created ?? "@-3d",
    updatedAt: o.updated ?? "@-10m",
    closedAt: null,
    author: "author" in o ? o.author : actor(o.by ?? ME),
    assignees: { nodes: (o.assignees ?? []).map((login) => ({ login })) },
    timelineItems: { nodes: o.timeline ?? [] },
    ...o.extra,
  };
}

// A spec is one notification thread and the item behind it. `expect` is what Radar should store.
// `gone` answers an alias whose item is null. `missing` leaves the key out, so the alias itself is null.
function spec(number, kind, reason, build, expect, opts = {}) {
  const { gone = false, missing = false, unread = true, mention = false, repo = REPO } = opts;
  const c = context(repo, number, kind);
  const o = build(c);
  const node = gone || missing ? null : kind === "pr" ? prNode(c, o) : issueNode(c, o);
  return {
    c, kind, reason, node, expect, unread, mention, missing,
    gone: gone || missing,
    title: o.title ?? `Item ${number}`,
    updated: o.updated ?? "@-10m",
  };
}

function eventCount(s) {
  if (s.gone) return 0;
  const ci = s.node.commits?.nodes?.[0]?.commit?.statusCheckRollup?.state;
  return 1 + s.node.timelineItems.nodes.length + (s.mention ? 1 : 0) + (ci === "SUCCESS" || ci === "FAILURE" ? 1 : 0);
}

const ok = { oid: "c0ffee1", state: "SUCCESS" };

function basic() {
  return [
    spec(12, "pr", "review_requested", (c) => ({
      by: "alice", requests: [ME], decision: "REVIEW_REQUIRED", ci: ok,
      timeline: [requested("RRE_12", "@-3h", "alice", ME)], updated: "@-3h",
    }), { turn: "me", reason: "review_requested", stage: "awaiting_review" }),
    spec(13, "pr", "author", (c) => ({
      decision: "CHANGES_REQUESTED", ci: ok, updated: "@-2h",
      latest: [{ id: "PRR_13", state: "CHANGES_REQUESTED", at: "@-2h", url: "https://github.com/octo/widgets/pull/13#pullrequestreview-13", author: actor("bob") }],
      timeline: [review(c, "PRR_13", "@-2h", "bob", "CHANGES_REQUESTED")],
    }), { turn: "me", reason: "changes_requested", stage: "changes_requested" }),
    spec(14, "pr", "ci_activity", (c) => ({
      decision: "REVIEW_REQUIRED", ci: { oid: "badc0de", state: "FAILURE" }, updated: "@-1h",
      timeline: [commit(c, "PRC_14", "@-1d", ME, "badc0de")],
    }), { turn: "me", reason: "ci_failed", stage: "ci_failing" }),
    spec(15, "pr", "comment", (c) => ({
      decision: "REVIEW_REQUIRED", ci: ok, updated: "@-1h",
      timeline: [comment(c, "IC_15_1", "@-5h", "bob"), comment(c, "IC_15_2", "@-1h", ME)],
    }), { turn: "them", reason: null, stage: "awaiting_review" }),
    spec(9, "issue", "comment", (c) => ({
      updated: "@-20m",
      timeline: [comment(c, "IC_9_1", "@-6h", ME), comment(c, "IC_9_2", "@-20m", "maint")],
    }), { turn: "me", reason: "replied", stage: null }),
    spec(16, "pr", "author", () => ({ draft: true, updated: "@-4h", timeline: [] }), { turn: "them", reason: null, stage: "draft" }),
  ];
}

function bot() {
  return [
    spec(20, "pr", "comment", (c) => ({
      decision: "REVIEW_REQUIRED", ci: ok, updated: "@-1h",
      timeline: [comment(c, "IC_20_1", "@-3h", ME), comment(c, "IC_20_2", "@-1h", "dependabot[bot]", "Bot")],
    }), { turn: "them", reason: null, stage: "awaiting_review", since: "@-3h" }),
  ];
}

function read() {
  return [
    spec(30, "pr", "review_requested", (c) => ({
      by: "alice", requests: [ME], decision: "REVIEW_REQUIRED", ci: ok, updated: "@-30m",
      timeline: [requested("RRE_30", "@-30m", "alice", ME)],
    }), { turn: "me", reason: "review_requested", stage: "awaiting_review" }, { unread: false }),
  ];
}

function many(count, first, mark = {}) {
  return Array.from({ length: count }, (_, i) => {
    const n = first + i;
    return spec(n, "pr", "comment", (c) => ({
      decision: "REVIEW_REQUIRED", ci: ok, updated: `@-${i + 1}m`,
      timeline: [comment(c, `IC_${n}_1`, `@-${i + 1}m`, "bob")],
      extra: mark.extra,
    }), { turn: "them", reason: null, stage: "awaiting_review" });
  });
}

function gone() {
  const specs = many(20, 200);
  specs[7] = spec(207, "pr", "comment", () => ({}), { turn: null, reason: null, stage: null }, { gone: true });
  return specs;
}

function bodyMarker() {
  return many(3, 300, { extra: { body: "BODYMARK-7f3a", bodyText: "BODYMARK-7f3a" } }).map((s) => {
    s.node.timelineItems.nodes.forEach((n) => Object.assign(n, { body: "BODYMARK-7f3a", bodyText: "BODYMARK-7f3a" }));
    return s;
  });
}

function ciStep() {
  const specs = basic();
  specs[3] = spec(15, "pr", "ci_activity", (c) => ({
    decision: "REVIEW_REQUIRED", ci: { oid: "d00d001", state: "FAILURE" }, updated: "@-1m",
    timeline: [comment(c, "IC_15_1", "@-5h", "bob"), comment(c, "IC_15_2", "@-1h", ME)],
  }), { turn: "me", reason: "ci_failed", stage: "ci_failing" });
  return specs;
}

function edge() {
  return [
    spec(41, "pr", "comment", (c) => ({
      author: null, decision: "REVIEW_REQUIRED", ci: ok, updated: "@-1h",
      timeline: [comment(c, "IC_41_1", "@-3h", "alice"), comment(c, "IC_41_2", "@-1h", "dependabot[bot]", "Bot")],
    }), { turn: "them", reason: null, stage: "awaiting_review" }),
    spec(42, "pr", "comment", () => ({}), { turn: null, reason: null, stage: null }, { missing: true }),
    spec(43, "issue", "assign", () => ({ by: "alice", assignees: [ME], updated: "@-2h", timeline: [] }), {
      turn: "me", reason: "assigned", stage: null,
    }),
  ];
}

function asked(number, hoursAgo, by, opts = {}) {
  const at = `@-${hoursAgo}h`;
  return spec(number, "pr", "review_requested", () => ({
    by, requests: [ME], decision: "REVIEW_REQUIRED", ci: ok, updated: at,
    timeline: [requested(`RRE_${number}`, at, by, ME)],
    ...opts,
  }), { turn: "me", reason: "review_requested", stage: "awaiting_review" });
}

function yourTurnEdge() {
  return [
    asked(51, 23, "alice"),
    asked(52, 25, "bob"),
    asked(53, 71, "carol", { head: "agent/fix-lexer", title: "Fix the lexer" }),
    asked(54, 73, "dave"),
    spec(55, "pr", "ci_activity", (c) => ({
      decision: "REVIEW_REQUIRED", ci: { oid: "badc0de", state: "FAILURE" }, updated: "@-2h",
      timeline: [commit(c, "PRC_55", "@-2h", ME, "badc0de")],
    }), { turn: "me", reason: "ci_failed", stage: "ci_failing" }),
  ];
}

function yourTurnHostile() {
  const img = asked(61, 5, "alice", { title: "<img src=x onerror=alert(1)>" });
  const script = asked(62, 4, "bob", { title: "<script>window.__pwned = 1</script>" });
  const link = asked(63, 3, "carol", { title: "A javascript: link" });
  link.node.url = "javascript:alert(1)";
  return [img, script, link];
}

function theirsOnly() {
  return Array.from({ length: 7 }, (_, i) => {
    const n = 600 + i;
    return spec(n, "pr", "comment", (c) => ({
      decision: "REVIEW_REQUIRED", ci: ok, updated: `@-${i + 1}m`,
      timeline: [comment(c, `IC_${n}_1`, "@-5h", "bob"), comment(c, `IC_${n}_2`, `@-${i + 1}m`, ME)],
    }), { turn: "them", reason: null, stage: "awaiting_review" });
  });
}

// My own PR where my comment is the last word, `days` days ago, so it waits on others.
function waitingOn(number, days, opts = {}) {
  const at = `@-${days}d`;
  return spec(number, "pr", "comment", (c) => ({
    decision: "REVIEW_REQUIRED", ci: ok, updated: at,
    timeline: [comment(c, `IC_${number}_1`, at, ME)],
    ...opts,
  }), { turn: "them", reason: null, stage: "awaiting_review" });
}

function theirTurnMixed() {
  const issue = spec(77, "issue", "comment", (c) => ({
    title: "Issue I opened and commented on", updated: "@-3d",
    timeline: [comment(c, "IC_77_1", "@-3d", ME)],
  }), { turn: "them", reason: null, stage: null });
  return [
    waitingOn(71, 6, { title: "Waiting six days" }),
    waitingOn(72, 8, { title: "Waiting eight days", head: "agent/fix-parser" }),
    waitingOn(73, 13, { title: "Waiting thirteen days" }),
    waitingOn(74, 15, { title: "Waiting fifteen days" }),
    waitingOn(75, 2, { title: "Two reviewers asked", requests: ["alice", "bob"] }),
    waitingOn(76, 1, { title: "No reviewer asked" }),
    issue,
    asked(78, 5, "erin"),
  ];
}

function theirTurnHostile() {
  return [
    waitingOn(91, 9, { title: "<script>alert(1)</script>" }),
    asked(92, 4, "frank"),
  ];
}

function theirTurnWaiting(count, first) {
  return Array.from({ length: count }, (_, i) => waitingOn(first + i, 1 + (i % 30)));
}


// R20 activity feed. Each pull request here has no CI node unless it says so, so `eventCount` stays easy to read.
const feedExpect = { turn: "them", reason: null, stage: "awaiting_review" };

function feedPr(number, repo, timeline, o = {}) {
  return spec(number, "pr", "comment", (c) => ({
    decision: "REVIEW_REQUIRED", updated: o.updated ?? "@-1h", created: o.created ?? "@-6h",
    timeline: timeline(c), ...o.extra,
  }), feedExpect, { repo });
}

// Twelve events across three repos: a bot comment, a failing check, and an issue.
function feedBasic() {
  return [
    feedPr(101, "octo/widgets", (c) => [comment(c, "IC_101_1", "@-5h", "bob"), comment(c, "IC_101_2", "@-4h", ME)]),
    feedPr(102, "octo/gadgets", (c) => [
      comment(c, "IC_102_1", "@-3h", "alice"),
      comment(c, "IC_102_2", "@-2h", "dependabot[bot]", "Bot"),
    ]),
    spec(103, "pr", "ci_activity", (c) => ({
      decision: "REVIEW_REQUIRED", ci: { oid: "badc0de", state: "FAILURE" }, updated: "@-30m", created: "@-6h",
      timeline: [comment(c, "IC_103_1", "@-90m", "bob")],
    }), { turn: "me", reason: "ci_failed", stage: "ci_failing" }, { repo: "octo/tools" }),
    spec(104, "issue", "comment", (c) => ({
      updated: "@-20m", created: "@-6h",
      timeline: [comment(c, "IC_104_1", "@-1h", "maint"), comment(c, "IC_104_2", "@-20m", ME)],
    }), { turn: "them", reason: null, stage: null }, { repo: "octo/widgets" }),
  ];
}

// Three pull requests whose events are 200, 190, and 170 days old. The nightly rollup folds the first two.
function feedOld() {
  return [200, 190, 170].map((days, i) => {
    const at = `@-${days}d`;
    return feedPr(110 + i, "octo/widgets", (c) => [comment(c, `IC_${110 + i}_1`, at, "bob")], {
      created: at, updated: at,
    });
  });
}

// R21 repo cards: 60 of my pull requests across 4 repos. `replies` are the days a maintainer took to answer, and
// null leaves the pull request unanswered. An open one ends with my own comment, so it sits on Their turn.
const repoPlan = [
  { repo: "octo/widgets", first: 200, states: ["MERGED", "MERGED", "MERGED", "CLOSED", "OPEN", "OPEN"], replies: [1, 2, 4, 9, null, null] },
  { repo: "octo/gadgets", first: 220, states: [...Array(18).fill("MERGED"), "CLOSED", "OPEN"], replies: Array(20).fill(0.5) },
  { repo: "octo/tools", first: 250, states: Array(18).fill("OPEN"), replies: Array(18).fill(null) },
  { repo: "octo/docs", first: 280, states: [...Array(8).fill("MERGED"), ...Array(8).fill("CLOSED")], replies: Array(16).fill(1) },
];

function repoCards() {
  return repoPlan.flatMap(({ repo, first, states, replies }) =>
    states.map((state, i) => {
      const number = first + i;
      const age = 40 + i;
      const reply = replies[i];
      const last = `@-${Math.max(1, 30 - i)}d`;
      return spec(number, "pr", "author", (c) => ({
        decision: "REVIEW_REQUIRED", ci: ok,
        created: `@-${age}d`, updated: last,
        timeline: [
          ...(reply === null ? [] : [comment(c, `IC_${number}_1`, `@-${(age - reply) * 24}h`, "bob")]),
          ...(state === "OPEN" ? [comment(c, `IC_${number}_2`, last, ME)] : []),
        ],
        state,
        extra: state === "OPEN" ? undefined : { closedAt: last, mergedAt: state === "MERGED" ? last : null },
      }), { turn: "them", reason: null, stage: null }, { repo });
    }),
  );
}

// 100 pull requests of 20 events each: the opening and 19 comments.
export function feedMany(count = 100, perPr = 20) {
  const repos = ["octo/widgets", "octo/gadgets", "octo/tools", "octo/docs", "octo/site"];
  return Array.from({ length: count }, (_, i) => {
    const n = 1000 + i;
    return feedPr(
      n,
      repos[i % repos.length],
      (c) => Array.from({ length: perPr - 1 }, (_, j) => comment(c, `IC_${n}_${j}`, `@-${i * 20 + j + 1}m`, j % 2 ? "bob" : ME)),
      { created: `@-${i * 20 + perPr}m`, updated: `@-${i * 20 + 1}m` },
    );
  });
}


const kinds = {
  "your-turn-edge": { specs: yourTurnEdge },
  "your-turn-hostile": { specs: yourTurnHostile },
  "your-turn-theirs-only": { specs: theirsOnly },
  "enrich-20": { specs: () => [...basic(), ...many(14, 1000)] },
  "enrich-edge": { specs: edge },
  "enrich-basic": { specs: basic },
  "enrich-bot": { specs: bot },
  "enrich-read": { specs: read },
  "enrich-45": { specs: () => many(45, 100) },
  "enrich-gone": { specs: gone },
  "enrich-limited": {
    specs: () => many(3, 400),
    faults: [
      {
        match: "POST /graphql",
        status: 200,
        times: 1,
        rate: { remaining: 0, reset: "@+90s" },
        body: { errors: [{ type: "RATE_LIMITED", message: "API rate limit exceeded for user ID 1001." }] },
      },
    ],
  },
  "enrich-502": {
    specs: () => many(3, 500),
    faults: [{ match: "POST /graphql", status: 502, times: 3 }],
  },
  "enrich-body-marker": { specs: bodyMarker },
  "enrich-ci-step": { specs: ciStep },
  "feed-basic": { specs: feedBasic },
  "feed-old": { specs: feedOld },
  "feed-many": { specs: () => feedMany() },
  "pipeline-all-stages": { specs: pipelineAllStages },
  "pipeline-two-stages": { specs: pipelineTwoStages },
  "pipeline-stage-move": { specs: () => pipelineAllStages({ moveToFailing: true }) },
  "pipeline-recent": { specs: pipelineRecent },
  "pipeline-hostile": { specs: pipelineHostile },
  // R16 freshness lanes. `scenario` merges into the file, so a kind can set scopes, rate, and faults.
  "fresh-ok": { specs: basic },
  "fresh-budget": { specs: basic, scenario: { rate: { limit: 5000, remaining: 4750, reset: "@+40m" } } },
  "scope-fixed-step": { specs: basic, scenario: { scopes: ["notifications", "read:user"] } },
  "missing-scope": { specs: basic, scenario: { scopes: ["read:user"] } },
  "no-scope-header": { specs: basic, scenario: { scopes: null } },
  "dead-work": {
    specs: basic,
    faults: [{ match: "POST /graphql", status: 502 }],
  },
  "their-turn-mixed": { specs: theirTurnMixed },
  "their-turn-none": { specs: () => [asked(81, 5, "alice"), asked(82, 6, "bob"), asked(83, 7, "carol"), asked(84, 8, "dave")] },
  "their-turn-hostile": { specs: theirTurnHostile },
  "their-turn-large": { specs: () => theirTurnWaiting(200, 700) },
  "repo-cards": { specs: repoCards },
};

// My own open pull requests, one per card. `created` sets the days in stage on first sync.
function mine(number, stage, o = {}) {
  const at = o.created ?? "@-10m";
  const changes = stage === "changes_requested";
  return spec(number, "pr", "author", (c) => ({
    created: at,
    updated: at,
    draft: stage === "draft",
    decision: stage === "approved" ? "APPROVED" : changes ? "CHANGES_REQUESTED" : "REVIEW_REQUIRED",
    ci:
      stage === "draft"
        ? undefined
        : stage === "ci_running"
          ? { oid: "a11ce01", state: "PENDING" }
          : stage === "ci_failing"
            ? { oid: "badc0de", state: "FAILURE" }
            : ok,
    timeline: changes ? [review(c, `PRR_${number}`, at, "bob", "CHANGES_REQUESTED")] : [],
    latest: changes
      ? [{ id: `PRR_${number}`, state: "CHANGES_REQUESTED", at, url: `https://github.com/${c.repo}/pull/${number}#pullrequestreview-${number}`, author: actor("bob") }]
      : [],
    title: o.title,
    head: o.head,
    state: o.state,
    extra: o.extra,
  }), { stage });
}

function pipelineAllStages({ moveToFailing = false } = {}) {
  return [
    mine(60, "draft", { created: "@-8d", title: "Draft for eight days" }),
    mine(61, moveToFailing ? "ci_failing" : "ci_running", { created: "@-10m", title: "CI running today" }),
    mine(62, "ci_failing", { created: "@-25h", title: "CI failing for a day" }),
    mine(63, "awaiting_review", { created: "@-6d", title: "Awaiting review six days" }),
    mine(64, "awaiting_review", { created: "@-8d", title: "Awaiting review eight days" }),
    mine(65, "awaiting_review", { created: "@-15d", title: "Awaiting review fifteen days" }),
    mine(66, "changes_requested", { created: "@-73h", title: "Changes requested three days" }),
    mine(67, "approved", { created: "@-74h", title: "Approved three days", head: "agent/fix-flake" }),
  ];
}

function pipelineTwoStages() {
  return [
    mine(70, "draft", { created: "@-1h", title: "Only draft" }),
    mine(71, "approved", { created: "@-2d", title: "Only approved" }),
  ];
}

function pipelineRecent() {
  const ended = (n, state, days, title) =>
    mine(n, "approved", {
      created: `@-${days + 2}d`,
      title,
      state,
      extra: { closedAt: `@-${days}d`, mergedAt: state === "MERGED" ? `@-${days}d` : null },
    });
  return [
    mine(80, "awaiting_review", { created: "@-1d", title: "Still open" }),
    ended(81, "MERGED", 13, "Merged thirteen days ago"),
    ended(82, "CLOSED", 3, "Closed three days ago"),
    ended(83, "MERGED", 15, "Merged fifteen days ago"),
  ];
}

function pipelineHostile() {
  return [
    mine(90, "awaiting_review", { created: "@-2d", title: "L".repeat(300) }),
    mine(91, "awaiting_review", { created: "@-2d", title: "<img src=x onerror=alert(1)>" }),
    mine(92, "awaiting_review", { created: "@-2d", title: "Bad link", extra: { url: "javascript:alert(1)" } }),
  ];
}

// A search entry is one item the Search operation can return. Authored items are mine and `involves` adds items
// others opened that name me. `ageDays` places `updatedAt` so the window lanes can tell 89 days from 91.
function found(number, kind, o = {}) {
  const c = context(REPO, number, kind);
  const age = o.ageDays === undefined ? null : `@-${o.ageDays}d`;
  const build = {
    ci: ok,
    decision: kind === "pr" ? "REVIEW_REQUIRED" : undefined,
    ...o,
    ...(age ? { updated: age, created: `@-${o.ageDays + 1}d` } : {}),
  };
  return kind === "pr" ? prNode(c, build) : issueNode(c, build);
}

function minePages(count, first, kind, mark) {
  return Array.from({ length: count }, (_, i) => {
    const n = first + i;
    const at = kind === "pr" ? `@-${i + 1}m` : `@-${i + 1}h`;
    const comment_ = comment(context(REPO, n, kind), `IC_${n}_1`, at, "bob");
    if (mark) Object.assign(comment_, { body: "BODYMARK-7f3a", bodyText: "BODYMARK-7f3a" });
    return found(n, kind, {
      updated: at,
      created: "@-5d",
      timeline: [comment_],
      extra: mark ? { body: "BODYMARK-7f3a", bodyText: "BODYMARK-7f3a" } : undefined,
    });
  });
}

function theirs(count, first) {
  return Array.from({ length: count }, (_, i) => {
    const n = first + i;
    return found(n, "pr", {
      by: "alice",
      requests: [ME],
      updated: `@-${i + 1}h`,
      created: "@-6d",
      timeline: [requested(`RRE_${n}`, `@-${i + 1}h`, "alice", ME)],
    });
  });
}

function backfill(prs, issues, others, sharedWithOthers, extra = {}) {
  const authoredPrs = minePages(prs, 1000, "pr", extra.mark);
  const authored = [...authoredPrs, ...minePages(issues, 2000, "issue", extra.mark)];
  return {
    authored,
    involves: [...authoredPrs.slice(0, sharedWithOthers), ...theirs(others, 3000)],
  };
}

const searchKinds = {
  "backfill-150": { windowDays: 90, search: () => backfill(150, 20, 40, 20) },
  "backfill-empty": { windowDays: 90, search: () => ({ authored: [], involves: [] }) },
  "backfill-overlap": {
    windowDays: 90,
    specs: () => many(12, 1000),
    search: () => {
      const authored = many(12, 1000).map((s) => s.node);
      return { authored, involves: [...authored.slice(0, 6), ...theirs(5, 3000)] };
    },
  },
  "backfill-old": {
    windowDays: 90,
    search: () => ({
      authored: [20, 89, 91, 400].map((days, i) => found(4100 + i, "pr", { by: ME, ageDays: days })),
      involves: [],
    }),
  },
  "nightly-new": {
    windowDays: 7,
    search: () => ({
      authored: [found(4000, "pr", { by: ME, ageDays: 1 }), found(4001, "pr", { by: ME, ageDays: 30 })],
      involves: [],
    }),
  },
  "low-budget": {
    windowDays: 90,
    rate: { limit: 5000, remaining: 400, reset: "@+20s" },
    search: () => backfill(12, 3, 5, 2),
  },
  "backfill-150-502": {
    windowDays: 90,
    search: () => backfill(150, 20, 40, 20),
    faults: [{ match: "POST /graphql", status: 502, times: 2, graphql: { after: cursorFor(20), q: "is:pr author" } }],
  },
  "backfill-150-dead": {
    windowDays: 90,
    search: () => backfill(150, 20, 40, 20),
    faults: [{ match: "POST /graphql", status: 502, times: 3, graphql: { after: cursorFor(20), q: "is:pr author" } }],
  },
  "backfill-marker": { windowDays: 90, search: () => backfill(12, 3, 5, 2, { mark: true }) },
};

const REASONS = new Set(["review_requested", "author", "ci_activity", "comment", "mention", "team_mention", "assign"]);

function notification(s, index) {
  const id = String(8000 + s.c.number);
  return {
    id,
    unread: s.unread,
    reason: s.reason,
    updated_at: s.updated,
    last_read_at: s.unread ? null : "@-5m",
    subject: {
      title: s.title,
      url: `https://api.github.com/repos/${s.c.repo}/${s.kind === "pr" ? "pulls" : "issues"}/${s.c.number}`,
      latest_comment_url: null,
      type: s.kind === "pr" ? "PullRequest" : "Issue",
    },
    repository: { full_name: s.c.repo, owner: { login: s.c.repo.split("/")[0] } },
    _order: index,
  };
}

const DAY_MS = 864e5;

function searchExpectation(search, specs, windowDays, now) {
  const start = Math.floor((now - windowDays * DAY_MS) / DAY_MS) * DAY_MS;
  const inWindow = (node) => Date.parse(resolveTimes(node.updatedAt, now)) >= start;
  const keys = new Set(specs.filter((s) => !s.gone).map((s) => `${s.c.repo}#${s.c.number}`));
  for (const node of [...search.authored, ...search.involves]) {
    if (inWindow(node)) keys.add(`${REPO}#${node.number}`);
  }
  return [...keys].sort();
}

function buildSearch(kind, def, now) {
  const specs = def.specs ? def.specs() : [];
  const search = def.search();
  const notifications = specs.map(notification).map(({ _order, ...rest }) => rest);
  return {
    extends: "basic",
    ...(def.rate ? { rate: def.rate } : {}),
    notifications: { last_modified: "@-1m", etag: `"stub-${kind}"`, items: notifications },
    items: Object.fromEntries(specs.filter((s) => !s.missing).map((s) => [`${s.c.repo}#${s.c.number}`, s.node])),
    search,
    ...(def.faults ? { faults: def.faults } : {}),
    _expect: {
      windowDays: def.windowDays,
      items: searchExpectation(search, specs, def.windowDays, now),
      authored: search.authored.length,
      involves: search.involves.length,
    },
  };
}

export function build(kind, now = Date.now()) {
  if (searchKinds[kind]) return buildSearch(kind, searchKinds[kind], now);
  const def = kinds[kind];
  if (!def) {
    const known = [...Object.keys(kinds), ...Object.keys(searchKinds)].join(", ");
    throw new Error(`unknown kind ${JSON.stringify(kind)}; kinds are ${known}`);
  }
  const specs = def.specs();
  for (const s of specs) if (!REASONS.has(s.reason)) throw new Error(`unknown reason ${s.reason}`);
  const items = Object.fromEntries(
    specs.filter((s) => !s.missing).map((s) => [`${s.c.repo}#${s.c.number}`, s.node]),
  );
  const notifications = specs.map(notification).map(({ _order, ...rest }) => rest);
  return {
    extends: "basic",
    notifications: { last_modified: "@-1m", etag: `"stub-${kind}"`, items: notifications },
    items,
    ...(def.scenario ?? {}),
    ...(def.faults ? { faults: def.faults } : {}),
    _expect: {
      threads: specs.length,
      items: specs.filter((s) => !s.gone).length,
      gone: specs.filter((s) => s.gone).length,
      events: specs.reduce((sum, s) => sum + eventCount(s), 0),
      rows: specs
        .filter((s) => !s.gone)
        .map((s) => ({ repo: s.c.repo, number: s.c.number, kind: s.kind, ...s.expect })),
    },
  };
}

export function threadIds(scenario) {
  return scenario.notifications.items.map((n) => n.id);
}

export function seedSql(scenario, now = Date.now()) {
  const resolved = resolveTimes(scenario, now);
  const quote = (text) => `'${String(text).replaceAll("'", "''")}'`;
  return resolved.notifications.items
    .map((n) => {
      const number = n.subject.url.split("/").pop();
      const kind = n.subject.type === "PullRequest" ? "pr" : "issue";
      const updated = Math.floor(Date.parse(n.updated_at) / 1000);
      return (
        "INSERT OR REPLACE INTO notification_threads " +
        "(id, repo, subject_kind, subject_title, subject_number, item_node_id, reason, updated_at, synced_at) VALUES " +
        `(${quote(n.id)}, ${quote(n.repository.full_name)}, ${quote(kind)}, ${quote(n.subject.title)}, ${number}, NULL, ${quote(n.reason)}, ${updated}, ${Math.floor(now / 1000)});`
      );
    })
    .join("\n");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [kind, flag] = process.argv.slice(2);
  try {
    const scenario = build(kind ?? "");
    if (flag === "--sql") console.log(seedSql(scenario));
    else if (flag === "--ids") console.log(threadIds(scenario).join(","));
    else console.log(JSON.stringify(scenario, null, 2));
  } catch (err) {
    console.error(err.message);
    process.exit(2);
  }
}
