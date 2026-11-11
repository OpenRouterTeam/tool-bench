import asyncio
import json
import tempfile
import unittest
from pathlib import Path

from deploy.runner.admission import (
    AdmissionError,
    CollectionGuard,
    MainState,
    adapter_sha256,
    admit_task,
)


def stopped(container_id="actual-main", **changes):
    return MainState(**{
        "container_id": container_id, "status": "exited",
        "running": False, "paused": False, "restarting": False,
        "started_at": "2026-01-01T00:00:00Z",
        "finished_at": "2026-01-01T00:01:00Z",
        **changes,
    })


class CollectionAdmissionTests(unittest.IsolatedAsyncioTestCase):
    async def test_success_requires_a_stop_and_an_independent_matching_state(self):
        guard = CollectionGuard("trial")
        actions = []

        async def stop_main():
            actions.append("stop")

        async def inspect_main():
            actions.append("inspect")
            return stopped()

        await guard.stop(stop_main, inspect_main)
        guard.allow_collection(await inspect_main())
        self.assertEqual(actions, ["stop", "inspect", "inspect"])
        self.assertEqual(guard.receipt()["phase"], "stopped")
        self.assertEqual(guard.receipt()["main"]["container_id"], "actual-main")
        self.assertEqual(guard.receipt()["collection_checks"], 1)

    async def test_a_stop_failure_cannot_be_repaired_by_later_success_or_collection(self):
        guard = CollectionGuard("trial")

        async def stop_main():
            raise RuntimeError("injected engine failure")

        async def inspect_main():
            return stopped()

        with self.assertRaises(RuntimeError):
            await guard.stop(stop_main, inspect_main)
        with self.assertRaises(AdmissionError):
            guard.allow_collection(stopped())
        with self.assertRaises(AdmissionError):
            await guard.stop(lambda: asyncio.sleep(0), inspect_main)
        self.assertEqual(guard.receipt()["phase"], "failed")

    async def test_cancellation_during_stop_is_terminal(self):
        guard = CollectionGuard("trial")
        entered = asyncio.Event()

        async def stop_main():
            entered.set()
            await asyncio.Event().wait()

        async def inspect_main():
            return stopped()

        task = asyncio.create_task(guard.stop(stop_main, inspect_main))
        await entered.wait()
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await task
        with self.assertRaises(AdmissionError):
            guard.allow_collection(stopped())
        self.assertEqual(guard.receipt()["phase"], "failed")

    async def test_successful_stop_command_with_a_running_container_is_not_evidence(self):
        guard = CollectionGuard("trial")

        async def inspect_main():
            return stopped(status="running", running=True)

        with self.assertRaises(AdmissionError):
            await guard.stop(lambda: asyncio.sleep(0), inspect_main)
        self.assertEqual(guard.receipt()["phase"], "failed")

    async def test_collection_before_shutdown_and_replaced_or_restarted_main_are_rejected(self):
        for state in [
            stopped("other-main"),
            stopped(status="restarting", restarting=True),
            stopped(status="paused", running=True, paused=True),
            stopped(
                started_at="2026-01-01T00:00:30Z",
            ),
        ]:
            guard = CollectionGuard("trial")

            async def inspect_main():
                return stopped()

            await guard.stop(lambda: asyncio.sleep(0), inspect_main)
            with self.assertRaises(AdmissionError):
                guard.allow_collection(state)
            self.assertEqual(guard.receipt()["phase"], "failed")
        with self.assertRaises(AdmissionError):
            CollectionGuard("trial").allow_collection(stopped())

    async def test_inspection_failure_cannot_enable_collection(self):
        guard = CollectionGuard("trial")

        async def inspect_main():
            raise OSError("injected unavailable engine")

        with self.assertRaises(OSError):
            await guard.stop(lambda: asyncio.sleep(0), inspect_main)
        with self.assertRaises(AdmissionError):
            guard.allow_collection(stopped())

    async def test_collection_racing_with_shutdown_permanently_invalidates_the_attempt(self):
        guard = CollectionGuard("trial")
        entered = asyncio.Event()
        release = asyncio.Event()

        async def stop_main():
            entered.set()
            await release.wait()

        async def inspect_main():
            return stopped()

        task = asyncio.create_task(guard.stop(stop_main, inspect_main))
        await entered.wait()
        with self.assertRaises(AdmissionError):
            guard.allow_collection(stopped())
        release.set()
        with self.assertRaises(AdmissionError):
            await task
        self.assertEqual(guard.receipt()["phase"], "failed")


class TaskAdmissionTests(unittest.TestCase):
    def test_generated_policy_binds_the_actual_adapter_implementation(self):
        from deploy.runner.inference import _adapter_sha256, _COMPUTER1_ADAPTER_NAME
        policy = json.loads(
            Path("tasks/business__stripe__customer-mrr/tests/runner-policy.json").read_text()
        )
        self.assertEqual(policy["adapter_sha256"], adapter_sha256())
        self.assertEqual(policy["harbor_version"], "0.23.0")
        self.assertEqual(policy["inference_adapter"]["name"], _COMPUTER1_ADAPTER_NAME)
        self.assertEqual(policy["inference_adapter"]["code_sha256"], _adapter_sha256())

    def test_actual_generated_task_requires_explicit_runner_protocol(self):
        committed = Path(
            "tasks/business__stripe__customer-mrr/task.toml"
        ).read_text()
        # The committed tasks are generated for stock Harbor. The strict controller admits
        # the form `taskgen --runner-admission required` writes, which also declares the
        # verifier's network cut.
        original = committed.replace(
            "[verifier.environment]\n",
            '[verifier.environment]\nnetwork_mode = "no-network"\n',
            1,
        )
        self.assertNotEqual(original, committed)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "task.toml"
            path.write_text(committed)
            with self.assertRaises(AdmissionError):
                admit_task(path)
            path.write_text(original)
            self.assertEqual(admit_task(path), "business/stripe/customer-mrr")
            for changed in [
                original.replace("tool-bench-runner-v3", "unknown-runner"),
                original.replace('environment_mode = "separate"', 'environment_mode = "shared"'),
                original.replace('network_mode = "no-network"', 'network_mode = "public"'),
                original.replace('service = "world"', 'service = "main"'),
                original.replace('schema_version = "1.4"', 'schema_version = "unknown"'),
                original.replace('destination = "agent/answer.md"', 'destination = "agent/answer.md"\nexclude = ["secret"]'),
                original.replace(
                    '[[verifier.collect]]',
                    '[[verifier.collect]]\nservice = "main"\ncommand = "true"\n\n[[verifier.collect]]',
                ),
            ]:
                path.write_text(changed)
                with self.assertRaises(AdmissionError):
                    admit_task(path)

    def test_malformed_or_nonboolean_inspect_evidence_is_rejected(self):
        valid = {
            "Status": "exited",
            "Running": False,
            "Paused": False,
            "Restarting": False,
            "StartedAt": "2026-01-01T00:00:00Z",
            "FinishedAt": "2026-01-01T00:01:00Z",
        }
        state = MainState.from_inspect("main", json.dumps(valid))
        state.require_stopped()
        for changed in [
            {},
            {**valid, "Running": "false"},
            {**valid, "Paused": 0},
            {**valid, "Restarting": None},
            {**valid, "StartedAt": "0001-01-01T00:00:00Z"},
            {**valid, "FinishedAt": None},
            [],
        ]:
            with self.assertRaises(AdmissionError):
                MainState.from_inspect("main", json.dumps(changed))


if __name__ == "__main__":
    unittest.main()
