import io
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from deploy.runner.admission import AdmissionError
from deploy.runner.snapshot import extract_snapshot, fingerprint, snapshot_tree


def archive(entries):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w") as bundle:
        for name, kind, content in entries:
            member = tarfile.TarInfo(name)
            member.type = kind
            if kind == tarfile.REGTYPE:
                member.size = len(content)
            else:
                member.linkname = content.decode()
            bundle.addfile(member, io.BytesIO(content) if member.isfile() else None)
    output.seek(0)
    return output


class SnapshotTests(unittest.TestCase):
    def test_files_and_dotted_directories_keep_their_actual_types(self):
        for entries, is_directory in [
            ([("answer", tarfile.REGTYPE, b"Maya\n")], False),
            (
                [
                    ("answer.md", tarfile.DIRTYPE, b""),
                    ("answer.md/note", tarfile.REGTYPE, b"Maya\n"),
                ],
                True,
            ),
        ]:
            with tempfile.TemporaryDirectory() as directory:
                snapshot = extract_snapshot(archive(entries), Path(directory) / "payload")
                self.assertEqual(snapshot.is_dir(), is_directory)
                self.assertTrue(fingerprint(snapshot).startswith("sha256:"))

    def test_paths_links_duplicates_devices_and_multiple_roots_are_rejected(self):
        for entries in [
            [("../outside", tarfile.REGTYPE, b"bad")],
            [("/absolute", tarfile.REGTYPE, b"bad")],
            [("root/../../outside", tarfile.REGTYPE, b"bad")],
            [("root", tarfile.SYMTYPE, b"/etc/passwd")],
            [("root", tarfile.LNKTYPE, b"/etc/passwd")],
            [("root", tarfile.FIFOTYPE, b"")],
            [("root", tarfile.CHRTYPE, b"")],
            [("root", tarfile.REGTYPE, b"a"), ("root", tarfile.REGTYPE, b"b")],
            [("root", tarfile.REGTYPE, b"a"), ("other", tarfile.REGTYPE, b"b")],
        ]:
            with self.subTest(entries=entries), tempfile.TemporaryDirectory() as directory:
                with self.assertRaises(AdmissionError):
                    extract_snapshot(archive(entries), Path(directory) / "payload")

    def test_resource_bounds_reject_before_oversized_extraction(self):
        entries = [("root", tarfile.REGTYPE, b"too large")]
        with tempfile.TemporaryDirectory() as directory:
            with patch("deploy.runner.snapshot.MAX_BYTES", 2):
                with self.assertRaises(AdmissionError):
                    extract_snapshot(archive(entries), Path(directory) / "payload")
            self.assertFalse((Path(directory) / "payload").exists())

    def test_empty_truncated_and_incomplete_archives_are_not_snapshots(self):
        for data in [b"", b"not a tar", archive([("root", tarfile.REGTYPE, b"x")]).read()[:512]]:
            with tempfile.TemporaryDirectory() as directory:
                with self.assertRaises(AdmissionError):
                    extract_snapshot(io.BytesIO(data), Path(directory) / "payload")

    def test_entry_and_depth_bounds_apply_to_archives_and_mounted_trees(self):
        entries = [
            ("root", tarfile.DIRTYPE, b""),
            ("root/child", tarfile.REGTYPE, b"text"),
        ]
        for setting in ["MAX_ENTRIES", "MAX_DEPTH"]:
            with self.subTest(setting=setting), tempfile.TemporaryDirectory() as directory:
                root = Path(directory) / "mounted"
                root.mkdir()
                child = root / "nested"
                child.mkdir()
                (child / "note").write_text("text")
                with patch(f"deploy.runner.snapshot.{setting}", 1):
                    with self.assertRaises(AdmissionError):
                        extract_snapshot(archive(entries), Path(directory) / "payload")
                    with self.assertRaises(AdmissionError):
                        snapshot_tree(root, Path(directory) / "protected")

    def test_mounted_snapshot_is_independent_and_preserves_empty_directories(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "mounted"
            source.mkdir()
            (source / "empty").mkdir()
            (source / "note").write_text("before")
            target = Path(directory) / "protected"
            digest = snapshot_tree(source, target)
            self.assertEqual(digest, fingerprint(target))
            self.assertTrue((target / "empty").is_dir())
            (source / "note").write_text("after")
            self.assertNotEqual(digest, fingerprint(source))
            self.assertEqual((target / "note").read_text(), "before")

    def test_mounted_symlinks_and_unreadable_types_are_not_followed(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "mounted"
            source.mkdir()
            (source / "link").symlink_to("/etc/passwd")
            with self.assertRaises(AdmissionError):
                snapshot_tree(source, Path(directory) / "protected")

    def test_fingerprints_bind_names_types_and_exact_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "root"
            root.mkdir()
            original = fingerprint(root)
            (root / "empty").mkdir()
            self.assertNotEqual(original, fingerprint(root))
            (root / "note").write_bytes(b"text\n")
            original = fingerprint(root)
            (root / "note").rename(root / "other")
            self.assertNotEqual(original, fingerprint(root))
            original = fingerprint(root)
            (root / "other").write_bytes(b"text")
            self.assertNotEqual(original, fingerprint(root))


if __name__ == "__main__":
    unittest.main()
