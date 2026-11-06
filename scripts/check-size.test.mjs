import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomBytes } from "node:crypto";
import { afterEach, test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("check-size.mjs", import.meta.url));
const temps = [];

function bundleDir(files) {
  const dir = mkdtempSync(join(tmpdir(), "check-size-"));
  temps.push(dir);
  for (const [name, content] of Object.entries(files)) {
    mkdirSync(join(dir, name, ".."), { recursive: true });
    writeFileSync(join(dir, name), content);
  }
  return dir;
}

function run(workerDir, uiDir, extra = []) {
  return spawnSync(process.execPath, [script, "--worker-dir", workerDir, "--ui-dir", uiDir, ...extra], {
    encoding: "utf8",
  });
}

afterEach(() => {
  while (temps.length) rmSync(temps.pop(), { recursive: true, force: true });
});

test("under_budget_exits_0", () => {
  const result = run(bundleDir({ "index.js": "x" }), bundleDir({ "app.wasm": "y" }));
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^worker: .* ok$/m);
  assert.match(result.stdout, /^ui: .* ok$/m);
});

test("over_budget_exits_1", () => {
  const worker = bundleDir({ "nested/index.wasm": randomBytes(4096) });
  const result = run(worker, bundleDir({ "app.css": "y" }), ["--worker-budget-kb", "1"]);
  assert.equal(result.status, 1);
  assert.match(result.stdout, /^worker: .* over budget$/m);
});

test("missing_dir_exits_2", () => {
  const missing = join(tmpdir(), `check-size-missing-${process.pid}`);
  assert.equal(run(missing, bundleDir({ "app.js": "y" })).status, 2);
  assert.equal(run(bundleDir({ "readme.txt": "no bundles" }), bundleDir({ "app.js": "y" })).status, 2);
});
