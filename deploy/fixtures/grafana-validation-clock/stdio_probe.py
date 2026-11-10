"""Drives the adapted mcp-grafana binary over stdio.

usage: stdio_probe.py <mcp-grafana binary> <servers/snapshots/grafana.json>

Checks (each in a fresh process, so the clock variable is per launch):
  1. initialize + tools/list equal the captured snapshot exactly (server_info,
     protocol_version, every tool definition in order);
  2. alerting_silences_write create with the authored window:
       declared scenario now      -> validation passes, the HTTP attempt fails
                                     at the closed fixture port (no fake success);
       declared instant after end -> rejected as already closed, no HTTP;
       malformed declared instant -> neutral misconfiguration error;
       unset                      -> upstream real-clock behavior.
No network service is contacted: GRAFANA_URL points at a closed local port.
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
from datetime import datetime, timezone

CLOCK_ENV = "TOOL_BENCH_GRAFANA_SILENCE_VALIDATION_NOW"
SCENARIO_NOW = "2026-11-12T15:30:00Z"
WINDOW = ("2026-11-13T02:00:00Z", "2026-11-13T04:00:00Z")
PROTOCOL = "2025-06-18"


def closed_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Server:
    def __init__(self, binary: str, clock: str | None):
        env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": os.environ.get("HOME", "/tmp"),
            "GRAFANA_URL": f"https://127.0.0.1:{closed_port()}",
            "GRAFANA_SERVICE_ACCOUNT_TOKEN": "glsa_stdio_probe_fixture",
        }
        if clock is not None:
            env[CLOCK_ENV] = clock
        self.proc = subprocess.Popen(
            [binary, "--transport", "stdio", "--usage-stats=disabled"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            env=env, text=True,
        )
        self.next_id = 0

    def send(self, message: dict) -> None:
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()

    def request(self, method: str, params: dict) -> dict:
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError(f"{method}: server closed stdout")
            message = json.loads(line)
            if message.get("id") == self.next_id:
                if "error" in message:
                    raise RuntimeError(f"{method}: {message['error']}")
                return message["result"]

    def initialize(self) -> dict:
        init = self.request("initialize", {
            "protocolVersion": PROTOCOL, "capabilities": {},
            "clientInfo": {"name": "grafana-validation-clock-probe", "version": "1"},
        })
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})
        return init

    def close(self) -> None:
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def create_silence(binary: str, clock: str | None) -> dict:
    server = Server(binary, clock)
    try:
        server.initialize()
        return server.request("tools/call", {"name": "alerting_silences_write", "arguments": {
            "operation": "create",
            "matchers": [{"name": "alertname", "value": "PostgresReplicationLag"},
                         {"name": "instance", "value": "pg-replica-2"}],
            "starts_at": WINDOW[0], "ends_at": WINDOW[1],
            "comment": "CHG-7781 storage maintenance on pg-replica-2", "created_by": "sre",
        }})
    finally:
        server.close()


def text_of(result: dict) -> str:
    return " ".join(item.get("text", "") for item in result.get("content", []))


def expect(condition: bool, label: str, detail: object = "") -> None:
    if not condition:
        sys.exit(f"FAIL {label}: {detail}")
    print(f"ok   {label}")


def main() -> None:
    binary, snapshot_path = sys.argv[1:3]
    with open(snapshot_path) as f:
        snapshot = json.load(f)

    server = Server(binary, SCENARIO_NOW)
    try:
        init = server.initialize()
        tools, cursor = [], None
        while True:
            page = server.request("tools/list", {"cursor": cursor} if cursor else {})
            tools.extend(page["tools"])
            cursor = page.get("nextCursor")
            if not cursor:
                break
    finally:
        server.close()
    expect(init["serverInfo"] == snapshot["server_info"], "serverInfo equals snapshot", init["serverInfo"])
    expect(init["protocolVersion"] == snapshot["protocol_version"], "protocolVersion equals snapshot")
    expect(len(tools) == len(snapshot["tools"]), f"{len(snapshot['tools'])} tools listed", len(tools))
    changed = [want["name"] for got, want in zip(tools, snapshot["tools"]) if got != want]
    expect(not changed, "every tool definition equals the snapshot, in order", changed)

    transport_markers = ("failed to execute request", "connection refused")

    at_now = create_silence(binary, SCENARIO_NOW)
    message = text_of(at_now)
    expect(at_now.get("isError") is True, "scenario now: no success without a backend", at_now)
    expect("in the past" not in message and any(m in message for m in transport_markers),
           "scenario now: validation passed and HTTP was attempted", message)

    expired = create_silence(binary, "2026-11-13T04:00:01Z")
    message = text_of(expired)
    expect(expired.get("isError") is True and "is in the past" in message,
           "after end: rejected as closed", message)
    expect(not any(m in message for m in transport_markers), "after end: no HTTP attempt", message)

    broken = create_silence(binary, "not-a-time")
    message = text_of(broken)
    expect(broken.get("isError") is True and "silence validation clock is misconfigured" in message,
           "malformed clock: fails closed", message)
    expect("bench" not in message.lower(), "malformed clock: error does not name the benchmark", message)

    unset = create_silence(binary, None)
    message = text_of(unset)
    end = datetime.fromisoformat(WINDOW[1].replace("Z", "+00:00"))
    if datetime.now(timezone.utc) < end:
        expect(any(m in message for m in transport_markers), "unset before real expiry: upstream accepts", message)
    else:
        expect("is in the past" in message, "unset after real expiry: upstream rejects", message)


if __name__ == "__main__":
    main()
