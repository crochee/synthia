#!/usr/bin/env node
// Cross-language smoke test: drives the `synthia-mcp-server`
// binary from Node.js the way a foreign MCP client (the
// `pi-mcp-adapter`, a Claude Code plugin, …) does — over a JSON
// stream on stdin/stdout.
//
// This is the "give it to pi" proof reduced to the protocol
// boundary: a pi extension that wants synthia's tool surface
// spawns this binary and exchanges the JSON-RPC envelopes
// asserted below. The script does not import synthia, so it is
// genuinely cross-language.

import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { mkdtempSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const BINARY = resolve(
  __dirname,
  "../../../target/debug/synthia-mcp-server"
);

class McpClient {
  constructor(child) {
    this.child = child;
    this.nextId = 1;
    this.pending = new Map();
    const rl = createInterface({ input: child.stdout });
    rl.on("line", (line) => {
      if (!line.trim()) return;
      let msg;
      try {
        msg = JSON.parse(line);
      } catch (err) {
        console.error("[server-stdout-parse-fail]", line, err.message);
        return;
      }
      if (msg.id !== undefined) {
        const waiter = this.pending.get(msg.id);
        if (waiter) {
          this.pending.delete(msg.id);
          if (msg.error) waiter.reject(new Error(`${msg.error.code}: ${msg.error.message}`));
          else waiter.resolve(msg.result ?? msg);
        }
      }
    });
  }

  request(method, params) {
    const id = this.nextId++;
    const envelope = JSON.stringify({
      jsonrpc: "2.0",
      id,
      method,
      params,
    });
    const { promise, resolve, reject } = Promise.withResolvers();
    this.pending.set(id, { resolve, reject });
    this.child.stdin.write(envelope + "\n");
    return promise;
  }

  notify(method, params) {
    const envelope = JSON.stringify({
      jsonrpc: "2.0",
      method,
      params,
    });
    this.child.stdin.write(envelope + "\n");
  }

  close() {
    this.child.stdin.end();
  }
}

async function main() {
  const workspace = mkdtempSync(join(tmpdir(), "synthia-pi-"));
  const filePath = join(workspace, "marker.txt");
  writeFileSync(filePath, "cross-language proof\n");

  const child = spawn(BINARY, ["--workspace", workspace, "--name", "pi"], {
    stdio: ["pipe", "pipe", "pipe"],
  });
  child.stderr.on("data", (chunk) => {
    process.stderr.write(`[server-stderr] ${chunk}`);
  });

  const client = new McpClient(child);

  // 1. initialize handshake
  const init = await client.request("initialize", {
    protocolVersion: "2025-06-18",
    clientInfo: { name: "node-smoke", version: "0.0.0" },
    capabilities: {},
  });
  console.log("initialize result:", JSON.stringify(init));
  if (init.serverInfo.name !== "pi") {
    throw new Error(`expected server name "pi", got ${init.serverInfo.name}`);
  }

  client.notify("notifications/initialized", {});

  // 2. tools/list — advertise to the model
  const list = await client.request("tools/list", {});
  const names = list.tools.map((t) => t.name);
  console.log("exposed tools:", names);
  for (const expected of [
    "read",
    "write",
    "shell",
    "TodoWrite",
    "web_fetch",
    "schedule",
    "search",
    "synthia",
  ]) {
    if (!names.includes(expected)) {
      throw new Error(`missing tool \`${expected}\` in ${JSON.stringify(names)}`);
    }
  }

  // 3. tools/call read — exercise the read path
  const readResult = await client.request("tools/call", {
    name: "read",
    arguments: { file_path: filePath },
  });
  if (readResult.isError) {
    throw new Error(`read failed: ${JSON.stringify(readResult)}`);
  }
  const readText = readResult.content[0].text;
  console.log("read result text:", readText);
  if (!readText.includes("cross-language proof")) {
    throw new Error(`read did not return the file body, got: ${readText}`);
  }

  // 4. tools/call synthia — overview tool
  const synthiaResult = await client.request("tools/call", {
    name: "synthia",
    arguments: {},
  });
  const synthiaBody = JSON.parse(synthiaResult.content[0].text);
  console.log(
    "synthia overview:",
    JSON.stringify({
      version: synthiaBody.version,
      session: synthiaBody.session_id,
      tools: synthiaBody.tools,
    })
  );
  if (!Array.isArray(synthiaBody.tools) || synthiaBody.tools.length !== 8) {
    throw new Error(
      `synthia overview missing tool list, got: ${JSON.stringify(synthiaBody)}`
    );
  }

  client.close();
  await new Promise((resolve) => child.on("exit", resolve));
  console.log("OK: cross-language smoke passed (Node ↔ synthia-mcp-server)");
}

main().catch((err) => {
  console.error("FAIL:", err.message);
  process.exit(1);
});