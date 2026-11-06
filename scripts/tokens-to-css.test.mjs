import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, test } from "node:test";
import { fileURLToPath } from "node:url";
import { collect } from "./lib/tokens.mjs";
import { render } from "./tokens-to-css.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const script = join(root, "scripts/tokens-to-css.mjs");
const tokensPath = join(root, "design/tokens.json");
const tokensCss = join(root, "crates/web/style/tokens.css");
const tree = JSON.parse(readFileSync(tokensPath, "utf8"));
const tokens = collect(tree);
const temps = [];

function temp() {
  const dir = mkdtempSync(join(tmpdir(), "tokens-to-css-"));
  temps.push(dir);
  return dir;
}

function run(...args) {
  return spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });
}

function filesUnder(dir, extension) {
  return readdirSync(join(root, dir), { recursive: true })
    .filter((name) => name.endsWith(extension))
    .map((name) => join(root, dir, name));
}

function declarations(css) {
  return [...css.matchAll(/^\s*(--[^:\s]+):\s*([^;]+);$/gm)].map(([, name, value]) => [name, value]);
}

afterEach(() => {
  while (temps.length) rmSync(temps.pop(), { recursive: true, force: true });
});

const namespaceOf = (token) => {
  const [group, sub] = token.path;
  if (group === "font") return { family: "font", weight: "font-weight", size: "text" }[sub];
  if (group === "motion") return { duration: "duration", easing: "ease" }[sub];
  return { color: "color", space: "spacing", radius: "radius", breakpoint: "breakpoint" }[group];
};

const tokenName = (token) => {
  const skip = ["font", "motion"].includes(token.path[0]) ? 2 : 1;
  return `--${namespaceOf(token)}-${token.path.slice(skip).join("-")}`;
};

test("every_token_becomes_a_custom_property", () => {
  const out = join(temp(), "tokens.css");
  assert.equal(run("--out", out).status, 0);
  const written = new Map(declarations(readFileSync(out, "utf8")));
  assert.equal(written.size, tokens.length + 1);
  for (const token of tokens) assert.ok(written.has(tokenName(token)), tokenName(token));
  assert.equal(written.get("--*"), "initial");
});

test("output_is_byte_stable", () => {
  const dir = temp();
  const first = join(dir, "a.css");
  const second = join(dir, "b.css");
  run("--out", first);
  run("--out", second);
  assert.equal(readFileSync(first, "utf8"), readFileSync(second, "utf8"));
  const reversed = JSON.parse(JSON.stringify(tree), function reorder(_key, value) {
    return value && typeof value === "object" && !Array.isArray(value)
      ? Object.fromEntries(Object.entries(value).reverse())
      : value;
  });
  assert.equal(render(reversed), render(tree));
  assert.equal(readFileSync(tokensCss, "utf8"), render(tree));
});

test("check_flags_a_stale_file", () => {
  const out = join(temp(), "tokens.css");
  assert.equal(run("--check", "--out", out).status, 1);
  run("--out", out);
  assert.equal(run("--check", "--out", out).status, 0);
  writeFileSync(out, readFileSync(out, "utf8").replace("#141519", "#000000"));
  const stale = run("--check", "--out", out);
  assert.equal(stale.status, 1);
  assert.match(stale.stderr, /stale/);
});

test("unknown_type_exits_nonzero", () => {
  const dir = temp();
  const bad = join(dir, "tokens.json");
  writeFileSync(bad, JSON.stringify({ shadow: { $type: "shadow", soft: { $value: "0 0 1px black" } } }));
  const result = run("--tokens", bad, "--out", join(dir, "out.css"));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /shadow/);
});

test("no_hex_literal_outside_tokens_css", () => {
  const hex = /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b/;
  const files = [
    ...filesUnder("crates/web/style", ".css").filter((file) => file !== tokensCss),
    ...filesUnder("crates/web/src", ".rs"),
  ];
  assert.ok(files.length >= 4);
  for (const file of files) assert.doesNotMatch(readFileSync(file, "utf8"), hex, file);
});

test("no_arbitrary_or_off_token_class_in_rust_sources", () => {
  const names = (prefix) => tokens.filter((t) => tokenName(t).startsWith(`--${prefix}-`)).map((t) => t.path.at(-1));
  const colors = names("color");
  const spacing = names("spacing");
  const sizeKeywords = ["auto", "full", "screen", "min", "max", "fit", "0"];
  const paints = ["transparent", "current", "inherit", "none"];
  const sides = ["t", "b", "l", "r", "x", "y", "s", "e"];
  const customProperties = new Set([
    ...tokens.map(tokenName),
    ...[...readFileSync(join(root, "crates/web/style/base.css"), "utf8").matchAll(/(--[a-z-]+):/g)].map((m) => m[1]),
  ]);
  const valued = [
    ["gap-x", [...spacing, "0"]],
    ["gap-y", [...spacing, "0"]],
    ["space-x", spacing],
    ["space-y", spacing],
    ["min-w", [...spacing, ...sizeKeywords]],
    ["max-w", [...spacing, ...sizeKeywords]],
    ["min-h", [...spacing, ...sizeKeywords]],
    ["max-h", [...spacing, ...sizeKeywords]],
    ...["gap", "px", "py", "pt", "pr", "pb", "pl", "p", "mx", "my", "mt", "mr", "mb", "ml", "m", "inset", "top", "right", "bottom", "left", "w", "h", "size", "basis"].map(
      (u) => [u, [...spacing, ...sizeKeywords]],
    ),
    ["text", [...colors, ...names("text"), "left", "center", "right", "start", "end", "justify", "nowrap", "wrap", "balance", "pretty", "clip", "ellipsis"]],
    ["font", [...names("font"), ...names("font-weight")]],
    ["bg", [...colors, ...paints]],
    ["border", [...colors, ...paints, ...sides, "solid", "dashed", "dotted"]],
    ...sides.map((s) => [`border-${s}`, [...colors, ...paints]]),
    ...["outline", "ring", "fill", "stroke", "divide", "decoration", "accent", "caret", "shadow"].map((u) => [u, [...colors, ...paints]]),
    ["rounded", [...names("radius"), "none", "full"]],
    ["ease", names("ease")],
    ["duration", names("duration")],
    ["leading", ["none"]],
    ["tracking", []],
  ].sort(([a], [b]) => b.length - a.length);

  function problem(candidate) {
    const utility = candidate.split(":").at(-1).replace(/^!/, "").replace(/^-/, "");
    if (/[\[\]]/.test(utility)) return "arbitrary value";
    const property = utility.match(/\((--[^)]+)\)$/);
    if (property) return customProperties.has(property[1]) ? null : `unknown custom property ${property[1]}`;
    if (utility === "rounded") return "rounded without a radius token";
    for (const [prefix, allowed] of valued) {
      if (utility.startsWith(`${prefix}-`)) {
        const value = utility.slice(prefix.length + 1);
        return allowed.includes(value) ? null : `${value} is not a token for ${prefix}`;
      }
    }
    return null;
  }

  for (const bad of ["bg-[#123456]", "p-[13px]", "text-sm", "rounded-lg", "p-13", "bg-gray-500", "font-sans", "tracking-tight", "hover:w-[3px]", "px-(--nope)", "rounded"]) {
    assert.ok(problem(bad), `${bad} should be rejected`);
  }
  for (const good of ["bg-surface", "text-muted", "px-3", "-m-1", "px-(--gutter)", "phone-down:w-full", "rounded-md", "text-body", "font-semibold", "flex", "border-b", "border-line"]) {
    assert.equal(problem(good), null, `${good} should pass`);
  }

  const classes = [];
  for (const file of filesUnder("crates/web/src", ".rs")) {
    const source = readFileSync(file, "utf8").split("#[cfg(test)]")[0];
    for (const match of source.matchAll(/\bclass\s*=\s*/g)) {
      let rest = source.slice(match.index + match[0].length);
      if (!rest.startsWith('"')) {
        const open = rest.indexOf("{");
        let depth = 0;
        let end = open;
        for (; end < rest.length; end++) {
          if (rest[end] === "{") depth++;
          if (rest[end] === "}" && --depth === 0) break;
        }
        rest = rest.slice(open, end);
      }
      for (const [, literal] of rest.matchAll(/"([^"]*)"/g)) {
        classes.push(...literal.split(/\s+/).filter(Boolean).map((c) => [file, c]));
        if (rest.startsWith('"')) break;
      }
    }
  }
  assert.ok(classes.length > 10, "found the class strings");
  for (const [file, candidate] of classes) assert.equal(problem(candidate), null, `${candidate} in ${file}`);
});

test("every_tailwind_default_is_cleared", () => {
  const css = readFileSync(tokensCss, "utf8");
  assert.equal(css.match(/@theme\b/g).length, 1);
  const inside = css.slice(css.indexOf("{") + 1);
  assert.equal(inside.trim().split("\n")[0].trim(), "--*: initial;");
  const known = new Set(tokens.map(tokenName));
  for (const [name] of declarations(css)) {
    if (name !== "--*") assert.ok(known.has(name), `${name} is not a token`);
  }
  assert.match(readFileSync(join(root, "crates/web/style/app.css"), "utf8"), /^@import "tailwindcss";$/m);
});

test("each_group_uses_its_tailwind_namespace", () => {
  const expected = {
    color: "--color-",
    "font.family": "--font-",
    "font.weight": "--font-weight-",
    "font.size": "--text-",
    space: "--spacing-",
    radius: "--radius-",
    "motion.duration": "--duration-",
    "motion.easing": "--ease-",
    breakpoint: "--breakpoint-",
  };
  const css = new Map(declarations(readFileSync(tokensCss, "utf8")));
  for (const token of tokens) {
    const group = token.path.slice(0, ["font", "motion"].includes(token.path[0]) ? 2 : 1).join(".");
    assert.ok(expected[group], `unmapped group ${group}`);
    assert.equal(tokenName(token), `${expected[group]}${token.path.at(-1)}`);
    assert.ok(css.has(tokenName(token)), tokenName(token));
  }
});

test("breakpoint_matches_the_base_css_media_query", () => {
  const phone = tree.breakpoint.phone.$value;
  assert.equal(phone.unit, "px");
  const base = readFileSync(join(root, "crates/web/style/base.css"), "utf8");
  const queries = [...base.matchAll(/@media \(max-width: (\d+(?:\.\d+)?)px\)/g)].map((m) => Number(m[1]));
  assert.deepEqual(queries, [phone.value]);
  assert.match(readFileSync(join(root, "crates/web/style/app.css"), "utf8"), /theme\(--breakpoint-phone\)/);
});
