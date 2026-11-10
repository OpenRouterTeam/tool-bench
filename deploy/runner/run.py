import argparse
import asyncio
from pathlib import Path

from harbor.models.trial.config import AgentConfig, EnvironmentConfig, TaskConfig, TrialConfig

from deploy.runner.controller import run_trial


def main() -> int:
    parser = argparse.ArgumentParser(description="Run one provenance-checked trial; native runtime qualification remains separate")
    parser.add_argument("-p", "--task", type=Path, required=True)
    parser.add_argument("-a", "--agent", choices=["oracle", "nop", "computer-1"], required=True)
    parser.add_argument("--model")
    parser.add_argument("--max-turns", type=int)
    parser.add_argument("--provider", choices=["litellm", "anthropic", "bedrock", "gemini", "openai"])
    parser.add_argument("-o", "--output", type=Path, required=True)
    options = parser.parse_args()
    native = options.agent == "computer-1"
    if native and (not options.model or options.max_turns is None or options.max_turns <= 0):
        parser.error("computer-1 requires --model and a positive --max-turns budget")
    if not native and any(value is not None for value in [options.model, options.max_turns, options.provider]):
        parser.error("model/provider/turn-budget settings require computer-1")
    agent_kwargs = {}
    if native:
        agent_kwargs["max_turns"] = options.max_turns
        if options.provider is not None:
            agent_kwargs["provider"] = options.provider
    config = TrialConfig(
        task=TaskConfig(path=options.task.resolve()),
        agent=AgentConfig(name=options.agent, model_name=options.model, kwargs=agent_kwargs),
        environment=EnvironmentConfig(
            import_path="deploy.runner.strict_docker:StrictDockerEnvironment"
        ),
        trials_dir=options.output.resolve(),
    )
    result = asyncio.run(run_trial(config))
    return 1 if result.exception_info is not None else 0


if __name__ == "__main__":
    raise SystemExit(main())
