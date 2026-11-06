import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readdirSync } from "node:fs";
import { connect } from "node:net";
import { once } from "node:events";
import { dirname, join } from "node:path";
import { after, before, describe, it } from "node:test";
import { fileURLToPath } from "node:url";

import { analyzeDocument, createStub, loadScenario, resolveTimes } from "./server.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const TOKEN = "gho_stubtoken0000";
const AUTH = { authorization: `Bearer ${TOKEN}` };

async function start(scenario = "basic") {
  const stub = createStub({ scenario });
  stub.server.listen(0, "127.0.0.1");
  await once(stub.server, "listening");
  const base = `http://127.0.0.1:${stub.server.address().port}`;
  const call = (path, init = {}) => fetch(base + path, { redirect: "manual", ...init });
  const authed = (path, init = {}) => call(path, { ...init, headers: { ...AUTH, ...init.headers } });
  const log = async () => (await call("/__log")).json();
  const swap = (name) =>
    call("/__scenario", { method: "POST", body: JSON.stringify({ name }) });
  const close = () =>
    new Promise((resolve, reject) => {
      stub.server.close((err) => (err ? reject(err) : resolve()));
      stub.server.closeAllConnections();
    });
  return { stub, base, call, authed, log, swap, close };
}

function rawRequest(port, target) {
  return new Promise((resolve, reject) => {
    const socket = connect(port, "127.0.0.1", () => {
      socket.write(`GET ${target} HTTP/1.1\r\nHost: stub\r\nConnection: close\r\n\r\n`);
    });
    let data = "";
    socket.on("data", (chunk) => (data += chunk));
    socket.on("end", () => resolve(data));
    socket.on("error", reject);
  });
}

describe("endpoints", () => {
  let s;
  before(async () => {
    s = await start();
  });
  after(() => s.close());

  it("user_returns_the_scenario_user_with_rate_and_scope_headers", async () => {
    const res = await s.authed("/user");
    assert.equal(res.status, 200);
    assert.equal((await res.json()).login, "karthik");
    assert.equal(res.headers.get("x-oauth-scopes"), "notifications, read:user");
    assert.equal(res.headers.get("x-ratelimit-limit"), "5000");
    assert.match(res.headers.get("x-ratelimit-reset"), /^\d{10}$/);
  });

  it("rate_limit_reports_the_core_budget", async () => {
    const body = await (await s.authed("/rate_limit")).json();
    assert.equal(body.resources.core.limit, 5000);
    assert.equal(body.rate.remaining, 4990);
  });

  it("notifications_sends_validators_and_poll_interval", async () => {
    const res = await s.authed("/notifications");
    assert.equal(res.status, 200);
    assert.equal(res.headers.get("x-poll-interval"), "60");
    assert.equal(res.headers.get("etag"), '"stub-etag-1"');
    assert.ok(res.headers.get("last-modified"));
    assert.equal((await res.json()).length, 2);
  });

  it("graphql_answers_a_query_by_operation_name", async () => {
    const res = await s.authed("/graphql", {
      method: "POST",
      body: JSON.stringify({ query: "query Viewer { viewer { login } }", variables: {} }),
    });
    assert.equal(res.status, 200);
    assert.deepEqual((await res.json()).data.viewer, { login: "karthik", databaseId: 1001 });
  });

  it("endpoints_that_need_auth_answer_401_without_the_scenario_token", async () => {
    for (const [method, path] of [
      ["GET", "/user"],
      ["GET", "/rate_limit"],
      ["GET", "/notifications"],
      ["POST", "/graphql"],
    ]) {
      const none = await s.call(path, { method, body: method === "POST" ? "{}" : undefined });
      assert.equal(none.status, 401, `${method} ${path} without auth`);
      const wrong = await s.call(path, {
        method,
        headers: { authorization: "Bearer nope" },
        body: method === "POST" ? "{}" : undefined,
      });
      assert.equal(wrong.status, 401, `${method} ${path} with a wrong token`);
    }
  });

  it("unknown_get_path_is_404_and_not_a_violation", async () => {
    await s.call("/__log/clear", { method: "POST" });
    assert.equal((await s.authed("/nope")).status, 404);
    const [entry] = await s.log();
    assert.equal(entry.violation, null);
  });

  it("double_slash_targets_are_logged_as_received_and_do_not_crash_the_stub", async () => {
    await s.call("/__log/clear", { method: "POST" });
    const port = s.stub.server.address().port;
    for (const target of ["//user", "//", "//user?a=b"]) {
      assert.match(await rawRequest(port, target), /^HTTP\/1\.1 404 /, target);
    }
    assert.equal((await s.authed("/user")).status, 200);
    const log = await s.log();
    assert.deepEqual(
      log.slice(0, 3).map((e) => [e.path, e.query]),
      [["//user", ""], ["//", ""], ["//user", "a=b"]],
    );
    assert.ok(log.slice(0, 3).every((e) => e.violation === null));
  });

  it("log_records_auth_presence_but_never_the_token", async () => {
    await s.call("/__log/clear", { method: "POST" });
    await s.authed("/user");
    await s.call("/user");
    const log = await s.log();
    assert.deepEqual(
      log.map((e) => [e.method, e.path, e.hasAuth]),
      [
        ["GET", "/user", true],
        ["GET", "/user", false],
      ],
    );
    assert.ok(!JSON.stringify(log).includes(TOKEN));
  });
});

describe("conditional requests", () => {
  let s;
  before(async () => {
    s = await start();
  });
  after(() => s.close());

  it("conditional_get_returns_304", async () => {
    const first = await s.authed("/notifications");
    const lastModified = first.headers.get("last-modified");
    const etag = first.headers.get("etag");

    const byDate = await s.authed("/notifications", { headers: { "if-modified-since": lastModified } });
    assert.equal(byDate.status, 304);
    assert.equal(byDate.headers.get("x-poll-interval"), "60");
    assert.equal(await byDate.text(), "");

    const byTag = await s.authed("/notifications", { headers: { "if-none-match": etag } });
    assert.equal(byTag.status, 304);

    const stale = await s.authed("/notifications", {
      headers: { "if-modified-since": "Mon, 01 Jan 2001 00:00:00 GMT" },
    });
    assert.equal(stale.status, 200);
    const wrongTag = await s.authed("/notifications", { headers: { "if-none-match": '"other"' } });
    assert.equal(wrongTag.status, 200);

    const log = await s.log();
    assert.ok(log.some((e) => e.ifModifiedSince === lastModified));
    assert.ok(log.some((e) => e.ifNoneMatch === etag));
  });
});

describe("read-only defence", () => {
  let s;
  before(async () => {
    s = await start();
  });
  after(() => s.close());

  it("mutation_is_rejected_and_logged", async () => {
    await s.call("/__log/clear", { method: "POST" });
    const res = await s.authed("/graphql", {
      method: "POST",
      body: JSON.stringify({ query: "mutation Star { addStar(input: {}) { clientMutationId } }" }),
    });
    assert.equal(res.status, 400);
    const [entry] = await s.log();
    assert.equal(entry.op, "mutation");
    assert.equal(entry.violation, "graphql_mutation");
  });

  it("mixed_and_subscription_documents_are_rejected", async () => {
    for (const query of [
      "query A { a } mutation B { b }",
      "subscription S { s }",
      "query A { a",
    ]) {
      const res = await s.authed("/graphql", { method: "POST", body: JSON.stringify({ query }) });
      assert.equal(res.status, 400, query);
    }
  });

  it("put_is_rejected_and_logged", async () => {
    await s.call("/__log/clear", { method: "POST" });
    for (const method of ["PUT", "PATCH", "DELETE", "POST"]) {
      const res = await s.authed("/user", { method, body: method === "DELETE" ? undefined : "{}" });
      assert.equal(res.status, 405, method);
    }
    const unknown = await s.authed("/repos/octo/widgets/issues", { method: "POST", body: "{}" });
    assert.equal(unknown.status, 405);
    const log = await s.log();
    assert.equal(log.length, 5);
    assert.ok(log.every((e) => e.violation === "method_not_allowed"));
  });

  it("graphql_comments_strings_and_field_names_do_not_trip_the_guard", () => {
    assert.deepEqual(analyzeDocument("query Q { mutationCount } # mutation M { x }"), {
      ok: true,
      kind: "query",
      name: "Q",
    });
    assert.equal(analyzeDocument('query Q($t: String = "mutation") { a(t: $t) }').ok, true);
    assert.equal(analyzeDocument("{ viewer { login } }").name, null);
    assert.equal(analyzeDocument("mutation M { a }").kind, "mutation");
    assert.equal(analyzeDocument("query A { a } subscription S { s }").kind, "subscription");
  });
});

describe("faults", () => {
  let s;
  before(async () => {
    s = await start("rate-limited");
  });
  after(() => s.close());

  it("fault_injection_counts_down", async () => {
    const seen = [];
    for (let i = 0; i < 6; i += 1) {
      const res = await s.authed("/rate_limit");
      seen.push([res.status, res.headers.get("retry-after"), res.headers.get("x-ratelimit-remaining")]);
    }
    assert.deepEqual(seen[0].slice(0, 2), [403, "17"]);
    assert.deepEqual(seen[1].slice(0, 2), [429, "23"]);
    assert.deepEqual(seen[2], [403, null, "0"]);
    assert.deepEqual(seen[3], [403, null, "4990"]);
    assert.equal(seen[4][0], 200);
    assert.equal(seen[5][0], 200);
  });

  it("swapping_a_scenario_rearms_its_faults", async () => {
    assert.equal((await s.swap("rate-limited")).status, 200);
    assert.equal((await s.authed("/rate_limit")).status, 403);
    assert.equal((await s.swap("basic")).status, 200);
    assert.equal((await s.authed("/rate_limit")).status, 200);
    assert.equal((await s.swap("does-not-exist")).status, 404);
    assert.equal((await s.swap("../package")).status, 404);
  });

  it("raw_faults_send_a_body_that_is_not_json", async () => {
    await s.swap("bad-body");
    const res = await s.authed("/user");
    assert.equal(res.status, 200);
    assert.equal(await res.text(), "<html>not json</html>");
  });

  it("server_error_scenario_fails_the_api_but_not_oauth", async () => {
    await s.swap("server-error");
    assert.equal((await s.authed("/user")).status, 500);
    const authorize = await s.call("/login/oauth/authorize?redirect_uri=http://127.0.0.1:1/cb&state=s");
    assert.equal(authorize.status, 302);
  });
});

describe("oauth", () => {
  let s;
  before(async () => {
    s = await start();
  });
  after(() => s.close());

  const exchange = (s, fields, type = "form") =>
    s.call("/login/oauth/access_token", {
      method: "POST",
      headers: {
        "content-type": type === "form" ? "application/x-www-form-urlencoded" : "application/json",
      },
      body: type === "form" ? new URLSearchParams(fields).toString() : JSON.stringify(fields),
    });

  it("oauth_round_trip", async () => {
    const authorize = await s.call(
      "/login/oauth/authorize?client_id=stub-client&redirect_uri=http%3A%2F%2F127.0.0.1%3A9%2Fauth%2Fcallback&state=abc123",
    );
    assert.equal(authorize.status, 302);
    const target = new URL(authorize.headers.get("location"));
    assert.equal(target.origin, "http://127.0.0.1:9");
    assert.equal(target.searchParams.get("state"), "abc123");
    const code = target.searchParams.get("code");

    for (const type of ["form", "json"]) {
      const token = await exchange(
        s,
        { client_id: "stub-client", client_secret: "stub-secret", code },
        type,
      );
      assert.deepEqual(await token.json(), {
        access_token: TOKEN,
        token_type: "bearer",
        scope: "notifications,read:user",
      });
    }
    const me = await s.authed("/user");
    assert.equal((await me.json()).login, "karthik");
  });

  it("token_endpoint_rejects_a_wrong_client_secret_or_code", async () => {
    for (const fields of [
      { client_id: "stub-client", client_secret: "wrong", code: "stub-code" },
      { client_id: "other", client_secret: "stub-secret", code: "stub-code" },
      { client_id: "stub-client", client_secret: "stub-secret", code: "nope" },
      {},
    ]) {
      const body = await (await exchange(s, fields)).json();
      assert.equal(body.error, "bad_verification_code");
      assert.equal(body.access_token, undefined);
    }
  });

  it("oauth_deny_redirects_with_access_denied_and_the_same_state", async () => {
    await s.swap("oauth-deny");
    const res = await s.call("/login/oauth/authorize?redirect_uri=http://127.0.0.1:9/cb&state=xyz");
    const target = new URL(res.headers.get("location"));
    assert.equal(target.searchParams.get("error"), "access_denied");
    assert.equal(target.searchParams.get("state"), "xyz");
    assert.equal(target.searchParams.get("code"), null);
  });

  it("oauth_bad_code_issues_a_code_the_token_endpoint_refuses", async () => {
    await s.swap("oauth-bad-code");
    const res = await s.call("/login/oauth/authorize?redirect_uri=http://127.0.0.1:9/cb&state=q");
    const code = new URL(res.headers.get("location")).searchParams.get("code");
    const body = await (
      await exchange(s, { client_id: "stub-client", client_secret: "stub-secret", code })
    ).json();
    assert.equal(body.error, "bad_verification_code");
  });

  it("oauth_other_user_signs_in_as_someone_else", async () => {
    await s.swap("oauth-other-user");
    assert.equal((await (await s.authed("/user")).json()).id, 2002);
  });

  it("authorize_refuses_a_missing_or_non_http_redirect", async () => {
    await s.swap("basic");
    assert.equal((await s.call("/login/oauth/authorize")).status, 400);
    assert.equal((await s.call("/login/oauth/authorize?redirect_uri=javascript:alert(1)")).status, 400);
  });

  it("token_exchange_log_has_no_secrets", async () => {
    await s.call("/__log/clear", { method: "POST" });
    await exchange(s, { client_id: "stub-client", client_secret: "stub-secret", code: "stub-code" });
    const text = JSON.stringify(await s.log());
    assert.ok(!text.includes("stub-secret"));
    assert.ok(text.includes('"oauth":"accepted"'));
  });
});

describe("scenarios", () => {
  it("relative_times_resolve_against_now", () => {
    const now = Date.parse("2026-11-06T12:00:00.000Z");
    assert.deepEqual(
      resolveTimes({ a: "@-2h", b: ["@+30s", "@now", "plain", "@-3d"], c: { d: "@+1w" } }, now),
      {
        a: "2026-11-06T10:00:00.000Z",
        b: ["2026-11-06T12:00:30.000Z", "2026-11-06T12:00:00.000Z", "plain", "2026-11-03T12:00:00.000Z"],
        c: { d: "2026-11-13T12:00:00.000Z" },
      },
    );
  });

  it("every_scenario_file_loads_and_has_a_token_and_rate", () => {
    const names = readdirSync(join(HERE, "scenarios"))
      .filter((f) => f.endsWith(".json"))
      .map((f) => f.replace(/\.json$/, ""));
    assert.ok(names.length >= 11);
    for (const name of names) {
      const scenario = loadScenario(name);
      assert.equal(scenario.token, TOKEN, name);
      assert.ok(scenario.rate.limit > 0, name);
      assert.equal(scenario.extends, undefined, name);
    }
  });

  it("the_planned_scenarios_exist", () => {
    for (const name of [
      "basic",
      "empty",
      "fast-poll",
      "low-budget",
      "rate-limited",
      "server-error",
      "graphql-errors",
      "no-notifications-scope",
      "oauth-other-user",
      "oauth-deny",
      "oauth-bad-code",
    ]) {
      assert.ok(loadScenario(name), name);
    }
    assert.equal(loadScenario("low-budget").rate.remaining, 400);
    assert.equal(loadScenario("fast-poll").poll_interval, 5);
    assert.deepEqual(loadScenario("empty").notifications.items, []);
    assert.deepEqual(loadScenario("no-notifications-scope").scopes, ["read:user"]);
  });
});

describe("process", () => {
  it("prints_the_listening_line", async () => {
    const child = spawn(process.execPath, [join(HERE, "server.mjs"), "--port", "0", "--scenario", "basic"], {
      stdio: ["ignore", "pipe", "inherit"],
    });
    try {
      let out = "";
      for await (const chunk of child.stdout) {
        out += chunk;
        if (out.includes("\n")) break;
      }
      assert.match(out, /^stub listening on 127\.0\.0\.1:\d+ scenario=basic\n/);
    } finally {
      child.kill();
    }
  });

  it("exits_non_zero_for_an_unknown_scenario", async () => {
    const child = spawn(process.execPath, [join(HERE, "server.mjs"), "--scenario", "nope"], {
      stdio: "ignore",
    });
    const [code] = await once(child, "exit");
    assert.equal(code, 1);
  });
});
