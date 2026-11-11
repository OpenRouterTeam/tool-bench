import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { callPublication, publicationTools, shopifyRequest } from "./publications.mjs";

test("publication tools use real GraphQL operations and returned identities", async () => {
  const calls = [];
  const request = async (query, variables, signal) => {
    calls.push({ query, variables, signal });
    return {
      publications: { nodes: [], pageInfo: { hasNextPage: false } },
      publishablePublish: { userErrors: [], publishable: { id: variables.id, resourcePublications: { nodes: [] } } },
    };
  };
  await callPublication("get-publications", {}, request);
  assert.deepEqual(calls[0].variables, { first: 100 });
  await callPublication("get-publications", { first: 1, after: "cursor" }, request);
  assert.deepEqual(calls[1].variables, { first: 1, after: "cursor" });
  await callPublication("publish-product", {
    id: "gid://shopify/Product/42", publicationIds: ["gid://shopify/Publication/9"],
  }, request);
  assert.match(calls[2].query, /publishablePublish/);
  assert.deepEqual(calls[2].variables, {
    id: "gid://shopify/Product/42", input: [{ publicationId: "gid://shopify/Publication/9" }],
  });
});

test("bad, scheduled, extra and missing arguments are not successful no-ops", async () => {
  const request = async () => { throw new Error("must not dispatch"); };
  for (const [name, args] of [
    ["get-publications", { first: 0 }],
    ["get-publications", { unknown: true }],
    ["publish-product", { id: "42", publicationIds: ["gid://shopify/Publication/9"] }],
    ["publish-product", { id: "gid://shopify/Product/42", publicationIds: [] }],
    ["publish-product", { id: "gid://shopify/Product/42", publicationIds: ["gid://shopify/Product/9"] }],
    ["publish-product", { id: "gid://shopify/Product/42", publicationIds: ["gid://shopify/Publication/9"], publishDate: "future" }],
  ]) {
    await assert.rejects(callPublication(name, args, request), /Invalid/);
  }
});

test("HTTP authentication, API version, cancellation and GraphQL errors remain explicit", async () => {
  let captured;
  const request = shopifyRequest({
    accessToken: "synthetic-token", domain: "example.myshopify.com",
    fetchImpl: async (url, options) => {
      captured = { url, options };
      return { ok: true, json: async () => ({ data: { publications: { nodes: [] } } }) };
    },
  });
  const controller = new AbortController();
  await request("query", { first: 1 }, controller.signal);
  assert.equal(captured.url, "https://example.myshopify.com/admin/api/2026-01/graphql.json");
  assert.equal(captured.options.headers["X-Shopify-Access-Token"], "synthetic-token");
  assert.deepEqual(JSON.parse(captured.options.body), { query: "query", variables: { first: 1 } });
  controller.abort();
  assert.equal(captured.options.signal.aborted, true);
  const failed = shopifyRequest({
    accessToken: "synthetic-token", domain: "example.myshopify.com",
    fetchImpl: async () => ({ ok: true, json: async () => ({ errors: [{ message: "invalid" }] }) }),
  });
  await assert.rejects(failed("query", {}), /GraphQL error/);
});

test("business user errors are returned and marked unsuccessful", async () => {
  const result = await callPublication("publish-product", {
    id: "gid://shopify/Product/42", publicationIds: ["gid://shopify/Publication/9"],
  }, async () => ({ publishablePublish: { userErrors: [{ message: "not found" }] } }));
  assert.equal(result.isError, true);
  assert.match(result.content[0].text, /not found/);
});

test("incomplete or cross-resource confirmations are not publication success", async () => {
  for (const data of [{}, { publishablePublish: { userErrors: [] } }, {
    publishablePublish: { userErrors: [], publishable: { id: "gid://shopify/Product/99", resourcePublications: { nodes: [] } } },
  }]) {
    await assert.rejects(callPublication("publish-product", {
      id: "gid://shopify/Product/42", publicationIds: ["gid://shopify/Publication/9"],
    }, async () => data), /Incomplete/);
  }
  await assert.rejects(callPublication("get-publications", {}, async () => ({
    publications: { nodes: [] },
  })), /Incomplete/);
});

test("stdio extension retains all upstream tool schemas and rejects malformed original calls", async () => {
  const require = createRequire("/opt/npm/node_modules/shopify-mcp/package.json");
  const sdk = async (path) => import(pathToFileURL(
    require.resolve(`@modelcontextprotocol/sdk/${path}`).replace("/dist/cjs/", "/dist/esm/"),
  ));
  const [{ Client }, { StdioClientTransport }] = await Promise.all([
    sdk("client/index.js"), sdk("client/stdio.js"),
  ]);
  const client = new Client({ name: "publication-extension-test", version: "1.0" });
  try {
    await client.connect(new StdioClientTransport({
      command: process.execPath,
      args: [new URL("./index.mjs", import.meta.url).pathname, "--accessToken=synthetic-token", "--domain=example.myshopify.com"],
      env: process.env, stderr: "inherit",
    }));
    const response = await client.listTools();
    const snapshot = JSON.parse(await readFile("/snapshot.json", "utf8"));
    const originalTools = snapshot.tools.filter((tool) => !publicationTools.some((added) => added.name === tool.name));
    assert.equal(originalTools.length, 14);
    assert.equal(response.tools.length, originalTools.length + publicationTools.length);
    assert.deepEqual(response.tools.filter((tool) => originalTools.some((original) => original.name === tool.name)), originalTools);
    assert.equal(client.getServerVersion().version, "1.0.8-publications.1");
    let rejected = false;
    try {
      const result = await client.callTool({ name: "get-products", arguments: { limit: "invalid" } });
      rejected = result.isError === true;
    } catch {
      rejected = true;
    }
    assert.equal(rejected, true);
  } finally {
    await client.close();
  }
});
