// Writes the two fixtures too large to type by hand: hostile-headers.eml and big-body.eml.
// big-body.eml is 2 MB and git ignores it, so run this before the big-body lane.
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dir = dirname(fileURLToPath(import.meta.url));

const common = (reason, listId, sender) =>
  [
    "Return-Path: <notifications@github.com>",
    "From: Maintainer <notifications@github.com>",
    "To: Karthik <radar@karthik.nz>",
    "Message-ID: <fixture@github.com>",
    "Subject: [octo/radar-demo] Fixture",
    "Date: Sat, 07 Nov 2026 10:00:00 +0000",
    "MIME-Version: 1.0",
    "Content-Type: text/plain; charset=UTF-8",
    `List-ID: ${listId}`,
    `X-GitHub-Sender: ${sender}`,
    `X-GitHub-Reason: ${reason}`,
  ].join("\r\n");

const hostile = `${common("mention".padEnd(10240, "x"), "../../etc <x>", "bad name with spaces éè")}\r\n\r\nHostile headers.\r\n`;
writeFileSync(join(dir, "hostile-headers.eml"), hostile);

const line = "RADAR-FILLER-LINE ".repeat(4).trim();
const lines = [];
let size = 0;
while (size < 2 * 1024 * 1024) {
  lines.push(line);
  size += line.length + 2;
}
lines[Math.floor(lines.length / 2)] = "RADAR-BODY-MARKER-7f3a";
const big = `${common("mention", "octo/radar-demo <radar-demo.octo.github.com>", "maintainer")}\r\n\r\n${lines.join("\r\n")}\r\n`;
writeFileSync(join(dir, "big-body.eml"), big);
