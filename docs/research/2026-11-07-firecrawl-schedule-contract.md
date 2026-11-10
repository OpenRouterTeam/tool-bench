# Firecrawl schedule semantics and Rust parser assessment — 2026-11-07

## Scope and evidence

Research-only supplement to [the operation-contract note](2026-11-07-firecrawl-operation-contract.md). This document is the only file authored by this sidecar. The main agent's legacy grading reproductions are not repeated. No package was installed or executed; no dependency, implementation, fixture, frozen capsule/tag, or existing work was changed. No builds, tests, Docker, evaluated tools/models, live accounts, agents, commits, or pushes were used. Public package archives were read and hashed in memory.

This is **not qualification, implementation, or closure of any of the 19 overhaul items**.

Evidence labels:

- **[D]** Current first-party documentation/specification; not a hosted deployment pin.
- **[P]** Publisher package bytes or source at an explicitly identified revision.
- **[R]** Current local source/dependency configuration; not vendor behavior.
- **[I]** Static inference, compatibility objection, or proposed strategy; not an executed result.
- **[U]** Unestablished runtime, deployment, performance, or compatibility behavior.

## Conclusions

1. **The pinned Firecrawl API uses DOM AND DOW, not Unix-style DOM/DOW OR.** A default-OR Rust parser silently changes the vendor recurrence. Wildcard spelling does not change the API's conjunction. **[P]**[^fc-cron]
2. **“Daily” becomes a local-calendar cron, not an elapsed 1,440-minute timer.** Plain `daily` converts to `0 0 * * *`; supplied times are parsed and validated. Unsupported natural language must not become a successful daily monitor merely because it contains a keyword. **[P/R/I]**[^fc-cron][^local-text]
3. **Next occurrence is strictly later on the UTC-minute grid, matched against zoned calendar fields.** Missing spring-forward times are skipped; a repeated fall-back time can match twice. The publisher's own source tests compare chained results with a minute-by-minute reference across several DST transitions. These tests were read, not run here. **[P/I]**[^fc-cron][^fc-tests]
4. **`croner = "=2.2.0"` is a small, currently published parser/matcher candidate, not a drop-in scheduler.** Its explicit AND option is suitable; its direct next-occurrence helper, default OR mode, syntax extensions, and numeric-step semantics are not automatically compatible. **[P/I]**[^croner-registry][^croner-lib][^croner-pattern][^croner-component]
5. **Use an existing parser plus a bounded UTC search and a stated compatibility envelope.** Do not write another cron expander, infer “daily” from one interval, or present unsupported publisher edge syntax as proven invalid. Full acceptance parity remains unqualified. **[I/U]**

## 1. Which pinned implementation owns scheduling?

`firecrawl@4.50.0` supplies the `MonitorSchedule` TypeScript interface and forwards schedule objects through its monitor HTTP methods. It does **not** implement the API's cron evaluator in those methods. The schedule contract below is from API source at **`fc74e903469990c1ab72f33abdff4352c7e9d8e8`**, the monorepo revision associated with that SDK by the prior note's publisher-provenance inspection. This association is not evidence that a hosted API deploys that revision. **[P/U]**[^sdk-package][^sdk-types][^sdk-monitor][^prior]

Exact exported API-source functions:

| Function | Established signature/behavior |
| --- | --- |
| `parseMonitorScheduleText(input: string): string` | Deterministic text-to-cron conversion; no model or account call. |
| `getNextMonitorRunAt(cronExpression: string, from = new Date(), timeZone = "UTC"): Date` | Timezone validation, five-field parsing, bounded future search. |
| `validateMonitorCron(cronExpression: string, timeZone = "UTC"): { nextRunAt: Date; intervalMs: number }` | Finds the next two occurrences from the current server clock and rejects their gap below five minutes. |
| `estimateRunsPerMonth(intervalMs: number): number` | Ceiling of 30 days divided by that sampled gap; not a proof of uniform recurrence. |

These signatures and algorithms are pinned source facts. **[P]**[^fc-cron]

The API schema requires a strict schedule **object**, exactly one of `cron`/`text`, each supplied string length 1–128, and optional timezone length 1–128 defaulting to `UTC`. Text is transformed to `{cron, timezone}`. Create validates the cron before persistence; update revalidates the selected/new cron and timezone even when other fields are being updated. Unknown schedule keys such as the replica's `intervalMinutes` are not accepted by this strict schema. **[P]**[^fc-types][^fc-controller]

## 2. Exact five-field and day semantics

The field order is **minute, hour, day-of-month, month, day-of-week**, with numeric domains **0–59, 0–23, 1–31, 1–12, 0–7**, respectively. There is no seconds or year field. **[P]**[^fc-cron]

| Form | Pinned parser meaning |
| --- | --- |
| `*` | All values of that field's numeric domain. |
| `a,b,c` | Union of the individual arms, stored as a set; duplicates do not add executions. |
| `a-b` | Inclusive ascending numeric range; wrapping/descending ranges fail validation. |
| `*/n`, `a-b/n` | Expand from the wildcard/range start in positive integral steps, retaining values through the endpoint. |
| `a/n` | **Singleton `a`**, not “start at `a`, repeat to the field maximum.” The source sets both range endpoints to `a` before stepping. |
| Sunday `0` / `7` | Expand the DOW field first over 0–7, then replace 7 with 0. Ranges such as `6-7` therefore include Saturday and Sunday. |
| Names/macros/extensions | `MON`, `JAN`, `@daily`, `?`, `L`, `W`, `#` are not established supported syntax in this numeric parser. |

All meanings in this table follow `parseField`, `parseDayOfWeek`, and `parseCron`, not a generic cron manual. **[P]**[^fc-cron]

### DOM/DOW intersection is mandatory

The actual predicate is minute-membership **AND** hour-membership **AND** DOM-membership **AND** month-membership **AND** DOW-membership. Thus:

- `0 9 13 * 5` means 09:00 **only on a Friday the 13th**, not every Friday plus every 13th.
- `0 9 * * 1` means Mondays; `0 9 1-31 * 1` means the same thing.
- Expanding a full-domain list/range instead of using `*` does not turn intersection into OR.
- A default-OR library could make a sparse source schedule appear frequent enough to pass validation, or create unauthorized extra runs. **[P/I]**[^fc-cron]

**Acceptance caveat:** the publisher uses JavaScript `Number()` and destructured `split()` results, not a strict published grammar. Empty comma arms can become zero in zero-based fields; additional slash/range segments can be ignored; non-decimal numeric spellings can be accepted by numeric conversion. These are source quirks, not a recommended language to reimplement. A stricter library may reject inputs the pinned source accepts. Mark that boundary as unsupported/unqualified rather than claiming vendor rejection. **[P/I/U]**[^fc-cron]

### Calendar feasibility and minimum interval are separate

Parsing `0 0 30 2 *` succeeds at field-range validation, but next-run search fails because February 30 never exists. Conversely, a real but sparse calendar expression can fail the search horizon; “valid calendar recurrence” does not imply API admission. **[P/I]**[^fc-cron][^fc-tests]

Each next-run search begins at `floor_UTC_minute(from) + 1 minute`, uses an elapsed **366-day** bound (`527040` minutes), and examines candidates only while accumulated advancement remains below that bound. This is not “search until the next calendar year ends.” Validation performs **two** such searches. Consequently, a leap-day-only cron cannot pass normal two-occurrence validation: after one leap-day occurrence, the next lies beyond the second search's horizon. A sparse DOM/DOW intersection may fail for the same reason. **[P/I]**[^fc-cron]

The five-minute floor is checked on **only the next two occurrences**, not every adjacent pair in the recurrence. For example, by static arithmetic, `0,1 9 * * *` evaluated from `2026-11-07T09:00:30Z` has next occurrences at 09:01 that day and 09:00 the next day, passing the sampled-gap check despite the recurring one-minute 09:00→09:01 gap. This was not executed. Globally enforcing a minimum gap would be a defensible stronger policy, but is **not** this exact validator. **[P/I]**[^fc-cron]

Raw cron steps also need not divide 60 or 24. The divisibility rule below belongs to **text conversion**, not `parseField`. Moving that rule into all cron parsing would introduce false rejections. **[P/I]**[^fc-cron]

## 3. Natural-language conversion, especially daily

Input is trimmed, lowercased, stripped of periods and commas, and internal whitespace collapsed. Patterns are then **anchored**, not substring searches. **[P]**[^fc-cron]

| Text | Normalized cron / constraint |
| --- | --- |
| `daily`, `every day` | `0 0 * * *` |
| `daily at 9am`, `every day at 09:00` | `0 9 * * *` |
| `daily at 5:30 PM` | `30 17 * * *` |
| `daily at 12am`, `daily at 12pm` | Midnight / noon respectively. |
| `every 24 hours` | `0 */24 * * *`, whose expanded hour set is just 0: calendar-daily, not a start-relative 24-hour timer. |
| `hourly`, `every hour`, optionally `at :mm` / `starting at :mm` | Specified minute of every hour; minute defaults to 0. |
| `every N hours` / `hrs` | Positive integer 1–24, dividing 24; `0 */N * * *`. |
| `every N minutes` / `mins`, optional supported start-minute suffix | Positive integer 1–60, dividing 60; `*/N` or `start-59/N` minute field. |
| `weekly`, `every week` | `0 0 * * 0`, Sunday midnight. |

These are deterministic source conversions, before the separate next-two-occurrences/minimum-interval validation. **[P]**[^fc-cron]

For daily times, the hour is one or two digits; a supplied minute must be exactly two digits. Without AM/PM, hours must be 0–23. With AM/PM, input hours must be 1–12, with normal midnight/noon conversion. Minutes must be 0–59. Thus `daily at 9:5`, `daily at 24:00`, and `daily at 0am` fail. **[P]**[^fc-cron]

The pinned text converter does not accept arbitrary synonyms such as `nightly`, `each day`, `every weekday`, `every Monday`, `every two hours`, `monthly`, or `every 24h`. Several are accepted by the local Rust helper, which scans words/substrings and returns a duration. Those local acceptances must not be promoted to publisher guarantees. The earlier operation note already records the grading implications; no local counterexample was rerun here. **[P/R/I]**[^fc-cron][^local-text][^prior]

Current official monitoring documentation lists daily/time examples, timezone interpretation, normalized cron responses, and a five-minute minimum. It also says text schedules are spread by monitor ID before conversion. **That last detail does not describe this pinned converter:** it has no monitor ID parameter; the schema normalizes text before monitor creation. The inspected scheduler applies a separate monitor-ID jitter at dispatch. Record this as documentation/source drift, not evidence that the conversion should randomize the daily minute. **[D/P/I/U]**[^monitor-docs][^fc-types][^fc-store][^fc-scheduler][^fc-jitter]

## 4. Timezone, DST, and next occurrence

`getNextMonitorRunAt` validates the timezone by constructing `Intl.DateTimeFormat("en-US", {timeZone})`. Its zoned formatter requests local minute/hour/day/month/weekday with `hourCycle: "h23"`. It compares those local fields while advancing real UTC instants. Seconds and milliseconds are cleared before advancing to the next UTC minute. A matching instant equal to `from` is therefore never returned. **[P]**[^fc-cron]

The production source uses local-day/hour/minute jumps with UTC advancement and an overshoot correction, rather than scanning every minute. The same-revision tests retain a brute-force reference, compare both the first and **chained second** result, and include New York/London/Sydney DST, two-hour Troll jumps, half-hour Lord Howe changes, and midnight-transition zones. These are authored regression expectations, not test results obtained by this research. **[P/U]**[^fc-cron][^fc-tests]

### DST consequences

| Situation | Source-derived consequence |
| --- | --- |
| Spring-forward gap | A nonexistent scheduled wall time is **not shifted to a substitute time**; search proceeds to an actual later match. |
| Fall-back overlap | Both actual instants carrying the selected local minute/hour can match; the second need not be skipped merely because the local clock label already occurred. |
| Ordinary daily time away from the transition | Consecutive runs can be 23 or 25 elapsed hours apart, while retaining the same local time. |
| Business grading | Fixed `now + 86400 seconds`, or one exact UTC time for every permitted timezone, is not a calendar-daily equivalence test. |

These conclusions follow the source predicate and its minute-grid reference. **[P/I]**[^fc-cron][^fc-tests]

Concrete **static implications**, using transition cases present in the pinned tests:

- New York `30 2 * * *`, from **March 13, 2027, 12:00 UTC**, skips the nonexistent March 14 02:30 and next matches **March 15, 2027, 06:30 UTC**.
- New York `30 1 * * *`, from **November 7, 2027, 04:00 UTC**, matches **05:30 UTC** and then **06:30 UTC**: the two 01:30 occurrences.

These dates are future test fixtures relative to the note's requested November 7, 2026 date, not observed production runs. The UTC values are inferred from the source/reference and transition comments; this sidecar did not execute them. **[P/I/U]**[^fc-tests]

### Nominal schedule is not dispatch timing

The scheduler computes a separate offset as a monitor-ID SHA-256-derived integer modulo `min(floor(intervalMs / 2), 300000)`, and may defer a due monitor until nominal `next_run_at + offset`. Store helpers recompute later nominal times from the then-current server clock. Queuing, overlap handling, and completion introduce additional lifecycle effects. Neither a nominal next occurrence nor an elapsed interval proves an actual check ran at that instant. **[P/I/U]**[^fc-jitter][^fc-scheduler][^fc-store]

### Timezone compatibility remains a boundary

`chrono-tz@0.10.4` embeds **IANA tzdb `2025b`** and provides `chrono::TimeZone` implementations. The API's actual Node/ICU/tzdb build is not established by the SDK pin. Different timezone data releases, aliases, and case handling can disagree; current ECMA-402 named-zone lookup is ASCII-case-insensitive, while this workspace does not enable chrono-tz's optional case-insensitive feature. Do not promise universal `Intl`/`Tz` equivalence from shared IANA terminology. **[P/D/R/U]**[^tz-source][^tz-docs][^intl-spec][^local-deps]

## 5. Small Rust candidate: `croner@2.2.0`

### Publication and dependency fit

The current crates.io publisher record lists **2.2.0**, published **June 17, 2025**, not yanked; its compressed source archive is **33,326 bytes**, MIT-licensed, Rust edition 2021. This is a deliberately assessed historical release, **not a claim that it is the latest**. Its manifest declares ordinary dependency `chrono = "0.4.38"` and optional Serde; chrono-tz `0.10.0` is a **development** dependency. **[P]**[^croner-registry][^croner-manifest]

Chrono `0.4.45` satisfies that Cargo semver requirement; chrono-tz `0.10.4` implements the same Chrono timezone trait. Therefore the existing pins are compatible at the declared-dependency/type-interface level without choosing a different Chrono version. **No build, dependency resolution, or runtime compatibility was verified.** The Firecrawl crate currently declares neither Chrono nor chrono-tz directly. **[P/R/I/U]**[^croner-manifest][^croner-lib][^tz-docs][^local-deps][^local-manifest]

**Feature objection:** 2.2.0's Chrono dependency does not disable Chrono defaults. Adding it would enable Chrono's default feature set through Cargo feature unification, despite the workspace's own `default-features = false`. Setting `default-features = false` on **croner** does not disable defaults on its transitive Chrono declaration. Assess that dependency/clock/platform expansion before selecting it; the small archive is not a complete dependency-cost argument. **[P/R/I]**[^croner-manifest][^local-deps]

### Exact usable APIs, verified in published source

```rust
let parsed = croner::Cron::new(expression)
    .with_dom_and_dow()
    .parse()?;
```

This is a source-established API example, not compiled code or an implementation change. Five fields are the 2.2.0 default; do not enable its optional/required seconds mode or alternative weekday numbering. The explicit AND option aligns ordinary numeric DOM/DOW predicates with Firecrawl. **[P/I/U]**[^croner-lib][^croner-pattern][^croner-docs]

Established methods:

- `Cron::new(&str) -> Cron`
- `Cron::with_dom_and_dow(&mut self) -> &mut Self`
- `Cron::parse(&mut self) -> Result<Cron, CronError>`
- `Cron::is_time_matching<Tz: chrono::TimeZone>(&self, &chrono::DateTime<Tz>) -> Result<bool, CronError>`
- Via public `parsed.pattern`: `minute_match(u32)`, `hour_match(u32)`, `month_match(u32)`, and `day_match(i32, u32, u32)`, each returning `Result<bool, CronError>`.

**Important 2.2.0 trap:** its `FromStr for Cron` merely calls `Cron::new` and returns the unparsed object. Do **not** use `expression.parse::<Cron>()?` as proof of validation in this version. Call the explicit `.parse()` method above. **[P]**[^croner-lib][^croner-pattern]

### Why its scheduler is not a faithful replacement

`find_next_occurrence(&DateTime<Tz>, inclusive)` exists, but 2.2.0 searches a **naive local** timeline. Its timezone conversion selects the earlier result for an ambiguous time; for a nonexistent time it searches forward up to 3,600 seconds and can return the adjusted time **even if it does not match the original cron**. Its search limit is also not Firecrawl's 366-day bound. **[P]**[^croner-lib]

This differs materially from Firecrawl:

- A missing 02:30 can become 03:00 instead of being skipped.
- Advancing a naive local timeline does not preserve both repeated wall-clock occurrences.
- Choosing the earlier ambiguous instant can even produce an instant earlier than the starting instant if the start was already in the second occurrence of that hour.

Those are static consequences, not reproduced crate bugs or qualified runtime results. Use the parser/matcher, **not** this next-occurrence helper, for the proposed compatible path. **[P/I/U]**[^croner-lib][^fc-cron]

### Syntax differences that require an explicit boundary

| Hazard | Risk / treatment |
| --- | --- |
| Default OR mode | False success through extra dates. Explicit AND is required. |
| Names, macros, `?`, `L`, `W`, `#` | Library extensions can admit requests the pinned API does not support. Limit the accepted character/surface envelope before parsing. |
| Numeric singleton steps, e.g. `5/10` | Croner extends from 5 through the field maximum; Firecrawl selects only 5. Never pass through unchanged. A narrowly specified singleton-to-singleton-range rewrite is possible; otherwise classify as unsupported. |
| Empty comma arms | Croner skips them; Firecrawl can turn them into zero. Reject/route unsupported malformed spellings rather than silently changing their meaning. |
| Literal `*` within a list, e.g. `*,5` | Firecrawl supports it; 2.2.0's per-arm parser does not treat that arm like a whole-field wildcard. This is a known false-rejection boundary. Do not collapse the field to `*` while ignoring potentially invalid other arms. |
| Large positive steps | Firecrawl uses JavaScript numbers; 2.2.0 parses steps as `u8` and checks addition overflow. Some publisher-valid steps fail. Do not assert complete numeric parity. |
| Sunday 0/7 and ascending ranges | Both expand 7 then map it to 0 in their ordinary numbering modes; retain that mode and qualify range/step/list combinations. |

These differences were established by comparing the published parser sources, not by invoking either package. **[P/I/U]**[^fc-cron][^croner-pattern][^croner-component]

Context7 was consulted, but resolves “croner”/“croner-rust” to the **JavaScript** `/hexagon/croner` project, not this Rust release. Its current JavaScript syntax and DST documentation were therefore **not used as Rust evidence**. Exact Rust archive/source/docs were used instead. **[U/P]**

## 6. Bounded faithful implementation strategy — proposal only

1. **Separate configuration, business recurrence, and execution.** Retain normalized cron plus timezone, not a synthetic duration as the primary representation. “Daily” business equivalence is a nominal local-calendar recurrence; actual gap/fold dispatch and execution evidence are separate questions. **[I]**
2. **Mirror the finite vendor text converter, not a cron language.** Use the pinned anchored text forms, normalization, hour/minute checks, and divisibility restrictions to produce cron. Keep exact daily times; unsupported phrases do not fall back to keyword-duration guesses. **[P/I]**[^fc-cron]
3. **Delegate cron parsing/expansion to a published parser.** With 2.2.0, enforce five fields and a numeric/punctuation-only surface, enable AND, and use explicit `.parse()`. Keep any singleton-step rewrite narrow, documented, and separately qualified. Unsupported library/source discrepancies must be distinguishable from proven vendor-invalid input. Do not rebuild sets/ranges/steps in local handwritten code to imitate JavaScript quirks. **[I/U]**
4. **Implement time search, not cron syntax.** Start with the next UTC minute; advance in 60-second UTC steps for at most **527,040 candidates** per search. Convert each instant with `DateTime::with_timezone(&tz)`, then ask the parsed pattern's `minute_match`, `hour_match`, `day_match`, and `month_match` methods. This leaves cron-language semantics in the dependency while preserving folds and gaps through UTC ordering. It follows the publisher's reference model; equivalence to every optimized publisher jump remains a qualification question. **[P/I/U]**[^fc-cron][^fc-tests][^croner-pattern][^chrono-docs]
5. **Ignore seconds just as the source does.** The default five-field Croner matcher additionally requires local second zero. Calling the field methods avoids a historical timezone-offset-with-seconds mismatch: Firecrawl compares only minute/hour/date fields on its UTC grid. `is_time_matching` is usable where whole-minute offsets are an established bound, but not a universal parity proof. **[P/I]**[^croner-lib][^fc-cron]
6. **Preserve the source's two searches and sampled minimum if API compatibility is the objective.** At most 1,054,080 candidate matches across validation; stop on checked date arithmetic overflow or the exact horizon. Do not silently expand to four-year searches or strengthen the floor to every possible interval. These are finite operation-count bounds, **not measured latency guarantees**. Avoid adding this potentially expensive loop to an unbounded request workload without budget/caching design. **[I/U]**
7. **Classify daily independently of the next two timestamps.** For the restricted five-field numeric language, require one expanded minute, one expanded hour, all months, and a day predicate true for every valid date. The public field methods allow bounded probing; a complete 400-year Gregorian cycle has 146,097 dates and avoids assuming a short sample proves all DOM/DOW combinations. Duplicate lists and full-domain ranges/steps should classify by their parsed meaning, not by exact spelling. Do not reject nominal daily schedules merely because a DST day has zero or two actual matching instants. **[I]**

Chrono's `TimeZone::from_local_datetime` returns `LocalResult::{Single, Ambiguous, None}`; ambiguity carries earliest/latest instants. Do not unwrap that result or choose one by habit if implementing a later optimized local-calendar search. The UTC-search proposal avoids having to choose between those local mappings in the first place. **[D/I]**[^chrono-local]

This strategy is **faithful within a declared/qualified syntax and timezone envelope**, not a claim to reproduce every malformed string accepted by the JavaScript implementation. If the required target is unrestricted byte-for-byte publisher acceptance, the listed parser differences are blocking design objections—not permission to call all rejected forms invalid. **[I/U]**

## 7. Actionable objections before implementation or admission

- **Do not introduce DOM/DOW OR as a “cron fix.”** It contradicts the pinned API. A standards-oriented alternative must be an explicit version/policy change.
- **Do not accept schedules through duration fallback.** The current helper permits unsupported words and ignores calendar/timezone constraints; the operation note already covers the legacy grading consequences.
- **Do not use default parser success as contract success.** Syntax, next-run existence, sampled floor, business-daily recurrence, and eventual checks are different assertions.
- **Decide strict subset versus full acceptance parity.** Record unsupported wildcard-list, large-step, singleton-step, and JavaScript-coercion cases; neither false success nor blanket “vendor invalid” rejection is acceptable.
- **Do not use Croner 2.2.0 `FromStr` or its direct next-run helper uncritically.** Both have source-established traps for this use case.
- **Review dependency features before selecting the small release.** Chrono default-feature activation is real; build/MSRV/lock resolution and latency remain untested.
- **Pin/adjudicate timezone data and documentation drift.** Chrono-tz's `2025b` is known; hosted ICU data and the docs' text-spreading claim are not established by this API-source pin.
- **Keep future qualification separate.** When authorized later, cover full-domain spellings, duplicate/list/range/step combinations, 0/7 Sundays, DOM/DOW intersection, impossible dates, sparse/leap dates, strict-after boundaries, sampled-floor timing, and chained gap/fold results. Include Lord Howe/Troll/midnight transitions, not just one-hour US DST. No such tests were run here.

These are research objections and recommendations **[I/U]**, not a migration, admission decision, passed control, or closure claim.

## Primary sources and byte bindings

All external technical evidence is first-party code, publisher distribution/metadata, versioned API documentation, or the owning specification. Source associations use exact revisions; current vendor docs remain mutable.

| Inspected object | SHA-256 |
| --- | --- |
| `firecrawl@4.50.0` published archive | `1aafbcd39f490e422902ba9dc611047c9de13823f90b71a36de15ee074ae3cb7` |
| Published SDK `src/v2/types.ts` | `debaf6f3b309ddd19b0a6af3106d459308d3cf3738b41f8dc57405531cca10af` |
| Pinned API `monitoring/cron.ts` | `616dd07a651f3363231219677a62f03143b3b498ccc2718f281f30db99266ebc` |
| Pinned API `monitoring/cron.test.ts` | `9df30517b3a2cad43b74dbc29050f96c0d681b37a35cfc0a7a071fcc035a544e` |
| `croner@2.2.0` published archive, matching crates.io checksum | `c344b0690c1ad1c7176fe18eb173e0c927008fdaaa256e40dfd43ddd149c0843` |
| Published Croner `src/lib.rs` | `04b329a50ef07909156846cd0b9ec36d78b5905f94f8761c19fadb2dbb75275d` |
| Published Croner `src/pattern.rs` | `25d7f47e678beee287689afd5fa8e5bcc731d2174af6d80f2247fb5136e2e87a` |
| Published Croner `src/component.rs` | `74a6f13139344a5c252d8729a293c503df856d6e1ce9b9931e8deaaa21c00351` |
| `chrono-tz@0.10.4` published archive, matching local lock checksum | `a6139a8597ed92cf816dfb33f5dd6cf0bb93a6adc938f11039f371bc5bcd26c3` |

Croner's published `.cargo_vcs_info.json` identifies **`13c00664d46c88a4a3832ad4c602fb079c9157b8`**. The three Rust source files above were also fetched at that revision and their hashes matched the archive. This is publisher source association, not an independently verified signed attestation. **[P/U]**

[^prior]: [Existing operation-contract research](2026-11-07-firecrawl-operation-contract.md), especially evidence discipline, publisher provenance, scheduling, and static grading objections.
[^sdk-package]: npm publisher [metadata for firecrawl 4.50.0](https://registry.npmjs.org/firecrawl/4.50.0) and [published archive](https://registry.npmjs.org/firecrawl/-/firecrawl-4.50.0.tgz), inspected without installation/execution.
[^sdk-types]: Firecrawl at `fc74e903469990c1ab72f33abdff4352c7e9d8e8`, [SDK `types.ts`, `MonitorSchedule`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/js-sdk/firecrawl/src/v2/types.ts#L1190); published source inspected too.
[^sdk-monitor]: Same revision, [SDK monitor HTTP methods](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/js-sdk/firecrawl/src/v2/methods/monitor.ts#L43); published source inspected too.
[^fc-cron]: Same revision, [`monitoring/cron.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/cron.ts): text conversion at line 32; fields at 108; DOW normalization at 148; five fields at 157; timezone at 172; day intersection at 221; next-run search at 287; validation at 329.
[^fc-tests]: Same revision, [`monitoring/cron.test.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/cron.test.ts), minute reference, chained DST cases, annual/impossible-date and floor cases; read, not executed.
[^fc-types]: Same revision, [`monitoring/types.ts`, schedule schema](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/types.ts#L104).
[^fc-controller]: Same revision, [`controllers/v2/monitor.ts`, create/update validation](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/controllers/v2/monitor.ts#L196).
[^fc-store]: Same revision, [`monitoring/store.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/store.ts#L440), create and later dispatch/completion/skipped-check schedule updates at lines 766, 802, and 834.
[^fc-scheduler]: Same revision, [`monitoring/scheduler.ts`, `deferForJitter`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/scheduler.ts#L160).
[^fc-jitter]: Same revision, [`monitoring/jitter.ts`](https://github.com/firecrawl/firecrawl/blob/fc74e903469990c1ab72f33abdff4352c7e9d8e8/apps/api/src/services/monitoring/jitter.ts).
[^monitor-docs]: Firecrawl-owned [Monitoring documentation, Schedules](https://docs.firecrawl.dev/features/monitoring#schedules), current mutable documentation read for this sidecar.
[^local-text]: [`crates/services/firecrawl/src/text.rs:271`](../../crates/services/firecrawl/src/text.rs#L271), `schedule_interval_minutes`; cron duration shortcut at line 326.
[^local-deps]: [`Cargo.toml:57`](../../Cargo.toml#L57), exact Chrono/chrono-tz requirements and disabled defaults; [`Cargo.lock:355`](../../Cargo.lock#L355), selected versions.
[^local-manifest]: [`crates/services/firecrawl/Cargo.toml:7`](../../crates/services/firecrawl/Cargo.toml#L7), current service dependencies. [`lib.rs:1910`](../../crates/services/firecrawl/src/lib.rs#L1910), schedule normalization; `next_run` at line 2019 uses a duration.
[^croner-registry]: crates.io publisher [croner metadata](https://crates.io/api/v1/crates/croner) and [2.2.0 source archive](https://crates.io/api/v1/crates/croner/2.2.0/download); metadata publication/yank state and archive checksum inspected directly.
[^croner-manifest]: Published 2.2.0 `Cargo.toml`/`Cargo.toml.orig`; corresponding [immutable source manifest](https://github.com/Hexagon/croner-rust/blob/13c00664d46c88a4a3832ad4c602fb079c9157b8/Cargo.toml).
[^croner-lib]: Published 2.2.0 [`src/lib.rs`](https://github.com/Hexagon/croner-rust/blob/13c00664d46c88a4a3832ad4c602fb079c9157b8/src/lib.rs): explicit parse at 121, matching at 170, next search at 233, AND option at 454, `FromStr` at 486, timezone conversion at 643.
[^croner-pattern]: Published 2.2.0 [`src/pattern.rs`](https://github.com/Hexagon/croner-rust/blob/13c00664d46c88a4a3832ad4c602fb079c9157b8/src/pattern.rs), parsing/default seconds/DOW handling, day matching at 288, public field-match APIs and options.
[^croner-component]: Published 2.2.0 [`src/component.rs`](https://github.com/Hexagon/croner-rust/blob/13c00664d46c88a4a3832ad4c602fb079c9157b8/src/component.rs), list parser at 227 and stepping at 390.
[^croner-docs]: [Versioned Rust documentation for Cron 2.2.0](https://docs.rs/croner/2.2.0/croner/struct.Cron.html), cross-checked against published source.
[^tz-source]: [Published chrono-tz 0.10.4 archive](https://crates.io/api/v1/crates/chrono-tz/0.10.4/download), `src/prebuilt/directory.rs` declares `IANA_TZDB_VERSION = "2025b"`; manifest contains optional case-insensitive feature.
[^tz-docs]: [Versioned chrono-tz 0.10.4 documentation](https://docs.rs/chrono-tz/0.10.4/chrono_tz/), timezone trait implementations, parsing, and elapsed-24-hour/DST example.
[^chrono-docs]: [Chrono 0.4.45 `DateTime` documentation](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html#method.with_timezone), `with_timezone` and UTC timestamp construction.
[^chrono-local]: [Chrono 0.4.45 `LocalResult`](https://docs.rs/chrono/0.4.45/chrono/offset/enum.LocalResult.html), gap/fold results and earliest/latest ordering.
[^intl-spec]: ECMA-402, [GetAvailableNamedTimeZoneIdentifier](https://tc39.es/ecma402/#sec-getavailablenamedtimezoneidentifier), ASCII-case-insensitive lookup; current living specification, not a measured API runtime version.
