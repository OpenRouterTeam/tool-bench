import hashlib
import shutil
import stat
import tarfile
from pathlib import Path, PurePosixPath
from typing import BinaryIO

from deploy.runner.admission import AdmissionError


MAX_BYTES = 64 * 1024 * 1024
MAX_ENTRIES = 10_000
MAX_DEPTH = 64


def extract_snapshot(archive: BinaryIO, destination: Path) -> Path:
    total = 0
    seen = set()
    root_name = None
    try:
        with tarfile.open(fileobj=archive, mode="r|") as bundle:
            for member in bundle:
                path = PurePosixPath(member.name)
                if (
                    path.is_absolute()
                    or ".." in path.parts
                    or not path.parts
                    or len(path.parts) > MAX_DEPTH
                    or path in seen
                    or len(seen) >= MAX_ENTRIES
                    or not (member.isfile() or member.isdir())
                    or member.sparse is not None
                ):
                    raise AdmissionError("unsupported or unsafe main artifact archive")
                if root_name is None:
                    root_name = path.parts[0]
                if path.parts[0] != root_name:
                    raise AdmissionError("main artifact archive has multiple roots")
                seen.add(path)
                total += member.size
                if total > MAX_BYTES:
                    raise AdmissionError("main artifact exceeds snapshot resource bounds")
                target = destination.joinpath(*path.parts[1:])
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    stream = bundle.extractfile(member)
                    if stream is None:
                        raise AdmissionError("main artifact archive has no file data")
                    with stream, target.open("xb") as output:
                        shutil.copyfileobj(stream, output)
                    if target.stat().st_size != member.size:
                        raise AdmissionError("incomplete main artifact archive")
        if not seen or not destination.exists():
            raise AdmissionError("empty main artifact archive")
        fingerprint(destination)
    except (tarfile.TarError, OSError, ValueError) as error:
        raise AdmissionError("cannot safely extract main artifact snapshot") from error
    return destination


def fingerprint(root: Path) -> str:
    digest = hashlib.sha256()
    total = 0
    entries = 0

    def visit(path: Path, relative: Path) -> None:
        nonlocal total, entries
        entries += 1
        metadata = path.lstat()
        if entries > MAX_ENTRIES or len(relative.parts) > MAX_DEPTH:
            raise AdmissionError("main artifact exceeds snapshot resource bounds")
        if not (stat.S_ISREG(metadata.st_mode) or stat.S_ISDIR(metadata.st_mode)):
            raise AdmissionError("main artifacts cannot contain links or special files")
        name = relative.as_posix().encode("utf-8")
        digest.update(len(name).to_bytes(8, "big"))
        digest.update(name)
        if stat.S_ISDIR(metadata.st_mode):
            digest.update(b"directory\0")
            for child in sorted(path.iterdir()):
                visit(child, relative / child.name)
        else:
            total += metadata.st_size
            if total > MAX_BYTES:
                raise AdmissionError("main artifact exceeds snapshot resource bounds")
            digest.update(b"file\0")
            digest.update(metadata.st_size.to_bytes(8, "big"))
            with path.open("rb") as source:
                while chunk := source.read(64 * 1024):
                    digest.update(chunk)

    try:
        visit(root, Path("."))
    except (OSError, UnicodeError) as error:
        raise AdmissionError("cannot read main artifact snapshot") from error
    return "sha256:" + digest.hexdigest()


def snapshot_tree(source: Path, destination: Path) -> str:
    before = fingerprint(source)
    shutil.copytree(source, destination, symlinks=True)
    if before != fingerprint(destination) or before != fingerprint(source):
        raise AdmissionError("mounted main artifact changed during its snapshot")
    return before
