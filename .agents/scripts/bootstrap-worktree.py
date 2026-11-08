#!/usr/bin/env python3
"""Safely prepare a local agent worktree; default mode is a non-mutating preview."""

import argparse
import os
from pathlib import Path
import platform
import subprocess
import sys


AGENTS = {
    "atlas": "Windows",
    "nova": "Windows",
    "forge": "Windows",
    "harbor": "Darwin",
    "orion": "Darwin",
}


def git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        text=True,
        encoding="utf-8",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=check,
    )


def exists_ref(repo: Path, ref: str) -> bool:
    result = git(repo, "show-ref", "--verify", "--quiet", ref, check=False)
    if result.returncode not in (0, 1):
        raise RuntimeError("Unable to inspect Git ref: " + ref)
    return result.returncode == 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--agent", required=True, choices=sorted(AGENTS))
    parser.add_argument("--target", required=True, help="Absolute path to a new local worktree")
    parser.add_argument("--apply", action="store_true", help="Actually create a worktree; default is dry-run")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[2]
    actual = Path(git(repo, "rev-parse", "--show-toplevel").stdout.strip()).resolve()
    if actual != repo:
        raise RuntimeError("Script must run from its own OpenDash CMS checkout")
    origin = git(repo, "remote", "get-url", "origin").stdout.strip()
    if "thientran2359/opendash-cms" not in origin.lower():
        raise RuntimeError("Origin must be the expected OpenDash CMS GitHub repository")

    target = Path(args.target).expanduser()
    if not target.is_absolute():
        raise RuntimeError("Use an absolute target path to avoid ambiguous worktrees")
    target = target.resolve()
    if target.exists():
        raise RuntimeError("Target already exists: " + str(target))
    if not target.parent.is_dir():
        raise RuntimeError("Target parent must exist: " + str(target.parent))
    if not exists_ref(repo, "refs/remotes/origin/main"):
        raise RuntimeError("Missing origin/main; fetch origin first")

    branch = "agent/" + args.agent
    local = exists_ref(repo, "refs/heads/" + branch)
    remote = exists_ref(repo, "refs/remotes/origin/" + branch)

    if local:
        command = ["git", "-C", str(repo), "worktree", "add", str(target), branch]
    elif remote:
        command = [
            "git", "-C", str(repo), "worktree", "add", "--track",
            "-b", branch, str(target), "origin/" + branch,
        ]
    else:
        command = [
            "git", "-C", str(repo), "worktree", "add", "--no-track",
            "-b", branch, str(target), "origin/main",
        ]

    print("Agent:", args.agent, "| expected host:", AGENTS[args.agent])
    print("Git origin:", origin)
    print("Branch:", branch, "| local:", local, "| remote:", remote)
    print("Target:", target)
    print("Command:", subprocess.list2cmdline(command))

    if not args.apply:
        print("DRY RUN: no files, branches or remote refs were changed.")
        return 0

    if platform.system() != AGENTS[args.agent]:
        raise RuntimeError("Agent belongs to a different host OS")
    if git(repo, "status", "--porcelain=v1").stdout.strip():
        raise RuntimeError("Source checkout has uncommitted changes; finish or back them up first")
    subprocess.run(command, check=True)
    print("Created local worktree; no GitHub push performed.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (RuntimeError, subprocess.CalledProcessError, OSError) as exc:
        print("ERROR:", exc, file=sys.stderr)
        sys.exit(2)
