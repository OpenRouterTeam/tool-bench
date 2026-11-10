"""Trusted native inference evidence capture and failure classification.

This module provides fail-closed adapters for capturing host-observed inference
evidence from harness transports. It replaces competing classification paths
(empty journal grep, transcript text matching) with structured evidence.

Scripted providers (oracle/nop) use ScriptedEvidenceStub which is labeled as
non-native and cannot certify model admission. Native harness adapters must be
registered and validated; unknown adapters fail closed.

The Computer1 adapter captures LLMResponse events from the host-side Computer1
agent loop.  It observes the public assistant response text (not hidden
analysis/reasoning), observed identity/usage/cost per turn, and session
finalization status.  Unknown retries remain None.  Transport classification
(infrastructure vs behavioral) is distinct from semantic grading (pass/fail).

Plan items: 3 (integrity gate), 4 (semantic events), 5 (failure classification),
11 (final-response capture), 15 (manifest binding), 16 (explainable results).
"""
from __future__ import annotations

import hashlib
import json
import math
import threading
from dataclasses import asdict, dataclass, field, replace
from enum import Enum
from pathlib import Path
from typing import Optional
from deploy.runner.admission import AdmissionError


INFERENCE_EVIDENCE_SCHEMA_VERSION = 2


class InferenceClassification(str, Enum):
    COMPLETED = "completed"
    ENVIRONMENT_STARTUP_FAILURE = "environment_startup_failure"
    SCHEMA_ADMISSION_FAILURE = "schema_admission_failure"
    FIRST_TURN_CONTEXT_REJECTION = "first_turn_context_rejection"
    LATER_TURN_CONTEXT_REJECTION = "later_turn_context_rejection"
    RECOVERED_RATE_LIMIT = "recovered_rate_limit"
    UNRECOVERED_RATE_LIMIT = "unrecovered_rate_limit"
    MALFORMED_MODEL_OUTPUT = "malformed_model_output"
    TOOL_EXECUTION_FAILURE = "tool_execution_failure"
    POLICY_REFUSAL = "policy_refusal"
    TASK_FAILURE = "task_failure"
    RESUMED_SESSION = "resumed_session"
    FAILED_FINALIZATION = "failed_finalization"
    BUDGET_EXHAUSTED = "budget_exhausted"
    NO_PUBLIC_RESPONSE = "no_public_response"

    def is_infrastructure_fault(self) -> bool:
        return self in {
            self.ENVIRONMENT_STARTUP_FAILURE,
            self.SCHEMA_ADMISSION_FAILURE,
            self.UNRECOVERED_RATE_LIMIT,
            self.FAILED_FINALIZATION,
        }

    def had_inference(self) -> bool:
        return self not in {
            self.ENVIRONMENT_STARTUP_FAILURE,
            self.SCHEMA_ADMISSION_FAILURE,
        }

    def is_context_rejection(self) -> bool:
        return self in {
            self.FIRST_TURN_CONTEXT_REJECTION,
            self.LATER_TURN_CONTEXT_REJECTION,
        }


class TurnKind(str, Enum):
    TOOL_USE = "tool_use"
    FINAL_RESPONSE = "final_response"
    UNPARSEABLE = "unparseable"
    CONTEXT_REJECTED = "context_rejected"
    PROVIDER_ERROR = "provider_error"


class FinalizationStatus(str, Enum):
    NORMAL = "normal"
    INTERRUPTED = "interrupted"
    CAPTURE_FAILED = "capture_failed"
    NO_RESPONSE = "no_response"


@dataclass(frozen=True)
class InferenceTurn:
    turn: int
    kind: str
    input_tokens: Optional[int] = None
    output_tokens: Optional[int] = None
    retried: Optional[bool] = None
    retry_count: Optional[int] = None
    cost_usd: Optional[float] = None
    observed_model: Optional[str] = None
    response_id: Optional[str] = None


@dataclass(frozen=True)
class FinalResponse:
    text: Optional[str]
    turn: int
    finalization: str
    resumed: bool = False


@dataclass
class CostAccounting:
    total_input_tokens: Optional[int] = None
    total_output_tokens: Optional[int] = None
    total_cost_usd: Optional[float] = None
    retry_input_tokens: Optional[int] = None
    retry_output_tokens: Optional[int] = None
    retry_cost_usd: Optional[float] = None
    successful_turns: int = 0
    retried_turns: Optional[int] = None
    total_retry_attempts: Optional[int] = None


@dataclass(frozen=True)
class AdapterIdentity:
    name: str
    code_sha256: str


@dataclass
class InferenceIdentity:
    configured_model: str
    configured_provider: str
    observed_model: Optional[str] = None
    observed_revision: Optional[str] = None
    sampling: dict = field(default_factory=dict)
    budget: dict = field(default_factory=dict)


@dataclass
class InferenceEvidence:
    schema_version: int
    adapter: AdapterIdentity
    classification: str
    turns: list
    final_response: Optional[FinalResponse]
    cost: CostAccounting
    identity: InferenceIdentity
    scripted: bool
    finalization: str = FinalizationStatus.NO_RESPONSE.value
    scenario: Optional[str] = None
    session_id: Optional[str] = None

    def to_dict(self) -> dict:
        result = {
            "schema_version": self.schema_version,
            "adapter": asdict(self.adapter),
            "classification": self.classification,
            "turns": [asdict(t) for t in self.turns],
            "final_response": asdict(self.final_response) if self.final_response else None,
            "cost": asdict(self.cost),
            "identity": {
                "configured_model": self.identity.configured_model,
                "configured_provider": self.identity.configured_provider,
                "observed_model": self.identity.observed_model,
                "observed_revision": self.identity.observed_revision,
                "sampling": self.identity.sampling,
                "budget": self.identity.budget,
            },
            "scripted": self.scripted,
            "finalization": self.finalization,
            "scenario": self.scenario,
            "session_id": self.session_id,
        }
        return result

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=2, sort_keys=False, allow_nan=False)

    def validates(self) -> tuple[bool, Optional[str]]:
        if type(self.schema_version) is not int or self.schema_version != INFERENCE_EVIDENCE_SCHEMA_VERSION:
            return False, f"unsupported schema version {self.schema_version}"
        if not self.adapter.name or not _digest_valid(self.adapter.code_sha256):
            return False, "adapter identity is incomplete"
        if self.classification not in {value.value for value in InferenceClassification}:
            return False, "unknown inference classification"
        if self.finalization not in {value.value for value in FinalizationStatus}:
            return False, "unknown session finalization"
        if type(self.scripted) is not bool:
            return False, "invalid scripted/native flag"
        if not isinstance(self.identity.configured_model, str) or not self.identity.configured_model or (
                not isinstance(self.identity.configured_provider, str) or not self.identity.configured_provider):
            return False, "configured inference identity is missing"
        for value in [*self.identity.sampling.values(), *self.identity.budget.values()]:
            if type(value) not in (bool, int, float, str) or isinstance(value, float) and not math.isfinite(value):
                return False, "invalid inference settings"
        for i, turn in enumerate(self.turns):
            if type(turn.turn) is not int or turn.turn != i:
                return False, f"turn {i} has non-sequential index {turn.turn}"
            if turn.kind not in {value.value for value in TurnKind}:
                return False, "unknown inference turn kind"
            if turn.retried is not None and type(turn.retried) is not bool:
                return False, "invalid retry observation"
            for value in [turn.observed_model, turn.response_id]:
                if value is not None and type(value) is not str:
                    return False, "invalid observed inference identity"
            for value in [turn.input_tokens, turn.output_tokens, turn.retry_count]:
                if value is not None and (type(value) is not int or value < 0):
                    return False, "invalid inference token or retry count"
            if turn.cost_usd is not None and (type(turn.cost_usd) not in (int, float)
                    or not math.isfinite(turn.cost_usd) or turn.cost_usd < 0):
                return False, "invalid inference cost"
            if (turn.retried is None) != (turn.retry_count is None):
                return False, "retry observation must be known or unknown together"
            if turn.retried and turn.retry_count == 0:
                return False, f"turn {i} marked retried with zero count"
            if turn.retried is False and turn.retry_count > 0:
                return False, f"turn {i} has retry count but not retried"
        if self.final_response is not None:
            if self.final_response.finalization != self.finalization:
                return False, "final response and session finalization disagree"
            if type(self.final_response.turn) is not int or self.final_response.turn < 0 or self.final_response.turn >= len(self.turns):
                return False, "final response references turn beyond sequence"
            if (self.final_response.finalization == FinalizationStatus.NORMAL.value
                    and self.final_response.text is None):
                return False, "normal finalization must have captured response text"
            if self.final_response.finalization not in {value.value for value in FinalizationStatus}:
                return False, "unknown finalization status"
            if type(self.final_response.resumed) is not bool or (
                    self.final_response.text is not None and type(self.final_response.text) is not str):
                return False, "invalid final-response observation"
            if self.final_response.finalization == FinalizationStatus.NORMAL.value and (
                    self.turns[self.final_response.turn].kind != TurnKind.FINAL_RESPONSE.value
                    or self.final_response.turn != len(self.turns) - 1):
                return False, "normal final response is not bound to a final-response turn"
        if not self.scripted and self.classification == InferenceClassification.COMPLETED.value and (
                self.final_response is None
                or self.finalization != FinalizationStatus.NORMAL.value):
            return False, "completed native inference lacks a normal final response"
        if self.classification == InferenceClassification.NO_PUBLIC_RESPONSE.value and (
                self.final_response is not None or self.finalization != FinalizationStatus.NO_RESPONSE.value):
            return False, "no-public-response classification cannot contain a fabricated reply"
        for name, value in asdict(self.cost).items():
            if value is not None and (type(value) not in (int, float) or not math.isfinite(value) or value < 0):
                return False, f"invalid cost accounting field {name}"
            if name not in ("total_cost_usd", "retry_cost_usd") and value is not None and type(value) is not int:
                return False, f"noninteger cost accounting field {name}"
        if self.scripted and "native" in self.adapter.name:
            return False, "scripted evidence must not claim native adapter"
        if self.cost.retry_input_tokens is not None and self.cost.total_input_tokens is not None:
            if self.cost.retry_input_tokens > self.cost.total_input_tokens:
                return False, "retry tokens exceed total"
        cls = self.classification
        if cls in (
            InferenceClassification.ENVIRONMENT_STARTUP_FAILURE.value,
            InferenceClassification.SCHEMA_ADMISSION_FAILURE.value,
        ) and self.turns:
            return False, "startup/admission failures must have no inference turns"
        if cls == InferenceClassification.FIRST_TURN_CONTEXT_REJECTION.value and len(self.turns) > 1:
            return False, "first-turn context rejection allows at most one turn"
        if cls == InferenceClassification.FIRST_TURN_CONTEXT_REJECTION.value and (
                len(self.turns) != 1 or self.turns[0].kind != TurnKind.CONTEXT_REJECTED.value):
            return False, "first-turn context rejection lacks a rejected turn"
        if cls == InferenceClassification.LATER_TURN_CONTEXT_REJECTION.value and (
                len(self.turns) < 2 or self.turns[-1].kind != TurnKind.CONTEXT_REJECTED.value):
            return False, "later-turn context rejection lacks its terminal rejection"
        if (cls == InferenceClassification.RESUMED_SESSION.value
                and self.final_response is not None
                and not self.final_response.resumed):
            return False, "resumed session classification requires resumed flag"
        return True, None

    def admits_native(self) -> bool:
        return False


def _digest_valid(value: str) -> bool:
    if not value.startswith("sha256:"):
        return False
    hexpart = value[7:]
    return len(hexpart) == 64 and all(c in "0123456789abcdef" for c in hexpart)


def _module_digest(*module_paths: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(module_paths, key=lambda path: path.name):
        content = path.read_bytes()
        name = path.name.encode()
        digest.update(name)
        digest.update(b"\0")
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return f"sha256:{digest.hexdigest()}"


_THIS_MODULE = Path(__file__)


def _adapter_sha256() -> str:
    return _module_digest(_THIS_MODULE, _THIS_MODULE.with_name("computer1_capture.py"))


# ---------------------------------------------------------------------------
# Scripted stub (oracle/nop)
# ---------------------------------------------------------------------------

class ScriptedEvidenceStub:
    def __init__(self, *, scenario=None, session_id=None) -> None:
        self._adapter_digest = _adapter_sha256()
        self._scenario = scenario
        self._session_id = session_id

    def capture(
        self,
        classification: InferenceClassification = InferenceClassification.COMPLETED,
    ) -> InferenceEvidence:
        return InferenceEvidence(
            schema_version=INFERENCE_EVIDENCE_SCHEMA_VERSION,
            adapter=AdapterIdentity(
                name="scripted-fixture",
                code_sha256=self._adapter_digest,
            ),
            classification=classification.value,
            turns=[],
            final_response=None,
            cost=CostAccounting(),
            identity=InferenceIdentity(
                configured_model="scripted",
                configured_provider="scripted",
            ),
            scripted=True,
            scenario=self._scenario,
            session_id=self._session_id,
        )


# ---------------------------------------------------------------------------
# Native adapter registry — fail closed for unknown adapters
# ---------------------------------------------------------------------------

_COMPUTER1_ADAPTER_NAME = "harbor-native-computer1-v1"


class Computer1InferenceAdapter:
    """Host-side LLMResponse lifecycle adapter for Computer1.

    Installed by the controller before trial.run().  Captures actual terminal
    public assistant response text (not hidden analysis/reasoning), observed
    identity/usage/cost per turn, and session finalization status.

    Unknown retries remain None.  Transport classification (completed vs
    infrastructure fault) is distinct from semantic grading (pass/fail).

    Thread-safety: all mutation is protected by a lock since the Computer1 agent
    loop and the controller finalization may run in different async tasks.
    """

    ADAPTER_NAME = _COMPUTER1_ADAPTER_NAME

    def __init__(
        self,
        configured_model: str,
        configured_provider: str,
        sampling: Optional[dict] = None,
        budget: Optional[dict] = None,
    ) -> None:
        self._lock = threading.Lock()
        self._adapter_digest = _adapter_sha256()
        self._configured_model = configured_model
        self._configured_provider = configured_provider
        self._sampling = sampling or {}
        self._budget = budget or {}
        self._turns: list[InferenceTurn] = []
        self._observed_model: Optional[str] = None
        self._last_assistant_text: Optional[str] = None
        self._last_assistant_turn: int = -1
        self._total_input_tokens: int = 0
        self._total_output_tokens: int = 0
        self._total_cost: float = 0.0
        self._finalized = False
        self._public_result_missing = False
        self._finalization_status: FinalizationStatus = FinalizationStatus.NO_RESPONSE
        self._early_termination_reason: Optional[str] = None

    def observe_turn(
        self,
        *,
        is_terminal: bool,
        assistant_text: Optional[str],
        input_tokens: Optional[int] = None,
        output_tokens: Optional[int] = None,
        cost_usd: Optional[float] = None,
        model_name: Optional[str] = None,
        is_parse_error: bool = False,
        response_id: Optional[str] = None,
    ) -> None:
        """Record one host-observed inference turn.

        Called from the host-side Computer1 loop after each LLMResponse.
        The assistant_text should be the public response only -- not
        reasoning_content or hidden analysis.
        """
        with self._lock:
            if self._finalized:
                raise AdmissionError("inference adapter received turn after finalization")
            turn_index = len(self._turns)
            if is_parse_error:
                kind = TurnKind.UNPARSEABLE.value
            elif is_terminal:
                kind = TurnKind.FINAL_RESPONSE.value
            else:
                kind = TurnKind.TOOL_USE.value
            self._turns.append(InferenceTurn(
                turn=turn_index,
                kind=kind,
                input_tokens=input_tokens,
                output_tokens=output_tokens,
                cost_usd=cost_usd,
                observed_model=model_name,
                response_id=response_id,
            ))
            if input_tokens is not None:
                self._total_input_tokens += input_tokens
            if output_tokens is not None:
                self._total_output_tokens += output_tokens
            if cost_usd is not None:
                self._total_cost += cost_usd
            if model_name is not None:
                self._observed_model = model_name
            if is_terminal and assistant_text is not None:
                self._last_assistant_text = assistant_text
                self._last_assistant_turn = turn_index

    def observe_context_rejected(self) -> None:
        """Record a context-length rejection."""
        with self._lock:
            if self._finalized:
                return
            turn_index = len(self._turns)
            self._turns.append(InferenceTurn(
                turn=turn_index,
                kind=TurnKind.CONTEXT_REJECTED.value,
            ))

    def observe_final_response(self, text: str) -> None:
        with self._lock:
            if self._finalized or not self._turns or type(text) is not str:
                raise AdmissionError("final response lacks an active observed turn")
            turn = self._turns[-1]
            if turn.kind in (TurnKind.CONTEXT_REJECTED.value, TurnKind.UNPARSEABLE.value):
                raise AdmissionError("final response cannot come from a failed inference turn")
            self._turns[-1] = replace(turn, kind=TurnKind.FINAL_RESPONSE.value)
            self._last_assistant_text = text
            self._last_assistant_turn = turn.turn

    def observe_missing_public_response(self) -> None:
        with self._lock:
            if self._finalized or not self._turns:
                raise AdmissionError("missing public result lacks an observed turn")
            self._public_result_missing = True

    def finalize(
        self,
        *,
        early_termination_reason: Optional[str] = None,
        interrupted: bool = False,
    ) -> InferenceEvidence:
        """Seal the evidence.  Must be called exactly once.

        After finalization, no more turns can be recorded.  Returns the
        complete InferenceEvidence for binding into the runner receipt.
        """
        with self._lock:
            if self._finalized:
                raise AdmissionError("inference adapter finalized more than once")
            self._finalized = True
            self._early_termination_reason = early_termination_reason

            classification = self._classify(early_termination_reason, interrupted)
            if interrupted:
                self._finalization_status = FinalizationStatus.INTERRUPTED
            elif self._last_assistant_text is not None and early_termination_reason == "task_complete":
                self._finalization_status = FinalizationStatus.NORMAL
            elif classification in (
                InferenceClassification.BUDGET_EXHAUSTED,
                InferenceClassification.FIRST_TURN_CONTEXT_REJECTION,
                InferenceClassification.LATER_TURN_CONTEXT_REJECTION,
                InferenceClassification.MALFORMED_MODEL_OUTPUT,
                InferenceClassification.NO_PUBLIC_RESPONSE,
            ):
                self._finalization_status = FinalizationStatus.NO_RESPONSE
            elif self._turns:
                self._finalization_status = FinalizationStatus.CAPTURE_FAILED
            else:
                self._finalization_status = FinalizationStatus.NO_RESPONSE

            final_response = None
            if self._last_assistant_text is not None:
                final_response = FinalResponse(
                    text=self._last_assistant_text,
                    turn=self._last_assistant_turn if self._last_assistant_turn >= 0 else len(self._turns) - 1,
                    finalization=self._finalization_status.value,
                    resumed=False,
                )

            def complete_sum(field_name):
                values = [getattr(turn, field_name) for turn in self._turns]
                return sum(values) if values and all(value is not None for value in values) else None

            cost = CostAccounting(
                total_input_tokens=complete_sum("input_tokens"),
                total_output_tokens=complete_sum("output_tokens"),
                total_cost_usd=complete_sum("cost_usd"),
                successful_turns=sum(
                    1 for t in self._turns
                    if t.kind not in (
                        TurnKind.CONTEXT_REJECTED.value,
                        TurnKind.PROVIDER_ERROR.value,
                        TurnKind.UNPARSEABLE.value,
                    )
                ),
            )

            return InferenceEvidence(
                schema_version=INFERENCE_EVIDENCE_SCHEMA_VERSION,
                adapter=AdapterIdentity(
                    name=self.ADAPTER_NAME,
                    code_sha256=self._adapter_digest,
                ),
                classification=classification.value,
                turns=list(self._turns),
                final_response=final_response,
                cost=cost,
                identity=InferenceIdentity(
                    configured_model=self._configured_model,
                    configured_provider=self._configured_provider,
                    observed_model=self._observed_model,
                    sampling=self._sampling,
                    budget=self._budget,
                ),
                scripted=False,
                finalization=self._finalization_status.value,
            )

    def _classify(
        self,
        early_termination_reason: Optional[str],
        interrupted: bool,
    ) -> InferenceClassification:
        if early_termination_reason == "max_turns_reached" and not interrupted:
            return InferenceClassification.BUDGET_EXHAUSTED
        if not self._turns:
            return (InferenceClassification.FAILED_FINALIZATION if interrupted or early_termination_reason is not None
                    else InferenceClassification.ENVIRONMENT_STARTUP_FAILURE)
        context_rejected = any(
            t.kind == TurnKind.CONTEXT_REJECTED.value for t in self._turns
        )
        if context_rejected:
            non_rejected = [
                t for t in self._turns
                if t.kind != TurnKind.CONTEXT_REJECTED.value
            ]
            if not non_rejected:
                return InferenceClassification.FIRST_TURN_CONTEXT_REJECTION
            return InferenceClassification.LATER_TURN_CONTEXT_REJECTION
        if interrupted:
            return InferenceClassification.FAILED_FINALIZATION
        if early_termination_reason == "task_complete":
            if self._last_assistant_text is None and self._public_result_missing:
                return InferenceClassification.NO_PUBLIC_RESPONSE
            return (InferenceClassification.COMPLETED if self._last_assistant_text is not None
                    else InferenceClassification.FAILED_FINALIZATION)
        if early_termination_reason == "runtime_session_dead":
            return InferenceClassification.FAILED_FINALIZATION
        if self._last_assistant_text is not None:
            return InferenceClassification.COMPLETED
        return InferenceClassification.FAILED_FINALIZATION


# ---------------------------------------------------------------------------
# Registry — only adapters declared in THIS module are trusted
# ---------------------------------------------------------------------------

_TRUSTED_ADAPTER_REGISTRY: dict[str, type] = {
    _COMPUTER1_ADAPTER_NAME: Computer1InferenceAdapter,
}


class NativeEvidenceCollector:
    """Fail-closed native inference evidence collector.

    The collector does NOT trust /app/answer.md, agent-editable trajectories, or
    agent logs as inference evidence.  It requires host-observed events from the
    harness's own transport layer.
    """

    def __init__(self, adapter_name: str) -> None:
        if adapter_name not in _TRUSTED_ADAPTER_REGISTRY:
            raise AdmissionError(
                f"no trusted native inference adapter '{adapter_name}'; "
                f"known adapters: {sorted(_TRUSTED_ADAPTER_REGISTRY.keys())}. "
                f"Unknown adapters fail closed."
            )
        self._adapter_name = adapter_name
        self._adapter_digest = _adapter_sha256()

    @staticmethod
    def is_registered(adapter_name: str) -> bool:
        return adapter_name in _TRUSTED_ADAPTER_REGISTRY

    def create_adapter(self, **kwargs) -> Computer1InferenceAdapter:
        cls = _TRUSTED_ADAPTER_REGISTRY[self._adapter_name]
        return cls(**kwargs)

    def fail_closed_stub(self) -> InferenceEvidence:
        return InferenceEvidence(
            schema_version=INFERENCE_EVIDENCE_SCHEMA_VERSION,
            adapter=AdapterIdentity(
                name=f"unavailable-capture",
                code_sha256=self._adapter_digest,
            ),
            classification=InferenceClassification.FAILED_FINALIZATION.value,
            turns=[],
            final_response=None,
            cost=CostAccounting(),
            identity=InferenceIdentity(
                configured_model="unavailable",
                configured_provider="unavailable",
            ),
            scripted=True,
        )


# ---------------------------------------------------------------------------
# Evidence admission — validates evidence before binding into receipt
# ---------------------------------------------------------------------------

def admit_evidence(evidence: InferenceEvidence) -> None:
    """Validate document structure and implementation identity, not host authority.

    Native binding additionally requires Computer1HostCapture's sealed in-memory
    authority and downstream receipt/manifest verification. Public code hashes
    alone are not an attestation.
    """
    ok, err = evidence.validates()
    if not ok:
        raise AdmissionError(f"inference evidence validation failed: {err}")
    if not evidence.scripted:
        if evidence.adapter.name not in _TRUSTED_ADAPTER_REGISTRY:
            raise AdmissionError(
                f"non-scripted evidence claims unregistered adapter '{evidence.adapter.name}'"
            )
        if evidence.adapter.code_sha256 != _adapter_sha256():
            raise AdmissionError(
                "non-scripted evidence adapter digest does not match this module"
            )


# ---------------------------------------------------------------------------
# Legacy compatibility
# ---------------------------------------------------------------------------

def legacy_classify(context_overflow: bool, api_calls: int) -> InferenceClassification:
    if context_overflow and api_calls == 0:
        return InferenceClassification.FIRST_TURN_CONTEXT_REJECTION
    elif context_overflow:
        return InferenceClassification.LATER_TURN_CONTEXT_REJECTION
    else:
        return InferenceClassification.COMPLETED
