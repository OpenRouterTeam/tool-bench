# Comparable tool/agent benchmarks

**Evidence checked:** November 3, 2026. **Scope:** primary-source research, not an implementation audit. Local context is limited to [README](../../README.md) and the [approved design](../superpowers/specs/2026-10-30-tool-bench-design.md).

The README describes 1,595 MCP tools across 30 services; the design targets **model + harness + router** comparisons with pinned MCP interfaces, deterministic mock APIs, state/journal grading, and one instruction per scenario. These are project claims, not independently verified here. The most useful external benchmarks supply complementary evaluation techniques—not interchangeable scores.

## Comparisons

### MCPMark — closest protocol-level comparator

**Observation.** MCPMark executes stateful tasks through MCP in environments including Notion, GitHub, PostgreSQL, Playwright, and filesystem; verification is programmatic and checks task outcomes.[^mcp-paper] Its official repository now defaults to **MCPMark Verified**, with pinned environments and stabilized verifiers, and explicitly deprecates earlier scores as non-comparable. It supports repeated runs and isolated environments.[^mcp-repo] The Verified change reviewed all 127 standard tasks, rebuilding some fixtures/verifiers and clarifying descriptions—not merely relabeling old results.[^mcp-verified] These are real MCP environments, rather than this repository's uniform mock-vendor layer.[^mcp-paper]

**Borrow.** Maintain explicit task-validity review, independent successful solutions, reset checks, and a versioned “verified” task set. **Boundary:** shared MCP transport does not imply equivalent backend realism, catalog exposure, or task difficulty; distinguish original/Verified results and pin environment configuration.

### ToolSandbox — state, intermediate milestones, and distractors

**Observation.** ToolSandbox uses executable tools over a stateful simulated environment, with a simulated user and implicit state dependencies. It evaluates trajectories using intermediate milestones, negative “minefields,” and dependency graphs rather than only a final answer or a rigid reference call sequence. Its paper's catalog has **34 tools**; augmentations vary distracting tools, names, and descriptions, testing discovery under controlled perturbations rather than thousand-tool exposure.[^sandbox-paper] Its repository provides execution and inspection tooling for scenarios.[^sandbox-repo]

**Borrow.** Add semantic intermediate constraints and negative checks where final-state equality could miss a harmful action subsequently undone; test tool-order and distractor robustness. **Boundary:** its user simulation and social/temporal state create failure modes absent from a fixed single instruction.

### τ-bench / τ²-bench — policy-constrained conversational completion

**Observation.** Original τ-bench combines executable retail/airline API simulations, domain policy, and an LLM-simulated user. It checks resulting database state against annotated goals and introduces **pass^k**, reliability across repeated attempts—not pass@k, success at least once.[^tau-paper] Its repository explicitly calls that implementation outdated and directs users to the successor.[^tau-original]

τ²-bench adds a telecom environment in which user and agent can both act, with coordination requirements.[^tau2-paper] Versioning now matters especially: the old repository directs readers to “τ³-bench,” while the linked repository currently presents **τ-bench 2.0**, with multilingual text/voice and distinct frozen `text-v1`, `voice-v1`, and `voice-v2` task sets. It also warns that an earlier banking grader fix invalidates comparisons to previous banking results.[^tau-original][^tau-current] The current evaluator supports state/action checks and optional LLM-judged natural-language assertions, with reward composition depending on task configuration; executable tools do not imply universally judge-free scoring.[^tau2-eval]

**Borrow.** Distinguish outcome success, policy compliance, and repeat reliability. **Boundary:** domain-scoped tools, interactive users, and changing task/evaluator versions make its percentages inappropriate baselines for a large-catalog router experiment. Treat conversational extensions here as a separate track, not a requirement to abandon the single-instruction design.

### AppWorld — richest adjacent sandbox and collateral-change checking

**Observation.** AppWorld supplies an executable simulated personal world with nine applications and 457 APIs. Tasks require API/code interactions over persistent app state; evaluation checks expected and allowed database changes, rejecting collateral changes. Database and time reset to task-specific starting states.[^app-paper] The repository separates train/dev/test usage and prohibits test-set error analysis or tuning, addressing leakage rather than assuming publicly downloadable tasks remain unseen.[^app-repo]

**Borrow.** Verify that the desired change happened **and unrelated state remained intact**; keep held-out task/evaluator material isolated from the agent. **Boundary:** a comprehensive simulator can provide stronger executable semantics without being a live service. Its application/API counts are not MCP schema counts or evidence that it tests an all-tools-upfront context window of this repository's size.

### BFCL — invocation correctness and multi-turn decomposition

**Observation.** BFCL separates AST-based matching from execution-based checking, includes relevance/irrelevance cases, and adds stateful multi-turn categories such as missing parameters, missing functions, and long context. Multi-turn evaluation combines per-turn state checks with subset-matched necessary-call checks, including read-only operations. Its “live” data means **user-contributed invocation cases**, not a guarantee of live vendor backends.[^bfcl3] The official leaderboard currently labels itself **BFCL V4**, with an April 12, 2026 update; V4 adds web-search and memory-oriented agentic evaluation.[^bfcl-board][^bfcl4]

**Borrow.** Report discovery/relevance, argument correctness, and downstream state success separately; add explicit “no suitable tool” controls. **Boundary:** AST agreement can test a call without establishing a valid world outcome. Do not pool its heterogeneous categories into a comparison with this repository's end-to-end reward, or treat web/memory scores as catalog retrieval scores.

### ToolBench / StableToolBench — large catalog, but different execution guarantees

**Observation.** ToolBench constructs instruction/solution data around **16,464 real-world APIs from 3,451 RapidAPI tools**, supplies an API retriever, and evaluates with ToolEval's LLM-based judgments. Task/instruction generation is synthetic even though API collection and execution involve real services.[^toolbench] These APIs, RapidAPI “tools,” and MCP tools are different counting units.

StableToolBench addresses unstable APIs using a cache, then real calls on misses, then LLM simulation when unavailable, plus revised LLM-based evaluation.[^stable-paper] Its current repository also offers **MirrorAPI** simulation and a final-answer-only correctness evaluator; report which variant is used.[^stable-repo] **Inference:** cached, live, and generated responses have different fidelity/repeatability guarantees; none alone establishes an independently specified executable database for every API.

**Borrow.** Separate retrieval quality from downstream completion, and freeze external responses when needed. **Boundary:** a large searchable API pool is not proof that every definition was supplied simultaneously. Learned response simulation or LLM judging would change this project's deterministic, judge-free contract; retain those, if useful, as separately labeled experiments.

### ToolRet — optional retrieval-only complement

**Observation.** The ToolRet paper reports roughly 7.6k retrieval queries over a pool of 43k tools assembled from multiple datasets, and evaluates retrieval methods rather than stateful task execution. It highlights a gap between document-retrieval performance and tool retrieval.[^toolret]

**Borrow.** Add a frozen query-to-catalog retrieval diagnostic with recall/ranking metrics and downstream follow-through. **Boundary:** tool recall is not task success, annotated relevant tools need not exhaust valid alternative plans, and its heterogeneous tool pool is not equivalent to 43k executable MCP endpoints.

## Concrete recommendations

These are recommendations derived from the comparisons, **not findings of missing or broken local implementation**.

1. **State the measured construct narrowly.** Position the benchmark as deterministic, large-catalog **system-level** evaluation of a model/harness/router arm. Publish the real-server/replica and mock-backend fidelity distinctions. Avoid claiming general autonomous-agent, vendor-production, or conversational-policy superiority from its score.
2. **Validate graders beyond oracle/nop.** Borrow AppWorld's collateral-change checks, ToolSandbox's temporal safety constraints, and MCPMark Verified's task review.[^app-paper][^sandbox-paper][^mcp-repo] For selected scenarios, build independent valid solutions plus deliberately invalid traces: wrong entity, omitted subgoal, plausible fabricated answer, harmful action later undone. Accept semantically equivalent plans; require exact calls only when the call is itself a task or safety requirement.
3. **Separate discovery from execution.** Alongside full-catalog arms, use an oracle/minimal-tool diagnostic and a context-feasible all-tools control; label these as different exposure conditions. Measure offered versus loaded tool/schema tokens, search requests/results, tool-loading failures, wrong-service selection, execution failure, final state, latency, and cost. Count retrieval overhead in the same budget. Treat deliberate context overflow as an outcome, not a silently dropped run.
4. **Freeze the evaluation unit and estimate variance.** Record task/data/grader revision, server/image/schema hashes, harness and router configuration, provider/model identity, serialized tool payload, sampling settings, search limits, and retries. Replay determinism is not evidence of deterministic model behavior. Repeat each arm; report paired per-scenario wins/losses and uncertainty over scenarios, preserving repeated attempts within each scenario when resampling. Report single-attempt success separately from repeat reliability; never substitute pass@k for pass^k.[^tau-paper] Publish infrastructure-failure counts and exclusion rules.
5. **Keep fidelity and scope tracks separate.** Maintain the existing deterministic, noninteractive core. Optional future tracks can add policy constraints, scripted clarification or a simulated user, and selected higher-fidelity backend conformance tests. Declare any user-model/judge randomness explicitly; do not blend these results into the core score.
6. **Make catalog difficulty auditable.** Publish tool/API counting rules, schema-token accounting, relevant-tool density, confusable alternatives, and task-by-service coverage. Add controlled distractor/order/schema-description perturbations while preserving the canonical catalog. If using ToolRet-style retrieval labels, annotate alternative successful tool sets rather than treating a single oracle trace as exhaustive.[^toolret]

**Score-comparison rule:** use these projects to compare evaluation *designs*. Numerical comparisons require matched tasks, initial state, tool exposure, user/policy protocol, budgets, evaluator version, retry semantics, and aggregation—not simply the same model name.

## Primary sources

Repository/leaderboard links below were inspected on November 3, 2026; they are moving references, not immutable reproduction pins. Papers document their stated versions. A reproducible experiment should archive the exact upstream commits and datasets used.

[^mcp-repo]: MCPMark maintainers, [official repository and Verified migration guidance](https://github.com/eval-sys/mcpmark), especially “MCPMark Verified” and “Task Execution & Evaluation.”
[^mcp-paper]: MCPMark authors, [MCPMark: A Benchmark for Stress-Testing Realistic and Comprehensive MCP Use](https://arxiv.org/abs/2509.24002).
[^mcp-verified]: MCPMark maintainers, [Verified release changes, PR #264](https://github.com/eval-sys/mcpmark/pull/264), including pinned environments and all-task verifier review.
[^sandbox-paper]: Apple researchers, [ToolSandbox: A Stateful, Conversational, Interactive Evaluation Benchmark for LLM Tool Use Capabilities](https://arxiv.org/html/2408.04682v1), especially evaluation and augmentation design.
[^sandbox-repo]: Apple, [official ToolSandbox repository](https://github.com/apple-aiml-research/ToolSandbox).
[^tau-paper]: τ-bench authors, [τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045).
[^tau-original]: Sierra, [original τ-bench repository and successor notice](https://github.com/sierra-research/tau-bench).
[^tau2-paper]: Sierra researchers, [τ²-bench: Evaluating Conversational Agents in a Dual-Control Environment](https://arxiv.org/abs/2506.07982).
[^tau-current]: Sierra, [current successor repository](https://github.com/sierra-research/tau2-bench), especially “τ-bench 2.0,” released benchmark frames, and previous-release grading warning.
[^tau2-eval]: Sierra, official [evaluator source](https://raw.githubusercontent.com/sierra-research/tau2-bench/main/src/tau2/evaluator/evaluator.py) and [natural-language assertion evaluator](https://raw.githubusercontent.com/sierra-research/tau2-bench/main/src/tau2/evaluator/evaluator_nl_assertions.py).
[^app-paper]: AppWorld authors, [AppWorld: A Controllable World of Apps and People for Benchmarking Interactive Coding Agents](https://arxiv.org/html/2407.18901v1), especially §§2.2 and 3.2.
[^app-repo]: AppWorld maintainers, [official repository](https://github.com/StonyBrookNLP/appworld), especially installation, environment interaction, and evaluation.
[^bfcl3]: BFCL authors, [BFCL V3: Multi-Turn & Multi-Step Function Calling](https://gorilla.cs.berkeley.edu/blogs/13_bfcl_v3_multi_turn.html), also describing AST/execution and the meaning of “live.”
[^bfcl-board]: BFCL maintainers, [official V4 leaderboard and category definitions](https://gorilla.cs.berkeley.edu/leaderboard.html).
[^bfcl4]: BFCL authors, [BFCL V4: Web Search](https://gorilla.cs.berkeley.edu/blogs/15_bfcl_v4_web_search.html), including agentic category composition and links to memory evaluation.
[^toolbench]: OpenBMB, [official ToolBench repository](https://github.com/OpenBMB/ToolBench), including dataset construction, API retrieval, and ToolEval.
[^stable-paper]: StableToolBench authors, [StableToolBench: Towards Stable Large-Scale Benchmarking on Tool Learning of Large Language Models](https://arxiv.org/abs/2403.07714).
[^stable-repo]: THUNLP-MT, [official StableToolBench repository](https://github.com/THUNLP-MT/StableToolBench), especially MirrorAPI and final-answer-correctness evaluation.
[^toolret]: ToolRet authors, [Retrieval Models Aren't Tool-Savvy: Benchmarking Tool Retrieval for Large Language Models](https://arxiv.org/abs/2503.01763); [official code](https://github.com/mangopy/tool-retrieval-benchmark).
