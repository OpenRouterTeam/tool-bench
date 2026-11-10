import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { argocdRequest, callSpec, callSync, specFailure, specTools } from "./spec.mjs";

const packageFile = "/opt/npm/node_modules/argocd-mcp/package.json";
const nativeRequire = createRequire(packageFile);
const sdk = async (path) => import(pathToFileURL(
  nativeRequire.resolve(`@modelcontextprotocol/sdk/${path}`).replace("/dist/cjs/", "/dist/esm/"),
));
const [{ Client }, { StdioClientTransport }, { Server }, { StdioServerTransport }, types] = await Promise.all([
  sdk("client/index.js"), sdk("client/stdio.js"), sdk("server/index.js"),
  sdk("server/stdio.js"), sdk("types.js"),
]);
if (process.argv.length !== 2) throw new Error("The Application spec companion accepts environment configuration only");
const nativePackage = nativeRequire(packageFile);
if (nativePackage.version !== "0.9.0") throw new Error("Unsupported upstream Argo CD client version");
const request = argocdRequest({
  origin: process.env.ARGOCD_BASE_URL,
  token: process.env.ARGOCD_API_TOKEN,
});
const syncRequest = argocdRequest({
  origin: process.env.ARGOCD_BASE_URL,
  token: process.env.ARGOCD_API_TOKEN,
  operation: "sync",
});
const environment = Object.fromEntries([
  "PATH", "HOME", "NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE", "SSL_CERT_DIR",
  "ARGOCD_BASE_URL", "ARGOCD_API_TOKEN", "ARGOCD_TOKEN_REGISTRY_PATH", "MCP_READ_ONLY",
].filter((name) => process.env[name] !== undefined).map((name) => [name, process.env[name]]));
const binary = typeof nativePackage.bin === "string"
  ? nativePackage.bin : nativePackage.bin?.["argocd-mcp"];
if (typeof binary !== "string") throw new Error("The pinned Argo CD client has no executable");
const upstream = new Client({ name: "argocd-spec-proxy", version: "1.0.0" });
await upstream.connect(new StdioClientTransport({
  command: process.execPath,
  args: ["--import", fileURLToPath(new URL("./preload.mjs", import.meta.url)), join(dirname(packageFile), binary), "stdio"],
  cwd: dirname(packageFile),
  env: environment,
  stderr: "inherit",
}));
const nativeTools = await upstream.listTools();
if (nativeTools.nextCursor || nativeTools.tools.length !== 16
  || specTools.some((tool) => nativeTools.tools.some((native) => native.name === tool.name))) {
  await upstream.close();
  throw new Error("Unsupported upstream Argo CD tool inventory or read-only configuration");
}
const server = new Server(
  { name: "argocd-mcp", version: "0.9.0-operations.2" },
  { capabilities: { tools: {} } },
);
server.setRequestHandler(types.ListToolsRequestSchema, async () => ({
  tools: [...nativeTools.tools, ...specTools],
}));
server.setRequestHandler(types.CallToolRequestSchema, async (incoming, extra) => {
  if (specTools.some((tool) => tool.name === incoming.params.name) || incoming.params.name === "sync_application") {
    try {
      if (incoming.params.name === "sync_application") {
        return await callSync(incoming.params.arguments ?? {}, syncRequest, extra.signal);
      }
      return await callSpec(incoming.params.name, incoming.params.arguments ?? {}, request, extra.signal);
    } catch (error) {
      if (extra.signal.aborted) throw error;
      return specFailure(error);
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
