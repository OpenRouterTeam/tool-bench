from harbor.models.trial.config import TrialConfig
from harbor.trial.hooks import TrialEvent
from harbor.trial.trial import Trial
from harbor.agents.oracle import OracleAgent

from deploy.runner.admission import AdmissionError
from deploy.runner.strict_docker import StrictDockerEnvironment
from deploy.runner.manifest import admit_scripted_agent


def install_collection_gate(trial: Trial) -> None:
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
    admit_scripted_agent(trial)
    environment.register_collection_controller()
    if type(trial.agent) is OracleAgent:
        environment.register_oracle_program(trial.agent, trial.task.config.environment.os)

    async def observe_execution(event) -> None:
        await environment.observe_execution(trial)

    async def finish_agent(event) -> None:
        await environment.finish_agent()

    trial.add_hook(TrialEvent.AGENT_END, finish_agent)
    trial.add_hook(TrialEvent.AGENT_START, observe_execution)


async def run_trial(config: TrialConfig):
    trial = await Trial.create(config)
    install_collection_gate(trial)
    return await trial.run()
