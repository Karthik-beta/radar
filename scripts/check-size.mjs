import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";
import { gzipSync } from "node:zlib";

const { values } = parseArgs({
  options: {
    "worker-budget-kb": { type: "string", default: "400" },
    "ui-budget-kb": { type: "string", default: "200" },
    "worker-dir": { type: "string", default: "crates/worker/build" },
    "ui-dir": { type: "string", default: "crates/web/dist" },
  },
});

const bundles = [
  { name: "worker", dir: values["worker-dir"], budgetKb: Number(values["worker-budget-kb"]) },
  { name: "ui", dir: values["ui-dir"], budgetKb: Number(values["ui-budget-kb"]) },
];

const EXTENSIONS = [".wasm", ".js", ".mjs", ".css"];

function files(dir) {
  return readdirSync(dir, { recursive: true })
    .map((name) => join(dir, name))
    .filter((path) => EXTENSIONS.some((ext) => path.endsWith(ext)) && statSync(path).isFile());
}

let exitCode = 0;
for (const { name, dir, budgetKb } of bundles) {
  let found;
  try {
    found = files(dir);
  } catch {
    found = [];
  }
  if (found.length === 0) {
    console.error(`${name}: no .wasm, .js, .mjs, or .css files in ${dir}`);
    process.exit(2);
  }
  const bytes = found.reduce((sum, path) => sum + gzipSync(readFileSync(path), { level: 9 }).length, 0);
  const kb = bytes / 1024;
  const verdict = kb > budgetKb ? "over budget" : "ok";
  if (kb > budgetKb) exitCode = 1;
  console.log(`${name}: ${kb.toFixed(1)} KB gzipped, budget ${budgetKb} KB, ${verdict}`);
}
process.exit(exitCode);
