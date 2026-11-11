import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which("docker"), "Docker Compose config consumer is required")
class BootstrapTopologyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(
            ["cargo", "build", "--quiet", "--locked", "-p", "scenario", "--bin", "taskgen"],
            cwd=REPO, check=True,
            env={**os.environ, "CARGO_BUILD_JOBS": "2"},
        )
        metadata = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
            cwd=REPO, check=True, capture_output=True, text=True,
        )
        cls.binary = Path(json.loads(metadata.stdout)["target_directory"]) / "debug/taskgen"

    def generated(self, directory: Path, track: str):
        scenarios = directory / "scenarios"
        fixture = scenarios / "example/github/basic"
        shutil.copytree(REPO / "scenarios/example/github/basic", fixture)
        manifest = fixture / "scenario.toml"
        with manifest.open("a") as output:
            output.write(f'\n[expect]\ninterfaces = "{track}"\n')
        tasks = directory / "tasks"
        generated = subprocess.run(
            [
                str(self.binary), "--scenarios", str(scenarios), "--out", str(tasks),
                "--world-image", "world:source-test", "--tools-image", "tools:source-test",
                "--include-examples",
            ],
            cwd=REPO, capture_output=True, text=True,
        )
        self.assertEqual(generated.returncode, 0, generated.stderr)
        base = directory / "base.yaml"
        base.write_text('services:\n  main:\n    image: "agent:source-test"\n')
        compose = tasks / "example__github__basic/environment/docker-compose.yaml"
        normalized = subprocess.run(
            [
                "docker", "compose", "--project-name", "source-topology",
                "-f", str(base), "-f", str(compose), "config", "--format", "json",
            ],
            capture_output=True, text=True,
            env={**os.environ, "HARBOR_TRIAL_ADMIN_TOKEN": "source-fixture-token"},
        )
        self.assertEqual(normalized.returncode, 0, normalized.stderr)
        return json.loads(normalized.stdout)

    def test_the_real_compose_consumer_keeps_bootstrap_off_both_agent_tracks(self):
        for track in ["mcp_only", "direct_allowed"]:
            with self.subTest(track=track), tempfile.TemporaryDirectory() as temporary:
                model = self.generated(Path(temporary), track)
                main = model["services"]["main"]
                world = model["services"]["world"]
                tools = model["services"]["tools"]
                self.assertNotIn("bootstrap", main["networks"])
                self.assertIn("bootstrap", world["networks"])
                self.assertIn("bootstrap", tools["networks"])
                self.assertTrue(model["networks"]["bootstrap"]["internal"])
                self.assertIn("world-bootstrap", world["networks"]["bootstrap"]["aliases"])
                self.assertEqual(
                    world["environment"]["BENCH_BOOTSTRAP_BIND_HOST"], "world-bootstrap",
                )
                self.assertEqual(
                    world["healthcheck"]["test"],
                    ["CMD", "curl", "-fs", "http://localhost:9001/_bench/health"],
                )
                if track == "direct_allowed":
                    self.assertIn("backend", main["networks"])
                else:
                    self.assertNotIn("backend", main["networks"])


if __name__ == "__main__":
    unittest.main()
