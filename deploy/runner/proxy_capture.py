"""Host provider-proxy inference capture for container-side CLI agents.

Claude Code runs inside the task container, so no host object observes its turns.
Its provider traffic is the only seam the host owns: `deploy/arm-proxy.ts` runs on the
host, holds the real credential, and gives the agent nothing but its address and a
per-trial placeholder key. This module starts that proxy, reads its hash-chained
events from a host-only directory, and seals them into the same InferenceEvidence the
Rust verifier and report load.

`classify` mirrors `mock_core::transport::derive`. The verifier recomputes the
classification from the recorded exchanges and rejects a document that disagrees.

The capture certifies what the provider returned to the agent process. It does not
certify that the CLI relayed those replies faithfully to anything else, and it parses
Anthropic Messages traffic only.
"""
from __future__ import annotations

import hashlib
import json
import os
import secrets
import shutil
import signal
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Optional
from urllib.parse import urlsplit

from deploy.runner.admission import AdmissionError
from deploy.runner.inference import (
    INFERENCE_EVIDENCE_SCHEMA_VERSION,
    AdapterIdentity,
    CostAccounting,
    FinalizationStatus,
    FinalResponse,
    InferenceClassification,
    InferenceEvidence,
    InferenceIdentity,
    InferenceTurn,
    TurnKind,
    _PROXY_ADAPTER_NAME,
    _PROXY_SOURCE,
    _proxy_adapter_sha256,
)

PROXY_ADAPTER_NAME = _PROXY_ADAPTER_NAME
PROXY_AGENT_NAME = "claude-code"
SEAM = "host_provider_proxy"
WIRE_FORMAT = "anthropic_messages"
HOST_CONSUMER_SEAM = "host_stdout_stream_json"
ARMS = {
    # arm -> (client tool search, universal deferred-tools control)
    "off": (False, False),
    "native": (True, False),
    "universal": (False, True),
}
TRANSIENT = {"rate_limit", "overloaded", "provider_error", "transport_error", "incomplete_stream"}
_AGENT_ATTRIBUTE = "_tool_bench_provider_proxy"


def _sha256(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


# ---------------------------------------------------------------------------
# Events -> exchanges
# ---------------------------------------------------------------------------

def read_events(path: Path) -> tuple[list[dict], list[str]]:
    """Read the proxy's event chain. Returns the intact prefix and capture faults."""
    faults: list[str] = []
    events: list[dict] = []
    try:
        if path.is_symlink():
            raise OSError("capture events cannot be a link")
        lines = path.read_bytes().split(b"\n")
    except OSError:
        return [], ["capture_events_missing"]
    if lines and lines[-1] == b"":
        lines.pop()
    else:
        faults.append("event_chain_truncated")
    previous = None
    for index, line in enumerate(lines):
        try:
            event = json.loads(line)
        except ValueError:
            event = None
        if not isinstance(event, dict) or event.get("seq") != index or event.get("prev") != previous:
            faults.append("event_chain_broken")
            break
        events.append(event)
        previous = _sha256(line)
    if not events or events[0].get("kind") != "start" or events[0].get("v") != 1:
        faults.append("capture_not_started")
    final = events[-1] if events else {}
    if final.get("kind") != "finalized":
        faults.append("proxy_not_finalized")
    elif final.get("in_flight") != 0:
        faults.append("in_flight_at_finalization")
    if any(event.get("kind") == "finalized" for event in events[:-1]):
        faults.append("events_after_finalization")
    return events, faults


def _failure(response: dict) -> Optional[str]:
    failure = response.get("failure")
    if failure is not None:
        return failure
    if response.get("malformed"):
        return "malformed_response"
    if not response.get("complete"):
        return "incomplete_stream"
    return None


def _number(value, kind=int):
    return value if type(value) in ((int,) if kind is int else (int, float)) and value >= 0 else None


def build_exchanges(events: list[dict], faults: list[str]) -> tuple[list[dict], dict]:
    """Pair request and response events into exchanges; record what cannot be paired."""
    requests = {event["id"]: event for event in events if event.get("kind") == "request"}
    responses = {event["id"]: event for event in events if event.get("kind") == "response"}
    exchanges = []
    other = 0
    for identifier in sorted(requests):
        request = requests[identifier]
        response = responses.get(identifier)
        if not request.get("inference"):
            other += 1
            continue
        if request.get("wire") != WIRE_FORMAT:
            faults.append("unsupported_wire_format")
            continue
        if response is None:
            faults.append("exchange_without_response")
            continue
        usage = response.get("usage") or {}
        failure = _failure(response)
        exchanges.append({
            "sequence": request["seq"],
            "role": None,
            "turn": None,
            "request_sha256": request["request_sha256"],
            "response_sha256": response.get("response_sha256"),
            "status": response.get("status"),
            "streamed": bool(request.get("stream")),
            "complete": bool(response.get("complete")) and failure is None,
            "latency_ms": _number(response.get("latency_ms")),
            "first_byte_ms": _number(response.get("first_byte_ms")),
            "response_id": response.get("response_id"),
            "observed_model": response.get("response_model"),
            "stop_reason": response.get("stop_reason"),
            "failure": failure,
            "input_tokens": _number(usage.get("input_tokens")),
            "cache_read_input_tokens": _number(usage.get("cache_read_input_tokens")),
            "cache_creation_input_tokens": _number(usage.get("cache_creation_input_tokens")),
            "output_tokens": _number(usage.get("output_tokens")),
            "cost_usd": _number(usage.get("cost"), float),
            # Private to the sealer; removed before the document is written.
            "_main": request.get("main"),
            "_thread": request.get("thread"),
            "_messages": request.get("messages_sha256"),
            "_assistant_turns": request.get("assistant_turns", 0),
            "_tools": request.get("tools", 0),
            "_final_text": response.get("final_text"),
            "_end": response["seq"],
        })
    timed = [event for event in events if event.get("kind") in ("request", "response")]
    summary = {
        "other_requests": other,
        "rejected_requests": sum(event.get("kind") == "rejected" for event in events),
        "wall_clock_ms": timed[-1]["t_ms"] - timed[0]["t_ms"] if timed else None,
    }
    return exchanges, summary


def assign_threads(exchanges: list[dict], faults: list[str]) -> bool:
    """Mark main-thread exchanges and their turns. Returns whether the session was resumed."""
    main = [exchange for exchange in exchanges if exchange["_main"] is True]
    for exchange in exchanges:
        exchange["role"] = "main" if exchange["_main"] is True else "auxiliary"
    if any(exchange["_main"] is None for exchange in exchanges):
        faults.append("main_thread_unidentified")
    if not main:
        # Tool-bearing work with no identifiable main thread is not an idle agent.
        if any(exchange["_tools"] for exchange in exchanges):
            faults.append("main_thread_unidentified")
        return False
    if len({exchange["_thread"] for exchange in main}) > 1:
        faults.append("ambiguous_main_thread")
    last_end = main[-1]["_end"]
    if any(exchange["role"] == "auxiliary" and exchange["_tools"] and exchange["failure"] is None
           and exchange["sequence"] > last_end for exchange in exchanges):
        faults.append("untracked_continuation")
    resumed = main[0]["_assistant_turns"] > 0
    succeeded = 0
    for exchange in main:
        if not resumed and exchange["_assistant_turns"] > succeeded:
            faults.append("unobserved_assistant_turn")
            break
        succeeded += exchange["failure"] is None
    admission_failure = not any(exchange["failure"] is None for exchange in main) and (
        main[-1]["failure"] in ("authentication", "invalid_request"))
    if not admission_failure:
        turn = -1
        previous = None
        for exchange in main:
            retry = (previous is not None and previous["failure"] in TRANSIENT
                     and previous["_messages"] == exchange["_messages"])
            if not retry:
                turn += 1
            exchange["turn"] = turn
            previous = exchange
    return resumed


# ---------------------------------------------------------------------------
# The classification (mirrors mock_core::transport::derive)
# ---------------------------------------------------------------------------

def _kind(exchange: dict) -> str:
    failure = exchange["failure"]
    if failure == "context_length":
        return TurnKind.CONTEXT_REJECTED.value
    if failure == "malformed_response":
        return TurnKind.UNPARSEABLE.value
    if failure is not None:
        return TurnKind.PROVIDER_ERROR.value
    if exchange["stop_reason"] in ("tool_use", "pause_turn"):
        return TurnKind.TOOL_USE.value
    if exchange["stop_reason"] in ("end_turn", "stop_sequence", "max_tokens", "refusal"):
        return TurnKind.FINAL_RESPONSE.value
    return TurnKind.UNPARSEABLE.value


def classify(transport: dict, final_text: bool, max_turns: Optional[int]):
    """Return (classification, finalization, turn kinds) for recorded provider exchanges."""
    C, F = InferenceClassification, FinalizationStatus
    main = [exchange for exchange in transport["exchanges"] if exchange["role"] == "main"]
    if transport["capture_faults"]:
        return C.FAILED_FINALIZATION, F.CAPTURE_FAILED, []
    if not main:
        return C.ENVIRONMENT_STARTUP_FAILURE, F.NO_RESPONSE, []
    terminal = main[-1]
    failure = terminal["failure"]
    if not any(exchange["failure"] is None for exchange in main) and failure in (
            "authentication", "invalid_request"):
        return (C.SCHEMA_ADMISSION_FAILURE if failure == "invalid_request"
                else C.ENVIRONMENT_STARTUP_FAILURE), F.NO_RESPONSE, []
    kinds: list[str] = []
    for exchange in main:
        if exchange["turn"] == len(kinds):
            kinds.append(_kind(exchange))
        else:
            kinds[exchange["turn"]] = _kind(exchange)
    if transport["resumed"]:
        return C.RESUMED_SESSION, F.NO_RESPONSE, kinds
    retried = any(a["turn"] == b["turn"] for a, b in zip(main, main[1:]))
    if failure == "context_length":
        return (C.FIRST_TURN_CONTEXT_REJECTION if terminal["turn"] == 0
                else C.LATER_TURN_CONTEXT_REJECTION), F.NO_RESPONSE, kinds
    if failure in TRANSIENT:
        return C.UNRECOVERED_RATE_LIMIT, F.NO_RESPONSE, kinds
    if failure == "malformed_response":
        return C.MALFORMED_MODEL_OUTPUT, F.NO_RESPONSE, kinds
    if failure is not None:
        return C.FAILED_FINALIZATION, F.CAPTURE_FAILED, kinds
    if transport["interrupted"]:
        return C.FAILED_FINALIZATION, F.INTERRUPTED, kinds
    stop = terminal["stop_reason"]
    if stop in ("end_turn", "stop_sequence") and final_text:
        return (C.RECOVERED_RATE_LIMIT if retried else C.COMPLETED), F.NORMAL, kinds
    if stop == "max_tokens":
        return C.BUDGET_EXHAUSTED, F.NO_RESPONSE, kinds
    if stop in ("tool_use", "pause_turn") and max_turns is not None and len(kinds) >= max_turns:
        return C.BUDGET_EXHAUSTED, F.NO_RESPONSE, kinds
    return C.NO_PUBLIC_RESPONSE, F.NO_RESPONSE, kinds


def _prompt_tokens(exchange: dict) -> Optional[int]:
    if exchange["input_tokens"] is None:
        return None
    return (exchange["input_tokens"] + (exchange["cache_read_input_tokens"] or 0)
            + (exchange["cache_creation_input_tokens"] or 0))


def _complete_sum(values: list):
    return sum(values) if all(value is not None for value in values) else None


def seal_events(
    events: list[dict],
    faults: list[str],
    *,
    identity: InferenceIdentity,
    scenario: Optional[str],
    session_id: Optional[str],
    interrupted: bool,
    host_stdout_result: Optional[str] = None,
    host_stdout_faults: Optional[list[str]] = None,
    host_stdout_captured: bool = False,
) -> InferenceEvidence:
    """Turn proxy events into inference evidence. Unknown measurements stay None."""
    faults = list(faults)
    exchanges, summary = build_exchanges(events, faults)
    resumed = assign_threads(exchanges, faults)
    faults = sorted(set(faults))
    if faults:
        for exchange in exchanges:
            exchange["turn"] = None
    main = [exchange for exchange in exchanges if exchange["role"] == "main"]
    auxiliary = [exchange for exchange in exchanges if exchange["role"] == "auxiliary"]
    terminal_text = main[-1]["_final_text"] if main and not faults else None
    transport = {
        "seam": SEAM,
        "wire_format": WIRE_FORMAT,
        "exchanges": [{key: value for key, value in exchange.items() if not key.startswith("_")}
                      for exchange in exchanges],
        "capture_faults": faults,
        "rejected_requests": summary["rejected_requests"],
        "other_requests": summary["other_requests"],
        "interrupted": bool(interrupted),
        "resumed": resumed and not faults,
        "wall_clock_ms": summary["wall_clock_ms"],
        "exchanges_without_usage": sum(_prompt_tokens(exchange) is None or exchange["output_tokens"] is None
                                       for exchange in main),
        "auxiliary": {
            "requests": len(auxiliary),
            "input_tokens": _complete_sum([_prompt_tokens(exchange) for exchange in auxiliary]),
            "output_tokens": _complete_sum([exchange["output_tokens"] for exchange in auxiliary]),
            "cost_usd": _complete_sum([exchange["cost_usd"] for exchange in auxiliary]),
        },
    }
    max_turns = identity.budget.get("max_turns")
    carrier_required = classify(
        transport, bool(terminal_text), max_turns if type(max_turns) is int else None
    )[1] is FinalizationStatus.NORMAL
    provider_failure_observed = bool(main) and not carrier_required and not faults
    # Host-observed consumer evidence: the real ExecResult.stdout from
    # environment.exec, parsed for the terminal type=result envelope.
    host_stdout = None
    if host_stdout_captured:
        all_host_faults = list(host_stdout_faults or [])
        if carrier_required and host_stdout_result is not None and terminal_text is not None:
            if host_stdout_result != terminal_text:
                all_host_faults.append("host_stdout_provider_mismatch")
        elif carrier_required and terminal_text is not None and host_stdout_result is None:
            all_host_faults.append("host_stdout_missing_result")
        host_stdout = {
            "seam": HOST_CONSUMER_SEAM,
            "result_sha256": _sha256(host_stdout_result.encode()) if host_stdout_result is not None else None,
            "faults": sorted(set(all_host_faults)),
        }
        if all_host_faults and not provider_failure_observed:
            faults = sorted(set(faults + all_host_faults))
            transport["capture_faults"] = faults
    if host_stdout is not None:
        transport["host_stdout"] = host_stdout
    classification, finalization, kinds = classify(
        transport, bool(terminal_text), max_turns if type(max_turns) is int else None)
    # A completed native proxy capture REQUIRES the host-observed CLI terminal
    # carrier. Missing or faulted host_stdout is infrastructure/finalization
    # uncertainty, not ordinary model failure. The local CLI diagnostic path
    # (seal_events with host_stdout_captured=False) is unbound and does not
    # claim completed admission.
    if host_stdout_captured and finalization is FinalizationStatus.NORMAL:
        if host_stdout is None or host_stdout.get("faults"):
            classification = InferenceClassification.FAILED_FINALIZATION
            finalization = FinalizationStatus.CAPTURE_FAILED
            terminal_text = None
    turns = []
    retries = []
    for index, kind in enumerate(kinds):
        attempts = [exchange for exchange in main if exchange["turn"] == index]
        last = attempts[-1]
        retries.extend(attempts[:-1])
        turns.append(InferenceTurn(
            turn=index, kind=kind,
            input_tokens=_prompt_tokens(last), output_tokens=last["output_tokens"],
            retried=len(attempts) > 1, retry_count=len(attempts) - 1,
            cost_usd=last["cost_usd"], observed_model=last["observed_model"],
            response_id=last["response_id"],
        ))
    attempts = main if turns else []
    final_response = None
    if finalization is FinalizationStatus.NORMAL:
        final_response = FinalResponse(
            text=terminal_text, turn=len(turns) - 1, finalization=finalization.value)
    succeeded = (TurnKind.TOOL_USE.value, TurnKind.FINAL_RESPONSE.value)
    cost = CostAccounting(
        total_input_tokens=_complete_sum([_prompt_tokens(exchange) for exchange in attempts]) if attempts else None,
        total_output_tokens=_complete_sum([exchange["output_tokens"] for exchange in attempts]) if attempts else None,
        total_cost_usd=_complete_sum([exchange["cost_usd"] for exchange in attempts]) if attempts else None,
        retry_input_tokens=_complete_sum([_prompt_tokens(exchange) for exchange in retries]),
        retry_output_tokens=_complete_sum([exchange["output_tokens"] for exchange in retries]),
        retry_cost_usd=_complete_sum([exchange["cost_usd"] for exchange in retries]),
        successful_turns=sum(turn.kind in succeeded for turn in turns),
        retried_turns=sum(turn.retried for turn in turns),
        total_retry_attempts=len(retries),
    )
    if cost.retry_cost_usd is not None:
        cost.retry_cost_usd = float(cost.retry_cost_usd)
    if faults:
        # An incomplete capture knows nothing about retries; it does not report zero.
        cost = CostAccounting()
    observed = next((turn.observed_model for turn in reversed(turns) if turn.observed_model is not None), None)
    return InferenceEvidence(
        schema_version=INFERENCE_EVIDENCE_SCHEMA_VERSION,
        adapter=AdapterIdentity(name=PROXY_ADAPTER_NAME, code_sha256=_proxy_adapter_sha256()),
        classification=classification.value,
        turns=turns,
        final_response=final_response,
        cost=cost,
        identity=InferenceIdentity(
            configured_model=identity.configured_model,
            configured_provider=identity.configured_provider,
            observed_model=observed,
            sampling=dict(identity.sampling),
            budget=dict(identity.budget),
        ),
        scripted=False,
        finalization=finalization.value,
        scenario=scenario,
        session_id=session_id,
        transport=transport,
    )


# ---------------------------------------------------------------------------
# The host capture: owns the proxy process and the sealed document
# ---------------------------------------------------------------------------


def parse_host_stdout_result(stdout: Optional[str], *, exit_code: Optional[int] = None) -> tuple[Optional[str], list[str]]:
    """Extract the public terminal result text from Claude Code stream-json stdout.

    Returns (result_text, faults). The stdout is the host-observed ExecResult.stdout
    from environment.exec — not an agent-editable file. Claude Code's
    ``--output-format=stream-json --print`` emits a final ``{"type":"result", ...}``
    envelope whose ``"result"`` field is the public assistant reply shown to the user.

    Only one ``type=result`` event is accepted. Reasoning/thinking content is never
    extracted. If the stdout is missing, truncated, or contains no valid result
    envelope, the capture fails closed.

    Malformed JSON lines are faulted rather than silently skipped. Events after
    the terminal ``type=result`` are rejected. The ``session_id`` field must be
    a non-empty string. A nonzero process exit code is recorded as a fault.

    Carrier integrity requirements:
    - ``exit_code`` must be exactly ``0`` (integer); ``None`` or absent faults.
    - ``subtype`` on the result envelope must be exactly ``"success"``; absent faults.
    - ``is_error`` must be exactly ``False`` (bool) for a successful carrier;
      ``True`` faults, absent or non-bool faults.
    - The ``session_id`` on the ``system/init`` event and the ``type=result``
      event must be non-empty strings and must match.
    - Exactly one ``system/init`` event must precede the result; repeated init
      or init after the terminal result is faulted.
    """
    faults: list[str] = []
    if stdout is None:
        return None, ["host_stdout_missing"]
    if type(exit_code) is not int:
        faults.append("host_stdout_exit_not_int")
    elif exit_code != 0:
        faults.append("host_stdout_nonzero_exit")
    results = []
    seen_result = False
    has_malformed = False
    has_non_json_line = False
    init_count = 0
    init_session_id: Optional[str] = None
    for line in stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        if not line.startswith("{"):
            has_non_json_line = True
            continue
        try:
            event = json.loads(line)
        except (json.JSONDecodeError, ValueError):
            has_malformed = True
            continue
        if not isinstance(event, dict):
            has_malformed = True
            continue
        event_type = event.get("type")
        if event_type == "system" and event.get("subtype") == "init":
            if seen_result:
                faults.append("host_stdout_init_after_terminal")
            else:
                init_count += 1
                if init_count == 1:
                    sid = event.get("session_id")
                    if isinstance(sid, str) and sid.strip():
                        init_session_id = sid
                elif init_count > 1:
                    faults.append("host_stdout_repeated_init")
        elif event_type == "result":
            if seen_result:
                return None, faults + ["host_stdout_multiple_results"]
            seen_result = True
            results.append(event)
        elif seen_result:
            faults.append("host_stdout_post_terminal_event")
    if has_non_json_line:
        faults.append("host_stdout_non_json_line")
    if has_malformed:
        faults.append("host_stdout_malformed_json")
    if not results:
        return None, faults + ["host_stdout_no_result"]
    result_event = results[0]
    result_subtype = result_event.get("subtype")
    if result_subtype != "success":
        faults.append("host_stdout_result_subtype_not_success")
    text = result_event.get("result")
    if text is not None and not isinstance(text, str):
        return None, faults + ["host_stdout_result_not_text"]
    is_error = result_event.get("is_error")
    if is_error is True:
        faults.append("host_stdout_result_is_error")
    elif is_error is not False:
        faults.append("host_stdout_is_error_missing")
    session_id = result_event.get("session_id")
    if not isinstance(session_id, str) or not session_id.strip():
        faults.append("host_stdout_session_missing")
    elif init_session_id is not None and session_id != init_session_id:
        faults.append("host_stdout_session_mismatch")
    if init_count == 0:
        faults.append("host_stdout_no_init")
    elif init_session_id is None:
        faults.append("host_stdout_init_session_missing")
    return text, faults


class ProviderProxyCapture:
    """Runs the host provider proxy for one trial and seals what it observed."""

    def __init__(
        self,
        *,
        instruction: str,
        configured_model: str,
        arm: str,
        target: str,
        upstream_key: Optional[str],
        provider_pin: Optional[str] = None,
        model_rewrite: Optional[str] = None,
        max_turns: Optional[int] = None,
        base_path: str = "",
        listen: str = "127.0.0.1",
        advertise_host: str = "host.docker.internal",
        bun: str = "bun",
    ) -> None:
        if arm not in ARMS:
            raise AdmissionError("unsupported comparison arm")
        if not instruction.strip() or not configured_model:
            raise AdmissionError("provider proxy capture lacks the task instruction or model")
        target_parts = urlsplit(target)
        if target_parts.scheme not in ("http", "https") or not target_parts.hostname:
            raise AdmissionError("provider proxy target is not an HTTP origin")
        if max_turns is not None and (type(max_turns) is not int or max_turns <= 0):
            raise AdmissionError("turn budget must be a positive integer")
        self._instruction = instruction.strip()
        self._arm = arm
        self._target = target.rstrip("/")
        self._upstream_key = upstream_key
        self._provider_pin = provider_pin
        self._model_rewrite = model_rewrite
        self._base_path = base_path
        self._listen = listen
        self._advertise_host = advertise_host
        self._bun = bun
        self._token = "tb-trial-" + secrets.token_hex(24)
        self._adapter_digest = _proxy_adapter_sha256()
        self._source_sha256 = _sha256(_PROXY_SOURCE.read_bytes())
        tool_search, universal = ARMS[arm]
        self._identity = InferenceIdentity(
            configured_model=configured_model,
            configured_provider=target_parts.hostname,
            sampling={
                "arm": arm,
                "client_tool_search": tool_search,
                "proxy_universal_control": universal,
                **({"proxy_provider_pin": provider_pin} if provider_pin else {}),
                **({"proxy_model_rewrite": model_rewrite} if model_rewrite else {}),
            },
            budget={"max_turns": max_turns} if max_turns is not None else {},
        )
        self._directory: Optional[Path] = None
        self._process: Optional[subprocess.Popen] = None
        self._port: Optional[int] = None
        self._scenario: Optional[str] = None
        self._session_id: Optional[str] = None
        self._agent = None
        self._execution_failed = False
        self._host_stdout_result: Optional[str] = None
        self._host_stdout_faults: list[str] = []
        self._host_stdout_captured = False
        self._sealed = None
        self._sealed_json = None

    # -- lifecycle -------------------------------------------------------

    def start(self) -> None:
        if self._process is not None:
            raise AdmissionError("provider proxy was already started")
        executable = shutil.which(self._bun)
        if executable is None:
            raise AdmissionError("the host provider proxy runtime is unavailable")
        # Host-only state: never inside a trial directory an agent mount can reach.
        self._directory = Path(tempfile.mkdtemp(prefix="tool-bench-provider-proxy-"))
        self._directory.chmod(0o700)
        instruction = self._directory / "instruction.txt"
        instruction.write_text(self._instruction)
        events = self._directory / "events.jsonl"
        command = [
            executable, str(_PROXY_SOURCE), "--port", "0", "--listen", self._listen,
            "--target", self._target, "--capture", str(events),
            "--instruction-file", str(instruction),
        ]
        if ARMS[self._arm][1]:
            command.append("--universal")
        if self._provider_pin:
            command += ["--provider", self._provider_pin]
        if self._model_rewrite:
            command += ["--model", self._model_rewrite]
        environment = {"PATH": os.environ.get("PATH", ""), "HOME": os.environ.get("HOME", ""),
                       "ARM_PROXY_CLIENT_TOKEN": self._token}
        if self._upstream_key:
            environment["ARM_PROXY_UPSTREAM_KEY"] = self._upstream_key
        self._process = subprocess.Popen(
            command, env=environment, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
        )
        line = self._process.stdout.readline()
        prefix = "arm-proxy :"
        try:
            if not line.startswith(prefix):
                raise ValueError(line)
            self._port = int(line[len(prefix):].split(" ", 1)[0])
        except ValueError:
            self.close()
            raise AdmissionError("the host provider proxy did not start") from None

    @property
    def local_url(self) -> str:
        if self._port is None:
            raise AdmissionError("provider proxy is not running")
        return f"http://127.0.0.1:{self._port}{self._base_path}"

    def agent_env(self) -> dict[str, str]:
        """Everything the agent receives: the proxy address, a placeholder key, the arm switch."""
        if self._port is None:
            raise AdmissionError("provider proxy is not running")
        return {
            "ANTHROPIC_BASE_URL": f"http://{self._advertise_host}:{self._port}{self._base_path}",
            "ANTHROPIC_API_KEY": self._token,
            "ENABLE_TOOL_SEARCH": "true" if ARMS[self._arm][0] else "false",
        }

    def identity(self) -> dict:
        return {
            "configured_model": self._identity.configured_model,
            "configured_provider": self._identity.configured_provider,
            "observed_model": None,
            "observed_revision": None,
            "sampling": dict(self._identity.sampling),
            "budget": dict(self._identity.budget),
        }

    def bind_trial(self, trial, *, scenario: str, session_id: str) -> None:
        """Tie the running proxy to one admitted container-side agent."""
        from harbor.agents.installed.claude_code import ClaudeCode

        if self._agent is not None or self._sealed is not None:
            raise AdmissionError("provider proxy capture is already bound to a trial")
        if self._process is None or self._process.poll() is not None:
            raise AdmissionError("provider proxy is not running")
        if type(trial.agent) is not ClaudeCode or not scenario or not session_id:
            raise AdmissionError("unsupported container-side inference producer")
        if trial.task.instruction.strip() != self._instruction:
            raise AdmissionError("provider proxy was started for a different instruction")
        expected = self.agent_env()
        if trial.config.agent.model_name != self._identity.configured_model:
            raise AdmissionError("agent model differs from the declared proxy condition")
        if dict(trial.config.agent.env) != expected:
            raise AdmissionError("agent environment differs from the proxy-only condition")
        kwargs = dict(trial.config.agent.kwargs)
        budget = self._identity.budget.get("max_turns")
        if set(kwargs) - {"max_turns"} or kwargs.get("max_turns") != budget:
            raise AdmissionError("unsupported container-agent settings")
        resolved = trial.agent._resolve_auth_env()
        if resolved != {key: expected[key] for key in ("ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL")}:
            raise AdmissionError("a provider credential would reach the agent container")
        if self._upstream_key and self._upstream_key in json.dumps(
                [resolved, dict(trial.config.agent.env), kwargs]):
            raise AdmissionError("the real provider credential would reach the agent container")
        self._agent = trial.agent
        self._scenario = scenario
        self._session_id = session_id
        setattr(trial.agent, _AGENT_ATTRIBUTE, self)
        capture = self

        original_exec = trial.agent.exec_as_agent
        self._bound_exec = None
        self._bound_run = None

        async def observed_exec(environment, command, **kwargs):
            result = await original_exec(environment, command, **kwargs)
            if (isinstance(command, str)
                    and "output-format=stream-json" in command
                    and "--print" in command):
                if capture._host_stdout_captured:
                    capture._host_stdout_faults = ["host_stdout_duplicate_cli_execution"]
                    capture._host_stdout_result = None
                else:
                    stdout = getattr(result, "stdout", None)
                    exit_code = getattr(result, "return_code", None)
                    text, faults = parse_host_stdout_result(stdout, exit_code=exit_code)
                    capture._host_stdout_result = text
                    capture._host_stdout_faults = faults
                    capture._host_stdout_captured = True
            return result

        trial.agent.exec_as_agent = observed_exec
        self._bound_exec = observed_exec

        original_run = trial.agent.run

        async def run(*arguments, **keywords):
            try:
                return await original_run(*arguments, **keywords)
            except BaseException:
                capture._execution_failed = True
                raise

        trial.agent.run = run
        self._bound_run = run

    def finalize(self, *, interrupted: bool = False) -> InferenceEvidence:
        if self._sealed is not None:
            raise AdmissionError("provider proxy capture was already sealed")
        if self._process is None or self._directory is None:
            raise AdmissionError("provider proxy was never started")
        if self._agent is not None:
            if (getattr(self._agent, "exec_as_agent", None) is not self._bound_exec
                    or getattr(self._agent, "run", None) is not self._bound_run):
                raise AdmissionError("agent observer bindings were replaced after bind_trial")
        bound_no_cli = self._agent is not None and not self._host_stdout_captured
        faults = []
        if self._process.poll() is None:
            self._process.send_signal(signal.SIGTERM)
            try:
                self._process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self._process.kill()
                self._process.wait()
                faults.append("proxy_not_finalized")
        else:
            faults.append("proxy_exited_early")
        if (_sha256(_PROXY_SOURCE.read_bytes()) != self._source_sha256
                or _proxy_adapter_sha256() != self._adapter_digest):
            faults.append("capture_implementation_changed")
        events, chain_faults = read_events(self._directory / "events.jsonl")
        start = events[0] if events else {}
        if start.get("instruction_sha256") != _sha256(self._instruction.encode()) or not start.get("client_token"):
            faults.append("capture_not_started")
        evidence = seal_events(
            events, faults + chain_faults, identity=self._identity,
            scenario=self._scenario, session_id=self._session_id,
            interrupted=interrupted or self._execution_failed,
            host_stdout_result=self._host_stdout_result,
            host_stdout_faults=self._host_stdout_faults if self._host_stdout_captured else None,
            host_stdout_captured=self._host_stdout_captured,
        )
        if bound_no_cli and evidence.finalization == FinalizationStatus.NORMAL.value:
            evidence = seal_events(
                events, faults + chain_faults + ["host_stdout_cli_not_observed"],
                identity=self._identity,
                scenario=self._scenario, session_id=self._session_id,
                interrupted=interrupted or self._execution_failed,
                host_stdout_result=None,
                host_stdout_faults=["host_stdout_cli_not_observed"],
                host_stdout_captured=True,
            )
        self._sealed = evidence
        self._sealed_json = evidence.to_json()
        return evidence

    def validate_sealed(self, evidence, *, agent=None) -> None:
        if self._sealed is None or self._sealed is not evidence or evidence.to_json() != self._sealed_json:
            raise AdmissionError("inference evidence lacks its host capture authority")
        if self._agent is None or (agent is not None and agent is not self._agent):
            raise AdmissionError("inference capture belongs to a different host producer")
        if evidence.scenario != self._scenario or evidence.session_id != self._session_id:
            raise AdmissionError("inference capture belongs to a different trial")

    def close(self) -> None:
        if self._process is not None and self._process.poll() is None:
            self._process.kill()
            self._process.wait()
        if self._process is not None and self._process.stdout is not None:
            self._process.stdout.close()
        if self._directory is not None:
            shutil.rmtree(self._directory, ignore_errors=True)


def is_proxied_agent(agent) -> bool:
    try:
        from harbor.agents.installed.claude_code import ClaudeCode
    except ImportError:
        return False
    return type(agent) is ClaudeCode


def capture_for(trial) -> ProviderProxyCapture:
    capture = getattr(trial.agent, _AGENT_ATTRIBUTE, None)
    if type(capture) is not ProviderProxyCapture or capture._agent is not trial.agent:
        raise AdmissionError("container-side agent has no host provider proxy")
    return capture


def proxied_identity(trial) -> dict:
    """Configured inference identity of a proxied agent, for the realized manifest."""
    return capture_for(trial).identity()


# ---------------------------------------------------------------------------
# Local smoke path (deploy/smoke-arms.sh): same sealer, same classification
# ---------------------------------------------------------------------------

def main(arguments: list[str]) -> int:
    import argparse

    parser = argparse.ArgumentParser(
        description="Seal arm-proxy capture events into inference evidence (unbound local diagnostic)")
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--provider", required=True)
    parser.add_argument("--arm", choices=sorted(ARMS), required=True)
    parser.add_argument("--exit-code", type=int, default=0)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args(arguments)
    events, faults = read_events(options.events)
    tool_search, universal = ARMS[options.arm]
    evidence = seal_events(
        events, faults,
        identity=InferenceIdentity(
            configured_model=options.model, configured_provider=options.provider,
            sampling={"arm": options.arm, "client_tool_search": tool_search,
                      "proxy_universal_control": universal},
        ),
        scenario=None, session_id=None, interrupted=options.exit_code != 0,
    )
    ok, error = evidence.validates()
    if not ok:
        print(f"proxy capture is not valid inference evidence: {error}", file=sys.stderr)
        return 1
    options.output.write_text(evidence.to_json() + "\n")
    print(evidence.classification)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
