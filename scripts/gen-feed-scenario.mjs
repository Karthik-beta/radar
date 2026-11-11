// Writes the R20 feed scenarios next to the other stub scenarios, with times relative to now.
//   node scripts/gen-feed-scenario.mjs            feed-basic, feed-old, and feed-many
//   node scripts/gen-feed-scenario.mjs feed-many  one of them
// feed-many holds 100 pull requests of 20 events each, so 2,000 events.
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { build } from "../tools/github-stub/scenarios/make.mjs";

const KINDS = ["feed-basic", "feed-old", "feed-many"];
const dir = new URL("../tools/github-stub/scenarios/", import.meta.url);

const wanted = process.argv.slice(2);
const unknown = wanted.filter((kind) => !KINDS.includes(kind));
if (unknown.length > 0) {
  console.error(`unknown kind ${unknown.join(", ")}; kinds are ${KINDS.join(", ")}`);
  process.exit(2);
}
for (const kind of wanted.length > 0 ? wanted : KINDS) {
  const file = new URL(`${kind}.json`, dir);
  writeFileSync(file, `${JSON.stringify(build(kind), null, 2)}\n`);
  console.log(`${fileURLToPath(file)} events=${build(kind)._expect.events}`);
}
