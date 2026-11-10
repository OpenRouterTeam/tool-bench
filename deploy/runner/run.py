import argparse
import asyncio
import os
from pathlib import Path

from harbor.models.trial.config import AgentConfig, EnvironmentConfig, TaskConfig, TrialConfig

from deploy.runner.controller import run_trial


def main() -> int:
    parser = argparse.ArgumentParser(description="Run one provenance-checked trial; native runtime qualification remains separate")
    parser.add_argument("-p", "--task", type=Path, required=True)
    parser.add_argument("-a", "--agent", choices=["oracle", "nop", "computer-1", "claude-code"], required=True)
    parser.add_argument("--model")
    parser.add_argument("--max-turns", type=int)
    parser.add_argument("--provider", choices=["litellm", "anthropic", "bedrock", "gemini", "openai"])
    parser.add_argument("-o", "--output", type=Path, required=True)
    # Container-side CLI agent: all provider traffic goes through the host proxy.
    parser.add_argument("--arm", choices=["off", "native", "universal"])
    parser.add_argument("--provider-target", help="provider origin the host proxy forwards to")
    parser.add_argument("--provider-key-env", help="host variable holding the real provider credential")
    parser.add_argument("--provider-pin", help="pin every request to one upstream provider")
    parser.add_argument("--model-rewrite", help="model slug the proxy sends upstream")
    parser.add_argument("--proxy-base-path", default="", help="path prefix of the provider API, e.g. /api")
    parser.add_argument("--proxy-listen", default="127.0.0.1")
    parser.add_argument("--proxy-advertise-host", default="host.docker.internal")
    options = parser.parse_args()
    proxied = options.agent == "claude-code"
    proxy_options = [options.arm, options.provider_target, options.provider_key_env,
                     options.provider_pin, options.model_rewrite]
    if proxied:
        return run_proxied(parser, options)
    if any(value is not None for value in proxy_options):
        parser.error("arm and provider-proxy settings require claude-code")
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


def run_proxied(parser, options) -> int:
    from deploy.runner.proxy_capture import ProviderProxyCapture

    if not options.model or options.max_turns is None or options.max_turns <= 0:
        parser.error("claude-code requires --model and a positive --max-turns budget")
    if options.provider is not None or not (options.arm and options.provider_target and options.provider_key_env):
        parser.error("claude-code requires --arm, --provider-target and --provider-key-env (and no --provider)")
    # The credential leaves this process environment before Harbor resolves agent settings.
    upstream_key = os.environ.pop(options.provider_key_env, None)
    if not upstream_key:
        parser.error(f"{options.provider_key_env} is not set")
    proxy = ProviderProxyCapture(
        instruction=(options.task / "instruction.md").read_text(),
        configured_model=options.model, arm=options.arm, target=options.provider_target,
        upstream_key=upstream_key, provider_pin=options.provider_pin,
        model_rewrite=options.model_rewrite, max_turns=options.max_turns,
        base_path=options.proxy_base_path, listen=options.proxy_listen,
        advertise_host=options.proxy_advertise_host,
    )
    proxy.start()
    try:
        config = TrialConfig(
            task=TaskConfig(path=options.task.resolve()),
            agent=AgentConfig(
                name="claude-code", model_name=options.model,
                kwargs={"max_turns": options.max_turns}, env=proxy.agent_env(),
            ),
            environment=EnvironmentConfig(
                import_path="deploy.runner.strict_docker:StrictDockerEnvironment"
            ),
            trials_dir=options.output.resolve(),
        )
        result = asyncio.run(run_trial(config, provider_proxy=proxy))
    finally:
        proxy.close()
    return 1 if result.exception_info is not None else 0


if __name__ == "__main__":
    raise SystemExit(main())
