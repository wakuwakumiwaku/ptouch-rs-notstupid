"""Release invariants tested with synthetic data, never purported build outputs."""

import hashlib
import importlib.util
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("release_helper", ROOT / "scripts/release.py")
assert SPEC is not None and SPEC.loader is not None
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)
TAG = "v0.9.1"
SHA = "a" * 40
OTHER_SHA = "b" * 40


class TagTests(unittest.TestCase):
    def test_strict_valid_tags(self):
        for tag in (TAG, "v0.0.0", "v10.23.45"):
            self.assertEqual(release.version_for_tag(tag), tag[1:])

    def test_invalid_tags_are_refused(self):
        for tag in ("0.9.1", "v01.9.1", "v0.09.1", "v0.9.01", "v0.9.1-rc1",
                    "v0.9.1+build", "v0.9.1\n", " v0.9.1", "v0.9", "v0.9.1.2",
                    "v0.9.١", "refs/tags/v0.9.1", "v0.9.1;echo bad", "", "--help"):
            with self.subTest(tag=tag), self.assertRaises(release.ReleaseError):
                release.version_for_tag(tag)

    def manifest(self, version="0.9.1"):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        repo = Path(temporary.name)
        (repo / "Cargo.toml").write_text(
            f'[workspace.package]\nversion = "{version}"\n', encoding="utf-8"
        )
        return repo

    def test_matching_checkout_and_version(self):
        repo = self.manifest()
        with mock.patch.object(release.subprocess, "check_output", return_value=SHA + "\n") as git:
            self.assertEqual(release.validate_checkout(TAG, SHA, repo),
                             {"tag": TAG, "version": "0.9.1", "commit": SHA})
            self.assertEqual(git.call_args_list, [
                mock.call(["git", "rev-parse", "--verify", f"refs/tags/{TAG}^{{commit}}"], cwd=repo, text=True),
                mock.call(["git", "rev-parse", "--verify", "HEAD^{commit}"], cwd=repo, text=True),
                mock.call(["git", "rev-parse", "--verify", f"{SHA}^{{commit}}"], cwd=repo, text=True),
            ])

    def test_mismatched_workspace_version_refused_before_git(self):
        with mock.patch.object(release.subprocess, "check_output") as git:
            with self.assertRaisesRegex(release.ReleaseError, "workspace version"):
                release.validate_checkout(TAG, repo=self.manifest("0.9.0"))
            git.assert_not_called()

    def test_mismatched_head_refused(self):
        with mock.patch.object(release.subprocess, "check_output", side_effect=[SHA, OTHER_SHA]):
            with self.assertRaisesRegex(release.ReleaseError, "HEAD"):
                release.validate_checkout(TAG, repo=self.manifest())

    def test_moved_tag_refused(self):
        with mock.patch.object(release.subprocess, "check_output", side_effect=[SHA, SHA, OTHER_SHA]):
            with self.assertRaisesRegex(release.ReleaseError, "validated/event commit"):
                release.validate_checkout(TAG, OTHER_SHA, self.manifest())

    def test_missing_tag_refused(self):
        with mock.patch.object(release.subprocess, "check_output",
                               side_effect=subprocess.CalledProcessError(128, "git")):
            with self.assertRaises(subprocess.CalledProcessError):
                release.validate_checkout(TAG, repo=self.manifest())

    def test_short_expected_commit_refused(self):
        with mock.patch.object(release.subprocess, "check_output", return_value=SHA):
            with self.assertRaisesRegex(release.ReleaseError, "full SHA"):
                release.validate_checkout(TAG, "a123", self.manifest())

    def test_annotated_event_tag_object_is_peeled(self):
        # The event SHA may be an annotated tag object, but must peel to HEAD.
        with mock.patch.object(release.subprocess, "check_output", return_value=SHA):
            self.assertEqual(release.validate_checkout(TAG, OTHER_SHA, self.manifest())["commit"], SHA)


class AssetTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.artifacts = self.root / "artifacts"
        self.output = self.root / "release"
        self.mapping = release.expected_assets(TAG)
        for index, name in enumerate(self.mapping):
            path = self.artifacts / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"synthetic validation fixture {index}\n".encode("ascii"))

    def collect(self):
        release.collect_assets(TAG, self.artifacts, self.output)

    def test_exact_positive_set_and_independent_checksums(self):
        expected = {
            "ptouch-linux-amd64", "ptouch-gui-linux-amd64",
            "ptouch-windows-amd64.exe", "ptouch-gui-windows-amd64.exe",
            "ptouch-macos-arm64", "ptouch-gui-macos-arm64",
            "ptouch-gui-macos-arm64.app.zip", "ptouch_0.9.1_amd64.deb",
            "ptouch-0.9.1.x86_64.rpm",
        }
        self.assertEqual(set(self.mapping.values()), expected)
        self.assertEqual(len(self.mapping), 9)
        self.collect()
        self.assertEqual({p.name for p in self.output.iterdir()}, expected | {"SHA256SUMS"})
        lines = (self.output / "SHA256SUMS").read_text().splitlines()
        self.assertEqual(len(lines), 9)
        self.assertEqual([line.split("  ")[1] for line in lines], sorted(expected))
        for line in lines:
            digest, filename = line.split("  ")
            self.assertEqual(digest, hashlib.sha256((self.output / filename).read_bytes()).hexdigest())
        release.verify_assets(TAG, self.output)

    def test_missing_asset_refused_without_output(self):
        (self.artifacts / next(iter(self.mapping))).unlink()
        with self.assertRaises(release.ReleaseError):
            self.collect()
        self.assertFalse(self.output.exists())

    def test_unexpected_asset_refused(self):
        (self.artifacts / "unexpected").write_bytes(b"fixture")
        with self.assertRaises(release.ReleaseError):
            self.collect()

    def test_unexpected_empty_directory_refused(self):
        (self.artifacts / "unexpected-directory").mkdir()
        with self.assertRaises(release.ReleaseError):
            self.collect()

    def test_empty_asset_refused(self):
        (self.artifacts / next(iter(self.mapping))).write_bytes(b"")
        with self.assertRaises(release.ReleaseError):
            self.collect()

    def test_symlink_file_refused(self):
        path = self.artifacts / next(iter(self.mapping))
        target = self.root / "fixture-target"
        path.rename(target)
        path.symlink_to(target)
        with self.assertRaises(release.ReleaseError):
            self.collect()

    def test_symlink_directory_refused(self):
        path = self.artifacts / "ptouch-linux-amd64"
        target = self.root / "fixture-directory"
        path.rename(target)
        path.symlink_to(target, target_is_directory=True)
        with self.assertRaises(release.ReleaseError):
            self.collect()

    def test_wrong_version_filename_refused(self):
        path = self.artifacts / "ptouch-linux-packages/ptouch_0.9.1_amd64.deb"
        path.rename(path.with_name("ptouch_0.9.0_amd64.deb"))
        with self.assertRaises(release.ReleaseError):
            self.collect()

    def test_stale_output_refused(self):
        self.output.mkdir()
        (self.output / "stale").write_bytes(b"fixture")
        with self.assertRaisesRegex(release.ReleaseError, "empty"):
            self.collect()

    def test_empty_output_directory_allowed(self):
        self.output.mkdir()
        self.collect()
        release.verify_assets(TAG, self.output)

    def test_corrupted_asset_refused(self):
        self.collect()
        (self.output / "ptouch-linux-amd64").write_bytes(b"changed fixture")
        with self.assertRaisesRegex(release.ReleaseError, "SHA256SUMS"):
            release.verify_assets(TAG, self.output)

    def test_extra_checksum_line_refused(self):
        self.collect()
        checksums = self.output / "SHA256SUMS"
        checksums.write_bytes(checksums.read_bytes() * 2)
        with self.assertRaisesRegex(release.ReleaseError, "SHA256SUMS"):
            release.verify_assets(TAG, self.output)

    def test_extra_published_asset_refused(self):
        self.collect()
        (self.output / "leftover").write_bytes(b"fixture")
        with self.assertRaises(release.ReleaseError):
            release.verify_assets(TAG, self.output)

    def test_symlink_checksum_refused(self):
        self.collect()
        checksums = self.output / "SHA256SUMS"
        target = self.root / "fixture-checksums"
        checksums.rename(target)
        checksums.symlink_to(target)
        with self.assertRaises(release.ReleaseError):
            release.verify_assets(TAG, self.output)

    def test_empty_checksum_refused(self):
        self.collect()
        (self.output / "SHA256SUMS").write_bytes(b"")
        with self.assertRaises(release.ReleaseError):
            release.verify_assets(TAG, self.output)

    def _check_collection_failure(self, kind, position=1):
        for error_type in (OSError, KeyboardInterrupt):
            for existing in (False, True):
                with self.subTest(kind=kind, position=position,
                                  error=error_type.__name__, existing=existing):
                    with tempfile.TemporaryDirectory(dir=self.root) as temporary:
                        root = Path(temporary)
                        output = root / "release"
                        canary = root / "unrelated"
                        canary.write_bytes(b"preserve unrelated output")
                        if existing:
                            output.mkdir()
                        before = set(root.iterdir())
                        inputs = {name: (self.artifacts / name).read_bytes()
                                  for name in self.mapping}
                        reached = []
                        real_copy = shutil.copyfile
                        real_write = Path.write_text
                        copies = []

                        def copy(source, target, *args, **kwargs):
                            copies.append(source)
                            if len(copies) == position:
                                Path(target).write_bytes(b"partial asset")
                                reached.append(True)
                                raise error_type("injected partial copy")
                            return real_copy(source, target, *args, **kwargs)

                        def checksum(path, text, *args, **kwargs):
                            if path.name == "SHA256SUMS":
                                path.write_bytes(b"partial checksum")
                                reached.append(True)
                                raise error_type("injected checksum write")
                            return real_write(path, text, *args, **kwargs)

                        def publish(*args, **kwargs):
                            reached.append(True)
                            raise error_type("injected directory publication")

                        patcher = {
                            "copy": lambda: mock.patch("shutil.copyfile", side_effect=copy),
                            "checksum": lambda: mock.patch.object(Path, "write_text", checksum),
                            "publish": lambda: mock.patch("os.replace", side_effect=publish),
                        }[kind]
                        with patcher():
                            with self.assertRaises(error_type):
                                release.collect_assets(TAG, self.artifacts, output)
                        self.assertEqual(reached, [True])
                        self.assertEqual(set(root.iterdir()), before)
                        if existing:
                            self.assertEqual(list(output.iterdir()), [])
                        else:
                            self.assertFalse(output.exists())
                        self.assertEqual(canary.read_bytes(), b"preserve unrelated output")
                        self.assertEqual(
                            {name: (self.artifacts / name).read_bytes()
                             for name in self.mapping}, inputs)
                        release.collect_assets(TAG, self.artifacts, output)
                        release.verify_assets(TAG, output)

    def test_partial_copy_preserves_output_and_cleans_stage(self):
        for position in range(1, 10):
            self._check_collection_failure("copy", position)

    def test_partial_checksum_preserves_output_and_cleans_stage(self):
        self._check_collection_failure("checksum")

    def test_failed_directory_publication_preserves_output_and_cleans_stage(self):
        self._check_collection_failure("publish")

    def test_cli_collect_and_verify(self):
        command = [sys.executable, str(ROOT / "scripts/release.py")]
        for args in (
            ["collect", "--tag", TAG, "--artifacts", str(self.artifacts), "--output", str(self.output)],
            ["verify", "--tag", TAG, "--output", str(self.output)],
        ):
            result = subprocess.run(command + args, capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Verified nine assets", result.stdout)
        (self.output / "ptouch-linux-amd64").write_bytes(b"tampered fixture")
        result = subprocess.run(command + ["verify", "--tag", TAG, "--output", str(self.output)],
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn("release validation failed", result.stderr)


if __name__ == "__main__":
    unittest.main()
