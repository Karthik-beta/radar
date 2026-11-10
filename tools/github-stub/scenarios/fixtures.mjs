// Rewrites crates/worker/tests/fixtures/enrich_20.json, enrich_edge.json, search_page.json and search_last_page.json from the stub's own
// answers, with times pinned to FIXTURE_NOW so the files only change when the scenarios do.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { answerEnrich } from "../enrich.mjs";
import { answerSearch, cursorFor } from "../search.mjs";
import { resolveTimes } from "../server.mjs";
import { build } from "./make.mjs";

const FIXTURE_NOW = Date.parse("2026-11-07T12:00:00Z");
const OUT = join(dirname(fileURLToPath(import.meta.url)), "../../../crates/worker/tests/fixtures");

export function responseFor(kind) {
  const rate = { limit: 5000, remaining: 4990, reset: "@+50m" };
  const scenario = resolveTimes({ rate, ...build(kind) }, FIXTURE_NOW);
  const variables = {};
  scenario.notifications.items.forEach((n, i) => {
    const [owner, name] = n.repository.full_name.split("/");
    variables[`o${i}`] = owner;
    variables[`n${i}`] = name;
    variables[`i${i}`] = Number(n.subject.url.split("/").pop());
  });
  return answerEnrich(scenario, variables).body;
}

const SEARCH_WINDOW_START = "2026-08-09";

export function searchResponseFor(kind, first, after) {
  const rate = { limit: 5000, remaining: 4990, reset: "@+50m" };
  const scenario = resolveTimes({ rate, ...build(kind, FIXTURE_NOW) }, FIXTURE_NOW);
  const q = `is:pr author:@me updated:>=${SEARCH_WINDOW_START}`;
  return answerSearch(scenario, { q, first, after }).body;
}

if (process.argv[1] && import.meta.url === new URL(`file:///${process.argv[1].replaceAll("\\", "/")}`).href) {
  mkdirSync(OUT, { recursive: true });
  for (const [kind, file] of [
    ["enrich-20", "enrich_20.json"],
    ["enrich-edge", "enrich_edge.json"],
  ]) {
    writeFileSync(join(OUT, file), `${JSON.stringify(responseFor(kind), null, 2)}\n`);
    console.log(`wrote ${file}`);
  }
  for (const [file, body] of [
    ["search_page.json", searchResponseFor("backfill-150", 10, null)],
    ["search_last_page.json", searchResponseFor("low-budget", 10, cursorFor(10))],
  ]) {
    writeFileSync(join(OUT, file), `${JSON.stringify(body, null, 2)}\n`);
    console.log(`wrote ${file}`);
  }
}
