"""Run one attempt through the production worker's real entry point.

Calls gcp_harbor_trial.trial.main() with the arguments the trial pod passes:
  --config <TrialConfig json> --output-dir <dir> --attempt-id <id> --tasks-root <dir>
so issue_trial_admin_token, execute_attempt, load_config (which forces
BullseyeSecurityPinnedEnvironment), run_harbor, private_trial_console, trial_session_proxy,
run_with_rate_limit_retries and Trial.create(config).run() all run unmodified.

--proxy real      nothing substituted. Needs a host that shares a network namespace with
                  dockerd (the trial pod, or Linux with the docker0 bridge): the session proxy
                  binds the Docker bridge gateway address.
--proxy loopback  ONE substitution: SessionProxy's listening socket binds 127.0.0.1 instead of
                  the bridge gateway (on macOS the gateway address exists only inside the Docker
                  VM, so bind() fails with EADDRNOTAVAIL). Everything else in
                  trial_session_proxy still runs: docker_bridge_gateway(), the gateway appended
                  to agent/environment extra_allowed_hosts, the base-url env templates. Scripted
                  agents never call the proxy, so nothing they do depends on its address.

Credentials: OPENROUTER_API_KEY is forced to a placeholder (the planner's env templates need
the variable to exist; oracle and nop make no model calls) and Cloudflare Access variables are
dropped, so no real secret can reach a container even if the caller's shell has one.
"""

import argparse
import os
import sys

PLACEHOLDER_KEY = "sk-or-v1-placeholder-not-a-real-key"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--proxy", choices=["real", "loopback"], default="loopback")
    args, worker_args = parser.parse_known_args()

    os.environ["OPENROUTER_API_KEY"] = PLACEHOLDER_KEY
    for key in ("CF_ACCESS_CLIENT_ID", "CF_ACCESS_CLIENT_SECRET"):
        os.environ.pop(key, None)

    if args.proxy == "loopback":
        import gcp_harbor_trial.proxy as proxy

        real_init = proxy.SessionProxy.__init__

        def loopback_init(self, session_id, *, bind_host, **kwargs):
            real_init(self, session_id, bind_host="127.0.0.1", **kwargs)

        proxy.SessionProxy.__init__ = loopback_init

    from gcp_harbor_trial import trial

    sys.argv = ["gcp-harbor-trial", *worker_args]
    return trial.main()


if __name__ == "__main__":
    raise SystemExit(main())
