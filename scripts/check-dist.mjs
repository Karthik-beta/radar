import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: { dist: { type: "string", default: "crates/web/dist" } },
});

const required = [
  "index.html",
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

const problems = required.filter((name) => !existsSync(join(values.dist, name))).map((name) => `missing ${name}`);

if (existsSync(join(values.dist, "index.html"))) {
  const html = readFileSync(join(values.dist, "index.html"), "utf8");
  for (const [, attributes, body] of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)) {
    if (!/\bsrc\s*=/.test(attributes) || body.trim() !== "") problems.push("index.html has an inline <script>");
  }
  if (/<style\b/i.test(html)) problems.push("index.html has an inline <style>");
}

if (problems.length > 0) {
  for (const problem of problems) console.error(`${values.dist}: ${problem}`);
  process.exit(1);
}

const count = readdirSync(values.dist, { recursive: true }).filter((name) =>
  statSync(join(values.dist, name)).isFile(),
).length;
console.log(`dist ok, ${count} files`);
