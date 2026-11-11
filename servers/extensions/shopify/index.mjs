import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import { callPublication, publicationTools, shopifyRequest } from "./publications.mjs";

const packageFile = "/opt/npm/node_modules/shopify-mcp/package.json";
const nativeRequire = createRequire(packageFile);
const sdk = async (path) => import(pathToFileURL(
  nativeRequire.resolve(`@modelcontextprotocol/sdk/${path}`).replace("/dist/cjs/", "/dist/esm/"),
));
const [{ Client }, { StdioClientTransport }, { Server }, { StdioServerTransport }, types] = await Promise.all([
  sdk("client/index.js"), sdk("client/stdio.js"), sdk("server/index.js"),
  sdk("server/stdio.js"), sdk("types.js"),
]);

const options = {};
for (const argument of process.argv.slice(2)) {
  const match = /^--(accessToken|domain|apiVersion)=(.*)$/.exec(argument);
  if (!match || Object.hasOwn(options, match[1])) throw new Error("Unsupported or duplicate Shopify client option");
  options[match[1]] = match[2];
}
const request = shopifyRequest(options);
const nativePackage = nativeRequire(packageFile);
if (nativePackage.version !== "1.0.8") throw new Error("Unsupported upstream Shopify client version");
const environment = { ...process.env };
delete environment.SHOPIFY_CLIENT_ID;
delete environment.SHOPIFY_CLIENT_SECRET;
const upstream = new Client({ name: "shopify-publication-proxy", version: "1.0.0" });
const transport = new StdioClientTransport({
  command: process.execPath,
  args: [join(dirname(packageFile), "dist/index.js"), ...process.argv.slice(2)],
  cwd: dirname(packageFile),
  env: environment,
  stderr: "inherit",
});
await upstream.connect(transport);
const nativeTools = await upstream.listTools();
if (nativeTools.nextCursor || nativeTools.tools.length !== 14
  || publicationTools.some((tool) => nativeTools.tools.some((native) => native.name === tool.name))) {
  await upstream.close();
  throw new Error("Unsupported upstream Shopify tool inventory");
}
const server = new Server(
  { name: "shopify", version: "1.0.8-publications.1" },
  { capabilities: { tools: {} } },
);
server.setRequestHandler(types.ListToolsRequestSchema, async () => ({
  tools: [...nativeTools.tools, ...publicationTools],
}));
server.setRequestHandler(types.CallToolRequestSchema, async (incoming, extra) => {
  if (publicationTools.some((tool) => tool.name === incoming.params.name)) {
    try {
      return await callPublication(incoming.params.name, incoming.params.arguments ?? {}, request, extra.signal);
    } catch (error) {
      if (extra.signal.aborted) throw error;
      return { isError: true, content: [{ type: "text", text: error.message }] };
    }
  }
  return upstream.callTool(incoming.params, undefined, {
    signal: extra.signal,
    onprogress: incoming.params._meta?.progressToken === undefined ? undefined : async (progress) => {
      await extra.sendNotification({
        method: "notifications/progress",
        params: { ...progress, progressToken: incoming.params._meta.progressToken },
      });
    },
  });
});
let closing = false;
async function close() {
  if (closing) return;
  closing = true;
  await server.close();
  await upstream.close();
}
server.onclose = () => { void close(); };
upstream.onclose = () => { void close(); };
process.on("SIGTERM", () => { void close(); });
process.on("SIGINT", () => { void close(); });
await server.connect(new StdioServerTransport());
