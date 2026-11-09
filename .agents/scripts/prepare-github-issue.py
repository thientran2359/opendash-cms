#!/usr/bin/env python3
"""Render a readable GitHub Issue from an approved G3 task at a Git commit.

No network calls, GitHub writes, worktree changes or permission grants occur.
Muse can consume the JSON and perform duplicate checks / publishing separately.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = "thientran2359/opendash-cms"
PROJECT_URL = "https://github.com/users/thientran2359/projects/3"
# GitHub milestone IDs verified on 2026-11-09. The publishing coordinator
# MUST resolve/compare these against live GitHub before creating an Issue.
# They are not authoritative for the roadmap status, only GitHub references.
GITHUB_MILESTONES = {
    "M1": (1, "M1 — CMS Baseline"),
    "M2": (2, "M2 — Plugin Sandbox Baseline"),
    "M2.5": (3, "M2.5 — Core Foundation"),
    "M3": (4, "M3 — Content & Media"),
    "M4": (5, "M4 — Visual Builder"),
    "M5": (6, "M5 — Theme & Templates"),
    "M6": (7, "M6 — Plugin Extensions"),
    "M7": (8, "M7 — Site Features"),
    "M8": (9, "M8 — Operations & Release"),
}
TASK_ID = re.compile(r"M\d+(?:\.\d+)?-S\d{2}-T\d{2}\Z")
HEADING = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
TASK_HEADING = re.compile(
    r"^\[?(M\d+(?:\.\d+)?-S\d{2}-T\d{2})\]?\s*[-–—:]\s*(.+?)\s*$"
)
METADATA = re.compile(r"^\s*(?:[-*]\s+)?\*\*([^*]+):\*\*\s*(.*?)\s*$")
CHECKBOX = re.compile(r"(?m)^\s*[-*]\s+\[ \]\s+\S")
CHECKED = re.compile(r"(?mi)^\s*[-*]\s+\[[xX]\]\s+")
PLACEHOLDER = re.compile(r"\.\.\.|…|\bTODO\b|\bTBD\b|<[^>]+>", re.I)

SECTIONS = (
    "Tổng quan và giá trị",
    "Mục tiêu và hành vi quan sát được",
    "Phạm vi công việc",
    "Ngoài phạm vi",
    "Phụ thuộc và điều kiện chặn",
    "Kỹ thuật và file ownership",
    "Nghiệm thu",
    "Kiểm thử, rủi ro và rollback",
)


class TaskError(ValueError):
    """Invalid or unapproved task; safe to show to coordinator."""


def source_path(raw: str) -> str:
    path = PurePosixPath(raw.replace("\\", "/"))
    if (
        path.is_absolute()
        or ".." in path.parts
        or "." in path.parts
        or path.suffix.lower() != ".md"
        or path.parts[:2] != ("docs", "plans")
        or len(path.parts) < 4
        or path.parts[2] not in {"modules", "milestones", "tasks"}
    ):
        raise TaskError("Nguồn task phải là Markdown trong docs/plans/modules|milestones|tasks, không chứa traversal")
    return path.as_posix()


def git_output(*args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode:
        raise TaskError(
            "Không đọc được Git revision/blob: "
            + result.stderr.decode("utf-8", errors="replace").strip()[:300]
        )
    return result.stdout.decode("utf-8-sig")


def resolve_revision(ref: str) -> str:
    sha = git_output("rev-parse", "--verify", f"{ref}^{{commit}}").strip()
    if not re.fullmatch(r"[0-9a-f]{40,64}", sha):
        raise TaskError("Git ref không phân giải được thành một commit hash")
    return sha


def read_task_source(path: str, ref: str) -> tuple[str, str]:
    sha = resolve_revision(ref)
    return git_output("show", f"{sha}:{source_path(path)}"), sha


def parse_task(document: str, expected_id: str) -> dict[str, object]:
    if not TASK_ID.fullmatch(expected_id):
        raise TaskError("Task ID không đúng định dạng M*-Sxx-Txx")
    lines = document.splitlines()
    matches = []
    for index, line in enumerate(lines):
        heading = HEADING.match(line)
        if not heading:
            continue
        task = TASK_HEADING.match(heading.group(2))
        if task and task.group(1) == expected_id:
            matches.append((index, len(heading.group(1)), task.group(2)))
    if len(matches) != 1:
        raise TaskError(
            f"Phải có đúng một heading task {expected_id} (không lấy ID từ bảng ứng viên G3); thấy {len(matches)}"
        )
    start, level, title = matches[0]
    if PLACEHOLDER.search(title):
        raise TaskError("Tiêu đề task còn placeholder")
    stop = len(lines)
    for idx in range(start + 1, len(lines)):
        heading = HEADING.match(lines[idx])
        if heading and len(heading.group(1)) <= level:
            stop = idx
            break

    metadata: dict[str, str] = {}
    sections: dict[str, str] = {}
    active_section: str | None = None
    pending: list[str] = []

    def save_section() -> None:
        if active_section is not None:
            if active_section in sections:
                raise TaskError(f"Heading section bị lặp: {active_section}")
            sections[active_section] = "\n".join(pending).strip()

    for line in lines[start + 1 : stop]:
        heading = HEADING.match(line)
        if heading and len(heading.group(1)) > level:
            save_section()
            active_section = heading.group(2).strip()
            pending = []
            continue
        if active_section is None:
            match = METADATA.match(line)
            if match:
                key = match.group(1).strip().casefold()
                if key in metadata:
                    raise TaskError(f"Metadata trùng: {key}")
                metadata[key] = match.group(2).strip()
        else:
            pending.append(line)
    save_section()

    for field in ("g3", "g2", "module", "capability", "track"):
        if not metadata.get(field) or PLACEHOLDER.search(metadata[field]):
            raise TaskError(f"Thiếu metadata {field} rõ ràng cho {expected_id}")
    if metadata["g3"] != "READY":
        raise TaskError(f"Task {expected_id} chưa có G3 READY trong tài liệu nguồn")
    if metadata["track"] not in {"Full-Track", "Fast-Track"}:
        raise TaskError("Track phải là Full-Track hoặc Fast-Track")
    if metadata["track"] == "Full-Track" and metadata["g2"] != "AGREED":
        raise TaskError("Full-Track chỉ phát hành khi G2 AGREED")
    if metadata["track"] == "Fast-Track" and metadata["g2"] not in {"AGREED", "NOT_APPLICABLE"}:
        raise TaskError("Fast-Track cần G2 AGREED hoặc NOT_APPLICABLE có giải thích")
    if metadata["g2"] == "NOT_APPLICABLE" and (
        not metadata.get("lý do fast-track") or PLACEHOLDER.search(metadata["lý do fast-track"])
    ):
        raise TaskError("Fast-Track G2 NOT_APPLICABLE phải ghi Lý do Fast-Track")
    if not re.fullmatch(r"[A-Z]{3}-\d{2}", metadata["module"]):
        raise TaskError("Module ID không hợp lệ")
    if not re.fullmatch(re.escape(metadata["module"]) + r"\.\d+", metadata["capability"]):
        raise TaskError("Capability không thuộc module đã khai báo")

    for heading in SECTIONS:
        value = sections.get(heading, "")
        if not value or PLACEHOLDER.search(value):
            raise TaskError(f"Thiếu hoặc chưa viết xong section: {heading}")
    if CHECKED.search(sections["Nghiệm thu"]):
        raise TaskError("Không đánh dấu nghiệm thu đã xong tại lúc tạo Issue")
    if len(CHECKBOX.findall(sections["Nghiệm thu"])) < 2:
        raise TaskError("Nghiệm thu phải có ít nhất hai checklist chưa hoàn thành")

    return {"id": expected_id, "title": title, "metadata": metadata, "sections": sections}


def issue_payload(
    task: dict[str, object], source: str, source_sha: str, baseline_sha: str, agent: str | None
) -> dict[str, object]:
    task_id = str(task["id"])
    metadata = task["metadata"]
    sections = task["sections"]
    assert isinstance(metadata, dict) and isinstance(sections, dict)
    milestone, slice_number, _ = task_id.split("-")
    if milestone not in GITHUB_MILESTONES:
        raise TaskError(
            f"Milestone {milestone} chưa có ánh xạ GitHub được xác minh; cần review roadmap"
        )
    github_milestone, milestone_title = GITHUB_MILESTONES[milestone]
    url = f"https://github.com/{REPOSITORY}/blob/{source_sha}/{source}"
    display_names = {
        "Tổng quan và giá trị": "Tổng quan — vì sao cần làm",
        "Mục tiêu và hành vi quan sát được": "Mục tiêu và kết quả mong đợi",
        "Phạm vi công việc": "Phạm vi thực hiện",
        "Ngoài phạm vi": "Ngoài phạm vi",
        "Phụ thuộc và điều kiện chặn": "Dependencies và điều kiện chặn",
        "Kỹ thuật và file ownership": "Kỹ thuật, contract và file ownership",
        "Nghiệm thu": "Tiêu chí nghiệm thu",
        "Kiểm thử, rủi ro và rollback": "Kiểm thử, rủi ro và rollback",
    }
    body = [
        f"> **{task_id}** · {metadata['module']} / {metadata['capability']} · {metadata['track']}",
        "",
    ]
    for key in SECTIONS:
        body.extend([f"## {display_names[key]}", "", str(sections[key]), ""])
    for optional, label in (
        ("Cách triển khai theo contract", "Hướng triển khai theo contract"),
        ("Bàn giao và review", "Bàn giao và review"),
    ):
        if sections.get(optional) and not PLACEHOLDER.search(str(sections[optional])):
            body.extend([f"## {label}", "", str(sections[optional]), ""])
    body.extend(
        [
            "## Nguồn chính thức và phân công",
            "",
            f"- **Task ID:** `{task_id}`",
            f"- **Milestone / Slice:** `{milestone}` / `{milestone}-{slice_number}`",
            f"- **Module / Capability:** `{metadata['module']}` / `{metadata['capability']}`",
            f"- **Track / Gate:** `{metadata['track']}`; G2 `{metadata['g2']}`; G3 `READY`",
            f"- **Agent thực hiện:** `{agent}`" if agent else "- **Agent thực hiện:** Chưa phân công",
            f"- **Đặc tả đã duyệt:** [{source}@{source_sha[:12]}]({url})",
            f"- **Source commit SHA:** `{source_sha}`",
            f"- **Baseline code SHA:** `{baseline_sha}`",
            "",
            "## Bàn giao và tiến độ",
            "",
            "Cập nhật trạng thái/assignee tại GitHub Issue/Project; đính PR, commit, "
            "kết quả test (PASS/FAIL/NOT_RUN), blocker và đường dẫn verification khi có. "
            "`MERGED` không đồng nghĩa `VERIFIED`. Nếu thay scope/acceptance, "
            "sửa đặc tả trong repo qua review trước rồi đồng bộ Issue.",
        ]
    )
    # GitHub Milestone tracks the roadmap; GitHub Project Status tracks execution.
    # Do not duplicate either one into status:* or milestone:* labels.
    labels = ["type:implementation", f"module:{metadata['module']}"]
    if agent:
        labels.append(f"agent:{agent}")
    return {
        "repository_full_name": REPOSITORY,
        "title": f"[{task_id}] {task['title']}",
        "body": "\n".join(body) + "\n",
        "labels": labels,
        "milestone": github_milestone,
        "milestone_title": milestone_title,
        "project_url": PROJECT_URL,
        "task_id": task_id,
        "source_path": source,
        "source_sha": source_sha,
        "base_sha": baseline_sha,
        "agent": agent,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, help="Đường dẫn repo-relative đến module brief/task brief")
    parser.add_argument("--task-id", required=True, help="Task ID đã qua G3, ví dụ M3-S07-T01")
    parser.add_argument("--ref", default="origin/main", help="Commit đã duyệt chứa đặc tả (mặc định origin/main)")
    parser.add_argument("--base-ref", help="Commit source code nền để agent thực hiện (mặc định --ref)")
    parser.add_argument("--agent", choices=["atlas", "nova", "forge", "harbor", "orion"])
    parser.add_argument("--format", choices=["json", "markdown"], default="json")
    args = parser.parse_args(argv)
    try:
        path = source_path(args.source)
        document, source_sha = read_task_source(path, args.ref)
        task = parse_task(document, args.task_id)
        base_sha = resolve_revision(args.base_ref) if args.base_ref else source_sha
        payload = issue_payload(task, path, source_sha, base_sha, args.agent)
    except TaskError as exc:
        print(f"BLOCKED: {exc}", file=sys.stderr)
        return 2
    if args.format == "markdown":
        print(payload["title"])
        print()
        print(payload["body"])
    else:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
