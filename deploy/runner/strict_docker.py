import asyncio
import hashlib
import json
import secrets
import shlex
import shutil
import tempfile
import tomllib
from importlib.metadata import version
from pathlib import Path

from harbor.environments.docker.docker import DockerEnvironment
from harbor.environments.capabilities import EnvironmentCapabilities
from harbor.models.task.config import NetworkMode, TaskOS
from harbor.agents.oracle import OracleAgent, EnvironmentPaths, build_execution_command

from deploy.runner.admission import (
    HARBOR_VERSION,
    AdmissionError,
    CollectionGuard,
    MainState,
    RUNNER_PROTOCOL,
    VERIFICATION_PROTOCOL,
    adapter_sha256,
    admit_task,
)
from deploy.runner.snapshot import MAX_BYTES, extract_snapshot, fingerprint, snapshot_tree
from deploy.runner.manifest import digest, execution_context, image_fingerprints
from deploy.runner.inference import InferenceEvidence, admit_evidence
from deploy.runner.surface_audit import SurfaceAudit, capture_pre_agent, seal_post_stop


class StrictDockerEnvironment(DockerEnvironment):
    def __init__(self, *args, **kwargs):
        installed = version("harbor")
        if installed != HARBOR_VERSION:
            raise AdmissionError(
                f"unsupported Harbor version {installed}; validated version is {HARBOR_VERSION}"
            )
        environment_dir = kwargs.get("environment_dir", args[0] if args else None)
        if environment_dir is None:
            raise AdmissionError("a materialized task environment is required")
        directory = Path(environment_dir)
        if directory.name not in {"environment", "tests"}:
            raise AdmissionError("unsupported materialized environment role")
        self._is_verifier = directory.name == "tests"
        self._offline_network_directory = None
        self._offline_network_override = None
        if self._is_verifier:
            configuration = kwargs.get("task_env_config", args[4] if len(args) > 4 else None)
            if configuration is None or configuration.os != TaskOS.LINUX:
                raise AdmissionError("the offline verifier requires Linux containers")
            network_policy = kwargs.get("network_policy")
            mode = getattr(getattr(network_policy, "network_mode", None), "value", None)
            if mode != "no-network":
                raise AdmissionError("a separate verifier must have no network")
            for policy in [network_policy, *kwargs.get("phase_network_policies", ())]:
                if policy.network_mode != NetworkMode.NO_NETWORK or policy.allowed_hosts:
                    raise AdmissionError("offline verifier networking is immutable")
        self._scenario = admit_task(Path(environment_dir).parent / "task.toml")
        self._adapter_sha256 = adapter_sha256()
        self._trial_admin_token = None if self._is_verifier else secrets.token_hex(32)
        task_root = Path(environment_dir).parent
        self._task_root = task_root
        context = json.loads((task_root / "tests/task-context.json").read_text())
        self._connector_env_sha256 = context.get("connector_env_sha256", {})
        if not isinstance(self._connector_env_sha256, dict) or any(
            not isinstance(name, str)
            or not name
            or not name.isascii()
            or not name.replace("_", "").isalnum()
            or name != name.upper()
            or not isinstance(value, str)
            or len(value) != 71
            or not value.startswith("sha256:")
            or any(character not in "0123456789abcdef" for character in value[7:])
            for name, value in self._connector_env_sha256.items()
        ):
            raise AdmissionError("invalid declared connector configuration fingerprints")
        self._contract_sha256 = "sha256:" + hashlib.sha256(
            (task_root / "tests/expect.json").read_bytes()
        ).hexdigest()
        self._evidence_sha256 = None
        task = tomllib.loads((task_root / "task.toml").read_text())
        policy = json.loads((task_root / "tests/runner-policy.json").read_text())
        if (
            policy.get("schema_version") != 1
            or policy.get("runner_protocol") != RUNNER_PROTOCOL
            or policy.get("verification_protocol") != VERIFICATION_PROTOCOL
            or policy.get("harbor_version") != installed
            or policy.get("adapter_sha256") != self._adapter_sha256
        ):
            raise AdmissionError("materialized task requires a different runner implementation")
        world_hooks = [
            hook["command"]
            for hook in task["verifier"]["collect"]
            if hook["service"] == "world"
        ]
        if len(world_hooks) != 1:
            raise AdmissionError("expected exactly one world evidence collect hook")
        self._collect_command = world_hooks[0]
        super().__init__(*args, **kwargs)
        if self._is_verifier:
            self._offline_network_directory = tempfile.TemporaryDirectory(
                prefix="tool-bench-offline-network-"
            )
            self._offline_network_override = Path(self._offline_network_directory.name) / "network.yaml"
            self._offline_network_override.write_text(
                "services:\n  main:\n    network_mode: none\n"
            )
            return
        self._collection_guard = CollectionGuard(self.session_id)
        self._receipt_path = self.trial_paths.trial_dir / "runner-admission.json"
        self._controller_registered = False
        self._agent_finished = False
        self._oracle_command = None
        self._oracle_executions = 0
        self._oracle_completed = False
        self._snapshot_root = self.trial_paths.trial_dir / "main-snapshot"
        self._mounted_publish = self.trial_paths.artifacts_dir / "logs/artifacts"
        self._publish_sha256 = None
        self._answer_sha256 = None
        self._answer_snapshot_loaded = False
        self._answer_present = False
        self._answer_copied = False
        self._main_artifacts_sha256 = None
        self._execution_manifest = None
        self._manifest_sha256 = None
        self._inference_evidence_sha256 = None
        self._observed_agent = None
        self._surface_audit = SurfaceAudit.create(task_root, self._scenario, self._trial_admin_token)
        self._surface_audit_sealed = None
        self._write_receipt()

    def _requires_egress_control(self, *, startup_network_policy, phase_network_policies):
        if self._is_verifier:
            return False
        return super()._requires_egress_control(
            startup_network_policy=startup_network_policy,
            phase_network_policies=phase_network_policies,
        )

    @property
    def capabilities(self):
        if self._is_verifier:
            return EnvironmentCapabilities(
                disable_internet=True, mounted=True, docker_compose=True
            )
        return super().capabilities

    def validate_network_policy_support(self, network_policy=None):
        if self._is_verifier:
            policy = network_policy or self.network_policy
            if policy.network_mode != NetworkMode.NO_NETWORK or policy.allowed_hosts:
                raise AdmissionError("offline verifier networking is immutable")
            return
        return super().validate_network_policy_support(network_policy)

    @property
    def _docker_compose_paths(self):
        paths = super()._docker_compose_paths
        if self._is_verifier and self._offline_network_override is not None:
            paths.append(self._offline_network_override)
        return paths

    async def _require_offline_network(self):
        if not self._is_verifier:
            raise AdmissionError("offline network observation requires the verifier role")
        result = await self._run_docker_compose_command(
            ["ps", "--all", "--quiet", "main"], timeout_sec=20
        )
        containers = (result.stdout or "").split()
        if len(containers) != 1:
            raise AdmissionError("cannot independently identify the offline verifier")
        observed = await self._engine_json(
            "inspect", "--format",
            '{"network_mode":{{json .HostConfig.NetworkMode}},"networks":{{json .NetworkSettings.Networks}}}',
            containers[0],
        )
        networks = observed.get("networks")
        if observed.get("network_mode") != "none" or not isinstance(networks, dict) or set(networks) - {"none"}:
            raise AdmissionError("the actual verifier container is not network-isolated")

    async def start(self, force_build):
        if self._is_verifier and self._offline_network_override is None:
            raise AdmissionError("missing offline verifier network configuration")
        await super().start(force_build)
        if self._is_verifier:
            try:
                await self._require_offline_network()
            except BaseException:
                try:
                    await super().stop(delete=True)
                except Exception:
                    self.logger.warning("failed to clean up an inadmissible offline verifier")
                raise

    async def stop(self, delete):
        try:
            return await super().stop(delete)
        finally:
            directory = getattr(self, "_offline_network_directory", None)
            if directory is not None:
                directory.cleanup()

    def register_collection_controller(self) -> None:
        if self._is_verifier or self._controller_registered:
            raise AdmissionError("strict collection controller cannot be registered twice")
        self._controller_registered = True

    def register_oracle_program(self, agent, task_os) -> None:
        if (
            self._is_verifier
            or not self._controller_registered
            or self._oracle_command is not None
            or type(agent) is not OracleAgent
            or task_os != TaskOS.LINUX
        ):
            raise AdmissionError("unsupported or repeated oracle execution registration")
        solution_dir, solve_path = agent._resolve_solution_paths()
        paths = EnvironmentPaths.for_os(task_os)
        script = paths.solution_dir / solve_path.relative_to(solution_dir).as_posix()
        self._oracle_command = build_execution_command(
            str(script), stdout_path=str(paths.agent_dir / OracleAgent._ORACLE_LOG_FILE),
            task_os=task_os,
        )

    async def exec(self, command, *args, **kwargs):
        expected = getattr(self, "_oracle_command", None)
        observed = not self._is_verifier and expected is not None and command == expected
        if observed:
            if self._oracle_executions:
                self._collection_guard.reject("oracle solution execution was repeated")
                self._write_receipt()
                raise AdmissionError("oracle solution execution was repeated")
            self._oracle_executions += 1
        try:
            result = await super().exec(command, *args, **kwargs)
        except BaseException:
            if observed:
                self._collection_guard.reject("oracle solution execution did not complete")
                self._write_receipt()
            raise
        if observed:
            if type(result.return_code) is not int or result.return_code != 0:
                self._collection_guard.reject("oracle solution execution returned a nonzero or invalid code")
                self._write_receipt()
                raise AdmissionError("oracle solution execution returned a nonzero or invalid code")
            self._oracle_completed = True
        return result

    async def _engine_json(self, *arguments) -> dict:
        process = await asyncio.create_subprocess_exec(
            *type(self).runtime().engine_cmd(*arguments),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL,
        )
        try:
            output, _ = await asyncio.wait_for(process.communicate(), timeout=20)
            if process.returncode != 0:
                raise AdmissionError("cannot independently inspect execution metadata")
            return json.loads(output)
        finally:
            if process.returncode is None:
                process.kill()
            await process.wait()

    async def _observe_image(self, role: str) -> dict:
        result = await self._run_docker_compose_command(
            ["ps", "--all", "--quiet", role], timeout_sec=20
        )
        containers = (result.stdout or "").split()
        if len(containers) != 1:
            raise AdmissionError(f"expected exactly one {role} execution container")
        template = (
            '{"container_id":{{json .Id}},"image_id":{{json .Image}},'
            '"reference":{{json .Config.Image}},"resource_limits":{'
            '"memory":{{.HostConfig.Memory}},"nano_cpus":{{.HostConfig.NanoCpus}},'
            '"cpu_quota":{{.HostConfig.CpuQuota}},"cpu_period":{{.HostConfig.CpuPeriod}}}}'
        )
        observed = await self._engine_json("inspect", "--format", template, containers[0])
        image = await self._engine_json("image", "inspect", "--format", '{"Config":{{json .Config}},"RootFS":{{json .RootFS}}}', observed["image_id"])
        result = {**observed, **image_fingerprints(image)}
        expected = getattr(self, "_connector_env_sha256", {})
        if role == "tools" and expected:
            configuration = await self._engine_json(
                "inspect", "--format", '{"env":{{json .Config.Env}}}', containers[0]
            )
            entries = configuration.get("env")
            if not isinstance(entries, list) or any(not isinstance(value, str) for value in entries):
                raise AdmissionError("cannot observe actual tools-container connection settings")
            actual = {}
            for name in expected:
                values = [value.partition("=")[2] for value in entries if value.partition("=")[0] == name and "=" in value]
                if len(values) != 1:
                    raise AdmissionError("declared connector configuration is missing or ambiguous")
                actual[name] = digest(values[0].encode())
            if actual != expected:
                raise AdmissionError("actual tools-container connection settings differ from the declared profile")
            result["connector_env_sha256"] = actual
        return result

    async def observe_execution(self, trial) -> None:
        if self._execution_manifest is not None:
            raise AdmissionError("execution metadata was already captured")
        context_bytes = (self._task_root / "tests/task-context.json").read_bytes()
        context = json.loads(context_bytes)
        producer = execution_context(trial, self._task_root, context)
        self._observed_agent = trial.agent
        images = {role: await self._observe_image(role) for role in ["main", "world", "tools"]}
        for role in ["world", "tools"]:
            if images[role]["reference"] != context[f"{role}_reference"]:
                raise AdmissionError("realized sidecar differs from the declared image reference")
        self._execution_manifest = {
            "schema_version": 1,
            "runner_protocol": RUNNER_PROTOCOL,
            "verification_protocol": VERIFICATION_PROTOCOL,
            "scenario": self._scenario,
            "session_id": self.session_id,
            "task_source_sha256": digest((self._task_root / "tests/task-source.json").read_bytes()),
            "task_context_sha256": digest(context_bytes),
            "context": context,
            **producer,
            "images": images,
        }
        path = self.trial_paths.trial_dir / "realized-manifest.json"
        path.write_text(json.dumps(self._execution_manifest, indent=2) + "\n")
        self._manifest_sha256 = digest(path.read_bytes())
        self._write_receipt()
        if getattr(self, "_surface_audit", None) is not None:
            await capture_pre_agent(self, trial)

    async def _publish_manifest(self) -> None:
        if self._execution_manifest is None:
            raise AdmissionError("execution provenance was not observed before the agent phase")
        for role, expected in self._execution_manifest["images"].items():
            if await self._observe_image(role) != expected:
                raise AdmissionError("execution container, image or resource limits changed")
        path = self.trial_paths.trial_dir / "realized-manifest.json"
        content = path.read_text()
        if digest(content.encode()) != self._manifest_sha256:
            raise AdmissionError("protected execution manifest was changed")
        result = await super().service_exec(
            "printf '%s' " + shlex.quote(content) + " > /exports/realized-manifest.json",
            service="world", timeout_sec=20,
        )
        if result.return_code != 0:
            raise AdmissionError("cannot deliver the protected execution manifest")

    async def _publish_inference(self) -> None:
        if self._inference_evidence_sha256 is None:
            raise AdmissionError("host inference evidence was not sealed")
        source = self.trial_paths.trial_dir / "inference-evidence.json"
        if source.is_symlink():
            raise AdmissionError("protected inference evidence cannot be a link")
        content = source.read_bytes()
        if digest(content) != self._inference_evidence_sha256:
            raise AdmissionError("protected inference evidence changed after capture")
        text = content.decode("utf-8")
        result = await super().service_exec(
            "printf '%s' " + shlex.quote(text) + " > /exports/inference-evidence.json.tmp"
            + " && mv /exports/inference-evidence.json.tmp /exports/inference-evidence.json",
            service="world", timeout_sec=20,
        )
        if result.return_code != 0:
            raise AdmissionError("cannot publish protected inference evidence")

    async def finish_agent(self) -> None:
        try:
            if not self._controller_registered or self._agent_finished:
                raise AdmissionError("missing or repeated strict agent-end lifecycle hook")
            producer = (getattr(self, "_execution_manifest", None) or {}).get("agent", {}).get("name")
            if (getattr(self, "_oracle_command", None) is not None or producer == "oracle") and (
                not getattr(self, "_oracle_completed", False)
                or getattr(self, "_oracle_executions", 0) != 1
            ):
                self._collection_guard.reject("oracle solution did not complete successfully")
                raise AdmissionError("oracle solution did not complete successfully")
            await super().prepare_logs_for_host()
            await self.stop_service("main")
            self._agent_finished = True
            await self._admit_collection()
            self._snapshot_root.mkdir(mode=0o700)
            self._publish_sha256 = snapshot_tree(
                self._mounted_publish, self._snapshot_root / "publish"
            )
            await self._admit_collection()
        except BaseException:
            self._collection_guard.reject("post-stop main snapshot preparation failed")
            raise
        finally:
            self._write_receipt()

    async def prepare_logs_for_host(self) -> None:
        if not self._is_verifier and self._collection_guard.phase != "open":
            return
        await super().prepare_logs_for_host()

    def _compose_env_vars(self, include_os_env: bool = True) -> dict[str, str]:
        environment = super()._compose_env_vars(include_os_env=include_os_env)
        if self._trial_admin_token is not None:
            environment["HARBOR_TRIAL_ADMIN_TOKEN"] = self._trial_admin_token
        else:
            environment.pop("HARBOR_TRIAL_ADMIN_TOKEN", None)
        return environment

    def bind_inference_evidence(self, evidence: InferenceEvidence, *, host_capture=None) -> None:
        """Bind validated inference evidence into the runner receipt.

        The evidence must already have passed admit_evidence() validation.
        This binds its content hash into the receipt for downstream verification.
        """
        if self._is_verifier:
            raise AdmissionError("verifier cannot bind inference evidence")
        if self._inference_evidence_sha256 is not None:
            raise AdmissionError("inference evidence was already bound")
        admit_evidence(evidence)
        if evidence.scenario != self._scenario or evidence.session_id != self.session_id:
            raise AdmissionError("inference capture belongs to a different trial")
        if not evidence.scripted:
            from deploy.runner.computer1_capture import Computer1HostCapture
            from deploy.runner.proxy_capture import ProviderProxyCapture
            if type(host_capture) not in (Computer1HostCapture, ProviderProxyCapture):
                raise AdmissionError("native evidence has no host capture authority")
            observed_agent = getattr(self, "_observed_agent", None)
            if observed_agent is None:
                raise AdmissionError("native execution producer was not observed")
            host_capture.validate_sealed(evidence, agent=observed_agent)
        evidence_json = evidence.to_json()
        evidence_path = self.trial_paths.trial_dir / "inference-evidence.json"
        if evidence_path.exists() or evidence_path.is_symlink():
            raise AdmissionError("inference evidence output already exists")
        with evidence_path.open("x", encoding="utf-8") as output:
            output.write(evidence_json + "\n")
        self._inference_evidence_sha256 = digest(evidence_path.read_bytes())
        self._write_receipt()

    def _write_receipt(self) -> None:
        receipt = self._collection_guard.receipt()
        receipt.update(
            {
                "scenario": self._scenario,
                "harbor_version": HARBOR_VERSION,
                "adapter_sha256": self._adapter_sha256,
                "contract_sha256": self._contract_sha256,
                "evidence_sha256": self._evidence_sha256,
                "main_artifacts_sha256": self._main_artifacts_sha256,
                "answer_sha256": self._answer_sha256,
                "manifest_sha256": self._manifest_sha256,
                "inference_evidence_sha256": self._inference_evidence_sha256,
            }
        )
        if getattr(self, "_surface_audit_sealed", None) is not None:
            receipt["surface_audit"] = self._surface_audit_sealed
        temporary = self._receipt_path.with_suffix(".json.tmp")
        temporary.write_text(json.dumps(receipt, indent=2) + "\n")
        temporary.replace(self._receipt_path)

    async def _publish_receipt(self) -> None:
        await self._admit_collection()
        self._verify_main_artifacts()
        await self._publish_manifest()
        await self._publish_inference()
        result = await super().service_exec(
            "sha256sum /exports/evidence.json", service="world", timeout_sec=20
        )
        fields = (result.stdout or "").split()
        if (
            result.return_code != 0
            or len(fields) != 2
            or fields[1] != "/exports/evidence.json"
            or len(fields[0]) != 64
            or any(character not in "0123456789abcdef" for character in fields[0])
        ):
            raise AdmissionError("cannot bind runner admission to collected world evidence")
        self._evidence_sha256 = f"sha256:{fields[0]}"
        await seal_post_stop(self)
        self._write_receipt()
        receipt = self._receipt_path.read_text()
        command = (
            "mkdir -p /exports && printf '%s' "
            + shlex.quote(receipt)
            + " > /exports/runner-admission.json.tmp"
            + " && mv /exports/runner-admission.json.tmp /exports/runner-admission.json"
        )
        result = await super().service_exec(command, service="world", timeout_sec=20)
        if result.return_code != 0:
            raise AdmissionError("cannot publish trusted runner admission")

    async def _inspect_main(self) -> MainState:
        result = await self._run_docker_compose_command(
            ["ps", "--all", "--quiet", "main"], timeout_sec=20
        )
        containers = (result.stdout or "").split()
        if len(containers) != 1:
            raise AdmissionError("expected exactly one main container")
        process = await asyncio.create_subprocess_exec(
            *type(self).runtime().engine_cmd(
                "inspect", "--format", "{{json .State}}", containers[0]
            ),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.DEVNULL,
        )
        try:
            output, _ = await asyncio.wait_for(process.communicate(), timeout=20)
        except BaseException:
            if process.returncode is None:
                process.kill()
            await process.wait()
            raise
        if process.returncode != 0:
            raise AdmissionError("cannot independently inspect the main container")
        return MainState.from_inspect(containers[0], output.decode("utf-8"))

    async def stop_service(self, service: str) -> None:
        if self._is_verifier or service != "main":
            await super().stop_service(service)
            return

        async def stop_main() -> None:
            await self._run_docker_compose_command(
                ["stop", "--timeout", "10", "main"], timeout_sec=30
            )

        try:
            await self._collection_guard.stop(stop_main, self._inspect_main)
        finally:
            self._write_receipt()

    async def _admit_collection(self) -> None:
        if self._is_verifier:
            raise AdmissionError("a separate verifier cannot access sidecar services")
        try:
            if not self._agent_finished:
                raise AdmissionError("strict agent-end collection controller has not run")
            self._collection_guard.require_ready()
            self._collection_guard.allow_collection(await self._inspect_main())
        except BaseException:
            self._collection_guard.reject("collection lacks a valid agent-end shutdown proof")
            raise
        finally:
            self._write_receipt()

    async def _snapshot_answer(self, source: str) -> Path | None:
        try:
            await self._admit_collection()
            if source != "/app/answer.md" or self._publish_sha256 is None:
                raise AdmissionError("unsupported main snapshot source or lifecycle")
            target = self._snapshot_root / "answer"
            if not self._answer_snapshot_loaded:
                await self._copy_main_archive(source, target)
                self._answer_present = target.exists()
                self._answer_snapshot_loaded = True
                if target.is_file():
                    self._answer_sha256 = "sha256:" + hashlib.sha256(
                        target.read_bytes()
                    ).hexdigest()
            await self._admit_collection()
            return target if self._answer_present else None
        except BaseException:
            self._collection_guard.reject("post-stop main artifact capture failed")
            self._write_receipt()
            raise

    async def _copy_main_archive(self, source: str, target: Path) -> None:
        container = self._collection_guard.main.container_id
        process = await asyncio.create_subprocess_exec(
            *type(self).runtime().engine_cmd("cp", f"{container}:{source}", "-"),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            with tempfile.TemporaryFile() as archive:
                async def receive_archive() -> None:
                    size = 0
                    while chunk := await process.stdout.read(64 * 1024):
                        size += len(chunk)
                        if size > MAX_BYTES + 16 * 1024 * 1024:
                            raise AdmissionError("main archive transfer exceeds resource bounds")
                        archive.write(chunk)

                async def receive_error() -> bytes:
                    result = bytearray()
                    while chunk := await process.stderr.read(4096):
                        result.extend(chunk)
                        if len(result) > 64 * 1024:
                            raise AdmissionError("main archive transfer has excessive diagnostics")
                    return bytes(result)

                async with asyncio.TaskGroup() as transfers:
                    transfers.create_task(receive_archive())
                    error_task = transfers.create_task(receive_error())
                    await asyncio.wait_for(process.wait(), timeout=30)
                error = error_task.result().decode("utf-8", errors="strict").strip()
                if process.returncode != 0:
                    missing = (
                        "Error response from daemon: Could not find the file "
                        f"{source} in container {container}"
                    )
                    if process.returncode == 1 and error == missing and archive.tell() == 0:
                        return
                    raise AdmissionError("host engine failed to copy the stopped main artifact")
                archive.seek(0)
                extract_snapshot(archive, target)
        finally:
            if process.returncode is None:
                process.kill()
            await process.wait()

    def _verify_main_artifacts(self) -> None:
        try:
            if not self._answer_snapshot_loaded:
                raise AdmissionError("main answer collection was not attempted")
            if (
                self._publish_sha256 != fingerprint(self._mounted_publish)
                or self._publish_sha256 != fingerprint(self._snapshot_root / "publish")
            ):
                raise AdmissionError("mounted publish artifacts changed after shutdown")
            answer = self.trial_paths.artifacts_dir / "agent/answer.md"
            manifest = json.loads(
                (self.trial_paths.artifacts_dir / "manifest.json").read_text()
            )
            entries = {
                entry["source"]: entry
                for entry in manifest
                if entry.get("service") in {None, "main"}
            }
            answer_entry = entries["/app/answer.md"]
            publish_entry = entries["/logs/artifacts"]
            if (
                len(entries) != 2
                or answer_entry["destination"] != "artifacts/agent/answer.md"
                or publish_entry["destination"] != "artifacts/logs/artifacts"
                or publish_entry["type"] != "directory"
            ):
                raise AdmissionError("main artifact manifest does not match the admitted layout")
            if self._answer_present:
                if (
                    not self._answer_copied
                    or answer_entry["status"] != "ok"
                    or fingerprint(answer) != fingerprint(self._snapshot_root / "answer")
                ):
                    raise AdmissionError("collected main answer does not match its stopped snapshot")
            elif answer.exists() or answer.is_symlink() or answer_entry["status"] != "failed":
                raise AdmissionError("missing main answer has inconsistent collection status")
            publish_status = "ok" if any(self._mounted_publish.iterdir()) else "empty"
            if publish_entry["status"] != publish_status:
                raise AdmissionError("mounted publish collection did not complete")
            binding = {
                "answer": fingerprint(self._snapshot_root / "answer")
                if self._answer_present else None,
                "publish": self._publish_sha256,
            }
            self._main_artifacts_sha256 = "sha256:" + hashlib.sha256(
                json.dumps(binding, sort_keys=True).encode()
            ).hexdigest()
            self._write_receipt()
        except BaseException:
            self._collection_guard.reject("main artifact collection is incomplete or inconsistent")
            self._write_receipt()
            raise

    async def service_is_dir(
        self,
        path: str,
        *,
        service: str | None = None,
        user: str | int | None = None,
    ) -> bool:
        if self._is_verifier or service not in {None, "main"}:
            return await super().service_is_dir(path, service=service, user=user)
        snapshot = await self._snapshot_answer(path)
        if snapshot is None:
            raise FileNotFoundError(path)
        return snapshot.is_dir()

    async def service_exec(
        self,
        command: str,
        *,
        service: str | None = None,
        cwd: str | None = None,
        env: dict[str, str] | None = None,
        timeout_sec: int | None = None,
        user: str | int | None = None,
    ):
        if service not in {None, "main"}:
            await self._admit_collection()
        collecting_world = service == "world" and command == self._collect_command
        try:
            result = await super().service_exec(
                command,
                service=service,
                cwd=cwd,
                env=env,
                timeout_sec=timeout_sec,
                user=user,
            )
            if collecting_world:
                if result.return_code != 0:
                    raise AdmissionError("world evidence collect hook failed")
                await self._publish_receipt()
        except BaseException:
            if collecting_world:
                self._collection_guard.reject("trusted world evidence collection failed")
                self._write_receipt()
            raise
        return result

    async def service_download_file(
        self,
        source_path: str,
        target_path: Path | str,
        *,
        service: str | None = None,
    ) -> None:
        if not self._is_verifier and service in {None, "main"}:
            snapshot = await self._snapshot_answer(source_path)
            if snapshot is None:
                raise FileNotFoundError(source_path)
            try:
                shutil.copyfile(snapshot, target_path)
            except BaseException:
                self._collection_guard.reject("main answer delivery failed")
                self._write_receipt()
                raise
            self._answer_copied = True
            return
        if service not in {None, "main"}:
            await self._admit_collection()
        await super().service_download_file(source_path, target_path, service=service)

    async def service_download_dir(
        self,
        source_dir: str,
        target_dir: Path | str,
        *,
        service: str | None = None,
    ) -> None:
        if not self._is_verifier and service in {None, "main"}:
            snapshot = await self._snapshot_answer(source_dir)
            if snapshot is None:
                raise FileNotFoundError(source_dir)
            try:
                shutil.copytree(snapshot, target_dir, dirs_exist_ok=True, symlinks=True)
            except BaseException:
                self._collection_guard.reject("main answer directory delivery failed")
                self._write_receipt()
                raise
            self._answer_copied = True
            return
        if service not in {None, "main"}:
            await self._admit_collection()
        await super().service_download_dir(source_dir, target_dir, service=service)
