#!/usr/bin/env python3
"""Pre-commit safety checks for active agent artifacts. No changes are made."""

from pathlib import Path, PurePosixPath
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
STRONG_SECRETS = [
    re.compile(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    re.compile(rb"ghp_[A-Za-z0-9]{36}"),
    re.compile(rb"github_pat_[A-Za-z0-9_]{60,}"),
    re.compile(rb"sk-proj-[A-Za-z0-9_-]{20,}"),
]


def main() -> int:
    tracked = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", ".agents/"],
        check=True,
        stdout=subprocess.PIPE,
    ).stdout
    errors: list[str] = []
    for raw in tracked.split(b"\0"):
        if not raw:
            continue
        name = raw.decode("utf-8").replace("\\", "/")
        path = PurePosixPath(name)
        file = ROOT / name
        # A retired tracked file may be deleted in the working tree but not yet
        # staged; do not flag it when auditing a documentation-only migration.
        if not file.is_file():
            continue
        if name == ".agents/hooks.json":
            errors.append("Retired lifecycle hook still at active root: " + name)
        if name.startswith(".agents/skills/") and path.name == "SKILL.md":
            errors.append("Retired or unapproved active skill detected: " + name)
        lower = name.lower()
        if lower.endswith((".pem", ".p12", ".pfx", ".key", ".sqlite", ".db", ".log")):
            errors.append("Sensitive or runtime artifact path: " + name)
        if any(part in {"secrets", "credentials", "tokens", "runtime", "cache", "logs"} for part in path.parts):
            errors.append("Runtime or credential directory: " + name)
        if path.name in {"state.lock", "state.json", ".env", "credentials.json", "token.json"}:
            errors.append("Runtime or credential file: " + name)
        content = file.read_bytes()
        if any(pattern.search(content) for pattern in STRONG_SECRETS):
            errors.append("Possible secret in " + name)

    if errors:
        print("FAIL: agent artifact safety checks")
        for error in errors:
            print(" -", error)
        return 1
    print("PASS: no active retired skills/hooks or obvious agent secrets in Git candidates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
