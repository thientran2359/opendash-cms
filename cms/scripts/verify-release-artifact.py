#!/usr/bin/env python3
"""Verify Linux release bundle manifest, checksums, secret hygiene, and native ELF static linking."""

import argparse
import hashlib
import json
import pathlib
import platform
import subprocess
import sys
import tempfile

FORBIDDEN_NAME_PARTS = (".key", ".pem", "age-secret", ".odbackup.age", ".sqlite", "secrets")


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def verify_bundle(bundle_dir: pathlib.Path, check_native_linux: bool = True) -> None:
    manifest_path = bundle_dir / "release-manifest.json"
    checksums_path = bundle_dir / "checksums.txt"
    binary_path = bundle_dir / "opendash"

    if not manifest_path.is_file():
        raise AssertionError("Missing release-manifest.json")
    if not checksums_path.is_file():
        raise AssertionError("Missing checksums.txt")
    if not binary_path.is_file():
        raise AssertionError("Missing opendash executable")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("format_version") != 1:
        raise AssertionError(f"Unexpected format_version: {manifest.get('format_version')}")
    if manifest.get("target") != "x86_64-unknown-linux-musl":
        raise AssertionError(f"Unexpected target: {manifest.get('target')}")
    for req_key in ("source_sha", "rust_version", "node_version", "binary_sha256", "admin_dist_sha256", "migration_checksums"):
        if not manifest.get(req_key):
            raise AssertionError(f"Missing or empty manifest field: {req_key}")

    actual_bin_sha = sha256_file(binary_path)
    if manifest["binary_sha256"] != actual_bin_sha:
        raise AssertionError("binary_sha256 mismatch against opendash binary")

    # Secret and runtime artifact hygiene
    for item in bundle_dir.rglob("*"):
        if item.is_file():
            rel = item.relative_to(bundle_dir).as_posix().lower()
            for bad in FORBIDDEN_NAME_PARTS:
                if bad in rel:
                    raise AssertionError(f"Forbidden secret/runtime file in release bundle: {rel}")

    # Verify checksums.txt
    seen_files = set()
    for line in checksums_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        digest, rel = line.split("  ", 1)
        target_file = bundle_dir / rel
        if not target_file.is_file():
            raise AssertionError(f"Checksum target missing: {rel}")
        if sha256_file(target_file) != digest:
            raise AssertionError(f"Checksum mismatch for {rel}")
        seen_files.add(rel)

    if "opendash" not in seen_files or "release-manifest.json" not in seen_files:
        raise AssertionError("checksums.txt missing required core entries")

    if check_native_linux and platform.system() == "Linux":
        file_out = subprocess.check_output(["file", str(binary_path)], text=True)
        if "ELF 64-bit" not in file_out or (
            "statically linked" not in file_out and "static-pie linked" not in file_out
        ):
            raise AssertionError(f"Expected static ELF64 binary, got: {file_out.strip()}")

        ldd_proc = subprocess.run(["ldd", str(binary_path)], capture_output=True, text=True)
        combined = (ldd_proc.stdout + ldd_proc.stderr).lower()
        if "not a dynamic executable" not in combined and "statically linked" not in combined:
            raise AssertionError(f"Unexpected dynamic library dependencies: {combined.strip()}")

        with tempfile.TemporaryDirectory() as tmp:
            env = {"OPENDASH_SITE_ROOT": tmp, "PATH": "/usr/bin:/bin"}
            chk = subprocess.run([str(binary_path), "--check-config"], env=env, capture_output=True, text=True)
            if chk.returncode != 0:
                raise AssertionError(f"--check-config failed: {chk.stderr}")


def run_self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        b = pathlib.Path(tmp)
        (b / "opendash").write_bytes(b"fake-static-binary")
        bin_sha = sha256_file(b / "opendash")
        manifest = {
            "format_version": 1,
            "target": "x86_64-unknown-linux-musl",
            "source_sha": "deadbeef",
            "rust_version": "rustc 1.99.1",
            "node_version": "v22.16.0",
            "binary_sha256": bin_sha,
            "admin_dist_sha256": "abcd",
            "migration_checksums": {"0001.sql": "1234"},
        }
        (b / "release-manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        (b / "checksums.txt").write_text(
            f"{bin_sha}  opendash\n{sha256_file(b / 'release-manifest.json')}  release-manifest.json\n",
            encoding="utf-8",
        )
        verify_bundle(b, check_native_linux=False)

        # Negative test: forbidden secret file
        secret = b / "test.key"
        secret.write_text("secret", encoding="utf-8")
        try:
            verify_bundle(b, check_native_linux=False)
            raise RuntimeError("Expected failure on forbidden secret file")
        except AssertionError:
            pass
    print("PASS verify-release-artifact --self-test")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle_dir", nargs="?")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        run_self_test()
        return 0
    if not args.bundle_dir:
        parser.error("bundle_dir is required unless --self-test is passed")
    verify_bundle(pathlib.Path(args.bundle_dir))
    print(f"PASS verify-release-artifact: {args.bundle_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
