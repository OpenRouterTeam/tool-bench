"""Per-trial wall clock, phase timings and peak container memory for one run directory.

Reads what run-plan.sh and monitor.sh left in $RUNS/<run-id>: attempts.tsv, the Harbor
result.json of each attempt, monitor/events.jsonl (docker events) and monitor/stats.tsv (docker
stats). Stdlib only. usage: python timeline.py $RUNS/<run-id> [--json OUT]
"""

import argparse
import collections
import json
import re
import sys
from datetime import datetime
from pathlib import Path

UNITS = {"B": 1 / 2**20, "KiB": 1 / 2**10, "MiB": 1.0, "GiB": 2.0**10, "kB": 1e3 / 2**20, "MB": 1e6 / 2**20, "GB": 1e9 / 2**20}  # fmt: skip
MEMORY = re.compile(r"^([0-9.]+)\s*([A-Za-z]+)")


def mebibytes(usage: str) -> float:
    match = MEMORY.match(usage.strip())
    return float(match[1]) * UNITS[match[2]] if match else 0.0


def role(container: str, attempt: str) -> str | None:
    """agent-env/<service> or verifier-env/<service> for a container of this attempt.

    Harbor names the projects <attempt>__env and <attempt>__verifier__trial, and shortens the
    second to <prefix>__<sha1> when it would exceed 63 characters (47-character attempt ids).
    """
    if not container.startswith(f"{attempt}__"):
        return None
    service = container.rsplit("-", 1)[0]  # drop the replica number
    if service.endswith("-harbor-docker-egress-control-sidecar"):
        name = "egress-sidecar"
    else:
        name = service.rsplit("-", 1)[1]
    environment = "agent-env" if container.startswith(f"{attempt}__env-") else "verifier-env"
    return f"{environment}/{name}"


def seconds(timing: dict | None) -> float | None:
    if not timing or not timing.get("started_at") or not timing.get("finished_at"):
        return None
    start = datetime.fromisoformat(timing["started_at"])
    return round((datetime.fromisoformat(timing["finished_at"]) - start).total_seconds(), 1)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("run_dir", type=Path)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    run_dir = args.run_dir

    events = [json.loads(line) for line in (run_dir / "monitor" / "events.jsonl").open()]
    stats = [line.rstrip("\n").split("\t") for line in (run_dir / "monitor" / "stats.tsv").open()]
    report = []
    for line in (run_dir / "attempts.tsv").read_text().splitlines():
        run_id, trial, retry, start, end, code = line.split("\t")
        start, end = int(start), int(end)
        attempt_dir = run_dir / "trials" / trial / retry / trial
        result_path = attempt_dir / "trials" / trial / "result.json"
        result = json.loads(result_path.read_text()) if result_path.exists() else {}

        # docker events of this attempt's two compose projects, seconds from worker start
        mine = []
        for event in events:
            attributes = event.get("Actor", {}).get("Attributes", {})
            name = attributes.get("name", "")
            when = event["timeNano"] / 1e9
            if event["Type"] == "container" and role(name, trial) and start - 1 <= when <= end + 1:
                mine.append((when - start, event["Action"], role(name, trial)))
        first = {}
        for when, action, who in mine:
            first.setdefault((action, who), when)
        graces = []  # docker stop: SIGTERM ("kill") ... SIGKILL ("kill") ... "die"
        kills = collections.defaultdict(list)
        for when, action, who in mine:
            if action == "kill":
                kills[who].append(when)
            if action == "die" and kills[who]:
                graces.append((who, round(when - kills[who][0], 1)))
        verifier_start = first.get(("create", "verifier-env/egress-sidecar"))
        # Where the separate verifier environment's time goes (seconds).
        verifier = {}
        timing = result.get("verifier") or {}
        if verifier_start is not None and timing.get("started_at"):
            began = datetime.fromisoformat(timing["started_at"]).timestamp() - start
            main_start = first.get(("start", "verifier-env/main"))
            main_kill = first.get(("kill", "verifier-env/main"))
            main_die = first.get(("die", "verifier-env/main"))
            ended = datetime.fromisoformat(timing["finished_at"]).timestamp() - start
            if None not in (main_start, main_kill, main_die):
                verifier = {
                    "prepare (kernel probe, sidecar + verifier image)": round(verifier_start - began, 1),
                    "containers up (sidecar healthy, main started)": round(main_start - verifier_start, 1),
                    "pin exec + artifact upload + test.sh": round(main_kill - main_start, 1),
                    "stop grace of verifier main": round(main_die - main_kill, 1),
                    "teardown": round(ended - main_die, 1),
                }
        sidecar = (
            round(first[("health_status: healthy", "verifier-env/egress-sidecar")] - first[("start", "verifier-env/egress-sidecar")], 1)  # fmt: skip
            if ("health_status: healthy", "verifier-env/egress-sidecar") in first
            else None
        )

        peaks: dict[str, float] = collections.defaultdict(float)
        totals: dict[str, float] = collections.defaultdict(float)
        for when, name, usage in stats:
            who = role(name, trial)
            if who and start <= int(when) <= end:
                value = mebibytes(usage.split("/")[0])
                peaks[who] = max(peaks[who], value)
                totals[when] += value
        report.append(
            {
                "trial": trial,
                "retry": int(retry),
                "worker_exit": int(code),
                "wall_s": end - start,
                "harbor_s": {
                    "environment_setup": seconds(result.get("environment_setup")),
                    "agent_setup": seconds(result.get("agent_setup")),
                    "agent_execution": seconds(result.get("agent_execution")),
                    "verifier": seconds(result.get("verifier")),
                    "total": seconds(result),
                },
                "verifier_env_created_at_s": round(verifier_start, 1) if verifier_start else None,
                "egress_sidecar_start_to_healthy_s": sidecar,
                "verifier_env_breakdown_s": verifier,
                "stop_grace_waits_s": graces,
                "stop_grace_total_s": round(sum(g for _, g in graces if g >= 5), 1),
                "peak_memory_mib": {who: round(value) for who, value in sorted(peaks.items())},
                "peak_memory_sum_mib": round(max(totals.values(), default=0)),
            }
        )
    if args.json:
        args.json.write_text(json.dumps(report, indent=2) + "\n")
    for row in report:
        harbor = row["harbor_s"]
        print(
            f"{row['trial']} retry={row['retry']} wall={row['wall_s']}s "
            f"(env {harbor['environment_setup']}s, agent {harbor['agent_execution']}s, "
            f"verifier {harbor['verifier']}s) stop-grace={row['stop_grace_total_s']}s "
            f"sidecar-healthy={row['egress_sidecar_start_to_healthy_s']}s "
            f"peak-sum={row['peak_memory_sum_mib']}MiB {row['peak_memory_mib']}"
        )
        if row["verifier_env_breakdown_s"]:
            print(f"    verifier env: {row['verifier_env_breakdown_s']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
