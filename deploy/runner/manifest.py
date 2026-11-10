import hashlib
import inspect
import platform
import json
from importlib.metadata import version
from pathlib import Path

import harbor
from harbor.agents.nop import NopAgent
from harbor.agents.oracle import OracleAgent

from deploy.runner.admission import AdmissionError


def digest(content: bytes) -> str:
    return "sha256:" + hashlib.sha256(content).hexdigest()

def image_fingerprints(image: dict) -> dict:
    configuration = json.loads(json.dumps(image["Config"]))
    labels = configuration.get("Labels") or {}
    project = labels.pop("com.docker.compose.project", None)
    configuration["Labels"] = labels
    encode = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return {
        "filesystem_sha256": digest(encode(image["RootFS"])),
        "runtime_config_sha256": digest(encode(configuration)),
        "config_components": {key: digest(encode(value)) for key, value in configuration.items()},
        "orchestration_project": project,
    }


def file_inventory(directory: Path) -> dict[str, str]:
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise AdmissionError("materialized environment cannot contain source links")
        if path.is_file():
            result[path.relative_to(directory).as_posix()] = digest(path.read_bytes())
    return result


def implementation_inventory() -> str:
    directory = Path(harbor.__file__).parent
    hasher = hashlib.sha256()
    for path in sorted(directory.rglob("*.py")):
        name = path.relative_to(directory).as_posix().encode()
        content = path.read_bytes()
        hasher.update(len(name).to_bytes(8, "big"))
        hasher.update(name)
        hasher.update(len(content).to_bytes(8, "big"))
        hasher.update(content)
    return "sha256:" + hasher.hexdigest()


def admit_scripted_agent(trial) -> None:
    if type(trial.agent) not in {OracleAgent, NopAgent}:
        raise AdmissionError("native runtime provenance is not admitted by this collector")
    if trial.config.agent.model_name or trial.config.agent.kwargs or trial.config.agent.env:
        raise AdmissionError("unsupported scripted-agent settings")


def execution_context(trial, task_root: Path, expected: dict) -> dict:
    admit_scripted_agent(trial)
    if digest(trial.task.instruction.encode()) != expected["instruction_sha256"]:
        raise AdmissionError("resolved instruction does not match the materialized task")
    if file_inventory(task_root / "environment") != expected["environment_files"]:
        raise AdmissionError("materialized environment sources changed before execution")
    for name in ["profile", "framing"]:
        if digest((task_root / f"tests/{name}.json").read_bytes()) != expected[f"{name}_sha256"]:
            raise AdmissionError(f"materialized {name} descriptor changed")
    implementation = Path(inspect.getfile(type(trial.agent))).read_bytes()
    if type(trial.agent) is OracleAgent:
        implementation += b"\0program\0" + json.dumps(
            file_inventory(task_root / "solution"), sort_keys=True, separators=(",", ":")
        ).encode()
    return {
        "execution_kind": "scripted",
        "python_version": platform.python_version(),
        "agent": {
            "name": trial.agent.name(),
            "version": trial.agent.version(),
            "code_sha256": digest(implementation),
        },
        "harness": {
            "name": "harbor",
            "version": version("harbor"),
            "code_sha256": implementation_inventory(),
        },
        "agent_timeout_sec": trial._agent_timeout_sec,
        "verifier_timeout_sec": trial._verifier_timeout_sec,
    }
