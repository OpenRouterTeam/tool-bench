import argparse
import gzip
import hashlib
import io
import json
import os
import stat
import subprocess
import tarfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SELECTION = [
    ".dockerignore", ".gitignore", ".github", "Cargo.toml", "Cargo.lock",
    "README.md", "crates", "catalog", "servers", "images", "scenarios",
    "deploy", "docs", "tasks",
]
SKIP = {".git", "target", "node_modules", "__pycache__"}


def digest(content):
    return "sha256:" + hashlib.sha256(content).hexdigest()


def command(*arguments):
    return subprocess.run(
        arguments, cwd=ROOT, env={**os.environ, "GIT_OPTIONAL_LOCKS": "0"},
        capture_output=True, text=True, check=True,
    ).stdout.strip()


def entries(path):
    yield path
    if path.is_dir() and not path.is_symlink():
        for child in sorted(path.iterdir()):
            if child.name not in SKIP:
                yield from entries(child)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--evaluator", type=int, required=True)
    parser.add_argument("--world-id", required=True)
    parser.add_argument("--tools-id", required=True)
    parser.add_argument("output", type=Path)
    options = parser.parse_args()
    output = options.output.resolve()
    assert output.is_dir() and not any(output.iterdir()), "retention directory must exist and be empty"
    assert not output.is_relative_to(ROOT), "retention must be outside the captured repository"
    os.chmod(output, 0o700)
    os.umask(0o077)
    archive = output / "source.tar.gz"
    manifest = []
    with archive.open("wb") as destination:
        with gzip.GzipFile(filename="", mode="wb", fileobj=destination, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as bundle:
                paths = sorted(path for selected in SELECTION for path in entries(ROOT / selected))
                for path in paths:
                    relative = path.relative_to(ROOT).as_posix()
                    observed = path.lstat()
                    header = tarfile.TarInfo(relative)
                    header.mode = stat.S_IMODE(observed.st_mode)
                    header.mtime = 0
                    record = {"path": relative, "mode": header.mode}
                    if path.is_symlink():
                        header.type = tarfile.SYMTYPE
                        header.linkname = os.readlink(path)
                        record.update(kind="symlink", target=header.linkname)
                        bundle.addfile(header)
                    elif path.is_dir():
                        header.type = tarfile.DIRTYPE
                        record.update(kind="directory")
                        bundle.addfile(header)
                    else:
                        assert stat.S_ISREG(observed.st_mode), path
                        content = path.read_bytes()
                        header.size = len(content)
                        record.update(kind="file", size=len(content), sha256=digest(content))
                        bundle.addfile(header, io.BytesIO(content))
                    manifest.append(record)
    expected = {record["path"]: record for record in manifest}
    with tarfile.open(archive, "r:gz") as bundle:
        assert len(bundle.getmembers()) == len(expected)
        for member in bundle.getmembers():
            assert not member.name.startswith("/") and ".." not in Path(member.name).parts
            record = expected[member.name]
            assert member.mode == record["mode"]
            if record["kind"] == "file":
                assert member.isfile() and member.size == record["size"]
                assert digest(bundle.extractfile(member).read()) == record["sha256"]
                assert digest((ROOT / member.name).read_bytes()) == record["sha256"]
            elif record["kind"] == "symlink":
                assert member.issym() and member.linkname == record["target"]
            else:
                assert member.isdir()
    metadata = {
        "schema_version": 1,
        "scope": "private_local_candidate_source_not_a_hermetic_or_public_release",
        "evaluator_version": options.evaluator,
        "world_image_id": options.world_id,
        "tools_image_id": options.tools_id,
        "build_provenance_limit": (
            "Local source retention is not a hermetic build attestation. "
            "Later validation drivers and documentation must be identified separately."
        ),
        "archive_sha256": digest(archive.read_bytes()),
        "capture_script_sha256": digest(Path(__file__).read_bytes()),
        "git_head": command("git", "rev-parse", "HEAD"),
        "git_status": command("git", "status", "--porcelain=v1"),
        "rustc": command("rustc", "-Vv"),
        "cargo": command("cargo", "-V"),
        "platform": command("uname", "-sm"),
        "selection": SELECTION,
        "excluded_directory_names": sorted(SKIP),
        "entries": manifest,
    }
    (output / "manifest.json").write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n")
    (output / "capture-source.py").write_bytes(Path(__file__).read_bytes())
    names = ["source.tar.gz", "manifest.json", "capture-source.py"]
    (output / "SHA256SUMS").write_text("".join(
        f"{hashlib.sha256((output / name).read_bytes()).hexdigest()}  {name}\n"
        for name in names
    ))
    print(f"{len(manifest)} source entries archived and independently re-read without extraction")
    print(f"archive_sha256={metadata['archive_sha256']}")
    print("No Git index, branch, commit, external resource or retained trial evidence was modified.")


if __name__ == "__main__":
    main()
