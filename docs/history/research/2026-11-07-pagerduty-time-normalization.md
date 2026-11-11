# PagerDuty classic timestamp normalization — 2026-11-07

## Scope and bottom line

Research-only companion to `2026-11-07-pagerduty-operation-contract.md`. Only this file is authored. No implementation, instruction, dataset, generated task, frozen capsule, image or tag is changed. No delegation, tests/builds, dependency installation, Docker operation, account/infrastructure action, benchmark inference or evaluated-tool model call is performed here. Published source archives were fetched, hashed and inspected **in memory**, not installed or executed. The pre-existing installed-model probe is evidence supplied to this investigation, not an execution by this sidecar.

Evidence labels:

- **[O] Observed local probe:** the retained, network-disabled `Override` model experiment, not a live API call or graded MCP trial.
- **[P] Pinned published source:** exact package bytes and the immutable PagerDuty OpenAPI revision identified below; static inspection is not runtime conformance.
- **[D] Documentation:** current first-party documentation retrieved November 7, 2026; not an observation of hosted behavior.
- **[L] Local authored source:** the inspected shared-checkout snapshot, identified by hashes where important.
- **[I/U] Inference, recommendation or uncertainty:** explicitly not an executed control.

**Principal conclusion:** compare a classic creation result with the **actual pinned-model-converted HTTP request**, then independently evaluate the persisted effective coverage against the user's window. Do not equate original MCP strings with transmitted timestamps. Also do not excuse a real microsecond wrong-window by truncating backend time to seconds or broadly classifying fractional inputs as environment failures. **[O/P/L/I]**[^probe][^mcp-wheel][^pydantic-wheel][^core-source][^local-comparator][^local-time]

The authored cover task's window is **October 31, 2026, 09:00–17:00 UTC**, with scenario time October 30, 2026, 14:00 UTC. It does not become November 8 because this research is dated November 7. Requested replacement is Jordan Lee `PJ0RLEE` on classic Platform Primary `PSPLTPR`; base rotations and other schedules must remain unchanged. **[L]**[^local-task]

## 1. What the existing probe actually establishes

The script discovers the installed class with `start`, `end`, `user_id`, imports that actual class, calls `model_validate`, and records both `model_dump(mode="json")` and `start.isoformat()`. It does **not** invoke FastMCP, `create_schedule_override`, the HTTP SDK, local world routes or PagerDuty. It always uses end `2026-10-31T17:00:00Z`. **[O]**[^probe]

Recorded installed versions: `pagerduty-mcp==1.1.0`, `pagerduty==5.3.0`, `pydantic==2.14.2`, `pydantic-core==2.50.2`, `mcp==1.30.0`. The class is `pagerduty_mcp.models.schedules.Override`; its installed path is under `/opt/py/pagerduty/lib/python3.13/site-packages/`. A full interpreter patch version, installed-file hashes and binary build provenance were not recorded by this script. **[O/U]**[^probe]

All values below are on October 31, 2026; the shortened cells retain the exact recorded time components. These five rows are **observations**, not predicted HTTP responses. **[O]**[^probe]

| Original `start` time | JSON-mode model time | `.isoformat()` time |
| --- | --- | --- |
| `09:00:00Z` | `09:00:00Z` | `09:00:00+00:00` |
| `09:00:00.000000001Z` | `09:00:00Z` | `09:00:00+00:00` |
| `09:00:00.123456789Z` | `09:00:00.123456Z` | `09:00:00.123456+00:00` |
| `09:00:00.999999999Z` | `09:00:00.999999Z` | `09:00:00.999999+00:00` |
| `04:00:00.000000999-05:00` | `04:00:00-05:00` | `04:00:00-05:00` |

Thus this actual pin **discards digits after the sixth fractional digit without rounding or carrying into the next second**. It does not preserve nanoseconds merely because the incoming string contains nine fractional digits. The negative-offset example also shows that serialization preserves that offset rather than rewriting the civil time to UTC. **[O]**[^probe]

Retained provenance:

| Artifact | Identity / SHA-256 |
| --- | --- |
| `/tmp/tool-bench-e37-native-time-probe.py` | `3ce689c27cf53b119309f4fd9e95ffbca11a974b19cc2f467734bb45e11aa248` |
| `/tmp/tool-bench-e37-native-time-probe.json` | `0df44fecbe26dc90162b625aaacaa6a44390665b439ce6a55bccfa89436797c0` |
| `/tmp/tool-bench-e37-time-probe-container.json` | `4677fb2297b9b67741466c563755c8eb27ad5130c745dfa84ba3df44b3e9839f` |
| Recorded container ID | `d1ad48d90cbb7d217bfaf79cb9c434b72959c3758ff1e895971cbfd8c6b86ac8` |
| Recorded tools image ID | `sha256:1aa960e21f314e3434085068b982e439c341e8c952375b451b64f11e2dbd6a61` |
| Recorded manifest descriptor | `sha256:8befcec36b4228a34ad304f05147583d33ed51d19ad8c76156a6112dd847d4c0`, Linux/arm64 |

The retained inspection reports `NetworkMode:"none"`, exit code `0`, and execution November 7, 2026, `19:24:00.730394139Z`–`19:24:01.097420527Z`. Its configured image name is `tool-bench-tools:pagerduty-contracts-v36-candidate-a`. The requester reports that the probe container was removed; the inspection itself records its earlier exited state, not proof of later removal. No Docker verification was attempted here. **[O/U]**[^probe]

The filename's `e37` is not evidence of a graded condition. Source36 and A remain frozen; the repair must receive a distinct new condition rather than overwrite or reinterpret them. `/tmp/tool-bench-e36-source-freeze.oLMS4F/SHA256SUMS` records archive hash `b38580dc82eab0bec8dfb6e93207a7d96509b09d0dbb7fd932479850c76ed900` and manifest hash `fadbe3779ba2353b028835d050a133b6a1d8e6217d49e83a8b27bae4854334bd`. These are retained manifest declarations; this sidecar did not rerun a full capsule integrity verification or establish a hermetic image build. **[L/U]**[^freeze]

## 2. Exact input conversion and serialization

### The native source path, not an assumed JSON serializer

Pinned `Override` declares plain Python `datetime` fields for `start` and `end`, and a string `user_id`. It has no timezone-awareness restriction, custom datetime validator or start-before-end validator. `ScheduleOverrideCreate` contains a list of those objects. **[P]**[^mcp-wheel]

FastMCP validates arguments into its Pydantic argument model before calling the function. The creation function then calls ordinary **Python-mode** `override_request.model_dump()`, calls `.isoformat()` on each resulting datetime, changes `user_id` into `user:{id,type:"user_reference"}`, and supplies that dictionary to `rpost`. The pinned SDK's classic override special case preserves the `overrides` request wrapper and returns the JSON response without a response-entity wrapper. **[P]**[^framework-wheel][^mcp-wheel][^sdk-wheel]

For example, source plus the observed model behavior predicts the following body for a start argument `2026-10-31T09:00:00.123456789Z`; **this body was not captured on HTTP by the probe**. **[O/P/I]**[^probe][^mcp-wheel]

```json
{
  "overrides": [{
    "start": "2026-10-31T09:00:00.123456+00:00",
    "end": "2026-10-31T17:00:00+00:00",
    "user": {"id": "PJ0RLEE", "type": "user_reference"}
  }]
}
```

The probe's JSON-mode `Z` spelling is therefore **not** the creation function's actual serialization method. Python documents `.isoformat()` with default `timespec="auto"`: no fraction for zero microseconds, six digits otherwise, and the object's numeric UTC offset when present. It does not automatically convert every aware datetime to UTC. Nanosecond loss here occurs during input validation, before `.isoformat()`; Python's separate documentation about truncating excluded `timespec` components is not, by itself, proof of Pydantic input parsing. **[D/P/O]**[^python-datetime][^mcp-wheel][^probe]

### Why the exact Pydantic pin does this

The inspected `pydantic==2.14.2` generator maps ordinary `datetime` to `core_schema.datetime_schema()`. In `pydantic-core==2.50.2`, that schema defaults `microseconds_precision` to `truncate`; the Rust datetime validator also defaults its overflow behavior to `Truncate`. String conversion passes that setting to speedate, then constructs a Python datetime from the resulting civil fields and microseconds. **[P]**[^pydantic-wheel][^core-source]

The core source's Cargo lock selects `speedate==0.18.0`. Its fractional parser accumulates only the first six digits and, in truncation mode, consumes further digits without adding them or rounding. Fewer than six digits are zero-padded to microseconds. The separately fetched crate matches the checksum recorded in that lock. This is published-source corroboration, not independent attestation that the installed binary was compiled from precisely those sources. **[P/U]**[^core-source][^speedate]

### Offsets, naive values and limits of extrapolation

These are **source-derived consequences**, except the explicitly observed negative-offset row above; no additional model or native probes were run here. **[P/I]**[^core-source][^speedate][^python-datetime]

| Input case | Pinned conversion consequence | Evaluation implication |
| --- | --- | --- |
| `2026-10-31T04:00:00.123456789-05:00` | Civil time remains `04:00:00.123456`; fixed offset remains `-05:00`. | Expected transmitted instant is `09:00:00.123456Z`, not `.123456789Z`. |
| `2026-10-31T18:00:00+01:00` | Offset is retained by `.isoformat()`. | Equal to `17:00:00Z`; literal-string inequality is not a wrong window. |
| `2026-10-31T09:00:00.123456789` with **no offset** | The parser leaves timezone absent; Python serialization remains naive and retains only `.123456`. | The connector has not established a UTC instant or attached a schedule/account/process timezone. |
| `1969-12-31T23:59:59.999999999Z` | Civil fraction becomes `.999999`, with no carry. | If converted to epoch units, this is `-1` microsecond, not zero; truncating a signed epoch value toward zero would be a different algorithm. |

The core's `unix_timestamp_offset:Some(0)` setting is not evidence that a **naive calendar string** becomes UTC: speedate's calendar-time parser initializes its offset to `None` and only fills it from a supplied zone. Numeric epoch input follows a different branch. Plain `datetime` also differs from Pydantic's explicit `AwareDatetime`/`NaiveDatetime` constrained types. **[P/D]**[^core-source][^speedate][^pydantic-types]

Consequently:

- For aware inputs, compare instants using the supplied signed offset, including date/leap-day rollover, with exact integer units or an exact seconds/fraction pair. Do not use process-local `mktime`, floating epoch seconds or a tolerance. **[I]**
- The inspected local parser/comparator treats naive calendar timestamps as UTC. That is an **authored compatibility policy**, not demonstrated native-to-hosted behavior. Keep it separately identified; use explicit `Z`/offsets for the primary acceptance controls. **[L/I/U]**[^local-time][^local-comparator]
- RFC 3339's date-time grammar requires an offset and allows a fractional digit sequence without specifying storage precision. Its `-00:00` convention is not the same metadata convention as `Z`/`+00:00`; do not confuse it with an absent offset. Neither this RFC nor PagerDuty's `format:"date-time"` declares vendor nanosecond persistence or a rounding rule. **[D/I]**[^rfc3339][^api-schema]
- Numeric timestamps, date-only input, unusual separators, more than nine fractional digits and other lax-parser spellings need their own admitted-domain controls if relied upon. The speedate parser can consume more than nine fractional digits; the inspected reference comparator currently restricts fractions to at most nine. This mismatch is not proof that the native model rejects longer strings. **[P/L/U]**[^speedate][^local-comparator]

### Official documentation and version caveats

Current official Pydantic core-schema docs explicitly give `microseconds_precision="truncate"` and describe handling beyond six fractional digits. The official standard-library guide distinguishes plain datetime validation, awareness constraints, Python-mode datetime objects and JSON-mode strings. The serialization guide explains that `model_dump()` and JSON-mode dumping are different paths. These corroborate the mechanism; the exact package sources and retained observation are stronger evidence for **this pin** than a mutable `/latest/` page. **[D/P/O]**[^pydantic-schema-doc][^pydantic-types][^pydantic-serialization][^core-source][^probe]

The official URLs currently redirect from `docs.pydantic.dev/latest/...` to `pydantic.dev/docs/validation/latest/...`. Context7 provided current/main documentation but no exact `2.14.2` documentation selection; it was used for discovery, not as a substitute release pin. Pydantic's temporal JSON configuration changed over v2 releases (`ser_json_temporal` was added in v2.12), and custom schemas/serializers can change behavior. Do not generalize this result to v1, every v2 version, every input type, a strict custom schema or a future dependency upgrade. **[D/P/U]**[^pydantic-schema-doc][^pydantic-serialization][^pydantic-wheel]

## 3. Documented classic PagerDuty semantics

The separately pinned first-party OpenAPI defines classic `Override.start`/`end` as required date-time strings and associates the override with a user. Classic `POST /schedules/{id}/overrides` creates one or more time-bounded substitutions; a later-created overlapping override takes priority. Its HTTP 201 body contains individual creation statuses and possible errors, including an illustrative invalid interval. Outer HTTP/MCP success is not proof that every item was created. **[P/D]**[^api-schema][^pd-create]

The first-party sources inspected do **not** specify the maximum retained fractional precision, fractional rounding, naive override interpretation or exact subsecond boundary behavior of the hosted implementation. Examples with whole seconds and numeric offsets establish examples, not a universal seconds-only backend rule. No live vendor request, persisted readback or notification-routing conformance was obtained. **[P/D/U]**[^api-schema][^pd-create]

For classic `/oncalls`, documentation says a returned period may **overlap** the requested range, not that its returned start/end equal the query bounds. Missing bounds default to current time; the documented horizon is 90 days into the future, and `until` must not precede `since`. `earliest` deduplicates by policy, escalation level and user. The native function defaults it to `true`; use explicit `earliest=false` when retaining all segments matters. Rendering `time_zone` is not permission to reinterpret an explicit UTC task window. **[D/P]**[^pd-oncalls][^mcp-wheel]

Product documentation describes temporary overrides, protection of past schedule history, and immediate resumption of the regular schedule when an ongoing override is removed. Classic DELETE separately distinguishes an active truncation from future deletion and prohibits removing a past override. These support preservation requirements, but do not supply an undocumented fractional rounding rule. The captured native classic surface does not expose override DELETE/list tools; REST-only cancellation is not a native alternative for this task. **[D/P/L]**[^pd-edit][^api-schema][^local-inventory]

Use `[start,end)` for the **authored exact-window contract and local rendering**: include the requested start, hand back at the requested end, and permit touching split intervals. Local `v2_segments` uses that convention. The inspected hosted wording does not explicitly prove nanosecond-resolution half-open boundary behavior; any live claim at that resolution needs actual controlled evidence. **[L/I/U]**[^local-schedules][^api-schema]

## 4. Keep four evidence layers separate

| Layer | What it can establish | What it cannot establish alone |
| --- | --- | --- |
| Original MCP arguments | Tool name, requested schedule/user and original timestamp spelling/precision. | Exact HTTP bytes, successful persistence or effective routing. |
| Converted native HTTP payload | Input validation outcome, six-digit conversion and actual URL/body/query sent by the pinned connector. | Server acceptance or final committed effects. |
| Persisted API effects and readback | Allocated override identities, stored intervals, effective winners, baseline preservation and partial commits. | That a successful response reached the caller, or that the agent observed a readback. |
| Reference-result comparison | Whether the returned native result satisfies a declared request/result contract. | Vendor conformance or business success independent of state/history. |

This separation is a recommended evidence model, not a claim the existing probe captured all four layers. **[I/U]**[^probe]

Creation returns raw dictionary/list data, so there is no basis to blindly truncate its returned string timestamps using the **input** model rule. Conversely, `list_oncalls` explicitly builds typed `Oncall` objects whose `start`/`end` are datetimes: their output can undergo the same six-digit model conversion before JSON serialization. A typed MCP readback alone therefore cannot certify preservation of hypothetical finer-than-microsecond HTTP/state precision. Capture the raw local HTTP response and authoritative state when testing that distinction. **[P/I/U]**[^mcp-wheel][^framework-wheel]

The current `pagerduty_instant` comparator converts calendar/offset fields into an integer seconds component plus a nine-digit padded fractional **string**. It compares returned user/start/end multisets to original `$arguments.override_request.overrides`, accepts offsetless values as UTC and requires positive original intervals. It does not currently reproduce pinned-model input conversion. Existing comparator test source includes a nine-digit request/response equality fixture; reading that fixture is not evidence such a value survives native creation. **[L]**[^local-comparator]

## 5. Evaluator objections and actionable recommendations

### A. A strict raw-nanosecond comparator can reject correct pinned normalization

Original `09:00:00.000000001Z` becomes native `09:00:00+00:00`. A returned/persisted `09:00:00Z` then disagrees with the raw nine-digit argument while agreeing with the actual converted request and the requested business boundary. Similarly, `.123456789` must bind to `.123456`, not to its original last three digits. **[O/P/I]**[^probe][^mcp-wheel]

**Recommendation:** derive a distinct expected wire value using the exact pin's input conversion, bind the result to that value, and separately check the desired window. Do not weaken all timestamp equality to seconds or a numerical tolerance. If raw-input exactness is separately scored, label it as a distinct declared input-conformance policy, not proof that the HTTP operation failed. Validate `start < end` **after conversion** too: a submicrosecond interval may collapse into equal endpoints. **[I]**

### B. Backend second truncation can hide a real wrong window

An input start `09:00:00.900000Z` survives native input validation as a genuine 900-millisecond shift. The older seconds-only parse/store/render path can erase that shift; an evaluator looking only at rounded stored state can then accept incomplete start coverage and extension after the requested end. Unlike the one-nanosecond example, this is not loss already imposed by the pinned model. **[P/L/I]**[^core-source][^local-time][^local-schedules]

**Recommendation:** preserve the converted payload's exact representable instants through classic creation, storage, overlap tests, rendering and read queries. Prove request-to-state correspondence, not just equality between two values processed by the same lossy helper. Retain exact comparison on returned/persisted values; do not apply input truncation indiscriminately to server results. **[I]**

### C. Broad “unsupported precision” faults are not an exact-window repair

At inspection, Source36/current classic source pre-scans a creation batch and returns `ApiErr::Unavailable` for nonzero fractional endpoints. `Query::time` similarly rejects fractional bounds. `run` records `unsupported_pagerduty_precision` via `world.fail_operation`, and the response status is HTTP **501**, not an ordinary per-item creation error. Existing integer-second `Seg`/`parse_time`/`norm_time` paths remain beneath these guards. This is a bounded capability-fault implementation, not a documented vendor precision limit. **[L]**[^local-time][^local-schedules][^freeze]

That can classify a genuine fractional wrong-window attempt as an environment fault instead of a negative business-outcome control. It can also reject a valid split such as `[09:00,13:00:00.123456)` plus `[13:00:00.123456,17:00)` for Jordan, whose exact union is the requested eight hours. Both split endpoints are representable in the pinned model. A mixed batch's blanket pre-scan also differs from ordinary per-item validation/partial results. **[P/L/I]**[^mcp-wheel][^local-schedules]

**Recommendation:** do not replace precision fidelity with blanket faults. Capability uncertainty must remain explicit and separate from agent correctness; it cannot become a passing correctness control or retroactively sanitize a frozen result. Valid microsecond split-shift/read-query controls must work in the distinct repaired local condition before claiming support for that domain. **[I]**

### D. Semantic equivalence does not authorize unrelated changes

Accept whole-window and contiguous split representations by exact interval union/effective routing, not `/1` storage allocation, one call, result order or fixed row count. Reject gaps, out-of-window extension, wrong user/parent, additional schedule effects or changes to baseline layers/history. Boundary samples alone are weaker than an exact sweep over authoritative intervals; a midpoint on-call result cannot establish all eight hours. Retry-generated duplicates need a separately stated lineage/side-effect policy, not an implicit unlimited-create exemption. **[L/I]**[^local-task][^local-schedules]

## 6. Tests still needed — proposals, not results

The main agent owns implementation and subsequent execution. These are future local controls through the installed pin, **not instructions to use real accounts or live infrastructure**. Capture original arguments, actual outbound local HTTP body/query, HTTP response, MCP result, final state/history and comparator disposition together. A pure `Override` probe cannot fill in the missing layers. **[I/U]**

| Proposed control | Required distinction / honest expectation |
| --- | --- |
| Whole-second reference and equivalent `-05:00`/`+01:00` representations | Same exact UTC window; different serialized offset spellings must not fail. |
| `.000000001`, `.123456789`, `.999999999` inputs | Actual HTTP request must confirm the observed model truncation, with no rounding/carry; result binds to converted input. |
| Positive/negative offsets crossing midnight/leap day; pre-epoch civil fraction | Exact instant arithmetic, no sign reversal, process timezone or signed-epoch toward-zero error. |
| Naive calendar input under different process timezones | Confirm actual MCP acceptance/serialization and the explicit local UTC policy, if admitted. No hosted interpretation claim follows. |
| `.000001` and `.900000` wrong start/end | Fractions survive wire/state/rendering; wrong exact coverage is detected, not rounded away or hidden by a broad environment fault. |
| Whole-second split and microsecond split at `13:00:00.123456Z` | Both unions equal the requested window and preserve the base rotation/outside coverage; no blanket capability fault. |
| One-microsecond gap or extension; short interval collapsing during input conversion | Gap/extension fails exact coverage; post-conversion nonpositive interval cannot be certified as a successful creation. |
| Fractional `list_oncalls` bounds with `earliest=false`; boundary-adjacent reads | Confirm actual transmitted query, overlap/full-shift behavior and representable precision; do not require returned endpoints to equal query endpoints. |
| Mixed valid/invalid creation batch and reordered successful results | Preserve per-item status/error classification, actual partial commits, identity/request matching and non-positional result matching. |
| Raw HTTP/state timestamp versus typed on-call readback | Distinguish storage precision from output-model normalization; do not infer raw precision from typed JSON alone. |

No proposed row above was executed by this sidecar. Negative/naive/pre-epoch source consequences remain unobserved in the retained five-case probe. Hosted fractional retention/rounding, naive-zone handling, subsecond handoff boundaries, actual paging and native HTTP conformance all remain unestablished. **[O/P/U]**[^probe][^api-schema]

## 7. Bounded acceptance criteria for the repaired local condition

These are recommendations, not qualification or evidence that the repair is complete. **[I]**

1. **Bind the condition and evidence.** Identify the new source/image/dependency condition, retain Source36/A unchanged, and keep the original instruction/datasets. The supplied model probe may support a conversion premise; it is not a graded trial or live-vendor control.
2. **Declare the admitted domain.** Initially claim exact classic behavior for reviewed aware calendar timestamps with conventional `Z`/signed `HH:MM` offsets and up to nine input fractional digits, converted to microseconds by this pin. Preserve all representable wire microseconds locally. State separate treatment of naive values and other lax-parser inputs; do not mislabel them as proven hosted behavior.
3. **Prove conversion and result binding.** Capture the actual native HTTP request. Match every successful returned creation to the converted request's user and exact UTC start/end, with real nonempty allocated identity and successful per-item status/errors. Do not require a fixed response order or apply input truncation to returned values.
4. **Prove the business outcome independently.** Authoritative persisted/effective coverage must be Jordan on `PSPLTPR` throughout `[2026-10-31T09:00:00Z,2026-10-31T17:00:00Z)`, with no newly authorized coverage outside it. Admit exact contiguous splits, including representable fractional internal boundaries. A normalized one-nanosecond raw start may satisfy this outcome; a surviving one-microsecond wrong start must not.
5. **Prove preservation and failure classification.** Preserve all baseline records/rotations, existing Payments override, other schedules and unrelated state/history. Classify tool errors, per-item failure, partial commit and environment capability faults distinctly. Do not infer rollback from MCP failure or business failure solely from an optional readback error.
6. **Keep claims local and finite.** Require positive and negative controls at model, HTTP/state and evaluator layers before accepting the condition. Report which controls ran and which did not. Passing them establishes the reviewed local classic-window contract, not arbitrary PagerDuty timestamp precision, production paging, all ISO inputs or live API conformance.

## Source ledger and snapshot binding

### Local evidence

The earlier operation-contract note was read as background, not rewritten. Its older statements about unobserved installed packages and legacy predicates must not override the newer, narrowly scoped model observation or the current shared snapshot. Local source may change while the main agent repairs it; this note's diagnosis binds to the inspected bytes, not a moving HEAD. **[L/U]**

| Inspected input | SHA-256 |
| --- | --- |
| `docs/research/2026-11-07-pagerduty-operation-contract.md` | `1ae7b8908ebcbad258e262c06d8ca9cac177500a71e5d60ab82d1b3c977e2916` |
| `crates/services/pagerduty/src/schedules.rs` | `2d19ede345e75e35abef6f2589d10dac646018b5e0edad87103c18dfdab7af3c` |
| `crates/services/pagerduty/src/util.rs` | `42c9eddedd4cc7c0712bba634959234a762a9550938baaa5260a6ddece9073ba` |
| `crates/scenario/assets/oracle-tool-result.jq` | `42b3f6f69a8d15317b65a5d03616ed3d0949ffcad55207db07f7582fc7194743` |
| Generated cover `solution/tool-result.jq` | Same hash as the source comparator above |
| `crates/scenario/tests/oracle_results.rs` | `635963b81773c10c5617d4231863f40f983cc5f527875d219bc200d4d97dd4f8` |
| `crates/mock-core/src/clock.rs` | `a6c38037c169e9e71c74ed8a0ac93b1069efabbf668e434e243ab8d906300e55` |
| `scenarios/ops/pagerduty/cover-shift-override/scenario.toml` | `022063e6a709e99461bb01306b811864e632e7403a9c2d5923b12b72046071d1` |

The Source36 manifest records the same inspected schedules/util/comparator hashes. This observation does not rebind a later repair to that capsule. **[L]**[^freeze]

[^probe]: Complete retained `/tmp/tool-bench-e37-native-time-probe.py`, `.json`, and `/tmp/tool-bench-e37-time-probe-container.json`; hashes and exact recorded container/image identities above. Scope string: `network_disabled_pinned_model_normalization_not_live_api_or_trial`.
[^freeze]: Read-only `/tmp/tool-bench-e36-source-freeze.oLMS4F/SHA256SUMS` and selected entries in `manifest.json`; no archive extraction, mutation, image operation or tag update.
[^local-task]: `scenarios/ops/pagerduty/cover-shift-override/instruction.md` and inspected `scenario.toml`; current declaration already uses `expect.version=2`, interval coverage, parent/user relations and history preservation. The earlier operation-contract document describes an earlier legacy declaration.
[^local-time]: Inspected `crates/services/pagerduty/src/util.rs`: `Query::time` near line 109, `ApiErr::Unavailable`/HTTP 501 near line 189, `run`/`world.fail_operation` near line 366, `parse_exact_time` near line 674, seconds-only `parse_time`/`norm_time` near lines 688/710. `crates/mock-core/src/clock.rs:14` independently supplies exact nanosecond parsing; its existence does not make every service consumer lossless.
[^local-schedules]: Inspected `crates/services/pagerduty/src/schedules.rs`: integer-second `Seg`, `override_segments`, `v2_segments`, `oncall_segments`, `create_one_override`, fractional batch guard in `create_overrides`, `list_overrides`, `list_oncalls`. Read-only source inspection, no test execution.
[^local-comparator]: Inspected `crates/scenario/assets/oracle-tool-result.jq:163` `pagerduty_instant`, and `:204` `pagerduty_api_result`; matching generated `tasks/ops__pagerduty__cover-shift-override/solution/tool-result.jq`; `crates/scenario/tests/oracle_results.rs:209` onward, timestamp-binding fixtures. No comparator or test was run.
[^local-inventory]: `servers/snapshots/pagerduty.json` and the earlier operation-contract inventory analysis; native classic `create_schedule_override`, `list_oncalls`, `get_schedule` are distinct from classic REST-only override list/delete and separate v3 tools.

### Independently fetched pinned published artifacts

Each distribution/crate below was downloaded into memory and its SHA-256 checked against published metadata (or, for speedate, the inspected core Cargo lock). No package was imported or installed.

| Artifact | SHA-256 | Inspected source relevant to this note |
| --- | --- | --- |
| `pagerduty_mcp-1.1.0-py3-none-any.whl` | `f2e3012e5c7b233ce855ec9bfaa41e160ef18a6af64753d858bf8b5d9e13fa05` | `models/schedules.py:127`, `tools/schedules.py:214`, `models/oncalls.py:11`, `tools/oncalls.py:13` |
| `pydantic-2.14.2-py3-none-any.whl` | `7efbf7507b937931cd32a5755acb00ea1cf5ccb1d4e8067a5850d5069e382118` | `_internal/_generate_schema.py:393`, `config.py:578`; metadata requires core `2.50.2` |
| `pydantic_core-2.50.2.tar.gz` | `1b0e8dc3932cf4e9ed5727570325cacc716b9409ffc794a221c0ccc72e8eb775` | `python/pydantic_core/core_schema.py:1230`; `src/validators/datetime.rs:30`; `src/input/datetime.rs:402`, `:444`, `:522`; `src/serializers/config.rs:171`; Cargo lock |
| `speedate-0.18.0.crate` | `da8f8c5f7f7ea2811a17f99bca4fec7f9ef0b0734713a5cbfc9baee203eba8eb` | `src/time.rs:301`, `:491`; `src/datetime.rs:356` |
| `pagerduty-5.3.0-py3-none-any.whl` | `bcc0686b273d652b4499aba51af5836b5b2abebdf0b8e24a65c5518e0d502f5b` | `rest_api_v2_client.py:399`; `rest_api_v2_base_client.py:386`, `:1032` |
| `mcp-1.30.0-py3-none-any.whl` | `666edb5009503e1047c9d60346a756f94b261f05cc2625f23d41c728ffc484d0` | `server/fastmcp/utilities/func_metadata.py:94`, `:117`; `tools/base.py:93` |

[^mcp-wheel]: [PagerDuty MCP 1.1.0 metadata](https://pypi.org/pypi/pagerduty-mcp/1.1.0/json); [exact wheel](https://files.pythonhosted.org/packages/20/52/341bdcae8c92feb2642884124087889edd9dc1aa75262392a31ed374873c/pagerduty_mcp-1.1.0-py3-none-any.whl). Package-relative paths in the table are under `pagerduty_mcp/`; no mutable GitHub main substitution.
[^pydantic-wheel]: [Pydantic 2.14.2 metadata](https://pypi.org/pypi/pydantic/2.14.2/json); [exact wheel](https://files.pythonhosted.org/packages/97/01/c71a71258b58659c72504e5fd65b451f6e36e34dc931d0b237ded213c814/pydantic-2.14.2-py3-none-any.whl). Source also documents temporal JSON configuration introduced in v2.12.
[^core-source]: [Pydantic core 2.50.2 metadata](https://pypi.org/pypi/pydantic-core/2.50.2/json); [exact source distribution](https://files.pythonhosted.org/packages/af/8e/23ec1021834e35041fbb2f10946141e6acdfd39379c45cc4d65a4e8a1d98/pydantic_core-2.50.2.tar.gz). This source pin is not an installed extension-file hash.
[^speedate]: [Exact speedate 0.18.0 crate](https://static.crates.io/crates/speedate/speedate-0.18.0.crate); manifest identifies first-party repository `pydantic/speedate`. Its checksum matches the core distribution's `Cargo.lock` entry.
[^sdk-wheel]: [PagerDuty SDK 5.3.0 metadata](https://pypi.org/pypi/pagerduty/5.3.0/json); [exact wheel](https://files.pythonhosted.org/packages/0d/81/edf610ffc8a9d4449f76ab4ae5fe8489df745086248d0c8f323998665b2e/pagerduty-5.3.0-py3-none-any.whl).
[^framework-wheel]: [MCP 1.30.0 metadata](https://pypi.org/pypi/mcp/1.30.0/json); [exact wheel](https://files.pythonhosted.org/packages/f5/f4/e58bc33317c92a0203664daaf00bf6f41166cc0149e5d6870a03f7cd004a/mcp-1.30.0-py3-none-any.whl).

### First-party documentation/specification

[^pydantic-schema-doc]: [Official core datetime-schema documentation](https://pydantic.dev/docs/validation/latest/api/pydantic-core/pydantic_core_schema/#pydantic_core.core_schema.datetime_schema), retrieved through `https://docs.pydantic.dev/latest/api/pydantic_core_schema/`; explicit truncation default and optional awareness constraints. Mutable current documentation, not a version-specific binary observation.
[^pydantic-types]: [Official standard-library datetime guide](https://pydantic.dev/docs/validation/latest/api/pydantic/standard_library_types/#datetimes), retrieved through `https://docs.pydantic.dev/latest/api/standard_library_types/`; awareness types and Python/JSON representations.
[^pydantic-serialization]: [Official serialization guide](https://pydantic.dev/docs/validation/latest/concepts/serialization/), retrieved through `https://docs.pydantic.dev/latest/concepts/serialization/`; Python-mode versus JSON-mode dumping.
[^python-datetime]: [Python 3.13 datetime documentation](https://docs.python.org/3.13/library/datetime.html#datetime.datetime.isoformat): `isoformat` default precision and numeric-offset rendering; aware/naive distinction. Python 3.13 documentation does not establish the probe's interpreter patch build.
[^rfc3339]: [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.txt), sections 4.2–4.4 and 5.6: signed offsets, unknown-local-offset convention, unqualified local times and fractional syntax. It specifies a timestamp representation, not a PagerDuty storage-precision contract.
[^api-schema]: [Immutable PagerDuty OpenAPI](https://raw.githubusercontent.com/PagerDuty/api-schema/9c84115d71fdeba32cb796ac711ea91e7bd7ad0e/reference/REST/openapiv3.json), revision `9c84115d71fdeba32cb796ac711ea91e7bd7ad0e`, fetched SHA-256 `6471be3950a0023c3e3e4153b2a44c6462c2d622ba94b881260f1d8e905c22c8`. Inspected classic override POST/GET/DELETE, `/oncalls`, schedule GET, `Override`/`Oncall` schemas and time parameters. Do not import v3 `start_time`/`end_time` semantics into classic `start`/`end`.
[^pd-create]: [Official classic override creation reference](https://docs.pagerduty.com/developer/api/reference/rest/schedules/create-schedule-override): time-bounded user substitution, newest-created priority and per-item statuses/errors.
[^pd-oncalls]: [Official on-call listing reference](https://docs.pagerduty.com/developer/api/reference/rest/on-calls/list-on-calls): overlap search, current-time defaults, future horizon, rendering timezone and `earliest` filtering.
[^pd-edit]: [Official edit-schedules documentation](https://docs.pagerduty.com/incident-management/schedules/edit-schedules): overrides, preservation of historical schedule records and regular-schedule resumption. Fractional hosted precision is not specified there.
