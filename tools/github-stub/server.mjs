import { readFileSync } from "node:fs";
import http from "node:http";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const SCENARIOS_DIR = join(dirname(fileURLToPath(import.meta.url)), "scenarios");
const MAX_BODY = 1_000_000;
const NAME_PATTERN = /^[a-z0-9][a-z0-9_-]*(\/[a-z0-9][a-z0-9_-]*)*$/i;
const RELATIVE_TIME = /^@(now|[+-]\d+[smhdw])$/;
const UNIT_MS = { s: 1e3, m: 6e4, h: 36e5, d: 864e5, w: 6048e5 };

const ROUTES = {
  "/user": { methods: ["GET"], auth: true },
  "/rate_limit": { methods: ["GET"], auth: true },
  "/notifications": { methods: ["GET"], auth: true },
  "/graphql": { methods: ["POST"], auth: true },
  "/login/oauth/authorize": { methods: ["GET"], auth: false },
  "/login/oauth/access_token": { methods: ["POST"], auth: false },
};

export function resolveTimes(value, now = Date.now()) {
  if (typeof value === "string") {
    const match = RELATIVE_TIME.exec(value);
    if (!match) return value;
    if (match[1] === "now") return new Date(now).toISOString();
    const sign = match[1][0] === "-" ? -1 : 1;
    const amount = Number(match[1].slice(1, -1));
    const unit = UNIT_MS[match[1].at(-1)];
    return new Date(now + sign * amount * unit).toISOString();
  }
  if (Array.isArray(value)) return value.map((item) => resolveTimes(item, now));
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, resolveTimes(v, now)]));
  }
  return value;
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function deepMerge(base, over) {
  if (!isPlainObject(base) || !isPlainObject(over)) return over;
  const merged = { ...base };
  for (const [key, value] of Object.entries(over)) {
    merged[key] = key in base ? deepMerge(base[key], value) : value;
  }
  return merged;
}

function readScenarioFile(name) {
  if (!NAME_PATTERN.test(name)) throw new Error(`bad scenario name ${JSON.stringify(name)}`);
  return JSON.parse(readFileSync(join(SCENARIOS_DIR, `${name}.json`), "utf8"));
}

function mergeExtends(name, depth = 0) {
  if (depth > 5) throw new Error(`scenario ${name} extends too deeply`);
  const { extends: parent, ...own } = readScenarioFile(name);
  return parent ? deepMerge(mergeExtends(parent, depth + 1), own) : own;
}

export function loadScenario(name, now = Date.now()) {
  return resolveTimes(mergeExtends(name), now);
}

function tokenize(text) {
  const tokens = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (/[\s,﻿]/.test(c)) i += 1;
    else if (c === "#") while (i < text.length && text[i] !== "\n" && text[i] !== "\r") i += 1;
    else if (text.startsWith('"""', i)) {
      i += 3;
      while (i < text.length && !(text.startsWith('"""', i) && text[i - 1] !== "\\")) i += 1;
      if (i >= text.length) return null;
      i += 3;
    } else if (c === '"') {
      i += 1;
      while (i < text.length && text[i] !== '"') {
        if (text[i] === "\n" || text[i] === "\r") return null;
        i += text[i] === "\\" ? 2 : 1;
      }
      if (i >= text.length) return null;
      i += 1;
    } else if ("{}()[]".includes(c)) {
      tokens.push(c);
      i += 1;
    } else if (/[_A-Za-z]/.test(c)) {
      const start = i;
      while (i < text.length && /[_0-9A-Za-z]/.test(text[i])) i += 1;
      tokens.push({ name: text.slice(start, i) });
    } else if (/[0-9\-]/.test(c)) {
      while (i < text.length && /[0-9A-Za-z.+\-]/.test(text[i])) i += 1;
    } else if ("$@:=!|&.".includes(c)) i += 1;
    else return null;
  }
  return tokens;
}

export function analyzeDocument(text) {
  const tokens = typeof text === "string" ? tokenize(text) : null;
  if (!tokens) return { ok: false, kind: "invalid", name: null };
  const stack = [];
  const closers = { "}": "{", ")": "(", "]": "[" };
  const operations = [];
  let wantDefinition = true;
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (typeof token === "string" && "{([".includes(token)) {
      if (!stack.length && wantDefinition) {
        if (token !== "{") return { ok: false, kind: "invalid", name: null };
        operations.push({ kind: "query", name: null });
        wantDefinition = false;
      }
      stack.push(token);
    } else if (typeof token === "string") {
      if (stack.pop() !== closers[token]) return { ok: false, kind: "invalid", name: null };
      if (!stack.length && token === "}") wantDefinition = true;
    } else if (!stack.length && wantDefinition) {
      wantDefinition = false;
      if (token.name === "fragment") continue;
      const next = tokens[i + 1];
      const name = next && typeof next === "object" ? next.name : null;
      if (!["query", "mutation", "subscription"].includes(token.name)) {
        return { ok: false, kind: "invalid", name: null };
      }
      operations.push({ kind: token.name, name });
    }
  }
  if (stack.length || !operations.length) return { ok: false, kind: "invalid", name: null };
  const bad = operations.find((op) => op.kind !== "query");
  if (bad) return { ok: false, kind: bad.kind, name: bad.name };
  return { ok: true, kind: "query", name: operations[0].name };
}

function epochSeconds(value) {
  return typeof value === "number" ? value : Math.floor(Date.parse(value) / 1000);
}

function rateHeaders(scenario, override = {}) {
  const rate = { ...scenario.rate, ...override };
  const headers = {
    "x-ratelimit-limit": String(rate.limit),
    "x-ratelimit-remaining": String(rate.remaining),
    "x-ratelimit-reset": String(epochSeconds(rate.reset)),
    "x-ratelimit-used": String(rate.limit - rate.remaining),
    "x-ratelimit-resource": "core",
  };
  if (Array.isArray(scenario.scopes)) headers["x-oauth-scopes"] = scenario.scopes.join(", ");
  return headers;
}

function rateResource(scenario) {
  const { limit, remaining, reset } = scenario.rate;
  return { limit, remaining, used: limit - remaining, reset: epochSeconds(reset) };
}

function notModified(headers, notifications) {
  const inm = headers["if-none-match"];
  if (inm) {
    return inm
      .split(",")
      .map((tag) => tag.trim().replace(/^W\//, ""))
      .some((tag) => tag === "*" || tag === notifications.etag);
  }
  const ims = Date.parse(headers["if-modified-since"] ?? "");
  const last = Date.parse(notifications.last_modified ?? "");
  return !Number.isNaN(ims) && !Number.isNaN(last) && ims >= Math.floor(last / 1000) * 1000;
}

const PAGE_SIZE_DEFAULT = 30;
const PAGE_SIZE_MAX = 50;

function withoutMillis(items) {
  const trim = (value) => (typeof value === "string" ? value.replace(/\.\d+Z$/, "Z") : value);
  return items.map((item) => ({ ...item, updated_at: trim(item.updated_at), last_read_at: trim(item.last_read_at) }));
}

function generatedItems({ count, repo = "octo/widgets", updated_at = "2026-11-01T00:00:00Z", first_id = 8000 }) {
  return Array.from({ length: count }, (_, i) => ({
    id: String(first_id + i),
    unread: i % 2 === 0,
    reason: "subscribed",
    updated_at,
    last_read_at: null,
    subject: {
      title: `Generated thread ${i + 1}`,
      url: `https://api.github.com/repos/${repo}/issues/${i + 1}`,
      latest_comment_url: null,
      type: "Issue",
    },
    repository: { full_name: repo, owner: { login: repo.split("/")[0] } },
  }));
}

export function notificationPage(notifications, searchParams, base) {
  const all = withoutMillis(notifications.generate ? generatedItems(notifications.generate) : (notifications.items ?? []));
  const perPage = Math.min(Math.max(Number(searchParams.get("per_page")) || PAGE_SIZE_DEFAULT, 1), PAGE_SIZE_MAX);
  const page = Math.max(Number(searchParams.get("page")) || 1, 1);
  const last = Math.max(Math.ceil(all.length / perPage), 1);
  const link = (n, rel) => {
    const next = new URLSearchParams(searchParams);
    next.set("page", String(n));
    return `<${base}/notifications?${next}>; rel="${rel}"`;
  };
  const rels = [];
  if (page > 1) rels.push(link(page - 1, "prev"));
  if (page < last) rels.push(link(page + 1, "next"), link(last, "last"));
  return { items: all.slice((page - 1) * perPage, page * perPage), link: rels.join(", ") };
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size > MAX_BODY) {
        reject(new Error("body too large"));
        req.destroy();
      } else chunks.push(chunk);
    });
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

function parseForm(body, contentType) {
  if (contentType?.includes("json")) return JSON.parse(body);
  return Object.fromEntries(new URLSearchParams(body));
}

function faultMatches(fault, method, path, searchParams) {
  const [m, p] = fault.match.split(" ");
  const [name, value] = (fault.query ?? "").split("=");
  const queryOk = fault.query === undefined || searchParams.get(name) === value;
  return (m === "*" || m === method) && (p === "*" || p === path) && queryOk;
}

export function createStub({ scenario: initial = "basic" } = {}) {
  const state = { name: initial, scenario: loadScenario(initial), faults: [], log: [] };

  function armFaults() {
    state.faults = (state.scenario.faults ?? []).map((fault) => ({ ...fault, left: fault.times ?? Infinity }));
  }
  armFaults();

  function reply(res, entry, status, body, headers = {}) {
    entry.status = status;
    const isRaw = typeof body === "string";
    res.writeHead(status, {
      "content-type": isRaw ? "text/html; charset=utf-8" : "application/json; charset=utf-8",
      ...headers,
    });
    res.end(body === undefined || status === 204 || status === 304 ? undefined : isRaw ? body : JSON.stringify(body));
  }

  function applyFault(res, entry, method, path, searchParams) {
    const fault = state.faults.find((f) => f.left > 0 && faultMatches(f, method, path, searchParams));
    if (!fault) return false;
    fault.left -= 1;
    const headers = rateHeaders(state.scenario, fault.rate);
    if (fault.retry_after !== undefined) headers["retry-after"] = String(fault.retry_after);
    const status = fault.status ?? 500;
    const body = fault.raw ?? fault.body ?? { message: http.STATUS_CODES[status] ?? "error" };
    reply(res, entry, status, body, headers);
    return true;
  }

  const handlers = {
    "GET /user": (req, res, entry) =>
      reply(res, entry, 200, state.scenario.user, rateHeaders(state.scenario)),

    "GET /rate_limit": (req, res, entry) => {
      const core = rateResource(state.scenario);
      const headers = rateHeaders(state.scenario);
      if (state.scenario.rate_limit_poll_interval) headers["x-poll-interval"] = String(state.scenario.poll_interval);
      reply(res, entry, 200, { resources: { core, graphql: core, search: core }, rate: core }, headers);
    },

    "GET /notifications": (req, res, entry, body, url) => {
      const notifications = state.scenario.notifications ?? { items: [] };
      const headers = rateHeaders(state.scenario);
      if (notifications.last_modified) headers["last-modified"] = new Date(notifications.last_modified).toUTCString();
      if (notifications.etag) headers.etag = notifications.etag;
      if (state.scenario.poll_interval !== undefined) headers["x-poll-interval"] = String(state.scenario.poll_interval);
      if (notModified(req.headers, notifications)) return reply(res, entry, 304, undefined, headers);
      const { items, link } = notificationPage(notifications, url.searchParams, `http://${req.headers.host}`);
      if (link) headers.link = link;
      reply(res, entry, 200, items, headers);
    },

    "POST /graphql": async (req, res, entry, body) => {
      const headers = rateHeaders(state.scenario);
      let payload;
      try {
        payload = JSON.parse(body);
      } catch {
        return reply(res, entry, 400, { message: "Problems parsing JSON" }, headers);
      }
      entry.body = body;
      entry.variables = payload?.variables ?? null;
      const info = analyzeDocument(payload?.query);
      entry.op = info.kind;
      entry.opName = info.name;
      if (!info.ok) {
        entry.violation = info.kind === "invalid" ? "graphql_invalid_document" : `graphql_${info.kind}`;
        return reply(res, entry, 400, { errors: [{ message: "Radar stub only answers query operations" }] }, headers);
      }
      let response = state.scenario.graphql?.[info.name];
      if (response?.$pages) response = response.$pages[payload?.variables?.after ?? ""];
      if (response === undefined) {
        response = { errors: [{ message: `stub has no response for operation ${info.name}` }] };
      }
      reply(res, entry, 200, response, headers);
    },

    "GET /login/oauth/authorize": (req, res, entry, body, url) => {
      const redirect = url.searchParams.get("redirect_uri") ?? "";
      if (!/^https?:\/\//.test(redirect)) return reply(res, entry, 400, { error: "bad_redirect_uri" });
      const target = new URL(redirect);
      const oauth = state.scenario.oauth ?? {};
      if (oauth.deny) target.searchParams.set("error", "access_denied");
      else target.searchParams.set("code", oauth.issue_code);
      const requestState = url.searchParams.get("state");
      if (requestState !== null) target.searchParams.set("state", requestState);
      entry.status = 302;
      res.writeHead(302, { location: target.toString() });
      res.end();
    },

    "POST /login/oauth/access_token": (req, res, entry, body) => {
      let form;
      try {
        form = parseForm(body, req.headers["content-type"]);
      } catch {
        return reply(res, entry, 400, { error: "bad_request" });
      }
      const oauth = state.scenario.oauth ?? {};
      const ok =
        form.client_id === oauth.client_id && form.client_secret === oauth.client_secret && form.code === oauth.code;
      entry.oauth = ok ? "accepted" : "rejected";
      if (!ok) {
        return reply(res, entry, 200, {
          error: "bad_verification_code",
          error_description: "The code passed is incorrect or expired.",
        });
      }
      const scope = (state.scenario.scopes ?? []).join(",");
      reply(res, entry, 200, { access_token: state.scenario.token, token_type: "bearer", scope });
    },
  };

  async function control(req, res, url) {
    const key = `${req.method} ${url.pathname}`;
    const entry = {};
    if (key === "GET /__log") return reply(res, entry, 200, state.log);
    if (key === "POST /__log/clear") {
      const cleared = state.log.length;
      state.log.length = 0;
      return reply(res, entry, 200, { cleared });
    }
    if (key === "POST /__scenario") {
      try {
        const { name } = JSON.parse(await readBody(req));
        state.scenario = loadScenario(name);
        state.name = name;
        armFaults();
        return reply(res, entry, 200, { scenario: name });
      } catch (err) {
        const missing = err.code === "ENOENT" || /bad scenario name/.test(err.message);
        return reply(res, entry, missing ? 404 : 400, { error: missing ? "unknown_scenario" : "bad_request" });
      }
    }
    return null;
  }

  const server = http.createServer(async (req, res) => {
    const target = req.url ?? "";
    const cut = target.indexOf("?");
    const search = cut < 0 ? "" : target.slice(cut);
    const url = {
      pathname: cut < 0 ? target : target.slice(0, cut),
      search,
      searchParams: new URLSearchParams(search),
    };
    if (url.pathname.startsWith("/__")) {
      if ((await control(req, res, url)) !== null) return;
    }
    const entry = {
      n: state.log.length + 1,
      method: req.method,
      path: url.pathname,
      query: url.search.slice(1),
      ifModifiedSince: req.headers["if-modified-since"] ?? null,
      ifNoneMatch: req.headers["if-none-match"] ?? null,
      hasAuth: Boolean(req.headers.authorization),
      violation: null,
    };
    state.log.push(entry);
    try {
      const route = Object.hasOwn(ROUTES, url.pathname) ? ROUTES[url.pathname] : undefined;
      if (!route || !route.methods.includes(req.method)) {
        if (route || req.method !== "GET") {
          entry.violation = "method_not_allowed";
          return reply(res, entry, 405, { message: "Method not allowed" }, { allow: route ? route.methods.join(", ") : "GET" });
        }
        return reply(res, entry, 404, { message: "Not Found" });
      }
      const body = req.method === "POST" ? await readBody(req) : "";
      if (route.auth) {
        const header = req.headers.authorization ?? "";
        const token = header.replace(/^(Bearer|token) /i, "");
        if (token !== state.scenario.token) return reply(res, entry, 401, { message: "Bad credentials" });
      }
      if (route.auth && state.scenario.delay_ms) await new Promise((r) => setTimeout(r, state.scenario.delay_ms));
      if (applyFault(res, entry, req.method, url.pathname, url.searchParams)) return;
      await handlers[`${req.method} ${url.pathname}`](req, res, entry, body, url);
    } catch {
      if (!res.headersSent) reply(res, entry, 500, { message: "stub error" });
      else res.end();
    }
  });

  return { server, state };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { values } = parseArgs({
    options: { port: { type: "string", default: "8900" }, scenario: { type: "string", default: "basic" } },
  });
  let stub;
  try {
    stub = createStub({ scenario: values.scenario });
  } catch (err) {
    console.error(`stub failed to start: ${err.message}`);
    process.exit(1);
  }
  stub.server.listen(Number(values.port), "127.0.0.1", () => {
    const { port } = stub.server.address();
    console.log(`stub listening on 127.0.0.1:${port} scenario=${values.scenario}`);
  });
}
