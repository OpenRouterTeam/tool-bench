import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import http from "node:http";
import readline from "node:readline";

const installedPackage = JSON.parse(readFileSync("/opt/npm/node_modules/firecrawl-mcp/package.json"));
assert.equal(installedPackage.version, "3.36.0");

const monitor = { id: "actual-monitor", name: "Pricing", status: "active" };
const check = {
  id: "actual-check",
  monitorId: monitor.id,
  status: "queued",
  trigger: "manual",
  createdAt: "2026-10-29T15:00:00Z",
};
const crawl = {
  success: true,
  status: "completed",
  total: 1,
  completed: 1,
  data: [{ markdown: "Error budget: ordinary API documentation." }],
};
const cases = [
  {
    name: "map",
    tool: "firecrawl_map",
    arguments: { url: "https://docs.example.test" },
    payload: { success: true, links: [{ url: "https://docs.example.test/api/one" }] },
    accepted: true,
  },
  ...["completed", "failed", "cancelled"].map((status) => ({
    name: `crawl_${status}`,
    tool: "firecrawl_crawl",
    arguments: { url: "https://docs.example.test/api/one", limit: 1 },
    payload: { ...crawl, status },
    accepted: status === "completed",
  })),
  {
    name: "crawl_missing_identity",
    tool: "firecrawl_crawl",
    arguments: { url: "https://docs.example.test/api/one" },
    start: { success: true },
    payload: crawl,
    accepted: false,
  },
  ...["scraping", "failed", "cancelled"].map((status) => ({
    name: `status_${status}`,
    tool: "firecrawl_check_crawl_status",
    arguments: { id: "actual-crawl" },
    payload: { ...crawl, status },
    accepted: true,
  })),
  {
    name: "status_missing_state",
    tool: "firecrawl_check_crawl_status",
    arguments: { id: "actual-crawl" },
    payload: { success: true },
    accepted: false,
  },
  {
    name: "monitor_create",
    tool: "firecrawl_monitor_create",
    arguments: {
      name: "Pricing",
      page: "https://vendor.example.test/pricing",
      goal: "Alert on prices and plan limits.",
    },
    payload: { success: true, data: monitor },
    accepted: true,
  },
  {
    name: "monitor_list_error_in_business_name",
    tool: "firecrawl_monitor_list",
    arguments: {},
    payload: { success: true, data: [{ ...monitor, name: "Error budget documentation" }] },
    accepted: true,
  },
  {
    name: "monitor_get",
    tool: "firecrawl_monitor_get",
    arguments: { id: monitor.id },
    payload: { success: true, data: monitor },
    accepted: true,
  },
  {
    name: "monitor_wrong_identity",
    tool: "firecrawl_monitor_get",
    arguments: { id: "different-monitor" },
    payload: { success: true, data: monitor },
    accepted: false,
  },
  {
    name: "monitor_update",
    tool: "firecrawl_monitor_update",
    arguments: { id: monitor.id, body: { status: "paused" } },
    payload: { success: true, data: { ...monitor, status: "paused" } },
    accepted: true,
  },
  {
    name: "monitor_update_ignored_status",
    tool: "firecrawl_monitor_update",
    arguments: { id: monitor.id, body: { status: "paused" } },
    payload: { success: true, data: monitor },
    accepted: false,
  },
  {
    name: "run_queued",
    tool: "firecrawl_monitor_run",
    arguments: { id: monitor.id },
    payload: { success: true, id: check.id, data: check },
    accepted: true,
  },
  {
    name: "run_wrong_parent",
    tool: "firecrawl_monitor_run",
    arguments: { id: "different-monitor" },
    payload: { success: true, id: check.id, data: check },
    accepted: false,
  },
  {
    name: "historical_failed_checks",
    tool: "firecrawl_monitor_checks",
    arguments: { id: monitor.id, status: "failed" },
    payload: { success: true, data: [{ ...check, status: "failed" }] },
    accepted: true,
  },
  {
    name: "historical_failed_check",
    tool: "firecrawl_monitor_check",
    arguments: { id: monitor.id, checkId: check.id },
    payload: { success: true, data: { ...check, status: "failed", pages: [] } },
    accepted: true,
  },
  {
    name: "monitor_malformed_200",
    tool: "firecrawl_monitor_list",
    arguments: {},
    payload: { success: true },
    accepted: false,
  },
  {
    name: "monitor_200_error",
    tool: "firecrawl_monitor_list",
    arguments: {},
    payload: { success: false, error: "Fixture operation rejected." },
    accepted: false,
  },
];
let active;
const requests = [];
const server = http.createServer(async (request, response) => {
  let body = "";
  for await (const chunk of request) body += chunk;
  requests.push({
    case: active?.name,
    method: request.method,
    path: request.url,
    body: body ? JSON.parse(body) : null,
  });
  const result = request.method === "POST" && request.url === "/v2/crawl"
    ? active.start ?? { success: true, id: "actual-crawl", url: "http://127.0.0.1/poll/actual-crawl" }
    : active.payload;
  response.writeHead(200, { "content-type": "application/json" });
  response.end(JSON.stringify(result));
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const port = server.address().port;
const child = spawn("/opt/npm/node_modules/.bin/firecrawl-mcp", [], {
  env: {
    ...process.env,
    FIRECRAWL_API_URL: `http://127.0.0.1:${port}`,
    FIRECRAWL_API_KEY: "fc-fixture-only",
  },
  stdio: ["pipe", "pipe", "pipe"],
});
const pending = new Map();
let stderr = "";
let sequence = 0;
child.stderr.on("data", (chunk) => { stderr += chunk; });
readline.createInterface({ input: child.stdout }).on("line", (line) => {
  const response = JSON.parse(line);
  const callback = pending.get(response.id);
  if (callback) {
    pending.delete(response.id);
    callback(response);
  }
});
function send(method, params) {
  const identity = ++sequence;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(identity);
      reject(new Error(`No native response for ${method} (${active?.name})`));
    }, 15000);
    pending.set(identity, (response) => {
      clearTimeout(timer);
      resolve(response);
    });
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: identity, method, params }) + "\n");
  });
}
try {
  const initialized = await send("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "codec-fixture", version: "1" },
  });
  assert.ok(initialized.result);
  child.stdin.write(JSON.stringify({
    jsonrpc: "2.0", method: "notifications/initialized",
  }) + "\n");
  const definitions = await send("tools/list", {});
  assert.ok(Array.isArray(definitions.result?.tools));
  const results = [];
  for (const control of cases) {
    active = control;
    const response = await send("tools/call", {
      name: control.tool, arguments: control.arguments,
    });
    results.push({ ...control, response });
  }
  process.stdout.write(JSON.stringify({
    scope: "Installed native client against loopback fixtures, not a world or model campaign",
    package: installedPackage,
    node: process.versions,
    initialized,
    definitions,
    results,
    requests,
    stderr,
  }, null, 2) + "\n");
} finally {
  child.kill("SIGTERM");
  server.close();
}
