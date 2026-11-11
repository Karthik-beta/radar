import assert from "node:assert/strict";
import { test } from "node:test";

import { createStub } from "./server.mjs";

async function run(options, fn) {
  const { server } = createStub(options);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  const quiet = console.log;
  console.log = () => {};
  try {
    await fn(base);
  } finally {
    console.log = quiet;
    server.close();
  }
}

const send = (base, text) =>
  fetch(`${base}/bottok/sendMessage`, { method: "POST", body: JSON.stringify({ chat_id: "42", text }) });

test("a send is recorded and answered ok", async () => {
  await run({}, async (base) => {
    const res = await send(base, "hello");
    assert.equal(res.status, 200);
    assert.deepEqual(await res.json(), { ok: true });
    const seen = await (await fetch(`${base}/requests`)).json();
    assert.equal(seen.length, 1);
    assert.equal(seen[0].body.text, "hello");
  });
});

test("--fail-first fails the first sends and then succeeds", async () => {
  await run({ failFirst: 2 }, async (base) => {
    const codes = [];
    for (let i = 0; i < 3; i += 1) codes.push((await send(base, "x")).status);
    assert.deepEqual(codes, [500, 500, 200]);
  });
});

test("--status answers every send with that code", async () => {
  await run({ status: 429 }, async (base) => {
    assert.equal((await send(base, "x")).status, 429);
    assert.equal((await send(base, "x")).status, 429);
  });
});
