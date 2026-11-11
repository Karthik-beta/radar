#!/usr/bin/env node
// Usage: node scripts/send-webhook.mjs <port> <fixture> [--secret <value>] [--event <name>]
// Signs the fixture's raw bytes the way GitHub does and posts it to /hooks/github.
import { createHmac } from "node:crypto";
import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { pathToFileURL } from "node:url";

export function sign(secret, body) {
  return `sha256=${createHmac("sha256", secret).update(body).digest("hex")}`;
}

export function eventFor(fixture) {
  return basename(fixture).startsWith("ping") ? "ping" : "push";
}

export function parseArgs(argv) {
  const [port, fixture, ...rest] = argv;
  if (!/^\d+$/.test(port ?? "") || !fixture) {
    throw new Error("usage: send-webhook.mjs <port> <fixture> [--secret <value>] [--event <name>]");
  }
  const options = { port: Number(port), fixture, secret: process.env.WEBHOOK_SECRET ?? "", event: eventFor(fixture) };
  for (let i = 0; i < rest.length; i += 2) {
    const value = rest[i + 1];
    if (value === undefined) throw new Error(`${rest[i]} needs a value`);
    if (rest[i] === "--secret") options.secret = value;
    else if (rest[i] === "--event") options.event = value;
    else throw new Error(`unknown option ${rest[i]}`);
  }
  return options;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const body = readFileSync(options.fixture);
  const response = await fetch(`http://127.0.0.1:${options.port}/hooks/github`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-github-event": options.event,
      "x-github-delivery": crypto.randomUUID(),
      "x-hub-signature-256": sign(options.secret, body),
    },
    body,
  });
  console.log(response.status, await response.text());
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
