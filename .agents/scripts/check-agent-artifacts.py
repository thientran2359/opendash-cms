#!/usr/bin/env python3
"""Pre-commit safety checks for active agent artifacts. No changes are made."""

from pathlib import Path, PurePosixPath
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
# Only these manually invoked, owner-requested project skills are active.
# Any other SKILL.md under the active directory must still fail governance.
ACTIVE_SKILLS = {
    ".agents/skills/github-task-pickup/SKILL.md": "github-task-pickup",
    ".agents/skills/discussion-turn-responder/SKILL.md": "discussion-turn-responder",
    ".agents/skills/github-pr-review-merge/SKILL.md": "github-pr-review-merge",
}
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
        content = file.read_bytes()
        if name.startswith(".agents/skills/") and path.name.lower() == "skill.md":
            approved_name = ACTIVE_SKILLS.get(name)
            if approved_name is None:
                errors.append("Unapproved active skill detected: " + name)
            else:
                header = re.match(rb"\A---\r?\n(.*?)\r?\n---\r?\n", content, flags=re.DOTALL)
                metadata = header.group(1) if header else b""
                name_line = rb"(?m)^name:\s*" + re.escape(approved_name.encode()) + rb"\s*$"
                if not header or not re.search(name_line, metadata):
                    errors.append("Missing/mismatched skill frontmatter name: " + name)
                if not re.search(rb"(?m)^description:\s*\S.{20,}\s*$", metadata):
                    errors.append("Missing skill frontmatter description: " + name)
        lower = name.lower()
        if lower.endswith((".pem", ".p12", ".pfx", ".key", ".sqlite", ".db", ".log")):
            errors.append("Sensitive or runtime artifact path: " + name)
        if any(part in {"secrets", "credentials", "tokens", "runtime", "cache", "logs"} for part in path.parts):
            errors.append("Runtime or credential directory: " + name)
        if path.name in {"state.lock", "state.json", ".env", "credentials.json", "token.json"}:
            errors.append("Runtime or credential file: " + name)
        if any(pattern.search(content) for pattern in STRONG_SECRETS):
            errors.append("Possible secret in " + name)

    if errors:
        print("FAIL: agent artifact safety checks")
        for error in errors:
            print(" -", error)
        return 1
    print("PASS: only approved manual skills active; no retired hooks or obvious agent secrets in Git candidates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
