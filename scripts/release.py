#!/usr/bin/env python3
"""Validate release identity and collect the existing three-platform asset set.

Requires Python 3.11+; uses only the standard library. Fixture files in the unit
suite exercise validation, not executable/package correctness.
"""

import argparse
import hashlib
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path


class ReleaseError(ValueError):
    """A release invariant was not satisfied."""


def version_for_tag(tag):
    if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag):
        raise ReleaseError("tag must have strict vX.Y.Z format (no leading zeros)")
    return tag[1:]


def validate_checkout(tag, expected_commit=None, repo=Path(".")):
    version = version_for_tag(tag)
    with (repo / "Cargo.toml").open("rb") as manifest:
        actual_version = tomllib.load(manifest)["workspace"]["package"]["version"]
    if actual_version != version:
        raise ReleaseError(f"tag {tag} disagrees with workspace version {actual_version!r}")

    def revision(ref):
        return subprocess.check_output(
            ["git", "rev-parse", "--verify", ref], cwd=repo, text=True
        ).strip()

    commit = revision(f"refs/tags/{tag}^{{commit}}")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ReleaseError("tag did not resolve to a GitHub commit SHA")
    if revision("HEAD^{commit}") != commit:
        raise ReleaseError("checked-out HEAD does not match the release tag")
    if expected_commit is not None:
        if not re.fullmatch(r"[0-9a-f]{40}", expected_commit):
            raise ReleaseError("expected commit must be a full SHA")
        # Peel annotated push-event tag objects as well as ordinary commit SHAs.
        if revision(f"{expected_commit}^{{commit}}") != commit:
            raise ReleaseError("release tag no longer matches the validated/event commit")
    return {"tag": tag, "version": version, "commit": commit}


def expected_assets(tag):
    version = version_for_tag(tag)
    return {
        "ptouch-linux-amd64/ptouch": "ptouch-linux-amd64",
        "ptouch-gui-linux-amd64/ptouch-gui": "ptouch-gui-linux-amd64",
        "ptouch-windows-amd64/ptouch.exe": "ptouch-windows-amd64.exe",
        "ptouch-gui-windows-amd64/ptouch-gui.exe": "ptouch-gui-windows-amd64.exe",
        "ptouch-macos-arm64/ptouch": "ptouch-macos-arm64",
        "ptouch-gui-macos-arm64/ptouch-gui": "ptouch-gui-macos-arm64",
        "ptouch-gui-macos-arm64-app/ptouch-gui-macos-arm64.app.zip":
            "ptouch-gui-macos-arm64.app.zip",
        f"ptouch-linux-packages/ptouch_{version}_amd64.deb": f"ptouch_{version}_amd64.deb",
        f"ptouch-linux-packages/ptouch-{version}.x86_64.rpm": f"ptouch-{version}.x86_64.rpm",
    }


def require_directory(path):
    if path.is_symlink() or not path.is_dir():
        raise ReleaseError(f"not a real directory: {path}")


def require_file(path):
    mode = path.lstat().st_mode
    if not stat.S_ISREG(mode) or path.stat().st_size == 0:
        raise ReleaseError(f"not a nonempty regular, nonsymlink file: {path}")


def require_tree(root, files):
    """Reject missing files, symlinks, special files and even extra directories."""
    require_directory(root)
    expected = set(files)
    directories = {str(parent) for name in files for parent in Path(name).parents
                   if str(parent) != "."}
    entries = {path.relative_to(root).as_posix(): path for path in root.rglob("*")}
    if set(entries) != expected | directories:
        raise ReleaseError(
            f"unexpected/missing asset paths: expected {sorted(expected | directories)}, "
            f"got {sorted(entries)}"
        )
    for name, path in entries.items():
        if name in directories:
            require_directory(path)
        else:
            require_file(path)


def checksum_text(root, names):
    lines = []
    for name in sorted(names):
        with (root / name).open("rb") as asset:
            digest = hashlib.file_digest(asset, "sha256").hexdigest()
        lines.append(f"{digest}  {name}\n")
    return "".join(lines)


def verify_assets(tag, output):
    names = set(expected_assets(tag).values())
    require_tree(output, names | {"SHA256SUMS"})
    if (output / "SHA256SUMS").read_bytes() != checksum_text(output, names).encode("ascii"):
        raise ReleaseError("SHA256SUMS does not exactly match the nine release assets")


def collect_assets(tag, artifacts, output):
    mapping = expected_assets(tag)
    require_tree(artifacts, mapping)
    if output.exists() or output.is_symlink():
        require_directory(output)
        if any(output.iterdir()):
            raise ReleaseError("output directory must be empty (refusing stale assets)")
    output.parent.mkdir(parents=True, exist_ok=True)
    # Assemble and verify the complete set beside the destination. A failed
    # copy/checksum/rename must leave an absent or preexisting empty output
    # unchanged, never a partially publishable set. Publication runs on Linux,
    # where replace can also atomically replace an existing empty directory.
    with tempfile.TemporaryDirectory(prefix=f".{output.name}-", dir=output.parent) as temporary:
        staged = Path(temporary)
        for source, destination in mapping.items():
            shutil.copyfile(artifacts / source, staged / destination)
        require_tree(staged, set(mapping.values()))
        (staged / "SHA256SUMS").write_text(
            checksum_text(staged, mapping.values()), encoding="ascii", newline="\n"
        )
        verify_assets(tag, staged)
        os.replace(staged, output)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate", help="check tag, Cargo version and checkout")
    validate.add_argument("--tag", required=True)
    validate.add_argument("--expected-commit")
    for command in ("collect", "verify"):
        subparser = commands.add_parser(command)
        subparser.add_argument("--tag", required=True)
        subparser.add_argument("--output", type=Path, required=True)
        if command == "collect":
            subparser.add_argument("--artifacts", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "validate":
            for key, value in validate_checkout(args.tag, args.expected_commit).items():
                print(f"{key}={value}")
        elif args.command == "collect":
            collect_assets(args.tag, args.artifacts, args.output)
            print("Verified nine assets and SHA256SUMS")
        else:
            verify_assets(args.tag, args.output)
            print("Verified nine assets and SHA256SUMS")
    except (ReleaseError, OSError, KeyError, tomllib.TOMLDecodeError,
            subprocess.CalledProcessError) as error:
        print(f"release validation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
