import json
import re
import subprocess
import sys
from pathlib import Path


def main():
    containers = set()
    projects = set()
    receipts = 0
    manifests = 0

    def session_projects(session):
        if isinstance(session, str) and session:
            stem = session.lower().removesuffix("__env")
            projects.update([stem + "__env", stem + "__verifier__trial", stem + "__verifier"])

    for argument in sys.argv[1:]:
        root = Path(argument)
        assert root.is_dir(), root
        for path in root.rglob("runner-admission.json"):
            data = json.loads(path.read_bytes())
            receipts += 1
            session_projects(data.get("session_id"))
            identifier = (data.get("main") or {}).get("container_id")
            if identifier:
                containers.add(identifier)
        for path in root.rglob("realized-manifest.json"):
            data = json.loads(path.read_bytes())
            if not data.get("images"):
                continue
            manifests += 1
            session_projects(data.get("session_id"))
            for image in data["images"].values():
                if image.get("container_id"):
                    containers.add(image["container_id"])
        for path in root.rglob("result.json"):
            data = json.loads(path.read_bytes())
            session_projects(data.get("trial_name"))
        for path in root.glob("*.log"):
            projects.update(re.findall(r"\btb-isolation-[0-9]+\b", path.read_text(errors="replace")))
    assert len(sys.argv) > 1, "provide explicit retained attempt roots"
    template = '[{{json .ID}},{{json .Names}},{{json .State}},{{json (.Label "com.docker.compose.project")}}]'
    response = subprocess.run(
        ["docker", "ps", "-a", "--no-trunc", "--format", template],
        capture_output=True, text=True, check=True,
    )
    survivors = []
    for line in response.stdout.splitlines():
        identifier, name, state, project = json.loads(line)
        if identifier in containers or project in projects:
            survivors.append({
                "container_id": identifier, "name": name, "state": state, "project": project,
            })
    print(f"owned_ids={len(containers)} explicit_projects={len(projects)} retained_receipts={receipts} retained_manifests={manifests}")
    print(f"owned_survivors={len(survivors)}")
    for survivor in survivors:
        print(json.dumps(survivor))
    assert not survivors, "owned containers remain; no shared resource has been removed"
    print("No owned containers remain. No shared resource, image, volume or registry cache was pruned.")


if __name__ == "__main__":
    main()
