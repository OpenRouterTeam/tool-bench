import argparse
import asyncio
from pathlib import Path

from harbor.models.trial.config import AgentConfig, EnvironmentConfig, TaskConfig, TrialConfig

from deploy.runner.controller import run_trial


def main() -> int:
    parser = argparse.ArgumentParser(description="Run one admitted scripted diagnostic trial")
    parser.add_argument("-p", "--task", type=Path, required=True)
    parser.add_argument("-a", "--agent", choices=["oracle", "nop"], required=True)
    parser.add_argument("-o", "--output", type=Path, required=True)
    options = parser.parse_args()
    config = TrialConfig(
        task=TaskConfig(path=options.task.resolve()),
        agent=AgentConfig(name=options.agent),
        environment=EnvironmentConfig(
            import_path="deploy.runner.strict_docker:StrictDockerEnvironment"
        ),
        trials_dir=options.output.resolve(),
    )
    result = asyncio.run(run_trial(config))
    return 1 if result.exception_info is not None else 0


if __name__ == "__main__":
    raise SystemExit(main())
