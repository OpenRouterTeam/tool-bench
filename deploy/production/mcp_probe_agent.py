"""A no-model Harbor agent that runs the production MCP readiness probe, then behaves like an
MCP client that connected to every task server and is idle.

McpReadyWaitMixin is the worker's own class (agents/mcp_ready.py): the probe command, its
timeout, the infra-fail policy and the expected-tool comparison all run unmodified. Production
only attaches the mixin to model harnesses (Claude Code, Codex, ...), which this rehearsal may
not run, so this agent stands in for the harness: after the probe it opens one MCP session per
server (initialize, notifications/initialized, tools/list), holds them for `hold_sec` seconds
so container memory can be sampled, then closes them with DELETE. It never calls a tool.
"""

import json
import shlex
from typing import Any, override

from harbor.agents.installed.base import BaseInstalledAgent
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext

from gcp_harbor_trial.agents.mcp_ready import (
    INITIALIZE_BODY,
    INITIALIZED_BODY,
    TOOLS_LIST_BODY,
    McpReadyWaitMixin,
    parse_probe_status,
    parse_probe_tools,
)

HOLD_SCRIPT = r"""
H='-H content-type:application/json -H accept:application/json,text/event-stream'
tmp=$(mktemp -d); opened=0
for entry in $SERVERS; do
  name=${entry%%|*}; url=${entry#*|}
  sid=$(curl -sS -m 60 -D - -o /dev/null $H -d "$INIT" "$url" | tr -d '\r' \
        | awk -F': *' 'tolower($1)=="mcp-session-id" {print $2; exit}')
  [ -n "$sid" ] || { echo "no-session $name"; continue; }
  echo "$sid $url" >> "$tmp/sessions"; opened=$((opened + 1))
  curl -sS -m 60 -o /dev/null $H -H "mcp-session-id: $sid" -d "$INITED" "$url"
  curl -sS -m 60 -o /dev/null $H -H "mcp-session-id: $sid" -d "$LIST" "$url"
done
echo "held-open $opened"
sleep "$HOLD"
while read -r sid url; do
  curl -sS -m 10 -o /dev/null -X DELETE -H "mcp-session-id: $sid" "$url"
done < "$tmp/sessions"
echo "closed $opened"
"""


class _IdleMcpClient(BaseInstalledAgent):
    def __init__(self, *args: Any, hold_sec: int = 30, **kwargs: Any) -> None:
        self._hold_sec = int(hold_sec)
        super().__init__(*args, **kwargs)

    @staticmethod
    @override
    def name() -> str:
        return "mcp-probe"

    @override
    def version(self) -> str:
        return "1"

    @override
    async def install(self, environment: BaseEnvironment) -> None:
        return

    @override
    async def run(
        self, instruction: str, environment: BaseEnvironment, context: AgentContext
    ) -> None:
        servers = " ".join(f"{s.name}|{s.url}" for s in self.mcp_servers if s.url)
        command = (
            f"SERVERS={shlex.quote(servers)}; HOLD={self._hold_sec}; "
            f"INIT={shlex.quote(INITIALIZE_BODY)}; INITED={shlex.quote(INITIALIZED_BODY)}; "
            f"LIST={shlex.quote(TOOLS_LIST_BODY)}; {HOLD_SCRIPT}"
        )
        result = await self.exec_as_root(
            environment, command=command, timeout_sec=self._hold_sec + 300
        )
        (self.logs_dir / "idle-client.txt").write_text(result.stdout or "")


class McpProbeAgent(McpReadyWaitMixin, _IdleMcpClient):
    @override
    def log_expected_tool_mismatches(self, stdout: str | None) -> dict[str, list[str]]:
        """Unchanged behaviour; additionally saves what the probe reported, host-side."""
        mismatches = super().log_expected_tool_mismatches(stdout)
        status = parse_probe_status(stdout)
        listed = parse_probe_tools(stdout)
        (self.logs_dir / "mcp-probe.json").write_text(
            json.dumps(
                {
                    "status": status.kind if status else None,
                    "missing_servers": list(status.missing) if status else None,
                    "servers_ready": len(listed),
                    "tools_listed": {server: len(tools) for server, tools in listed.items()},
                    "expected_tools": self.mcp_expected_tools(),
                    "expected_tools_not_listed": mismatches,
                },
                indent=2,
            )
        )
        return mismatches
