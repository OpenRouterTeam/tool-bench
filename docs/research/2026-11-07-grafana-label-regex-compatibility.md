# Grafana35 label-regex compatibility review — 2026-11-07

## Result and boundaries

**The current admitted syntax is not yet a sound Go/Alertmanager-compatible subset.** In particular, leading-zero/whitespace repetition spellings and leading-`]` character classes can turn a genuinely different native predicate into the exact requested ASCII singleton in the translated language. A successful DFA comparison would then prove the wrong proposition. Unicode case-folding pins also differ. Sharing this translator between grading and the server makes those errors consistent, not correct.

This is a bounded, research-only review of the current working files, not the full Grafana overhaul. Only this Markdown file is authored. No tests, builds, dependency installations, Docker/image operations, account/infrastructure operations, model inference, or delegation were performed. The Terraform34 freeze at `/tmp/tool-bench-e34-qualification-freeze.b8PkDv`, both v34 images, code, manifests, scenarios, generated tasks, and capsules were not modified.

Evidence conventions:

- **Source fact:** directly inspected local implementation or independently fetched pinned first-party source.
- **Static counterexample:** exact strings and their consequences derived from those sources; **not an executed test result**. Predicted grading success assumes the bounded product search completes.
- **Unproven:** a necessary compatibility condition not established by this inspection. This is not evidence of full conformance or a verified production deployment.

Patterns below are **regex-source text**, not JSON/Go/Rust string-literal syntax. Thus `\d` contains one backslash; a JSON payload would encode it as `"\\d"`. No conclusion is based on sampling the authored fixture's label values.

## Independent source pins

| Layer | Pin and evidence | What it establishes |
| --- | --- | --- |
| Local comparator | `Cargo.lock`: `regex 1.13.1`, `regex-automata 0.4.18`, `regex-syntax 0.8.11`; `Cargo.toml:57` permits compatible `regex` upgrades, while line 58 fixes automata with `=0.4.18` | These are the inspected lockfile versions, not a guarantee about a later resolution. |
| Rust regex | Crate VCS revision `2b527599eb9eea0dcc288c704584f242f26a5c61` | Public regex syntax and compilation-error contract.[^rust-regex][^rust-errors] |
| Rust automata | Crate VCS revision `e6ad938198e68339a593f85d228f2d44cb54718f` | DFA start, delayed-match, EOI, Unicode-boundary, and limit behavior.[^rust-automaton][^rust-dense] |
| Rust syntax | Crate VCS revision `140167995737fa11dfe11b8af8b9aa143b790b4e` | Actual count/escape/class parsing and Unicode **16.0.0** simple-fold table.[^rust-parser][^rust-escape][^rust-hir][^rust-fold] |
| Grafana server comparison | Grafana `11.6.4`, commit `0567ef29beee0e82ea8d4da6cd4e3511d277324f` | Its `go.mod` says `go 1.24.4`; its Dockerfile defaults to `golang:1.24.4-alpine`. The module replaces upstream Alertmanager with the Grafana fork below.[^grafana-module][^grafana-docker] |
| Go regexp comparison | `go1.24.4`, commit `6796ebb2cb66b316a07998cdcd69b1c486b8579e` | `regexp/syntax` parser and documentation, Unicode **15.0.0** tables. This is the reference selected for the advertised Grafana version, not a measured running-server toolchain.[^go-doc][^go-parser][^go-unicode] |
| Grafana's Alertmanager fork | `v0.25.1-0.20240930132144-b5e64e81e8d3`, full commit `b5e64e81e8d3630666a75582716a912a41093ba5` | Actual wrapper `regexp.Compile("^(?:" + v + ")$")`, silence validation/cache paths, and silence-list filter contract.[^fork-pin][^fork-matcher][^fork-silence][^fork-api] |
| RE2 companion comparison | RE2 `2024-07-02`, peeled commit `6dcd83d60f7944926bfd308cc13979fc53dd69ca` | Independently confirms count lexical rules and escape grammar. **Not** a claim that Grafana links C++ RE2: Go regexp is the relevant implementation, and C++ RE2 accepts `\C` whereas Go explicitly rejects it.[^re2-parser][^re2-syntax] |
| Client-dependency cross-check | Alertmanager `v0.34.1`, commit `73c6bfe7393929211294c1954f30d8ed78e4d0ad` | Its matcher uses the same wrapper. Its presence in a client dependency graph does not pin the Grafana server; it is not substituted for the Grafana fork in this review.[^am-matcher] |

The cached Rust crates' `.cargo_vcs_info.json` supplied their exact revisions. Read-only SHA-256 comparisons against the corresponding immutable upstream files matched for regex `src/lib.rs`, automata `dfa/automaton.rs`, and syntax `ast/parse.rs`, `src/lib.rs`, `hir/translate.rs`, and `unicode_tables/case_folding_simple.rs`. No package was installed. The earlier `2026-11-07-grafana-operation-contract.md` was read only as context; the relevant upstream sources above were independently retrieved.

Working HEAD is `4ef643dac101fba8e67701f8c0ac7f9dd6e3e5fd`, but extensive pre-existing changes mean HEAD does not identify the reviewed code. Observed input SHA-256 values:

| Working input | SHA-256 |
| --- | --- |
| `crates/mock-core/src/label_matchers.rs` | `d86e35668c0ee6617067d0edf6016026f60c17c2e8b76b68ca13c478102139cc` |
| `crates/mock-core/tests/label_matchers.rs` | `767c3d16361584813909212ad4f1c9c43ff4ce8e419b8c009460d8624d8e2992` |
| `crates/services/grafana/src/alerting.rs` | `7be965d37828926dbd380a71036f65c36222b200ec5027aba50faab0ea7d9b97` |
| `crates/services/grafana/tests/grafana.rs` | `cf10588412a8e23091bc44800d0907db7ffc81d922ec8ce4ace479acd2c7fe62` |
| `Cargo.toml` | `de212b0e6f9a8e5ee76a2b30317aa3cf501d7a8a5bdf15b1682d82ac954823be` |
| `Cargo.lock` | `bc26d3e891461d03bd8bdfa6867c0aec6eb678fb974b44bb0e5efcc9e4382f94` |

## Inspected admission and coverage

`crates/mock-core/src/label_matchers.rs:20` implements a character scanner, not a Go parser. It imposes a 4,096-byte source bound; translates Perl classes, ASCII boundaries, bell, and quoting; allows `i/m/s/U` flags; rejects selected forms; then interpolates the result into `\A(?:...)\z`. Most remaining syntax is delegated to Rust's parser. Successful Rust parsing is therefore being used as a proxy for Go parsing even where the grammars differ.

`compile_label_regex` at line 131 is shared by server matcher validation and `label_match` at `crates/services/grafana/src/alerting.rs:318`. The grading path instead calls `regex_source` and builds DFAs directly at `crates/mock-core/src/label_matchers.rs:285`. Translation errors become `NeedsAdjudication` there; server `Invalid` becomes HTTP 400 and `Unsupported` becomes unavailable at `crates/services/grafana/src/alerting.rs:351`.

The six existing matcher tests cover exact spellings, explicitly closed quoting, conjunction/negation, a simple bracket singleton, broad/contradictory predicates, missing/empty labels, outside-class `\d` versus an Arabic digit, malformed flag types, and one unsupported Unicode property. They do **not** cover the cases below, case-insensitive matching, multiline behavior, inside-class shorthand/range interactions, repetition lexical rules, or error-category distinctions. The Grafana alerting integration test at `crates/services/grafana/tests/grafana.rs:538` uses ordinary literal silence matchers; it does not independently establish regex compatibility. This is a coverage reading, not a statement that tests passed.

## Findings: admitted false-success and semantic differences

### F1 — Blocker: Go-literal braces become Rust quantifiers

**Location:** `crates/mock-core/src/label_matchers.rs:126`; braces pass through without Go lexical validation.

| Source pattern | Pinned Go/Alertmanager meaning | Current translated Rust meaning |
| --- | --- | --- |
| `pg-replica-2{01}` | Literal `pg-replica-2{01}` | Exactly `pg-replica-2` |
| `pg-replica-2{1,01}` | Literal `pg-replica-2{1,01}` | Exactly `pg-replica-2` |
| `pg-replica-2{ 1 }` | Literal `pg-replica-2{ 1 }` | Exactly `pg-replica-2` |

Go `parseInt` disallows leading zeros. If `parseRepeat` cannot recognize a count, the parser treats `{` literally; these are **valid Go regexes with different languages**, not invalid counts. Rust `parse_decimal` accepts `01` as one and skips leading/trailing whitespace around a decimal even without free-spacing mode.[^go-parser][^rust-parser] RE2's `ParseInteger` independently disallows leading zeros.[^re2-parser]

**Exact grading consequence:** keep the ordinary `alertname` matcher from `base()` and replace the `instance` matcher with any row above, `isRegex:true`. The Rust predicate is the requested singleton, while the native predicate does not match `pg-replica-2` at all. The DFA cannot repair this lexical mistranslation.

**Bounded fix:** recognize only canonical Go counts with a bounded lexer, preserving Go's literal-brace fallback, or classify ambiguous brace spellings as `Unsupported` pending qualification. Do not normalize noncanonical counts into numbers. Escaping a brace fallback must preserve the entire original source meaning.

### F2 — Blocker: leading literal `]` defeats class-state tracking

**Location:** `crates/mock-core/src/label_matchers.rs:117` sets `in_class=false` for every unescaped `]`, including a legal first literal `]`.

Exact source:

```text
pg-replica-[]2&&2]
```

Go treats the leading `]` as a member, so the suffix class is `{']', '2', '&'}`. Rust also recognizes a leading literal `]`, but then interprets the unescaped `&&` as intersection: `{']','2'} ∩ {'2'} = {'2'}`. The translator fails to escape those ampersands because its class-state bit is already false. Both parsers accept the pattern.[^go-parser][^rust-parser]

**Exact witnesses:** native matches `pg-replica-]` and `pg-replica-&`; translated Rust rejects both. Both match `pg-replica-2`. Replacing the base `instance` matcher with this pattern therefore gives another translated singleton that is natively broader.

**Bounded fix:** track class opening, optional leading negation, first-member status, escapes, and actual closure. Until that exists, conservatively return `Unsupported` for leading-`]` classes rather than forwarding them. The same state bug can expose Rust `~~` or `--` operators, or bypass the intended nested/POSIX-class admission gate.

### F3 — High: flattening positive shorthand classes changes range boundaries

**Location:** `crates/mock-core/src/label_matchers.rs:55` removes the brackets from positive `\d`, `\w`, and `\s` when inside a class.

| Source | Go meaning | Translator output / Rust meaning |
| --- | --- | --- |
| `[\w-a]` | ASCII word class, literal `-`, literal `a` | `[A-Za-z0-9_-a]`; `_` through `a` introduces backtick and consumes the hyphen as range syntax |
| `[\s-z]` | ASCII whitespace, literal `-`, literal `z` | `[\t\n\f\r -z]`; space through `z` becomes a range |
| `[0-\d]` | **Invalid**: a class escape cannot be a scalar range endpoint | `[0-0-9]`; valid Rust class containing `0`, `-`, and `9` |

For `[\w-a]`, `-` matches natively but not after translation; backtick matches after translation but not natively. For `[\s-z]`, `A` matches only after translation. Go consumes a shorthand as an entire class item before considering the next scalar/range; textual expansion loses that boundary.[^go-parser][^rust-parser]

**Bounded fix:** preserve class-item boundaries in a small AST/set-union representation; do not splice raw range text into an enclosing class. A conservative initial gate can return `Unsupported` for shorthand adjacent to raw hyphens. Explicitly reject native-invalid class endpoints such as `[0-\d]` when the bounded parser can establish invalidity.

### F4 — High: admitted `i` uses the wrong Unicode simple-fold table

**Location:** `crates/mock-core/src/label_matchers.rs:92` passes active `i` flags through.

The selected Go source has Unicode 15.0.0; the pinned Rust syntax table is Unicode 16.0.0 and explicitly contains the pair U+1C89 ↔ U+1C8A. The Go table does not have that pair. Thus `(?i:\x{1C89})` matches U+1C8A after translation but not in the selected Go reference.[^go-unicode][^go-parser][^rust-fold]

**Exact false-success construction without changing the ASCII target:** retain both base matchers and add an unconstrained label, for example:

```text
name: other_label
isRegex: true
value: (?:(?i:\x{1C89})|[^\x{1C8A}])*
```

In Rust, the single-character alternatives cover every Unicode scalar, so the starred language is every valid UTF-8 string, including empty. Under Go 15 tables, U+1C8A remains excluded; a label with that value is rejected. The translated auxiliary predicate is a tautology, while the native scope is narrower. The product proof only sees the tautology.

**Bounded fix:** temporarily classify active case-insensitive syntax as `Unsupported`, or explicitly qualify bounded case closures against a fixed Go-compatible table. Do not “fix” this by globally disabling Unicode: even ASCII source `(?i:k)` must match Kelvin sign U+212A under the pinned Go implementation, and `(?i:s)` must match long s U+017F. Likewise, an ASCII expected value does not restrict all possible competing label values to ASCII.[^go-parser][^go-unicode]

**Uncertainty:** a real hosted server may use a different Go toolchain/table. A newer toolchain might remove this particular version difference, but no running-server pin was inspected. It cannot justify equivalence to the explicitly selected reference.

### F5 — High: Rust accepts repetitions that Go rejects

**Location:** same unrestricted pass-through as F1.

- `a{1001}`, `a{0,1001}`, and `a{1001,}` exceed Go's 1,000 minimum/maximum count bound.
- `(?:a{100}){11}` violates Go's **nested effective-count** check even though each written count is at most 1,000.
- `a**`, `a++`, `a?+`, and `a{1}{1}` are invalid adjacent repetition operators in Go. Rust parses stacked operators as nested repetitions; accepting `a++` does not give it a valid Go “possessive” interpretation.[^go-doc][^go-parser][^rust-parser]

Two especially small false-success candidates for the existing exact-instance goal are:

```text
pg-replica-2{1}{1}
pg-replica-2(?:a{1001}){0}
```

Rust's language for each is exactly `pg-replica-2`; Go rejects each source. The second demonstrates why post-simplification Rust language equality cannot establish native validity: the invalid inner repeat can disappear from the translated HIR.

**Bounded fix:** validate the original token stream before optimization, enforce Go's count limit and nested count-product rule, and reject stacked quantifiers except the single legal nongreedy `?` suffix. If this is outside the intended grammar, gate it as unqualified rather than relying on Rust compilation.

### F6 — High: escaped punctuation can acquire Rust-only assertion semantics

**Location:** `crates/mock-core/src/label_matchers.rs:75` forwards backslash plus any non-alphanumeric character.

Go treats escaped ASCII punctuation `\<` and `\>` as literal `<` and `>`. The pinned Rust parser explicitly reserves them as start/end word-boundary assertions.[^go-parser][^re2-parser][^rust-escape][^rust-parser]

Exact source `\<pg-replica-2\>` matches the literal string `<pg-replica-2>` in Go, but the current server regex matches `pg-replica-2` and not the bracketed string. Inside a class, `[\<]` is valid Go for literal `<` but is rejected by Rust because an assertion is not a class member.

The dense-DFA path does not enable heuristic Unicode-word-boundary support, so the outside-class example should require adjudication there rather than necessarily produce grading success. That does not make the server's successful, changed-language compilation safe.[^rust-dense]

**Bounded fix:** decode valid Go escaped ASCII punctuation to a literal atom and re-escape that atom for Rust. Never forward foreign escape spellings merely because both parsers accept a backslash. Go also requires the generic escaped punctuation character to be ASCII; the current Unicode `is_alphanumeric` test is not that rule.

### F7 — High: unterminated quoting is valid raw Go, but not the native wrapper

**Location:** `crates/mock-core/src/label_matchers.rs:34` consumes to EOF and never records whether `\E` occurred.

Exact source:

```text
\Qpg-replica-2
```

Raw Go `regexp.Compile` accepts an unterminated `\Q` quote and treats the remainder literally. So does the RE2 reference. But the inspected matcher does **not** compile raw source: it compiles `^(?:\Qpg-replica-2)$`. The inserted `)$` is swallowed as quoted text, leaving the noncapturing group unclosed. That native anchored matcher cannot compile. The local translator instead closes the quote implicitly before adding its own wrapper, yielding a valid singleton and a potential false-success grading result.[^go-parser][^re2-parser][^fork-matcher]

The fork's raw-pattern silence validation and its later matcher-cache compilation are distinct paths. This inspection does not claim every HTTP entry point rejects this pattern at the same stage.[^fork-silence]

**Bounded fix:** require explicit `\E` for admitted quoting and report the exclusion precisely. This can be `Unsupported` as a subset rule; do not call the raw Go regex intrinsically invalid. If modeling `NewMatcher` errors, explain that its anchored wrapper is invalid. Explicitly closed `\Q...\E` remains a separate supported case.

### F8 — Proof precondition missing: interpolation need not preserve anchors

**Location:** `crates/mock-core/src/label_matchers.rs:128` adds a group without first validating source group balance.

Exact source:

```text
pg-replica-2)|(?:pg-replica-2
```

Raw Go rejects the unbalanced source, but the local wrapper becomes:

```text
\A(?:pg-replica-2)|(?:pg-replica-2)\z
```

That is a valid **partially anchored alternation**, not a full-string predicate. The server regex matches `pg-replica-2-suffix` and `prefix-pg-replica-2`. The Go `NewMatcher` string interpolation has the analogous structural hazard; raw-pattern validation and wrapper compilation must not be conflated.[^go-parser][^fork-matcher]

The DFA code requests anchored starts and only checks match states after EOI. The automata contract says search can enter a match state before EOI and later leave it; general search membership is not determined by its final state alone.[^rust-automaton] Full anchoring on **every** branch makes the current EOI-only technique appropriate, but the scanner has not established that invariant. **The exact product-search verdict for this injected pattern was not executed or independently established here.**

**Bounded fix:** parse and balance the admitted original grammar before interpolation, keeping group, class, quote, and escape states separate. Construct the anchored expression structurally. Do not broaden the proof engine to arbitrary search expressions just to accommodate malformed source.

## Checked details that are not themselves bugs

- **Vertical tab:** Go `\v` and the pinned Rust `\v` both mean exactly U+000B. Forwarding this escape is correct; it is not a vertical-whitespace class.[^go-doc][^rust-regex][^rust-parser]
- **Perl whitespace:** Go `\s` is exactly tab/LF/FF/CR/space, **excluding vertical tab**. The translator's expansion agrees outside the range-boundary and scanner hazards above. `\S` therefore includes VT. POSIX `[[:space:]]`, which does include VT, is a different class and currently intentionally excluded.[^go-doc]
- **Other ASCII classes:** `\d` denotes `0–9`, not Arabic-Indic digits; negated `\D` includes those non-ASCII digits. Basic `[\d]`, `[\D]`, and `[^\D]` can be represented by the generated class/nested-class forms without the range-adjacency problems. This is not qualification of every admitted class combination.[^go-doc][^rust-parser]
- **ASCII boundaries:** rewriting outside-class `\b`/`\B` to `(?-u:\b)`/`(?-u:\B)` is the right direction for Go's ASCII word-boundary definition. It should not be replaced with default Unicode boundaries just because literals are Unicode-aware.[^go-doc][^rust-hir]
- **Multiline anchors:** for well-formed balanced source, both default `$` implementations are strict end-of-text assertions, not PCRE-style “before final newline.” Active `m` makes inner `^`/`$` recognize LF line boundaries; it does not remove the enclosing full-string constraint. `(?m:^pg-replica-2$)` must still reject `pg-replica-2\n` and `x\npg-replica-2`. `(?m:a$\n^b)` matches `a\nb`, not `a\r\nb`. Rust CRLF mode is not enabled by the admitted flags.[^go-doc][^rust-hir]
- **Scope versus captures/greediness:** boolean language equivalence does not require identical capture numbering or greedy-match locations, provided the original syntax is valid and every branch remains fully anchored. This does not excuse admitting native-invalid stacked quantifiers.

## Unsupported is not synonymous with invalid

The current two-way enum is useful, but its classifications do not consistently denote native validity.

| Form | Pinned Go status | Current result / appropriate distinction |
| --- | --- | --- |
| `\p{L}`, `\P{Greek}` | Valid | Intentionally `Unsupported`; retaining this bounded exclusion is sound. |
| `\0`, `\00`, `\123` | Valid octal | Intentionally `Unsupported`; no need to implement octal for this task. |
| `\1`, `\18` | Invalid unsupported backreferences, not valid octal | Currently lumped into octal `Unsupported`; do not assert these are valid Go forms. |
| `[[:alpha:]]`, `[[:^space:]]` | Valid POSIX classes | Intentionally `Unsupported`; do not treat this as a failure to support required syntax. |
| `(?P<n>a)`, `(?<n>a)` | Valid named groups | Intentionally `Unsupported`; captures are not needed for the current bounded contract. |
| `[--]` | Valid literal-hyphen class | Intentionally `Unsupported` by the conservative repeated-hyphen gate. |
| `[a--b]` | Invalid descending scalar range | Same gate returns `Unsupported`; classification does not prove upstream validity. |
| `(?=a)`, `(?<=a)`, `(?>a)`, `(?x:a)` | Invalid in Go regexp | Currently generic group `Unsupported`; separate native-invalid forms where the grammar can establish invalidity. |
| `\C` | Invalid in Go; valid single-byte operation in C++ RE2 | Current rejection is correct for **Go**, not evidence of full RE2 conformance. |
| `\u0041`, `\U00000041`, `\E`, `[\b]` | Invalid Go syntax | Rejection is appropriate; Rust support for unrelated escape spellings is irrelevant. |
| `(?ii:a)` | Valid Go duplicate flag | Rust rejects a duplicate flag; current `Invalid` misdescribes native validity. Normalize a qualified flag grammar or return `Unsupported`. |
| `a{`, `a{word}` | Valid Go literal-brace expressions | Rust parsing errors currently become `Invalid`; escape Go-literal braces or gate them as unqualified. |
| `\x{D800}` | Go parser accepts a surrogate-valued escape | Rust rejects non-scalar code points. Either separately qualify the valid-UTF-8-domain semantics or mark `Unsupported`, not invalid Go syntax. |
| 300 capturing groups nested around `a` | Within the selected Go parser's height bound; 601 source bytes | Exceeds Rust's default AST nesting bound of 250; local resource admission is not native invalidity. |
| Source over 4,096 bytes / proof budget exhaustion | May be valid Go | Correctly a local bound: `Unsupported` / `NeedsAdjudication`, never evidence of semantic inequality. |

Validity classifications follow the actual pinned parser, not just the syntax summary.[^go-parser][^rust-parser][^re2-syntax] In particular, `Regex::new` distinguishes compiled-size exhaustion from syntax failure; `compile_label_regex` currently maps **all** errors to `Invalid`. Rust syntax errors can themselves reflect local nesting/resource limits or grammar differences.[^rust-errors]

For expressions the bounded validator cannot classify, returning `Unsupported` is acceptable, but the reason should mean “not qualified,” not “valid upstream” or “invalid upstream.” Preserve `NeedsAdjudication` in grading rather than inventing a failed predicate verdict.

## What the DFA proof does and does not establish

`crates/mock-core/src/label_matchers.rs:163` builds a product of the valid-UTF-8 universe, the requested singleton when applicable, and each translated matcher. Per-label conjunction and boolean complement model `isEqual`; extra label names must denote the universe. The universe includes empty and newline-containing strings. This is fundamentally stronger than checking a few fixture values.

The following are appropriate design choices under a correctly translated, fully anchored grammar:

- EOI transitions account for the automata API's one-byte-delayed matches.
- Byte traversal plus the universe DFA restricts comparison to valid UTF-8 strings; rejecting byte strings is not the same as claiming arbitrary Go-byte-string conformance.
- DFA build limits (2,000,000 bytes), determinization limits (8,000,000 bytes), product-work limits (2,000,000 machine transitions), and the visited-state bound return adjudication rather than a guessed boolean.
- Start/transition failures and quit states do not silently become nonmatches in the proof path.[^rust-automaton][^rust-dense]

**Not proven:** that `regex_source` preserves the pinned Go matcher language on its admitted inputs, that all admitted branches remain fully anchored, or that an observed hosted server uses the reference toolchain. F1–F7 supply concrete reasons the first claim is false as currently written; F8 exposes the anchoring precondition. A complete product search can establish exact equality of **the translated Rust languages**, not Go equivalence by itself.

The grading path and `Regex::new` server path also have different construction/resource admission: a server-accepted pattern may exceed DFA limits, correctly requiring adjudication. Conversely, the proof path does not pass through server regex error classification. Sharing the source translation is necessary but does not establish identical acceptance/error behavior of both consumers.

## Shared-server caveat outside the translator

`crates/services/grafana/src/alerting.rs:725` still selects silence-list results using name equality plus:

```text
f["isRegex"] == true || m["value"] == f["value"]
```

Thus a regex filter can accept a stored matcher with the same name regardless of value or operator, despite being compiled during route preflight. For example, an `instance=~"pg-replica-2"` filter can include a stored literal `instance="pg-replica-1"` matcher.

The pinned Grafana fork's `CheckSilenceMatchesFilterLabels` instead compares **name, matcher type, and source value** for silence-list filtering. This endpoint is not a request to run the filter regex against each stored pattern string or to prove language equivalence.[^fork-api] Therefore, “shared compiler” must not be presented as proof that silence-list filtering is corrected. A bounded follow-up should reproduce that endpoint-specific tuple contract, independently of alert-label predicate evaluation.

## Actionable bounded disposition

1. **Close admitted false-success holes before relying on the new semantic grade.** Initially gate noncanonical/raw brace spellings, leading-`]` classes, shorthand/hyphen combinations, active `i`, unterminated quoting, and unbalanced groups as unqualified where exact bounded parsing is absent. Translate escaped ASCII punctuation as literals. Native-invalid forms established by the parser should be rejected, not interpreted as Rust regexes.
2. **Specify a small explicit grammar and two separate obligations:** original syntax is valid/admitted under the selected matcher contract; translation preserves its language over valid UTF-8. Build anchors around a validated structure. This does not require pretending to implement all RE2.
3. **Keep omission honest:** Unicode properties, octal, POSIX classes, named captures, and hard cases may remain intentionally `Unsupported`. Distinguish those exclusions and local resource limits from genuinely invalid native syntax.
4. **For later implementation, add targeted differential regressions against the selected Go source and both local consumers:** all exact witnesses F1–F8; positive/negative shorthand inside and outside classes; `\s`/`\S`/`\v`/`\f`; Kelvin sign/long s/new fold pairs; counts at 1,000 and 1,001 plus nested products; LF/CRLF anchors; explicit/unclosed quotes; literal versus assertion escapes; and unsupported/invalid/resource error paths. These are proposed tests, **none were added or run**.
5. **Retain exact product proof and fail-closed adjudication.** Do not replace it with fixture-label sampling, assume ASCII because the expected instance is ASCII, enable heuristic Unicode boundaries as a conformance shortcut, or expand to full RE2 as a prerequisite for fixing this bounded subset.

## Primary references

[^grafana-module]: Grafana 11.6.4 [pinned `go.mod`](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/go.mod), Go directive and Alertmanager replacement.
[^grafana-docker]: Grafana 11.6.4 [pinned Dockerfile](https://github.com/grafana/grafana/blob/0567ef29beee0e82ea8d4da6cd4e3511d277324f/Dockerfile), default `GO_IMAGE`; read-only source evidence, no container operation.
[^fork-pin]: Grafana Alertmanager fork [published module metadata](https://proxy.golang.org/github.com/grafana/prometheus-alertmanager/@v/v0.25.1-0.20240930132144-b5e64e81e8d3.info), full origin revision.
[^fork-matcher]: Grafana Alertmanager fork [`pkg/labels/matcher.go`](https://github.com/grafana/prometheus-alertmanager/blob/b5e64e81e8d3630666a75582716a912a41093ba5/pkg/labels/matcher.go), `NewMatcher` and `Matches`.
[^fork-silence]: Grafana Alertmanager fork [`silence/silence.go`](https://github.com/grafana/prometheus-alertmanager/blob/b5e64e81e8d3630666a75582716a912a41093ba5/silence/silence.go), `validateMatcher`, `matcherCache.add`, and `QMatches`.
[^fork-api]: Grafana Alertmanager fork [`api/v2/api.go`](https://github.com/grafana/prometheus-alertmanager/blob/b5e64e81e8d3630666a75582716a912a41093ba5/api/v2/api.go), `getSilencesHandler` and `CheckSilenceMatchesFilterLabels`.
[^am-matcher]: Alertmanager v0.34.1 [`pkg/labels/matcher.go`](https://github.com/prometheus/alertmanager/blob/73c6bfe7393929211294c1954f30d8ed78e4d0ad/pkg/labels/matcher.go), independent wrapper cross-check only.
[^go-doc]: Go 1.24.4 [`regexp/syntax/doc.go`](https://github.com/golang/go/blob/6796ebb2cb66b316a07998cdcd69b1c486b8579e/src/regexp/syntax/doc.go), RE2-derived grammar summary, escapes, ASCII classes, anchors, count restriction.
[^go-parser]: Go 1.24.4 [`regexp/syntax/parse.go`](https://github.com/golang/go/blob/6796ebb2cb66b316a07998cdcd69b1c486b8579e/src/regexp/syntax/parse.go), `parse`, `parseInt`, `parseRepeat`, `repeat`, `repeatIsValid`, `parsePerlFlags`, `parseEscape`, `parseClass`, and `appendGroup`. Multiple findings use different independently inspected branches of this source.
[^go-unicode]: Go 1.24.4 [`unicode/tables.go`](https://github.com/golang/go/blob/6796ebb2cb66b316a07998cdcd69b1c486b8579e/src/unicode/tables.go), `Version = "15.0.0"` and case tables.
[^re2-parser]: RE2 [`re2/parse.cc`](https://github.com/google/re2/blob/6dcd83d60f7944926bfd308cc13979fc53dd69ca/re2/parse.cc), `ParseInteger`, `ParseEscape`, and quote parsing.
[^re2-syntax]: RE2 [`doc/syntax.txt`](https://github.com/google/re2/blob/6dcd83d60f7944926bfd308cc13979fc53dd69ca/doc/syntax.txt), documented supported/unsupported syntax; C++ RE2 is not interchangeable with Go regexp.
[^rust-regex]: Rust regex 1.13.1 [`src/lib.rs`](https://github.com/rust-lang/regex/blob/2b527599eb9eea0dcc288c704584f242f26a5c61/src/lib.rs), syntax, escape, Unicode, and resource discussion.
[^rust-errors]: Rust regex 1.13.1 [`src/error.rs`](https://github.com/rust-lang/regex/blob/2b527599eb9eea0dcc288c704584f242f26a5c61/src/error.rs), `Syntax` versus `CompiledTooBig`.
[^rust-parser]: Rust regex-syntax 0.8.11 [`src/ast/parse.rs`](https://github.com/rust-lang/regex/blob/140167995737fa11dfe11b8af8b9aa143b790b4e/regex-syntax/src/ast/parse.rs), `parse_decimal`, repetition parsing, flags, escapes, class-opening/range/operation parsing, and source-level unit-test expectations. Its existing unit-test source was read, not executed.
[^rust-escape]: Rust regex-syntax 0.8.11 [`src/lib.rs`](https://github.com/rust-lang/regex/blob/140167995737fa11dfe11b8af8b9aa143b790b4e/regex-syntax/src/lib.rs), `is_escapeable_character` and reserved `\<`/`\>`.
[^rust-hir]: Rust regex-syntax 0.8.11 [`src/hir/translate.rs`](https://github.com/rust-lang/regex/blob/140167995737fa11dfe11b8af8b9aa143b790b4e/regex-syntax/src/hir/translate.rs), assertion selection, folding/negation, and valid-UTF-8 guarantees.
[^rust-fold]: Rust regex-syntax 0.8.11 [`case_folding_simple.rs`](https://github.com/rust-lang/regex/blob/140167995737fa11dfe11b8af8b9aa143b790b4e/regex-syntax/src/unicode_tables/case_folding_simple.rs), Unicode 16.0.0 generator header and explicit U+1C89/U+1C8A pair.
[^rust-automaton]: Rust regex-automata 0.4.18 [`src/dfa/automaton.rs`](https://github.com/rust-lang/regex/blob/e6ad938198e68339a593f85d228f2d44cb54718f/regex-automata/src/dfa/automaton.rs), `Automaton` contract, `start_state_forward`, `next_eoi_state`, match-state and dead/quit-state semantics.
[^rust-dense]: Rust regex-automata 0.4.18 [`src/dfa/dense.rs`](https://github.com/rust-lang/regex/blob/e6ad938198e68339a593f85d228f2d44cb54718f/regex-automata/src/dfa/dense.rs), `Config` limits, anchored start support, and opt-in heuristic Unicode word boundaries.
