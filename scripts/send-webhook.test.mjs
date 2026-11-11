import assert from "node:assert/strict";
import test from "node:test";

import { eventFor, parseArgs, sign } from "./send-webhook.mjs";

test("signs the GitHub docs vector", () => {
  assert.equal(
    sign("It's a Secret to Everybody", Buffer.from("Hello, World!")),
    "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17",
  );
});

test("picks the event from the fixture name unless --event says otherwise", () => {
  assert.equal(eventFor("tools/github-stub/webhooks/ping.json"), "ping");
  assert.equal(eventFor("tools/github-stub/webhooks/push-main.json"), "push");
  const options = parseArgs(["8805", "x/ping.json", "--secret", "", "--event", "issues"]);
  assert.deepEqual(options, { port: 8805, fixture: "x/ping.json", secret: "", event: "issues" });
});

test("rejects a bad port, a missing value, and an unknown option", () => {
  assert.throws(() => parseArgs(["abc", "f.json"]));
  assert.throws(() => parseArgs(["8805", "f.json", "--secret"]));
  assert.throws(() => parseArgs(["8805", "f.json", "--nope", "x"]));
});
