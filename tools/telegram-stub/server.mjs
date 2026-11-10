// Plays the Telegram Bot API for local Radar runs. Node, no dependencies, 127.0.0.1 only.
//   node tools/telegram-stub/server.mjs --port 9003 [--fail-first <n>] [--status <code>]
// POST /bot<token>/sendMessage records the request and answers {"ok":true}.
// --fail-first answers the first n sends with 500. --status answers every send with that code.
// GET /requests lists the recorded requests. Each one is also printed as one JSON line.
import http from "node:http";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

const MAX_BODY = 100_000;
const SEND = /^\/bot[^/]+\/sendMessage$/;

export function createStub({ failFirst = 0, status = null } = {}) {
  const requests = [];
  let sends = 0;

  const server = http.createServer((req, res) => {
    const reply = (code, body) => {
      res.writeHead(code, { "content-type": "application/json" });
      res.end(JSON.stringify(body));
    };
    const path = new URL(req.url, "http://stub").pathname;
    if (req.method === "GET" && path === "/requests") return reply(200, requests);

    const chunks = [];
    let size = 0;
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size <= MAX_BODY) chunks.push(chunk);
    });
    req.on("end", () => {
      const text = Buffer.concat(chunks).toString("utf8");
      let body = null;
      try {
        body = JSON.parse(text);
      } catch {
        body = { raw: text };
      }
      const entry = { at: new Date().toISOString(), method: req.method, path, body };
      requests.push(entry);
      console.log(JSON.stringify(entry));
      if (req.method !== "POST" || !SEND.test(path)) return reply(404, { ok: false });
      sends += 1;
      if (status) return reply(status, { ok: false });
      if (sends <= failFirst) return reply(500, { ok: false });
      return reply(200, { ok: true });
    });
  });
  return { server, requests };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { values } = parseArgs({
    options: { port: { type: "string" }, "fail-first": { type: "string" }, status: { type: "string" } },
  });
  const port = Number(values.port);
  if (!Number.isInteger(port) || port < 1) {
    console.error("usage: server.mjs --port <port> [--fail-first <n>] [--status <code>]");
    process.exit(2);
  }
  const { server } = createStub({
    failFirst: Number(values["fail-first"] ?? 0),
    status: values.status ? Number(values.status) : null,
  });
  server.listen(port, "127.0.0.1", () => console.log(`telegram stub listening on 127.0.0.1:${port}`));
}
