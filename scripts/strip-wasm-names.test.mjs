import assert from "node:assert/strict";
import { test } from "node:test";
import { stripNames } from "./strip-wasm-names.mjs";

const header = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

function custom(name, payload) {
  const body = [name.length, ...Buffer.from(name), ...payload];
  return [0x00, body.length, ...body];
}

const typeSection = [0x01, 0x04, 0x01, 0x60, 0x00, 0x00];

test("drops the name section and keeps every other section", () => {
  const input = Buffer.from([
    ...header,
    ...typeSection,
    ...custom("name", [1, 2, 3]),
    ...custom("target_features", [0]),
  ]);
  const output = stripNames(input);
  assert.deepEqual(
    [...output],
    [...header, ...typeSection, ...custom("target_features", [0])],
  );
  assert.ok(WebAssembly.validate(output), "output is still a valid module");
});

test("leaves a module without names unchanged", () => {
  const input = Buffer.from([...header, ...typeSection]);
  assert.deepEqual([...stripNames(input)], [...input]);
});

test("rejects input that is not wasm", () => {
  assert.throws(() => stripNames(Buffer.from("not wasm at all")), /not a wasm module/);
});

test("rejects a section that runs past the end", () => {
  assert.throws(() => stripNames(Buffer.from([...header, 0x01, 0x10, 0x00])), /past the end/);
});
