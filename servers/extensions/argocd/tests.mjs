import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";
import { ArgoRequestError, argocdRequest, callSpec, callSync, specFailure, specTools } from "./spec.mjs";
import { checkedNativeFetch, preserveUnrepresented } from "./native-fetch.mjs";

const desired = {
  project: "billing",
  source: { repoURL: "https://github.com/example/deploy.git", path: "production", targetRevision: "v1.42.0" },
  destination: { server: "https://production.internal", namespace: "billing" },
  syncPolicy: { automated: { prune: true, selfHeal: true } },
};
const input = { applicationName: "billing-worker", applicationNamespace: "apps", spec: desired };

test("the advertised spec operation preserves metadata by using the actual spec-only wire binding", async () => {
  let sent;
  const request = argocdRequest({
    origin: "https://argo.example.test/control/",
    token: "local-fixture-credential",
    fetchImpl: async (endpoint, options) => {
      sent = { endpoint, options };
      return Response.json(desired);
    },
  });
  const result = await callSpec("update_application_spec", input, request);
  assert.equal(sent.endpoint.href, "https://argo.example.test/control/api/v1/applications/billing-worker/spec?appNamespace=apps");
  assert.equal(sent.options.method, "PUT");
  assert.equal(sent.options.headers.Authorization, "Bearer local-fixture-credential");
  assert.equal(sent.options.redirect, "error");
  assert.deepEqual(JSON.parse(sent.options.body), desired);
  assert.equal(JSON.parse(sent.options.body).metadata, undefined);
  assert.deepEqual(result.structuredContent.spec, desired);
  assert.deepEqual(JSON.parse(result.content[0].text), desired);
  assert.equal(result.isError, undefined);
  assert.equal(specTools.length, 1);
  assert.equal(specTools[0].name, "update_application_spec");
});

test("omitting the Application namespace leaves the server's default namespace policy intact", async () => {
  let endpoint;
  const request = argocdRequest({
    origin: "https://argo.example.test", token: "fixture",
    fetchImpl: async (url) => { endpoint = url; return Response.json(desired); },
  });
  await request({ applicationName: "billing-worker", spec: desired });
  assert.equal(endpoint.search, "");
});

test("equivalent default origins are accepted but credentials never follow an unregistered override", async () => {
  let calls = 0;
  const request = argocdRequest({
    origin: "https://ARGO.EXAMPLE.TEST:443/", token: "fixture",
    fetchImpl: async () => { calls += 1; return Response.json(desired); },
  });
  await request({ ...input, argocdBaseUrl: "https://argo.example.test" });
  for (const argocdBaseUrl of [
    "https://another.example.test", "https://argo.example.test/another",
    "http://argo.example.test", "https://user:pass@argo.example.test",
    "https://argo.example.test?credential=ignored", null,
  ]) {
    await assert.rejects(request({ ...input, argocdBaseUrl }));
  }
  assert.equal(calls, 1);
});

test("invalid names, namespaces, shapes and unadvertised credential arguments never reach the API", async () => {
  const request = argocdRequest({
    origin: "https://argo.example.test", token: "fixture",
    fetchImpl: async () => { assert.fail("invalid arguments reached the API"); },
  });
  for (const args of [
    null, [], {}, { ...input, applicationName: "../different" },
    { ...input, applicationName: "" }, { ...input, applicationName: "worker?query" },
    { ...input, applicationNamespace: "" }, { ...input, applicationNamespace: "apps/other" },
    { ...input, spec: null }, { ...input, spec: [] }, { ...input, token: "unadvertised" },
  ]) {
    await assert.rejects(request(args), /Invalid Application spec-update arguments/);
  }
  assert.throws(() => argocdRequest({ origin: "file:///etc/passwd", token: "fixture" }));
  assert.throws(() => argocdRequest({ origin: "https://argo.example.test", token: "line\nbreak" }));
});

test("HTTP errors retain structured status/code and redact the configured credential", async () => {
  const request = argocdRequest({
    origin: "https://argo.example.test", token: "private-fixture-token",
    fetchImpl: async () => Response.json(
      { code: 7, message: "permission denied private-fixture-token" }, { status: 403 },
    ),
  });
  const error = await request(input).catch((failure) => failure);
  assert.ok(error instanceof ArgoRequestError);
  const result = specFailure(error);
  assert.equal(result.isError, true);
  assert.deepEqual(result.structuredContent, { status: 403, code: 7 });
  assert.match(result.content[0].text, /permission denied \[redacted\]/);
  assert.doesNotMatch(JSON.stringify(result), /private-fixture-token/);
});

test("non-JSON and incomplete successful responses cannot become successful empty specs", async () => {
  for (const response of [
    new Response("unexpected upstream response", { status: 502 }),
    Response.json({}), Response.json({ code: 7, message: "error body" }),
  ]) {
    const request = argocdRequest({
      origin: "https://argo.example.test", token: "fixture",
      fetchImpl: async () => response,
    });
    const error = await request(input).catch((failure) => failure);
    assert.ok(error instanceof ArgoRequestError);
    assert.equal(specFailure(error).isError, true);
    assert.match(error.message, /reconcile/i);
  }
});

test("cancellation reaches the new HTTP operation without being converted into success", async () => {
  const controller = new AbortController();
  let observed;
  const request = argocdRequest({
    origin: "https://argo.example.test", token: "fixture",
    fetchImpl: async (_endpoint, options) => {
      observed = options.signal;
      return new Promise((_resolve, reject) => {
        options.signal.addEventListener("abort", () => reject(options.signal.reason), { once: true });
      });
    },
  });
  const pending = callSpec("update_application_spec", input, request, controller.signal);
  controller.abort(new Error("explicit test cancellation"));
  await assert.rejects(pending, /explicit test cancellation/);
  assert.equal(observed.aborted, true);
});

test("only the named spec capability can dispatch to the new HTTP operation", async () => {
  await assert.rejects(
    callSpec("generic_http", input, async () => { assert.fail("unknown capability reached the API"); }),
    /Unknown Application spec tool/,
  );
});

test("sync uses the published protobuf wire shape and body namespace without changing the tool schema", async () => {
  let sent;
  const application = { metadata: { name: "payments-api", namespace: "payments" }, spec: desired };
  const request = argocdRequest({
    origin: "https://argo.example.test/control", token: "fixture", operation: "sync",
    fetchImpl: async (endpoint, options) => { sent = { endpoint, options }; return Response.json(application); },
  });
  const result = await callSync({
    applicationName: "payments-api", applicationNamespace: "payments",
    revision: "main", prune: true, dryRun: false, syncOptions: ["CreateNamespace=true"],
  }, request);
  assert.equal(sent.endpoint.href, "https://argo.example.test/control/api/v1/applications/payments-api/sync");
  assert.equal(sent.options.method, "POST");
  assert.deepEqual(JSON.parse(sent.options.body), {
    appNamespace: "payments", revision: "main", prune: true, dryRun: false,
    syncOptions: { items: ["CreateNamespace=true"] },
  });
  assert.deepEqual(result.structuredContent.application, application);
});

test("sync rejects invalid flags, option types and mismatched successful acknowledgments", async () => {
  const request = argocdRequest({
    origin: "https://argo.example.test", token: "fixture", operation: "sync",
    fetchImpl: async () => Response.json({ metadata: { name: "different-app" }, spec: desired }),
  });
  for (const args of [
    { applicationName: "payments-api", prune: "true" },
    { applicationName: "payments-api", dryRun: null },
    { applicationName: "payments-api", syncOptions: { items: [] } },
    { applicationName: "payments-api", syncOptions: [5] },
  ]) {
    await assert.rejects(request(args), /Invalid Application sync arguments/);
  }
  await assert.rejects(request({ applicationName: "payments-api" }), /mismatched/);
});

test("the subset update preserves unrepresentable spec data without preserving omitted representable choices", () => {
  const current = {
    ...desired, info: [{ name: "opaque", value: "${literal}" }], revisionHistoryLimit: 2,
    syncPolicy: { automated: { enabled: false, prune: true, selfHeal: true }, syncOptions: [],
      retry: { limit: 5, refresh: true, backoff: { duration: "1s", factor: 2, maxDuration: "1m" } } },
  };
  const incoming = {
    ...desired, syncPolicy: { syncOptions: [], automated: { prune: false, selfHeal: true },
      retry: { limit: 3, backoff: { duration: "2s", factor: 2, maxDuration: "1m" } } },
  };
  const preserved = preserveUnrepresented(current, incoming);
  assert.deepEqual(preserved.info, current.info);
  assert.equal(preserved.revisionHistoryLimit, 2);
  assert.equal(preserved.syncPolicy.automated.enabled, false);
  assert.equal(preserved.syncPolicy.automated.prune, false);
  assert.equal(preserved.syncPolicy.retry.refresh, true);
  const withoutAutomation = structuredClone(incoming);
  delete withoutAutomation.syncPolicy.automated;
  assert.equal(preserveUnrepresented(current, withoutAutomation).syncPolicy.automated, undefined);
});

test("the native transport checks status before error bodies can become successful tool data", async () => {
  const fetchImpl = checkedNativeFetch(async () => Response.json({
    code: 7, message: "denied private-fixture-token",
  }, { status: 403 }));
  await assert.rejects(fetchImpl("https://argo.example.test/api/v1/applications/worker", {
    headers: { Authorization: "Bearer private-fixture-token" },
  }), (error) => {
    assert.match(error.message, /HTTP 403, gRPC 7/);
    assert.doesNotMatch(error.message, /private-fixture-token/);
    return true;
  });
});

const packageFile = "/opt/npm/node_modules/argocd-mcp/package.json";
test("actual stdio composition preserves sixteen native definitions and adds the metadata-safe operation", {
  skip: !existsSync(packageFile),
  timeout: 30_000,
}, async () => {
  const nativeRequire = createRequire(packageFile);
  const sdk = async (path) => import(pathToFileURL(
    nativeRequire.resolve(`@modelcontextprotocol/sdk/${path}`).replace("/dist/cjs/", "/dist/esm/"),
  ));
  const [{ Client }, { StdioClientTransport }] = await Promise.all([
    sdk("client/index.js"), sdk("client/stdio.js"),
  ]);
  let submitted;
  let syncSubmitted;
  let legacySubmitted;
  let nativeRead;
  const metadata = {
    name: "billing-worker", namespace: "apps",
    labels: { owner: "billing" }, finalizers: ["resources-finalizer.argocd.argoproj.io"],
  };
  const api = createServer(async (request, response) => {
    if (request.headers.authorization !== "Bearer stdio-fixture") {
      response.writeHead(403, { "content-type": "application/json" });
      response.end(JSON.stringify({ code: 7, message: "permission denied" }));
      return;
    }
    if (request.url.startsWith("/api/v1/applications/missing-app")) {
      response.writeHead(404, { "content-type": "application/json" });
      response.end(JSON.stringify({ code: 5, error: "not found", message: "not found" }));
      return;
    }
    if (request.method === "PUT" && request.url === "/api/v1/applications/billing-worker") {
      let body = "";
      for await (const chunk of request) body += chunk;
      legacySubmitted = JSON.parse(body);
      response.setHeader("content-type", "application/json");
      response.end(JSON.stringify(legacySubmitted));
      return;
    }
    if (request.method === "PUT" && request.url === "/api/v1/applications/billing-worker/spec?appNamespace=apps") {
      let body = "";
      for await (const chunk of request) body += chunk;
      submitted = JSON.parse(body);
      response.setHeader("content-type", "application/json");
      response.end(JSON.stringify(submitted));
      return;
    }
    if (request.method === "POST" && request.url === "/api/v1/applications/billing-worker/sync") {
      let body = "";
      for await (const chunk of request) body += chunk;
      syncSubmitted = JSON.parse(body);
      response.setHeader("content-type", "application/json");
      if (syncSubmitted.revision === "rejected-ref") {
        response.writeHead(400);
        response.end(JSON.stringify({ code: 9, message: "operation in progress" }));
      } else {
        response.end(JSON.stringify({ metadata, spec: desired, operation: { sync: syncSubmitted } }));
      }
      return;
    }
    if (request.method === "GET") nativeRead = new URL(request.url, "http://127.0.0.1");
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify({ metadata, spec: {
      ...desired, info: [{ name: "operator", value: "preserve this" }], revisionHistoryLimit: 2,
    } }));
  });
  await new Promise((resolve) => api.listen(0, "127.0.0.1", resolve));
  const environment = {
    PATH: process.env.PATH, HOME: "/tmp/argocd-companion-fixture",
    ARGOCD_BASE_URL: `http://127.0.0.1:${api.address().port}/`,
    ARGOCD_API_TOKEN: "stdio-fixture", MCP_READ_ONLY: "false",
  };
  const nativePackage = nativeRequire(packageFile);
  const binary = typeof nativePackage.bin === "string"
    ? nativePackage.bin : nativePackage.bin["argocd-mcp"];
  const native = new Client({ name: "native-fixture", version: "1" });
  const composed = new Client({ name: "composed-fixture", version: "1" });
  try {
    await native.connect(new StdioClientTransport({
      command: process.execPath, args: [join(dirname(packageFile), binary), "stdio"],
      cwd: dirname(packageFile), env: environment, stderr: "inherit",
    }));
    await composed.connect(new StdioClientTransport({
      command: process.execPath, args: [join(dirname(fileURLToPath(import.meta.url)), "index.mjs")],
      env: environment, stderr: "inherit",
    }));
    const original = (await native.listTools()).tools;
    const tools = (await composed.listTools()).tools;
    assert.equal(original.length, 16);
    assert.equal(tools.length, 17);
    assert.deepEqual(tools.filter((tool) => tool.name !== "update_application_spec"), original);
    assert.equal(composed.getServerVersion().version, "0.9.0-operations.2");
    const legacy = await composed.callTool({
      name: "update_application", arguments: {
        applicationName: "billing-worker", application: {
          metadata: { name: "billing-worker", namespace: "apps" },
          spec: { ...desired, syncPolicy: {
            automated: { prune: true, selfHeal: true }, syncOptions: [],
            retry: { limit: 2, backoff: { duration: "1s", maxDuration: "5s", factor: 2 } },
          } },
        },
      },
    });
    assert.equal(legacy.isError, false);
    assert.deepEqual(legacySubmitted.metadata, metadata);
    assert.deepEqual(legacySubmitted.spec.info, [{ name: "operator", value: "preserve this" }]);
    assert.equal(legacySubmitted.spec.revisionHistoryLimit, 2);
    const result = await composed.callTool({ name: "update_application_spec", arguments: input });
    assert.equal(result.isError, undefined);
    assert.deepEqual(submitted, desired);
    assert.deepEqual(result.structuredContent.spec, desired);
    const read = await composed.callTool({
      name: "get_application", arguments: { applicationName: "billing-worker", applicationNamespace: "apps" },
    });
    assert.equal(read.isError, false);
    assert.equal(nativeRead.pathname, "/api/v1/applications/billing-worker");
    assert.equal(nativeRead.searchParams.get("appNamespace"), "apps");
    assert.match(read.content[0].text, /resources-finalizer\.argocd\.argoproj\.io/);
    const failedRead = await composed.callTool({
      name: "get_application", arguments: { applicationName: "missing-app", applicationNamespace: "apps" },
    });
    assert.equal(failedRead.isError, true);
    const synced = await composed.callTool({
      name: "sync_application", arguments: {
        applicationName: "billing-worker", applicationNamespace: "apps", prune: true,
        syncOptions: ["CreateNamespace=true"],
      },
    });
    assert.deepEqual(syncSubmitted, { appNamespace: "apps", prune: true, syncOptions: { items: ["CreateNamespace=true"] } });
    assert.equal(synced.isError, undefined);
    const rejected = await composed.callTool({
      name: "sync_application", arguments: {
        applicationName: "billing-worker", applicationNamespace: "apps", revision: "rejected-ref",
      },
    });
    assert.equal(rejected.isError, true);
    assert.deepEqual(rejected.structuredContent, { status: 400, code: 9 });
    const invalid = await composed.callTool({
      name: "update_application_spec", arguments: { ...input, token: "unadvertised" },
    });
    assert.equal(invalid.isError, true);
  } finally {
    await composed.close();
    await native.close();
    await new Promise((resolve, reject) => api.close((error) => error ? reject(error) : resolve()));
  }
});
