from harbor.models.trial.config import TrialConfig
from harbor.trial.hooks import TrialEvent
from harbor.trial.trial import Trial
from harbor.agents.oracle import OracleAgent
from harbor.agents.nop import NopAgent

from deploy.runner.admission import AdmissionError
from deploy.runner.strict_docker import StrictDockerEnvironment
from deploy.runner.manifest import admit_scripted_agent, native_identity
from deploy.runner.computer1_capture import Computer1HostCapture
from deploy.runner.proxy_capture import is_proxied_agent
from deploy.runner.surface_audit import ProxyHarnessMessages
from deploy.runner.inference import (
    Computer1InferenceAdapter,
    NativeEvidenceCollector,
    ScriptedEvidenceStub,
    admit_evidence,
)


def _is_scripted_agent(trial: Trial) -> bool:
    return type(trial.agent) in {OracleAgent, NopAgent}


def install_collection_gate(trial: Trial, provider_proxy=None) -> None:
    if (
        type(trial.agent_environment) is not StrictDockerEnvironment
        or trial.config.artifacts
        or trial.config.source_trial is not None
        or trial.config.install_only
        or trial.config.user_agent is not None
        or trial.config.agent.load_trajectory is not None
        or trial.config.environment.mounts
        or trial.config.environment.extra_docker_compose
        or trial.config.environment.kwargs
        or trial.config.extra_instruction_paths
        or trial.config.extra_instructions
        or trial.task.has_steps
    ):
        raise AdmissionError("unsupported strict-controller execution condition")
    environment = trial.agent_environment
    harness_messages = None

    proxied = is_proxied_agent(trial.agent)
    if (provider_proxy is not None) != proxied:
        raise AdmissionError("a container-side agent and the host provider proxy require each other")
    if proxied:
        # A container-side CLI agent is observed at the host-owned provider proxy.
        provider_proxy.bind_trial(
            trial, scenario=environment._scenario, session_id=environment.session_id,
        )
        harness_messages = ProxyHarnessMessages.create(
            getattr(environment, "_surface_audit", None), provider_proxy,
        )
        inference_adapter = None
        host_capture = provider_proxy
        inference_evidence_fn = None
    elif _is_scripted_agent(trial):
        admit_scripted_agent(trial)
        inference_adapter = None
        host_capture = None
        inference_evidence_fn = lambda: ScriptedEvidenceStub(
            scenario=environment._scenario, session_id=environment.session_id,
        ).capture()
    else:
        _admit_native_agent(trial)
        adapter = _create_native_adapter(trial)
        inference_adapter = adapter
        host_capture = Computer1HostCapture(
            trial.agent, adapter, scenario=environment._scenario, session_id=environment.session_id,
        )
        inference_evidence_fn = None

    environment.register_collection_controller()
    if type(trial.agent) is OracleAgent:
        environment.register_oracle_program(trial.agent, trial.task.config.environment.os)

    async def observe_execution(event) -> None:
        await environment.observe_execution(trial)

    evidence_bound = False

    async def finish_agent(event) -> None:
        nonlocal evidence_bound
        try:
            if not evidence_bound:
                if host_capture is not None:
                    evidence = host_capture.finalize()
                    if harness_messages is not None:
                        harness_messages.finish()
                    admit_evidence(evidence)
                    environment.bind_inference_evidence(evidence, host_capture=host_capture)
                    evidence_bound = True
                elif inference_evidence_fn is not None:
                    evidence = inference_evidence_fn()
                    admit_evidence(evidence)
                    environment.bind_inference_evidence(evidence)
                    evidence_bound = True
        finally:
            await environment.finish_agent()

    trial.add_hook(TrialEvent.AGENT_END, finish_agent)
    trial.add_hook(TrialEvent.AGENT_START, observe_execution)


def _admit_native_agent(trial: Trial) -> None:
    """Admit a native (non-scripted) agent for inference capture.

    Currently only Computer1 is supported.  All others fail closed.
    """
    try:
        from harbor.agents.computer_1.computer_1 import Computer1
    except ImportError:
        raise AdmissionError("Computer1 agent import unavailable for native admission")
    if type(trial.agent) is not Computer1:
        raise AdmissionError(
            f"native runtime provenance is not admitted for {type(trial.agent).__name__}; "
            f"only Computer1 is supported"
        )
    if not NativeEvidenceCollector.is_registered(Computer1InferenceAdapter.ADAPTER_NAME):
        raise AdmissionError("Computer1 inference adapter is not registered")


def _create_native_adapter(trial: Trial) -> Computer1InferenceAdapter:
    """Create and return a Computer1 inference adapter for the trial."""
    collector = NativeEvidenceCollector(Computer1InferenceAdapter.ADAPTER_NAME)
    identity = native_identity(trial)
    return collector.create_adapter(
        configured_model=identity["configured_model"],
        configured_provider=identity["configured_provider"],
        sampling=identity["sampling"],
        budget=identity["budget"],
    )


async def run_trial(config: TrialConfig, provider_proxy=None):
    try:
        trial = await Trial.create(config)
        install_collection_gate(trial, provider_proxy)
        return await trial.run()
    finally:
        if provider_proxy is not None:
            provider_proxy.close()
