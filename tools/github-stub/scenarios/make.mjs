// Writes a scenario file for the enrich lanes to stdout, with times relative to now.
//   node tools/github-stub/scenarios/make.mjs <kind>          the scenario JSON
//   node tools/github-stub/scenarios/make.mjs <kind> --sql    SQL that seeds notification_threads
//   node tools/github-stub/scenarios/make.mjs <kind> --ids    the thread ids, comma separated
// Until R09 polls, the SQL stands in for the poll and the ids name the Enrich unit.
import { pathToFileURL } from "node:url";

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

export function build(kind) {
  const def = kinds[kind];
  if (!def) throw new Error(`unknown kind ${JSON.stringify(kind)}; kinds are ${Object.keys(kinds).join(", ")}`);
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
