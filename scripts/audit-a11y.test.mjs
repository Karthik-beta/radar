import assert from "node:assert/strict";
import { test } from "node:test";

import { cookieHeader, failedAudits } from "./audit-a11y.mjs";

const jar = [
  "# Netscape HTTP Cookie File",
  "#HttpOnly_127.0.0.1\tFALSE\t/\tFALSE\t0\t__Host-session\tabc123",
  "other.test\tFALSE\t/\tFALSE\t0\tnope\tx",
  "",
].join("\n");

test("cookieHeader reads an HttpOnly cookie for the host", () => {
  assert.equal(cookieHeader(jar, "127.0.0.1"), "__Host-session=abc123");
});

test("cookieHeader throws when the jar has no cookie for the host", () => {
  assert.throws(() => cookieHeader(jar, "localhost"), /no cookie for localhost/);
});

test("failedAudits lists audits that scored zero", () => {
  const report = { audits: { a: { id: "a", score: 1 }, b: { id: "b", score: 0 }, c: { id: "c", score: null } } };
  assert.deepEqual(failedAudits(report), ["b"]);
});
