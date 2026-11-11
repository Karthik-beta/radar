import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("check-contrast.mjs", import.meta.url));
const style = fileURLToPath(new URL("../crates/web/style", import.meta.url));
const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });

test("the self-test passes", () => {
  const out = run("--self-test");
  assert.equal(out.status, 0, out.stdout);
});

test("the real tokens have no failing pair", () => {
  const out = run();
  assert.equal(out.status, 0, out.stdout);
  assert.match(out.stdout, /0 failing pairs/);
});

test("a token below its minimum fails the run", () => {
  const dir = mkdtempSync(join(tmpdir(), "contrast-"));
  try {
    const color = (value) => ({ $value: value });
    const tokens = {
      color: {
        $type: "color",
        bg: color("#141519"),
        surface: color("#1c1d22"),
        text: color("#e8e9ec"),
        muted: color("#8a8d96"),
        faint: color("#5d6069"),
        warm: color("#e0b062"),
        hot: color("#f07a5a"),
        red: color("#ff5a4e"),
      },
    };
    writeFileSync(join(dir, "tokens.json"), JSON.stringify(tokens));
    const out = run("--tokens", join(dir, "tokens.json"));
    assert.equal(out.status, 1);
    assert.match(out.stdout, /FAIL faint on surface/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("css that removes an outline without :focus-visible fails, and the real css passes", () => {
  const dir = mkdtempSync(join(tmpdir(), "contrast-css-"));
  try {
    writeFileSync(join(dir, "bad.css"), "button { outline: none; }\n");
    const bad = run("--css", dir);
    assert.equal(bad.status, 1);
    assert.match(bad.stdout, /bad\.css/);
    writeFileSync(join(dir, "bad.css"), "button { outline: 0; }\nbutton:focus-visible { outline: 2px solid #fff; }\n");
    assert.equal(run("--css", dir).status, 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
  assert.equal(run("--css", style).status, 0);
});
