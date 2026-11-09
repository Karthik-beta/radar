import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

function readLeb(bytes, at) {
  let value = 0;
  let shift = 0;
  let pos = at;
  for (;;) {
    if (pos >= bytes.length) throw new Error("truncated LEB128");
    const byte = bytes[pos++];
    value += (byte & 0x7f) * 2 ** shift;
    if ((byte & 0x80) === 0) return { value, next: pos };
    shift += 7;
  }
}

// wasm-bindgen needs the `target_features` section, so `strip = "symbols"` cannot
// be used. The `name` section only symbolizes stack traces.
export function stripNames(bytes) {
  const magic = [0x00, 0x61, 0x73, 0x6d];
  if (bytes.length < 8 || magic.some((b, i) => bytes[i] !== b)) {
    throw new Error("not a wasm module");
  }
  const kept = [bytes.subarray(0, 8)];
  let pos = 8;
  while (pos < bytes.length) {
    const start = pos;
    const id = bytes[pos++];
    const size = readLeb(bytes, pos);
    const end = size.next + size.value;
    if (end > bytes.length) throw new Error("section runs past the end");
    let isName = false;
    if (id === 0) {
      const len = readLeb(bytes, size.next);
      const name = Buffer.from(bytes.subarray(len.next, len.next + len.value)).toString("utf8");
      isName = name === "name";
    }
    if (!isName) kept.push(bytes.subarray(start, end));
    pos = end;
  }
  return Buffer.concat(kept);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const path = process.argv[2];
  if (!path) {
    console.error("usage: node scripts/strip-wasm-names.mjs <file.wasm>");
    process.exit(2);
  }
  const before = readFileSync(path);
  const after = stripNames(before);
  writeFileSync(path, after);
  console.log(`stripped ${before.length - after.length} bytes of names from ${path}`);
}
