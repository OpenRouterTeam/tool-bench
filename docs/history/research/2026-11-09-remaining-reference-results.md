# Remaining operational reference results: GitHub `list_issues` and Terraform `get_apply_logs`

Research date: **November 9, 2026**. Scope: two pinned client result/HTTP contracts used by the remaining operational scenarios.

**Research only.** This Markdown file is the only file authored in the checkout. No code, data, scenario, taskgen, staging, commit or push. No paid inference, no live accounts, no Docker, no cleanup. Diagnostics ran from `/tmp/rr-gomod` (module cache, two built binaries, two Python stand-in servers on loopback). They are not in the repo and are not part of the benchmark. `apply_patch` was not available in this session, so the note was written with the file-write tool.

Scope limit: this reports **source-verified shape** of the pinned servers' output and requests. It says nothing about business correctness, native-model performance, or runtime admission of the benchmark worlds.

## 1. Bottom line

- **[P/R] The GitHub selector holds for the default configuration.** `/content/0/text` is a JSON document with `issues`, `totalCount` and `pageInfo`. `/issues` is an array whose elements carry `title` and `number`. A unique-title match to `/number` works in both protocol eras the server implements.
- **[R] It does not hold if CSV output is enabled.** `GITHUB_FEATURES=csv_output` (or insiders mode) turns `/content/0/text` into `# pageInfo...` comment lines plus CSV. It is off unless the runner or catalog turns it on, and the catalog does not. The parent should assert that.
- **[P] The parent's cited helper path does not exist at v2.0.3.** `pkg/github/internal/utils/utils.go` and `NewToolResultResponse` are absent from the v2.0.3 module zip. The helper the handler uses is `utils.NewToolResultText` in `pkg/utils/result.go`. Re-check the URL or ref that was opened.
- **[P/R] `get_apply_logs` is not one metadata read plus one object read.** The pinned client sends, in order: `GET /api/v2/ping`, two `GET /api/v2/applies/<id>`, **four or more chunked** `GET /archivist/v1/object/apply-log-<id>?limit=N&offset=M`, then a third applies read. A synthetic service must honor `limit` and `offset` and the STX/ETX framing, or the client loops, duplicates or stalls.
- **[P/R] No truncation or reformatting happens in either MCP server.** The final text is the apply `log` string byte for byte (815 ASCII bytes), after go-tfe strips the framing bytes.
- **[U] Not proven:** the real Rust world serving these requests. Both runtime checks used my own stand-ins, which copy the world's slicing logic as read from source.

## 2. Evidence and pins

Labels: **[P]** inspected pinned primary source; **[R]** observed by running the pinned code in this session; **[L]** inspected local implementation; **[I]** inference; **[U]** not established.

| Item | Pin |
| --- | --- |
| GitHub MCP server **[P/R]** | `github.com/github/github-mcp-server/v2@v2.0.3`, tag commit `b98d115bbb080e9555466a96656ec4df605aea21`, module hash `h1:hH0pFEZ3/4URrTa9w6bXfQkkTqlAPYC7fnW+2KXEa/g=`. Catalog pin: `catalog/services.toml` service `github`, args `stdio --toolsets all`. |
| GitHub MCP SDK **[P]** | `modelcontextprotocol/go-sdk v1.8.0`, commit `3f3b699b2b67e1ed033a63d6651671dab53c2d32`, hash `h1:KIvahhYqwtbeniWVPs3TcXEA7b8jEtwfBpOTAI+Urx4=`. |
| Terraform MCP server **[P/R]** | `github.com/hashicorp/terraform-mcp-server@v1.4.1`, commit `3781509a674482ca7e2821fab8159a25e3b29bdf`, hash `h1:v3Ta4sCNUFnr2XeaTB4mf+BWxWuG4Fhcczs/NCeQK2E=` (equals the checksum in the earlier Terraform note). Catalog: args `stdio --toolsets all`, `ENABLE_TF_OPERATIONS=true`. |
| Terraform HTTP client **[P]** | `hashicorp/go-tfe v1.111.2`, commit `76ed5bb13de750863b0a99c3a2a8961fa5edfb46`, hash `h1:Bzak6GbLANLsy+c335vdteJdBF7fRvMhgcM9fzKSvbs=`. |
| Terraform MCP SDKs **[P]** | `mark3labs/mcp-go v0.58.0` (default stdio) and `modelcontextprotocol/go-sdk v1.7.0` (opt-in). |
| Toolchain **[R]** | Local `go1.27.1`; `images/tools/Dockerfile` builds from `golang:1.27-bookworm`. Both binaries were built with `go build` from module-cache sources, then run over stdio with protocol `2025-06-18`. |

Sources were fetched with `go mod download` from the public Go module proxy, which serves the tagged module zips. Citations below are module-relative paths with line numbers from those zips.

## 3. GitHub v2.0.3 `list_issues`

### 3.1 What the handler does

- **[P]** `pkg/github/issues.go:3735` defines `ListIssues`. It is registered through the typed `NewTool[ListIssuesInput, ListIssuesOutput]` wrapper (`:3800`). The input normalizer is `normalizeTypedReadArguments([]string{"state","orderBy","direction"}, true)` (`:4026`).
- **[P]** Input casing: the normalizer upper-cases `state`, `orderBy` and `direction` before schema validation (`pkg/github/typed_read_normalizers.go`). The advertised enum is `OPEN`/`CLOSED`. The handler also upper-cases again and maps unknown `state` to both open and closed. The scenario's `state = "OPEN"` is the real native value. **[R]** a lowercase `open` also worked.
- **[P]** Transport: GraphQL, not REST. For `GITHUB_HOST=https://github.octo-corp.com` the GHES URL builder (`pkg/utils/api.go`, `newGHESHost`) yields `<origin>/api/graphql`. **[R]** The request was `POST /api/graphql` with header `GraphQL-Features: issue_fields, repo_issue_fields` and variables `{after:null, direction:"DESC", first:10, issueFieldValues:[], orderBy:"CREATED_AT", owner, repo, states:["OPEN"]}`. `perPage=10` becomes `first:10`; with no `perPage` the default is `first:30` (`DefaultGraphQLPageSize`, `discussions.go:21`).
- **[P]** Fallback: if that first query errors with an "unsupported issue-fields schema" message, the handler retries once without `issueFieldValues` (`issues.go` around `:3960-4000`). **[L]** the local GitHub service already has GraphQL coverage for the field-filter query (`crates/services/github/tests/api.rs:1159`).
- **[P]** Result construction (`issues.go:4012-4024`): `json.Marshal` of a `MinimalIssuesResponse` (`minimal_types.go:664`: `issues`, `totalCount`, `pageInfo`), wrapped by `utils.NewToolResultText` (`pkg/utils/result.go:5`). If the caller passes `fields`, the payload is a map with the same three keys and filtered issue objects. Typed structured output is returned alongside (`structuredListIssuesOutput`, `:3686`).
- **[P]** Per-issue fields in the text: `number`, `title`, `state`, `user.login`, `assignees` (always present, possibly `[]`), plus `body`, `labels`, `comments`, `created_at`, `updated_at` when non-empty. `issues` is built with `make(..., 0, n)`, so an empty page is `[]`, not `null` (`minimal_types.go:979`).
- **[P]** Title and body text are post-processed: the title goes through `sanitize.PlainText` (HTML-tag stripping, invisible-character and code-fence-metadata filtering); the body through `sanitize.Content` (`minimal_types.go:925-930`). Plain ASCII titles like the scenario's are unchanged. **[R]** Go's JSON encoder escapes `&`, `<`, `>` as `&` etc. in the text; that decodes to the original on parse.
- **[P]** No IFC metadata is added by default (`attachStaticIFCLabel` only fires with the `ifc_labels` feature, `ifc_labels.go:31`).

### 3.2 JSON versus other output modes

- **[P]** CSV: `AllTools` wraps every `list_*` tool in the default toolsets with `csvOutputMiddleware` (`tools.go:219`, `csv_output.go:45-86`). The wrapper only converts when feature `csv_output` is enabled. Features come from `--features`, `GITHUB_FEATURES` (viper `SetEnvPrefix("github")` + `AutomaticEnv`, `cmd/github-mcp-server/main.go:314-316`) or insiders mode (`feature_flags.go:58-64`, `InsidersFeatureFlags` includes `csv_output`). The catalog sets only `GITHUB_PERSONAL_ACCESS_TOKEN` and `GITHUB_HOST`.
- **[R]** With `GITHUB_FEATURES=csv_output` the same call returned text beginning `# pageInfo.endCursor: ...` then a CSV header `assignees,body,created_at,number,state,title,updated_at,user.login`. It is not JSON, so a `json_pointer = "/issues"` capture would fail.
- **[P]** Other mode switches that matter: none for this tool. `list_issues` has one registration (`grep '"list_issues"'` finds only `issues.go:3803`), unaffected by the `issues_granular` flag, which swaps write tools.

### 3.3 Through the MCP protocol, not only Go types

The typed wrapper post-processes results per protocol "era" (`pkg/inventory/typed_output.go`, `typed_schema.go:32`):

| Era | When | Observed result |
| --- | --- | --- |
| Legacy | Default for stdio. `requestEra` (`typed_output.go:402`) uses the request's `_meta["io.modelcontextprotocol/protocolVersion"]`, else `ghcontext.MCPMethod`, which only the HTTP middleware sets (`pkg/http/middleware/mcp_parse.go:96`). Anything below `2026-07-28` is legacy. | **[R]** `result` keys: `content` only. One text block (772 bytes for the stand-in data) holding the handler's JSON. `structuredContent` absent. `tools/list` shows no `outputSchema`. |
| Modern | Only when the call itself carries `_meta` with protocol version `2026-07-28`. **[R]** It also requires `io.modelcontextprotocol/clientCapabilities`; without it the server returns JSON-RPC error `-32602 missing or invalid _meta field "io.modelcontextprotocol/clientCapabilities"`. | **[R]** `result` keys: `_meta`, `content`, `resultType`, `structuredContent`. `applyModernStructuredText` (`typed_output.go:360`) replaces the text with the JSON of the structured output, so keys come out alphabetized and omitted when empty. Same top-level keys, same `title` and `number` fields. |

- **[R]** In both eras the capture chain works: `/content/0/text` (string) → JSON decode → `/issues` → select the element whose `title` equals the target → `/number`. The stand-in returned three issues (target, a `CHECKOUT-API-3C` look-alike, an other-repo same-title look-alike); selecting by exact title returned only the target number.
- **[L]** The benchmark reaches these stdio servers through its own HTTP bridge (ports 7101 and 7109, `crates/mcp-tools/src/bridge.rs`). The bridge forwards `tools/call` messages and replies unmodified; it only sorts `tools/list` by name (`bridge.rs:48-60`). `front.rs` and `stdio.rs` were not audited line by line, and I ran the servers directly over stdio, so bridge pass-through is **[U]** at runtime.
- **[I]** The benchmark's recorded oracle initializes at `2025-06-18` and sends no `_meta` (`crates/scenario/src/taskgen.rs:553`, `tasks/ops__multi__pagerduty-sentry-github-issue/solution/solve.sh:5`, `tasks/*/solution/call-N.json`), so it is in the legacy era. A model client that never sends the modern `_meta` is too. Keep the capture on `/content/0/text`: `/structuredContent` exists only in the modern era.

### 3.4 Objections and checks for the parent

1. Assert in source that the GitHub endpoint's env and args contain no `GITHUB_FEATURES`, `GITHUB_INSIDERS`, `--features` or `--insiders`. Today the catalog satisfies this; the runner's container environment is **[U]**.
2. The capture's selector compares the **sanitized** title. It is safe for the scenario titles. A title containing `<`, `&`, a code fence or invisible characters would not round-trip. Do not reuse this selector for such titles without checking.
3. Correct the cited path (§1, third bullet) before it is repeated in other notes.
4. The world must serve `POST /api/graphql` with the `issues(...filterBy:{issueFieldValues:$issueFieldValues})` shape and `isPrivate`. The existing service test covers the query; no test in this note covers the end-to-end captured call.

## 4. Terraform v1.4.1 `get_apply_logs`

### 4.1 Implementation chosen

- **[P]** Two implementations exist. `pkg/cli/flags.go:112-118` selects the official-SDK server only when `TF_X_OFFICIAL_SDK_ENABLED=true`; otherwise the mark3labs one runs. The catalog does not set that variable, so the benchmark uses `pkg/tools/tfe/get_apply_logs.go`.
- **[P]** Both implementations do the same thing: `Applies.Read`; reject `unreachable`; require terminal status (`errored`, `finished`, `canceled`) else return a "wait" message; `Applies.Logs`; `io.ReadAll`; return the bytes as one text content (`get_apply_logs.go:49-81`; official variant `pkg/mcp-official/tools/tfe/applies_get_logs_tool.go`). There is no size limit, truncation, trimming or JSON wrapping.
- **[P]** Client construction (`pkg/client/tfe_client.go:84-98`): `RetryServerErrors: true`, a custom `User-Agent: terraform-mcp-server/<version>`, bearer token from `TFE_TOKEN`, address from `TFE_ADDRESS`.

### 4.2 Exact go-tfe requests

- **[P]** `Applies.Read` is `GET <address>/api/v2/applies/<id>` (`apply.go:73`). The `log-read-url` attribute is the only one used for logs.
- **[P]** `Applies.Logs` (`apply.go:94`) first calls `Read` again, then returns a `LogReader` with a `done` callback that calls `Read` once more.
- **[P]** `LogReader.read` (`logreader.go:49`) **overwrites the whole query string** with `limit=<len(buffer)>&offset=<n>`, sends a plain `GET` with the client's static headers only, and reads once from the body. It therefore sends **no `Authorization` header** and drops any query parameters on the log URL.
- **[P]** Framing: a leading byte `0x02` (STX) at the start and a trailing `0x03` (ETX) at the end of a chunk are stripped and the offset advanced. After ETX, the next empty read calls `done`, which sees a terminal status and returns EOF. A log without STX/ETX framing is instead ended by empty reads plus the `reads > 1` rule, with backoff sleeps of about 0.57 s then 0.66 s (**[I]** derived from `backoff(500, 2000, n)`, not run).
- **[R]** Observed with the pinned binary, a stand-in apply (`status = errored`, `log-read-url` on the stand-in) and the dataset log:

```
GET /api/v2/ping                                             (Bearer; go-tfe client setup)
GET /api/v2/applies/apply-OttY2qKt1DNXCd1P                   (Bearer; handler Read)
GET /api/v2/applies/apply-OttY2qKt1DNXCd1P                   (Bearer; inside Logs)
GET /archivist/v1/object/apply-log-<id>?limit=512&offset=0   (no Authorization)
GET /archivist/v1/object/apply-log-<id>?limit=256&offset=512
GET /archivist/v1/object/apply-log-<id>?limit=384&offset=768
GET /archivist/v1/object/apply-log-<id>?limit=336&offset=817 (empty body; ETX already consumed)
GET /api/v2/applies/apply-OttY2qKt1DNXCd1P                   (done check)
```

  The limit values come from `io.ReadAll`'s buffer growth and may differ on other Go versions. The offsets and the byte-accounting rule do not.
- **[R]** Both stdio implementations returned `result.content = [ {type:"text"} ]`, no `isError`, no `structuredContent`, 815 characters, byte-equal to the dataset `log`, in 0.00 s.

### 4.3 What the world emits

- **[L]** Metadata: `crates/services/terraform/src/resources.rs:496-518` builds the apply resource with `log-read-url = <base>/archivist/v1/object/apply-log-<id>`, merging the stored record's attributes (the `log` field is excluded from attributes). `<base>` is `x-forwarded-proto` plus the request `Host` (`common.rs:103`, default scheme `http`).
- **[L]** Object: `runs.rs:1226-1255` (`sliced_log`) frames the text as `0x02 + text + 0x03` and honors `offset` and `limit` (offset clamped to the length, empty slice past the end). `apply_log` (`runs.rs:1060`) returns the record's `log` string verbatim when present; only otherwise does it synthesize one.
- **[L]** `archivist_routes` is merged **outside** the `require_token` layer (`lib.rs:66-70`), so the no-`Authorization` log GETs are accepted. This matches the pinned client.
- **[L]** The dataset log for `apply-OttY2qKt1DNXCd1P` is 815 bytes of ASCII with only `\n` as a control character. It is the same 815 bytes in the five scenario datasets that contain it (`payments-db-upgrade-failure`, `scale-checkout-staging`, `unlock-orders-prod`, `approve-nat-gateway-change`, `terraform-failure-grafana-annotate`). The framed object is 817 bytes. So the pinned client will need at least two reads; the observed pattern is four.

### 4.4 Objections and checks for the parent

1. The existing world test (`crates/world/tests/remaining_operational_communication_contracts.rs:279`) only issues `GET /api/v2/applies/<id>`. The existing service test (`crates/services/terraform/tests/terraform.rs:536-565`) fetches the log with `limit=100000&offset=0`. **No existing check exercises go-tfe's chunked pattern.** Suggested source-delivery check against the real service: request `limit=512&offset=0`, `offset=512`, `offset=768`, and `offset=817` and assert the first byte is `0x02`, the concatenation after stripping equals the record `log`, the last byte of the final non-empty chunk is `0x03`, and `offset=817` returns an empty body.
2. In `real-tls` mode the `log-read-url` origin must be the TLS-facing host (`https://app.terraform.io`), because the client follows the URL as given. The TLS test (`crates/world/tests/tls.rs:110-132`) proves this for state downloads only. Treat apply-log URLs in TLS mode as **[U]** until the same assertion is added.
3. Any rule that requires `Authorization` on every Terraform world request (including `synthetic_replay_headers = { authorization = ... }` replay checks) must exempt `/archivist/` reads, or the pinned client's log reads fail.
4. If the log text ever contains a byte `0x02` or `0x03`, or is edited so framing is missing, behavior changes (silent byte stripping, or a roughly 1.2 s delay). The dataset log has neither.
5. Do not describe the call as "metadata then one log fetch" in other notes. The server issues three apply reads and several chunked log reads, and its result is a single text block.

## 5. Reproduction (outside the checkout)

```
GOMODCACHE=/tmp/rr-gomod/cache GOFLAGS=-modcacherw go mod download \
  github.com/github/github-mcp-server/v2@v2.0.3 github.com/hashicorp/terraform-mcp-server@v1.4.1 \
  github.com/hashicorp/go-tfe@v1.111.2 github.com/modelcontextprotocol/go-sdk@v1.8.0
# build each ./cmd/<name> from the module-cache directory with the same env
# drive over stdio: initialize (2025-06-18) -> notifications/initialized -> tools/call
#   github:    list_issues {owner, repo, state:"OPEN", orderBy, direction, perPage:10}
#   terraform: get_apply_logs {apply_id:"apply-OttY2qKt1DNXCd1P"}
```

The GitHub stand-in returns a canned GraphQL `issues` page for every `POST /api/graphql`. The Terraform stand-in serves `/api/v2/ping`, the apply resource, and an archivist object that reimplements `sliced_log`. Neither is the benchmark world.

## 6. Not established

- **[U]** The real Rust world's response to these exact client requests (not run; building the workspace would write into a shared dirty checkout).
- **[U]** The runner container's environment for the two MCP processes (only the catalog was inspected).
- **[U]** Behavior with a client that sends the modern `_meta` fields on `tools/call` for `list_issues` against real GitHub; only the stand-in was exercised.
- **[U]** Whether the apply-log URL is HTTPS-correct behind the TLS front for this endpoint.
