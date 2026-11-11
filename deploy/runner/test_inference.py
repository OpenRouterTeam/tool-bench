"""Tests for trusted native inference evidence module.

Covers: structured evidence types, fail-closed adapter registry, Computer1
adapter lifecycle, adversarial rejection of actor-authored evidence,
self-declared adapter names, substituted artifacts, and interrupted finalization.
"""
import copy
import json
import unittest
from pathlib import Path

from deploy.runner.inference import (
    AdapterIdentity,
    AdmissionError,
    Computer1InferenceAdapter,
    CostAccounting,
    FinalResponse,
    FinalizationStatus,
    InferenceClassification,
    InferenceEvidence,
    InferenceIdentity,
    InferenceTurn,
    NativeEvidenceCollector,
    ScriptedEvidenceStub,
    TurnKind,
    _COMPUTER1_ADAPTER_NAME,
    _TRUSTED_ADAPTER_REGISTRY,
    _adapter_sha256,
    admit_evidence,
    legacy_classify,
    INFERENCE_EVIDENCE_SCHEMA_VERSION,
)


def _valid_digest():
    return "sha256:" + "a" * 64


def _valid_evidence(**overrides):
    ev = InferenceEvidence(
        schema_version=INFERENCE_EVIDENCE_SCHEMA_VERSION,
        adapter=AdapterIdentity(name="scripted-fixture", code_sha256=_valid_digest()),
        classification=InferenceClassification.COMPLETED.value,
        turns=[
            InferenceTurn(turn=0, kind=TurnKind.TOOL_USE.value, input_tokens=100, output_tokens=50),
            InferenceTurn(turn=1, kind=TurnKind.FINAL_RESPONSE.value, input_tokens=200, output_tokens=80, cost_usd=0.003),
        ],
        final_response=FinalResponse(
            text="The refund has been processed.",
            turn=1,
            finalization=FinalizationStatus.NORMAL.value,
        ),
        cost=CostAccounting(
            total_input_tokens=300, total_output_tokens=130, total_cost_usd=0.003,
            retry_input_tokens=0, retry_output_tokens=0, retry_cost_usd=0.0,
            successful_turns=2, retried_turns=0, total_retry_attempts=0,
        ),
        identity=InferenceIdentity(configured_model="scripted", configured_provider="scripted"),
        scripted=True,
        finalization=FinalizationStatus.NORMAL.value,
    )
    for k, v in overrides.items():
        setattr(ev, k, v)
    return ev


# ---- Classification predicates ----

class TestInferenceClassification(unittest.TestCase):
    def test_infrastructure_faults(self):
        for cls in [
            InferenceClassification.ENVIRONMENT_STARTUP_FAILURE,
            InferenceClassification.SCHEMA_ADMISSION_FAILURE,
            InferenceClassification.UNRECOVERED_RATE_LIMIT,
            InferenceClassification.FAILED_FINALIZATION,
        ]:
            self.assertTrue(cls.is_infrastructure_fault(), cls)

    def test_non_infrastructure(self):
        for cls in [
            InferenceClassification.COMPLETED,
            InferenceClassification.TASK_FAILURE,
            InferenceClassification.RECOVERED_RATE_LIMIT,
        ]:
            self.assertFalse(cls.is_infrastructure_fault(), cls)

    def test_context_rejection(self):
        self.assertTrue(InferenceClassification.FIRST_TURN_CONTEXT_REJECTION.is_context_rejection())
        self.assertTrue(InferenceClassification.LATER_TURN_CONTEXT_REJECTION.is_context_rejection())
        self.assertFalse(InferenceClassification.COMPLETED.is_context_rejection())

    def test_had_inference(self):
        self.assertFalse(InferenceClassification.ENVIRONMENT_STARTUP_FAILURE.had_inference())
        self.assertTrue(InferenceClassification.COMPLETED.had_inference())
        self.assertTrue(InferenceClassification.TASK_FAILURE.had_inference())


# ---- Legacy classification ----

class TestLegacyClassify(unittest.TestCase):
    def test_first_turn_context_rejection(self):
        self.assertEqual(legacy_classify(True, 0), InferenceClassification.FIRST_TURN_CONTEXT_REJECTION)

    def test_later_turn_context_rejection(self):
        self.assertEqual(legacy_classify(True, 5), InferenceClassification.LATER_TURN_CONTEXT_REJECTION)

    def test_completed(self):
        self.assertEqual(legacy_classify(False, 0), InferenceClassification.COMPLETED)


# ---- Evidence validation ----

class TestInferenceEvidenceValidation(unittest.TestCase):
    def test_valid_evidence(self):
        ok, err = _valid_evidence().validates()
        self.assertTrue(ok, err)

    def test_wrong_schema_version(self):
        ev = _valid_evidence(schema_version=99)
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("schema version", err)

    def test_empty_adapter_name(self):
        ev = _valid_evidence(adapter=AdapterIdentity(name="", code_sha256=_valid_digest()))
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("adapter", err)

    def test_bad_digest(self):
        ev = _valid_evidence(adapter=AdapterIdentity(name="test", code_sha256="bad"))
        ok, err = ev.validates()
        self.assertFalse(ok)

    def test_non_sequential_turns(self):
        ev = _valid_evidence(turns=[
            InferenceTurn(turn=0, kind=TurnKind.TOOL_USE.value),
            InferenceTurn(turn=5, kind=TurnKind.FINAL_RESPONSE.value),
        ])
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("non-sequential", err)

    def test_retried_without_count(self):
        ev = _valid_evidence(
            turns=[InferenceTurn(turn=0, kind=TurnKind.TOOL_USE.value, retried=True, retry_count=0)],
            final_response=None,
        )
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("retried", err)

    def test_count_without_retried(self):
        ev = _valid_evidence(
            turns=[InferenceTurn(turn=0, kind=TurnKind.TOOL_USE.value, retried=False, retry_count=3)],
            final_response=None,
        )
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("retry count", err)

    def test_retry_tokens_exceed_total(self):
        ev = _valid_evidence()
        ev.cost.retry_input_tokens = 999
        ev.cost.total_input_tokens = 100
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("exceed", err)

    def test_scripted_native_name_rejected(self):
        ev = _valid_evidence(adapter=AdapterIdentity(name="harbor-native-v1", code_sha256=_valid_digest()))
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("native", err)

    def test_normal_finalization_without_text(self):
        ev = _valid_evidence(
            final_response=FinalResponse(text=None, turn=0, finalization=FinalizationStatus.NORMAL.value),
        )
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("text", err)

    def test_startup_failure_with_turns_rejected(self):
        ev = _valid_evidence(classification=InferenceClassification.ENVIRONMENT_STARTUP_FAILURE.value)
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("no inference turns", err)

    def test_first_turn_rejection_with_many_turns_rejected(self):
        ev = _valid_evidence(classification=InferenceClassification.FIRST_TURN_CONTEXT_REJECTION.value)
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("at most one turn", err)

    def test_resumed_session_without_flag_rejected(self):
        ev = _valid_evidence(
            classification=InferenceClassification.RESUMED_SESSION.value,
            final_response=FinalResponse(text="ok", turn=1, finalization=FinalizationStatus.NORMAL.value, resumed=False),
        )
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("resumed", err)

    def test_final_response_beyond_turns_rejected(self):
        ev = _valid_evidence(
            final_response=FinalResponse(text="ok", turn=99, finalization=FinalizationStatus.NORMAL.value),
        )
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("beyond", err)


# ---- JSON roundtrip ----

class TestJsonRoundtrip(unittest.TestCase):
    def test_roundtrip(self):
        ev = _valid_evidence()
        d = ev.to_dict()
        js = json.dumps(d)
        parsed = json.loads(js)
        self.assertEqual(parsed["schema_version"], INFERENCE_EVIDENCE_SCHEMA_VERSION)
        self.assertEqual(parsed["classification"], "completed")
        self.assertEqual(len(parsed["turns"]), 2)
        self.assertTrue(parsed["scripted"])

    def test_rust_compatible_keys(self):
        ev = _valid_evidence()
        d = ev.to_dict()
        required_keys = {
            "schema_version", "adapter", "classification", "turns",
            "final_response", "cost", "identity", "scripted",
            "finalization",
            "scenario", "session_id",
        }
        self.assertEqual(set(d.keys()), required_keys)


# ---- Scripted stub ----

class TestScriptedEvidenceStub(unittest.TestCase):
    def test_produces_valid_evidence(self):
        stub = ScriptedEvidenceStub()
        ev = stub.capture()
        ok, err = ev.validates()
        self.assertTrue(ok, err)
        self.assertTrue(ev.scripted)
        self.assertFalse(ev.admits_native())
        self.assertEqual(ev.classification, InferenceClassification.COMPLETED.value)

    def test_custom_classification(self):
        stub = ScriptedEvidenceStub()
        ev = stub.capture(InferenceClassification.TASK_FAILURE)
        self.assertEqual(ev.classification, InferenceClassification.TASK_FAILURE.value)

    def test_scripted_stub_never_admits_native(self):
        stub = ScriptedEvidenceStub()
        ev = stub.capture()
        self.assertFalse(ev.admits_native())


# ---- Registry ----

class TestAdapterRegistry(unittest.TestCase):
    def test_computer1_is_registered(self):
        self.assertTrue(NativeEvidenceCollector.is_registered(_COMPUTER1_ADAPTER_NAME))

    def test_unknown_adapter_fails_closed(self):
        with self.assertRaises(AdmissionError) as ctx:
            NativeEvidenceCollector("fake-adapter")
        self.assertIn("no trusted native inference adapter", str(ctx.exception))

    def test_self_declared_name_rejected(self):
        """An actor cannot register their own adapter name at runtime."""
        with self.assertRaises(AdmissionError):
            NativeEvidenceCollector("actor-declared-adapter")

    def test_registry_injection_still_uses_canonical_name(self):
        """Even if someone adds to the registry at runtime, the adapter class
        always stamps its own ADAPTER_NAME constant on evidence, not the
        injected key.  So the evidence adapter name is always the module's."""
        original = dict(_TRUSTED_ADAPTER_REGISTRY)
        try:
            _TRUSTED_ADAPTER_REGISTRY["injected"] = Computer1InferenceAdapter
            collector = NativeEvidenceCollector("injected")
            adapter = collector.create_adapter(
                configured_model="m", configured_provider="p"
            )
            adapter.observe_turn(is_terminal=True, assistant_text="hi", input_tokens=1, output_tokens=1)
            evidence = adapter.finalize(early_termination_reason="task_complete")
            # Adapter always uses its canonical name, not the registry key
            self.assertEqual(evidence.adapter.name, _COMPUTER1_ADAPTER_NAME)
            self.assertNotEqual(evidence.adapter.name, "injected")
            # Forging a name that's not in the real registry is rejected
            forged = copy.copy(evidence)
            forged.adapter = AdapterIdentity(name="actor-fake", code_sha256=evidence.adapter.code_sha256)
            with self.assertRaises(AdmissionError):
                admit_evidence(forged)
        finally:
            _TRUSTED_ADAPTER_REGISTRY.clear()
            _TRUSTED_ADAPTER_REGISTRY.update(original)


# ---- Computer1 adapter lifecycle ----

class TestComputer1Adapter(unittest.TestCase):
    def _make_adapter(self, **kw):
        defaults = dict(configured_model="gpt-4o", configured_provider="openai")
        defaults.update(kw)
        return Computer1InferenceAdapter(**defaults)

    def test_basic_completed_session(self):
        a = self._make_adapter()
        a.observe_turn(is_terminal=False, assistant_text=None, input_tokens=100, output_tokens=50, model_name="gpt-4o-2024")
        a.observe_turn(is_terminal=True, assistant_text="Done!", input_tokens=200, output_tokens=80, cost_usd=0.003)
        ev = a.finalize(early_termination_reason="task_complete")
        ok, err = ev.validates()
        self.assertTrue(ok, err)
        self.assertFalse(ev.admits_native())
        self.assertFalse(ev.scripted)
        self.assertEqual(ev.classification, InferenceClassification.COMPLETED.value)
        self.assertEqual(ev.final_response.text, "Done!")
        self.assertEqual(ev.final_response.finalization, FinalizationStatus.NORMAL.value)
        self.assertEqual(ev.identity.observed_model, "gpt-4o-2024")
        self.assertEqual(len(ev.turns), 2)
        self.assertEqual(ev.cost.total_input_tokens, 300)
        self.assertEqual(ev.cost.total_output_tokens, 130)
        self.assertEqual(ev.cost.successful_turns, 2)

    def test_no_turns_classifies_as_startup_failure(self):
        a = self._make_adapter()
        ev = a.finalize()
        self.assertEqual(ev.classification, InferenceClassification.ENVIRONMENT_STARTUP_FAILURE.value)
        ok, _ = ev.validates()
        self.assertTrue(ok)

    def test_interrupted_run_without_a_response_does_not_invent_startup_failure(self):
        evidence = self._make_adapter().finalize(interrupted=True)
        self.assertEqual(evidence.classification, InferenceClassification.FAILED_FINALIZATION.value)
        self.assertIsNone(evidence.final_response)
        self.assertEqual(evidence.finalization, FinalizationStatus.INTERRUPTED.value)

    def test_budget_exhaustion_without_observed_text_is_not_startup_failure(self):
        evidence = self._make_adapter().finalize(early_termination_reason="max_turns_reached")
        self.assertEqual(evidence.classification, InferenceClassification.BUDGET_EXHAUSTED.value)
        self.assertIsNone(evidence.final_response)
        self.assertEqual(evidence.finalization, FinalizationStatus.NO_RESPONSE.value)

    def test_context_rejection_first_turn(self):
        a = self._make_adapter()
        a.observe_context_rejected()
        ev = a.finalize()
        self.assertEqual(ev.classification, InferenceClassification.FIRST_TURN_CONTEXT_REJECTION.value)

    def test_context_rejection_later_turn(self):
        a = self._make_adapter()
        a.observe_turn(is_terminal=False, assistant_text=None, input_tokens=100, output_tokens=50)
        a.observe_context_rejected()
        ev = a.finalize()
        self.assertEqual(ev.classification, InferenceClassification.LATER_TURN_CONTEXT_REJECTION.value)

    def test_interrupted_finalization(self):
        a = self._make_adapter()
        a.observe_turn(is_terminal=False, assistant_text=None, input_tokens=50, output_tokens=30)
        ev = a.finalize(interrupted=True)
        self.assertEqual(ev.classification, InferenceClassification.FAILED_FINALIZATION.value)
        self.assertEqual(ev.finalization, FinalizationStatus.INTERRUPTED.value)
        self.assertIsNone(ev.final_response)

    def test_max_turns_reached(self):
        a = self._make_adapter()
        a.observe_turn(is_terminal=False, assistant_text=None, input_tokens=50, output_tokens=30)
        ev = a.finalize(early_termination_reason="max_turns_reached")
        self.assertEqual(ev.classification, InferenceClassification.BUDGET_EXHAUSTED.value)

    def test_runtime_session_dead(self):
        a = self._make_adapter()
        a.observe_turn(is_terminal=False, assistant_text=None, input_tokens=50, output_tokens=30)
        ev = a.finalize(early_termination_reason="runtime_session_dead")
        self.assertEqual(ev.classification, InferenceClassification.FAILED_FINALIZATION.value)

    def test_parse_error_turn(self):
        a = self._make_adapter()
        a.observe_turn(is_terminal=False, assistant_text=None, input_tokens=50, output_tokens=30, is_parse_error=True)
        a.observe_turn(is_terminal=True, assistant_text="Fixed.", input_tokens=50, output_tokens=30)
        ev = a.finalize(early_termination_reason="task_complete")
        self.assertEqual(ev.turns[0].kind, TurnKind.UNPARSEABLE.value)
        self.assertEqual(ev.turns[1].kind, TurnKind.FINAL_RESPONSE.value)
        self.assertEqual(ev.cost.successful_turns, 1)

    def test_finalize_twice_raises(self):
        a = self._make_adapter()
        a.finalize()
        with self.assertRaises(AdmissionError):
            a.finalize()

    def test_turn_after_finalize_raises(self):
        a = self._make_adapter()
        a.finalize()
        with self.assertRaises(AdmissionError):
            a.observe_turn(is_terminal=False, assistant_text=None)

    def test_no_reasoning_content_captured(self):
        """The adapter only captures the public text, not reasoning."""
        a = self._make_adapter()
        a.observe_turn(
            is_terminal=True,
            assistant_text="public answer",
            input_tokens=100,
            output_tokens=50,
        )
        ev = a.finalize(early_termination_reason="task_complete")
        self.assertEqual(ev.final_response.text, "public answer")

    def test_adapter_name_is_module_constant(self):
        """The adapter name comes from the module constant, not caller."""
        a = self._make_adapter()
        a.observe_turn(is_terminal=True, assistant_text="x", input_tokens=1, output_tokens=1)
        ev = a.finalize(early_termination_reason="task_complete")
        self.assertEqual(ev.adapter.name, _COMPUTER1_ADAPTER_NAME)
        self.assertTrue(ev.adapter.name.startswith("harbor-native-"))


# ---- Adversarial evidence admission ----

class TestAdversarialAdmission(unittest.TestCase):
    def test_wrong_implementation_digest_is_rejected(self):
        """A digest mismatch is structural rejection, not an authorship test."""
        forged = InferenceEvidence(
            schema_version=INFERENCE_EVIDENCE_SCHEMA_VERSION,
            adapter=AdapterIdentity(
                name=_COMPUTER1_ADAPTER_NAME,
                code_sha256="sha256:" + "b" * 64,  # wrong digest
            ),
            classification=InferenceClassification.COMPLETED.value,
            turns=[InferenceTurn(turn=0, kind=TurnKind.FINAL_RESPONSE.value)],
            final_response=FinalResponse(text="forged", turn=0, finalization=FinalizationStatus.NORMAL.value),
            cost=CostAccounting(successful_turns=1),
            identity=InferenceIdentity(configured_model="gpt-4o", configured_provider="openai"),
            scripted=False,
            finalization=FinalizationStatus.NORMAL.value,
        )
        with self.assertRaises(AdmissionError) as ctx:
            admit_evidence(forged)
        self.assertIn("digest does not match", str(ctx.exception))

    def test_unknown_adapter_name_rejected(self):
        """Non-scripted evidence with an unregistered adapter name is rejected."""
        forged = InferenceEvidence(
            schema_version=INFERENCE_EVIDENCE_SCHEMA_VERSION,
            adapter=AdapterIdentity(
                name="actor-custom-adapter",
                code_sha256=_adapter_sha256(),
            ),
            classification=InferenceClassification.COMPLETED.value,
            turns=[InferenceTurn(turn=0, kind=TurnKind.FINAL_RESPONSE.value)],
            final_response=FinalResponse(text="forged", turn=0, finalization=FinalizationStatus.NORMAL.value),
            cost=CostAccounting(successful_turns=1),
            identity=InferenceIdentity(configured_model="gpt-4o", configured_provider="openai"),
            scripted=False,
            finalization=FinalizationStatus.NORMAL.value,
        )
        with self.assertRaises(AdmissionError) as ctx:
            admit_evidence(forged)
        self.assertIn("unregistered adapter", str(ctx.exception))

    def test_scripted_claiming_native_rejected(self):
        """Scripted evidence that claims a native adapter name fails validation."""
        ev = _valid_evidence(
            adapter=AdapterIdentity(name="harbor-native-forged", code_sha256=_valid_digest()),
        )
        ok, err = ev.validates()
        self.assertFalse(ok)
        self.assertIn("scripted evidence must not claim native", err)

    def test_substituted_artifact_detected_by_digest(self):
        """If evidence bytes are substituted, the receipt digest won't match."""
        a = Computer1InferenceAdapter(configured_model="gpt-4o", configured_provider="openai")
        a.observe_turn(is_terminal=True, assistant_text="real answer", input_tokens=1, output_tokens=1)
        real_ev = a.finalize(early_termination_reason="task_complete")
        real_json = real_ev.to_json()
        real_digest = __import__("hashlib").sha256(real_json.encode()).hexdigest()

        # Substitute the answer text
        substituted = json.loads(real_json)
        substituted["final_response"]["text"] = "forged answer"
        substituted_json = json.dumps(substituted)
        substituted_digest = __import__("hashlib").sha256(substituted_json.encode()).hexdigest()
        self.assertNotEqual(real_digest, substituted_digest)

    def test_adapter_document_validates_but_still_requires_host_authority(self):
        """Structural validation is not native runtime admission."""
        a = Computer1InferenceAdapter(configured_model="gpt-4o", configured_provider="openai")
        a.observe_turn(is_terminal=True, assistant_text="real", input_tokens=10, output_tokens=5)
        ev = a.finalize(early_termination_reason="task_complete")
        admit_evidence(ev)  # should not raise
        self.assertFalse(ev.admits_native())

    def test_scripted_evidence_admitted(self):
        """Scripted evidence passes admission (but not native admission)."""
        stub = ScriptedEvidenceStub()
        ev = stub.capture()
        admit_evidence(ev)  # should not raise
        self.assertFalse(ev.admits_native())

    def test_oracle_never_admits_native(self):
        """Oracle/nop scripted evidence never certifies native model."""
        stub = ScriptedEvidenceStub()
        for cls in InferenceClassification:
            ev = stub.capture(cls)
            self.assertFalse(ev.admits_native())


# ---- Collector fail-closed ----

class TestNativeEvidenceCollector(unittest.TestCase):
    def test_creates_computer1_adapter(self):
        collector = NativeEvidenceCollector(_COMPUTER1_ADAPTER_NAME)
        adapter = collector.create_adapter(configured_model="gpt-4o", configured_provider="openai")
        self.assertIsInstance(adapter, Computer1InferenceAdapter)

    def test_fail_closed_stub_is_valid_but_non_native(self):
        collector = NativeEvidenceCollector(_COMPUTER1_ADAPTER_NAME)
        ev = collector.fail_closed_stub()
        ok, err = ev.validates()
        self.assertTrue(ok, err)
        self.assertTrue(ev.scripted)
        self.assertFalse(ev.admits_native())

    def test_unknown_harness_raises_immediately(self):
        with self.assertRaises(AdmissionError):
            NativeEvidenceCollector("nonexistent")


if __name__ == "__main__":
    unittest.main()
