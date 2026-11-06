import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("check-dist.mjs", import.meta.url));
const temps = [];

const files = [
  "boot.js",
  "web.js",
  "web_bg.wasm",
  "manifest.webmanifest",
  "icon.svg",
  "favicon.ico",
  "apple-touch-icon.png",
  "icon-192.png",
  "icon-512.png",
  "icon-maskable-512.png",
  "fonts/fonts.css",
  "fonts/schibsted-grotesk.woff2",
  "fonts/funnel-display.woff2",
];

function dist(html = '<script type="module" src="/boot.js"></script>', omit = []) {
  const dir = mkdtempSync(join(tmpdir(), "check-dist-"));
  temps.push(dir);
  for (const name of ["index.html", ...files]) {
    if (omit.includes(name)) continue;
    mkdirSync(dirname(join(dir, name)), { recursive: true });
    writeFileSync(join(dir, name), name === "index.html" ? html : "x");
  }
  return dir;
}

const run = (dir) => spawnSync(process.execPath, [script, "--dist", dir], { encoding: "utf8" });

afterEach(() => {
  while (temps.length) rmSync(temps.pop(), { recursive: true, force: true });
});

test("complete_dist_prints_dist_ok", () => {
  const result = run(dist());
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^dist ok, 14 files\s*$/);
});

test("missing_font_fails", () => {
  const result = run(dist(undefined, ["fonts/funnel-display.woff2"]));
  assert.equal(result.status, 1);
  assert.match(result.stderr, /missing fonts\/funnel-display\.woff2/);
});

test("inline_script_fails", () => {
  const result = run(dist("<script type=\"module\">import init from './web.js'; init();</script>"));
  assert.equal(result.status, 1);
  assert.match(result.stderr, /inline <script>/);
});
