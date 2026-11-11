const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { stripTypeScriptTypes } = require("node:module");
const { pathToFileURL } = require("node:url");

async function main() {
  const root = "/opt/npm/node_modules/@supabase/mcp-server-supabase";
  const metadata = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
  assert.equal(metadata.version, "0.15.0");
  const image = process.env.CAPTURE_TOOLS_IMAGE_ID;
  assert.match(image, /^sha256:[0-9a-f]{64}$/);
  const distribution = path.join(root, "dist");
  const candidates = fs.readdirSync(distribution)
    .filter((name) => name.endsWith(".js.map"))
    .map((name) => ({ name, bytes: fs.readFileSync(path.join(distribution, name)) }))
    .map((entry) => ({ ...entry, map: JSON.parse(entry.bytes) }))
    .filter((entry) => entry.map.sources.includes("../src/pg-meta/index.ts"));
  assert.equal(candidates.length, 1);
  const selected = candidates[0];
  const sources = new Map(selected.map.sources.map(
    (name, index) => [name, selected.map.sourcesContent[index]]
  ));
  const source = sources.get("../src/pg-meta/index.ts")
    .replace(/^import .*;$/gm, "")
    .replace(/\bexport\s+/g, "");
  const context = {
    stripIndent: require("/opt/npm/node_modules/common-tags").stripIndent,
    tablesSql: sources.get("../src/pg-meta/tables.sql"),
    columnsSql: sources.get("../src/pg-meta/columns.sql"),
    extensionsSql: sources.get("../src/pg-meta/extensions.sql"),
  };
  vm.createContext(context);
  vm.runInContext(
    stripTypeScriptTypes(source, { mode: "strip" }) +
      "\nthis.probe = {tables: listTablesSql, extensions: listExtensionsSql};",
    context
  );
  const runtimePath = path.join(distribution, selected.name.slice(0, -4));
  const runtime = fs.readFileSync(runtimePath, "utf8");
  const tableOffset = runtime.indexOf("where schema in (");
  assert.ok(tableOffset >= 0);
  const tableStart = runtime.lastIndexOf("function ", tableOffset);
  const tableFunction = /^function\s+([\w$]+)/.exec(runtime.slice(tableStart))[1];
  const extensionFunction = /function\s+([\w$]+)\(\)\{return\s+[\w$]+\}/
    .exec(runtime.slice(tableOffset))[1];
  const instrumented = path.join(distribution, `query-contract-probe-${process.pid}.mjs`);
  fs.writeFileSync(instrumented, runtime +
    `\nexport {${tableFunction} as probeTables, ${extensionFunction} as probeExtensions};\n`);
  let compiled;
  try {
    compiled = await import(pathToFileURL(instrumented).href);
    for (const schemas of [[], ["public"], ["public", "auth"]]) {
      assert.equal(
        JSON.stringify(compiled.probeTables(schemas)),
        JSON.stringify(context.probe.tables(schemas))
      );
    }
    assert.equal(compiled.probeExtensions(), context.probe.extensions());
  } finally {
    fs.unlinkSync(instrumented);
  }
  const included = compiled.probeTables(["public"]);
  const excluded = compiled.probeTables([]);
  const includeSuffix = "where schema in ($1)";
  assert.ok(included.query.endsWith(includeSuffix));
  const prefix = included.query.slice(0, -includeSuffix.length);
  const placeholders = excluded.parameters.map((_, index) => `$${index + 1}`).join(", ");
  assert.equal(excluded.query, prefix + `where schema not in (${placeholders})`);
  const digest = (bytes) => "sha256:" + crypto.createHash("sha256").update(bytes).digest("hex");
  process.stdout.write(JSON.stringify({
    schema_version: 1,
    scope: "pinned_native_read_query_contracts_not_general_sql_or_runtime_wire_proof",
    package: metadata.name,
    version: metadata.version,
    tools_image_id: image,
    runtime_sha256: digest(runtime),
    source_map_sha256: digest(selected.bytes),
    generator_sha256: digest(fs.readFileSync(__filename)),
    compiled_and_source_generators_match: true,
    tables_prefix: prefix,
    system_schemas: excluded.parameters,
    extensions_query: compiled.probeExtensions(),
  }, null, 2) + "\n");
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
