// Compares the items in D1 with the items that should be there and prints what is missing and what is extra.
//   node scripts/compare-sync.mjs --db local --persist-to .wrangler/lane-<n> --expect generated/<kind>
//   node scripts/compare-sync.mjs --db remote --database <preview database> --source gh
// `--expect` names a stub scenario under tools/github-stub/scenarios and reads its `_expect.items`.
// `--source gh` asks GitHub through `gh search`, which only reads, for the account's items updated in the window.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const SCENARIOS = join(ROOT, "tools/github-stub/scenarios");
const DAY_MS = 864e5;
const GH_LIMIT = "1000";

export function itemKey(repo, number) {
  return `${repo}#${number}`.toLowerCase();
}

export function compare(actual, expected) {
  const have = new Set(actual);
  const want = new Set(expected);
  return {
    missing: [...want].filter((key) => !have.has(key)).sort(),
    extra: [...have].filter((key) => !want.has(key)).sort(),
  };
}

export function report({ missing, extra }) {
  return [
    ...missing.map((key) => `missing ${key}`),
    ...extra.map((key) => `extra ${key}`),
    `${missing.length} missing, ${extra.length} extra`,
  ].join("\n");
}

export function windowStart(now, days) {
  const day = new Date(now - days * DAY_MS).toISOString().slice(0, 10);
  return day;
}

export function expectedFromScenario(path) {
  const candidates = [resolve(path), join(SCENARIOS, path), join(SCENARIOS, `${path}.json`), `${resolve(path)}.json`];
  const file = candidates.find((candidate) => candidate.endsWith(".json") && existsSync(candidate));
  if (!file) throw new Error(`no scenario file for --expect ${path}`);
  const items = JSON.parse(readFileSync(file, "utf8"))?._expect?.items;
  if (!Array.isArray(items)) throw new Error(`${file} has no _expect.items`);
  return items.map((key) => key.toLowerCase());
}

export function itemsFromRows(output) {
  const parsed = JSON.parse(output);
  const rows = (Array.isArray(parsed) ? parsed : [parsed]).flatMap((entry) => entry.results ?? []);
  return rows.map((row) => itemKey(row.repo, row.number));
}

export function itemsFromGh(outputs) {
  return outputs.flatMap((output) =>
    JSON.parse(output).map((item) => itemKey(item.repository.nameWithOwner, item.number)),
  );
}

function wranglerBin() {
  const bin = join(ROOT, "node_modules/wrangler/bin/wrangler.js");
  if (!existsSync(bin)) throw new Error("wrangler is not installed, run npm ci");
  return bin;
}

function storedItems(values) {
  const common = ["d1", "execute", values.database, "--json", "--command", "SELECT repo, number FROM items"];
  const target =
    values.db === "local"
      ? ["--local", ...(values["persist-to"] ? ["--persist-to", values["persist-to"]] : [])]
      : ["--remote"];
  const out = execFileSync(process.execPath, [wranglerBin(), ...common, ...target], {
    cwd: ROOT,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return itemsFromRows(out.slice(out.indexOf("[")));
}

function githubItems(days, now = Date.now()) {
  const since = `>=${windowStart(now, days)}`;
  const searches = [
    ["search", "prs", "--author", "@me"],
    ["search", "issues", "--author", "@me"],
    ["search", "issues", "--involves", "@me", "--include-prs"],
  ];
  const outputs = searches.map((args) =>
    execFileSync(
      "gh",
      [...args, "--updated", since, "--limit", GH_LIMIT, "--json", "repository,number"],
      { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
    ),
  );
  return itemsFromGh(outputs);
}

export function parse(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      db: { type: "string" },
      "persist-to": { type: "string" },
      database: { type: "string", default: "radar" },
      expect: { type: "string" },
      source: { type: "string" },
      days: { type: "string", default: "90" },
    },
  });
  if (values.db !== "local" && values.db !== "remote") throw new Error("--db is local or remote");
  if (values.db === "remote" && values.database === "radar") {
    throw new Error("--db remote needs --database <preview database>");
  }
  if (Boolean(values.expect) === Boolean(values.source)) throw new Error("give exactly one of --expect and --source");
  if (values.source && values.source !== "gh") throw new Error("--source is gh");
  if (!Number.isInteger(Number(values.days)) || Number(values.days) < 1) throw new Error("--days is a whole number");
  return values;
}

function main() {
  let values;
  try {
    values = parse(process.argv.slice(2));
  } catch (err) {
    console.error(err.message);
    process.exit(2);
  }
  const expected = values.expect ? expectedFromScenario(values.expect) : githubItems(Number(values.days));
  const result = compare(storedItems(values), expected);
  console.log(report(result));
  process.exit(result.missing.length || result.extra.length ? 1 : 0);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
