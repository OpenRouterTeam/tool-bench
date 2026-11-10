import asyncio
import hashlib
import json
import tomllib
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Awaitable, Callable


RUNNER_PROTOCOL = "tool-bench-runner-v3"
VERIFICATION_PROTOCOL = "offline-world-evidence-v5"
HARBOR_VERSION = "0.23.0"


class AdmissionError(RuntimeError):
    pass


def adapter_sha256() -> str:
    digest = hashlib.sha256()
    for name in ["admission.py", "strict_docker.py", "snapshot.py", "controller.py", "manifest.py",
                 "inference.py", "computer1_capture.py"]:
        content = Path(__file__).with_name(name).read_bytes()
        digest.update(name.encode())
        digest.update(b"\0")
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def admit_task(path: Path) -> str:
    try:
        task = tomllib.loads(path.read_text())
        metadata = task["metadata"]
        verifier = task["verifier"]
        artifacts = task["artifacts"]
        scenario = metadata["scenario"]
        supported = (
            task["schema_version"] == "1.4"
            and metadata["verification_protocol"] == VERIFICATION_PROTOCOL
            and metadata.get("runner_protocol") == RUNNER_PROTOCOL
            and isinstance(scenario, str)
            and bool(scenario)
            and not task.get("steps")
            and task.get("environment", {}).get("os", "linux") == "linux"
            and verifier["environment_mode"] == "separate"
            and verifier["environment"]["network_mode"] == "no-network"
            and any(hook["service"] == "world" for hook in verifier["collect"])
            and all(hook["service"] != "main" for hook in verifier["collect"])
            and [
                artifact
                for artifact in artifacts
                if artifact.get("service", "main") == "main"
            ] == [{"source": "/app/answer.md", "destination": "agent/answer.md"}]
            and sum(
                artifact.get("service") == "world"
                and artifact.get("source") == "/exports/evidence.json"
                and artifact.get("destination") == "world/evidence.json"
                for artifact in artifacts
            )
            == 1
        )
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise AdmissionError("task does not declare a supported verification protocol") from error
    if not supported:
        raise AdmissionError("task does not declare a supported verification protocol")
    return scenario


@dataclass(frozen=True)
class MainState:
    container_id: str
    status: str
    running: bool
    paused: bool
    restarting: bool
    started_at: str
    finished_at: str

    @classmethod
    def from_inspect(cls, container_id: str, data: str) -> "MainState":
        try:
            state = json.loads(data)
            result = cls(
                container_id=container_id,
                status=state["Status"],
                running=state["Running"],
                paused=state["Paused"],
                restarting=state["Restarting"],
                started_at=state["StartedAt"],
                finished_at=state["FinishedAt"],
            )
        except (ValueError, KeyError, TypeError) as error:
            raise AdmissionError("missing or malformed main-container state") from error
        if (
            not isinstance(result.container_id, str)
            or not result.container_id
            or not isinstance(result.status, str)
            or any(
                not isinstance(value, str) or not value or value.startswith("0001-")
                for value in (result.started_at, result.finished_at)
            )
            or any(
                type(value) is not bool
                for value in (result.running, result.paused, result.restarting)
            )
        ):
            raise AdmissionError("missing or malformed main-container state")
        return result

    def require_stopped(self) -> None:
        if (
            self.running
            or self.paused
            or self.restarting
            or self.status not in {"exited", "dead"}
        ):
            raise AdmissionError("main container is not proven stopped")


class CollectionGuard:
    def __init__(self, session_id: str):
        self.session_id = session_id
        self.phase = "open"
        self.failure: str | None = None
        self.main: MainState | None = None
        self.collection_checks = 0
        self._stop_lock = asyncio.Lock()

    def reject(self, reason: str) -> None:
        if self.failure is None:
            self.failure = reason
        self.phase = "failed"

    def require_ready(self) -> None:
        if self.phase != "stopped" or self.failure is not None:
            raise AdmissionError(self.failure or "main shutdown has not been confirmed")

    async def stop(
        self,
        stop_main: Callable[[], Awaitable[None]],
        inspect_main: Callable[[], Awaitable[MainState]],
    ) -> None:
        async with self._stop_lock:
            if self.failure is not None:
                raise AdmissionError(self.failure)
            try:
                if self.phase == "stopped":
                    self.allow_collection(await inspect_main())
                    return
                self.phase = "stopping"
                await stop_main()
                state = await inspect_main()
                state.require_stopped()
                if self.failure is not None:
                    raise AdmissionError(self.failure)
                self.main = state
                self.phase = "stopped"
            except BaseException:
                self.reject("main shutdown or its independent state check failed")
                raise

    def allow_collection(self, state: MainState) -> None:
        try:
            self.require_ready()
            state.require_stopped()
            if self.main is None or state.container_id != self.main.container_id:
                raise AdmissionError("main-container identity changed before collection")
            if state != self.main:
                raise AdmissionError("main-container lifecycle changed after shutdown")
        except AdmissionError as error:
            self.reject(str(error))
            raise
        self.collection_checks += 1

    def receipt(self) -> dict:
        return {
            "schema_version": 1,
            "runner_protocol": RUNNER_PROTOCOL,
            "verification_protocol": VERIFICATION_PROTOCOL,
            "session_id": self.session_id,
            "phase": self.phase,
            "failure": self.failure,
            "main": asdict(self.main) if self.main is not None else None,
            "collection_checks": self.collection_checks,
        }
