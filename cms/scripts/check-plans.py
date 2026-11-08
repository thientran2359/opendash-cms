#!/usr/bin/env python3
"""Validate OpenDash planning Markdown without duplicating its source of truth.

Python 3.11+ standard library only. Run from any directory:
    python cms/scripts/check-plans.py
    python cms/scripts/check-plans.py --self-test

This is a structural consistency check, not a substitute for G2/design review.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys
import tempfile
from urllib.parse import unquote, urlsplit


MODULE_ROW = re.compile(r"^\|\s*\*\*([A-Z]{3,5}-\d{2})\*\*\s*\|")
MODULE_REF = re.compile(
    r"(?<![\w])([A-Z]{3,5})-(\d{2})"
    r"(?:(?:[–-]|\.\.)(\d{2})|((?:/\d{2})+))?(?![\w])"
)
MARKDOWN_LINK = re.compile(r"\]\(([^)]+)\)")
MODULE_COUNT_CLAIM = re.compile(r"(?<![\d–-])(\d+)\s+modules?\b", re.IGNORECASE)
STATUS_BOLD = re.compile(r"^\*\*([^*]+)\*\*")
MILESTONE = re.compile(r"^\*\*M\d+(?:\.\d+)?\*\*")
FENCE = re.compile(r"^\s*(?:`{3,}|~{3,})")


def content_lines(file: Path) -> list[tuple[int, str]]:
    """Ignore fenced code examples, which are not active Markdown references."""
    lines: list[tuple[int, str]] = []
    in_fence = False
    for number, line in enumerate(file.read_text(encoding="utf-8-sig").splitlines(), 1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if not in_fence:
            lines.append((number, line))
    return lines


def table_cells(line: str) -> list[str]:
    if not line.startswith("|"):
        return []
    # Split only on unescaped pipe characters (pipe inside a code span should be escaped).
    return [cell.replace(r"\|", "|").strip() for cell in re.split(r"(?<!\\)\|", line.strip().strip("|"))]


def links_in(text: str) -> list[str]:
    return [match.group(1).strip().split(" ", 1)[0].strip("<>") for match in MARKDOWN_LINK.finditer(text)]


def is_verification_link(file: Path, row: str, root: Path) -> bool:
    verified_dir = (root / "docs" / "verification").resolve()
    for link in links_in(row):
        target_path = unquote(urlsplit(link).path)
        if not target_path:
            continue
        target = (file.parent / target_path).resolve()
        if target.parent == verified_dir and target.is_file() and target.suffix == ".md":
            return True
    return False


def claims_verified(cell: str) -> bool:
    """Only the bold status label counts, not 'verified' in a Partial explanation."""
    match = STATUS_BOLD.match(cell)
    return bool(match and "verified" in match.group(1).lower())


def referenced_module_ids(text: str) -> set[str]:
    ids: set[str] = set()
    for match in MODULE_REF.finditer(text):
        domain, start_text, end_text, slash_endings = match.groups()
        start = int(start_text)
        ids.add(f"{domain}-{start:02d}")
        if end_text is not None:
            end = int(end_text)
            if start <= end <= 99:
                ids.update(f"{domain}-{number:02d}" for number in range(start, end + 1))
            else:
                # The caller separately flags inverted ranges.
                ids.add(f"{domain}-{end:02d}")
        if slash_endings:
            ids.update(f"{domain}-{int(n):02d}" for n in slash_endings.split("/") if n)
    return ids


def check(root: Path) -> list[str]:
    root = root.resolve()
    docs = root / "docs"
    plans = docs / "plans"
    catalog = plans / "MODULE_CATALOG_V1.md"
    roadmap = plans / "ROADMAP_V1.md"
    errors: list[str] = []
    if not catalog.is_file() or not roadmap.is_file():
        return ["Missing docs/plans/MODULE_CATALOG_V1.md or ROADMAP_V1.md"]

    catalog_lines = content_lines(catalog)
    catalog_rows: list[tuple[int, str, str]] = []
    for number, line in catalog_lines:
        match = MODULE_ROW.match(line)
        if match:
            catalog_rows.append((number, match.group(1), line))
    if not catalog_rows:
        errors.append("No module table rows found in MODULE_CATALOG_V1.md")
    seen: set[str] = set()
    for number, module_id, _ in catalog_rows:
        if module_id in seen:
            errors.append(f"{catalog.relative_to(root)}:{number}: duplicate module ID {module_id}")
        seen.add(module_id)

    # Check official docs, active agent guidance and repository entry points.
    # Retired skills and legacy handoffs are intentionally excluded: those are
    # historical snapshots with stale links, not current operational guidance.
    agent_dir = root / ".agents"
    agent_files = (
        [file for file in agent_dir.rglob("*.md") if "archive" not in file.relative_to(agent_dir).parts]
        if agent_dir.is_dir()
        else []
    )
    files = sorted(docs.rglob("*.md")) + sorted(agent_files)
    files += [
        file for file in (root / "README.md", root / "AGENTS.md") if file.is_file()
    ]
    for file in files:
        for number, line in content_lines(file):
            for link in links_in(line):
                parsed = urlsplit(link)
                if parsed.scheme or parsed.netloc or not parsed.path:
                    continue
                if parsed.path.startswith("/"):
                    errors.append(f"{file.relative_to(root)}:{number}: absolute Markdown link {link}")
                    continue
                target = (file.parent / unquote(parsed.path)).resolve()
                if not target.exists():
                    errors.append(f"{file.relative_to(root)}:{number}: broken Markdown link {link}")

    # Check all module references in the canonical catalog, roadmap and opened briefs.
    reference_files = [catalog, roadmap, *sorted((plans / "modules").glob("*.md"))]
    for file in reference_files:
        for number, line in content_lines(file):
            for match in MODULE_REF.finditer(line):
                if match.group(3) is not None and int(match.group(2)) > int(match.group(3)):
                    errors.append(
                        f"{file.relative_to(root)}:{number}: inverted module range {match.group(0)}"
                    )
            for module_id in sorted(referenced_module_ids(line)):
                if module_id not in seen:
                    errors.append(f"{file.relative_to(root)}:{number}: unknown module ID {module_id}")

    # Human-readable count claims must agree with the table (never hardcode 27).
    for file in [root / "README.md", plans / "README.md", catalog, roadmap, plans / "PLANNING_WORKFLOW.md"]:
        if not file.is_file():
            continue
        for number, line in content_lines(file):
            for match in MODULE_COUNT_CLAIM.finditer(line):
                claimed = int(match.group(1))
                if claimed != len(seen):
                    errors.append(
                        f"{file.relative_to(root)}:{number}: module count claim {claimed} "
                        f"does not match {len(seen)} catalog IDs"
                    )

    # Check evidence in the specific table row, not in an unrelated paragraph.
    for number, module_id, line in catalog_rows:
        cells = table_cells(line)
        if len(cells) >= 4 and claims_verified(cells[3]):
            if not is_verification_link(catalog, cells[3], root):
                errors.append(
                    f"{catalog.relative_to(root)}:{number}: {module_id} verified status "
                    "missing direct docs/verification/*.md evidence link"
                )
    for number, line in content_lines(roadmap):
        cells = table_cells(line)
        if len(cells) >= 4 and MILESTONE.match(cells[0]) and claims_verified(cells[3]):
            if not is_verification_link(roadmap, cells[3], root):
                errors.append(
                    f"{roadmap.relative_to(root)}:{number}: {cells[0]} verified status "
                    "missing direct docs/verification/*.md evidence link"
                )

    # The current M3 status register prevents a new agent from relying on
    # obsolete G1/G2 planning claims as the present implementation state.
    m3_status = plans / "M3_STATUS_2026-11-09.md"
    if m3_status.is_file():
        for file in (root / "README.md", plans / "README.md", roadmap, catalog):
            if file.is_file() and m3_status.name not in file.read_text(encoding="utf-8-sig"):
                errors.append(f"{file.relative_to(root)}: missing current M3 status reference")

    return errors


def fixture(root: Path) -> None:
    plans = root / "docs" / "plans"
    (plans / "modules").mkdir(parents=True, exist_ok=True)
    verification = root / "docs" / "verification"
    verification.mkdir(parents=True, exist_ok=True)
    (verification / "test.md").write_text("# Real test evidence\n", encoding="utf-8")
    (plans / "MODULE_CATALOG_V1.md").write_text(
        "# Catalog\n"
        "| ID | Scope | Dependencies | Status | Research |\n"
        "| --- | --- | --- | --- | --- |\n"
        "| **CNT-01** | CPT | FND-01 | **Baseline verified** [test](../verification/test.md) | — |\n"
        "| **FND-01** | Kernel | none | **Planned** | — |\n",
        encoding="utf-8",
    )
    (plans / "ROADMAP_V1.md").write_text(
        "| Milestone | Scope | Module | Status |\n"
        "| --- | --- | --- | --- |\n"
        "| **M1** | baseline | CNT-01 | **Baseline verified** [test](../verification/test.md) |\n",
        encoding="utf-8",
    )
    (plans / "modules" / "CNT-01-content.md").write_text(
        "# CNT-01\nUses FND-01 and M1-S01\n",
        encoding="utf-8",
    )
    (plans / "README.md").write_text("The project has 2 module IDs.\n", encoding="utf-8")
    (root / "README.md").write_text(
        "[Roadmap](docs/plans/ROADMAP_V1.md)\n", encoding="utf-8"
    )


def self_test() -> None:
    cases = [
        ("valid", None, None),
        ("duplicate", "duplicate module ID", "catalog"),
        ("unknown", "unknown module ID", "roadmap"),
        ("range", "inverted module range", "inverted_range"),
        ("count", "module count claim", "plans_readme"),
        ("link", "broken Markdown link", "readme"),
        ("evidence", "missing direct docs/verification", "catalog"),
        ("roadmap_evidence", "missing direct docs/verification", "roadmap_evidence"),
    ]
    for name, expected, location in cases:
        with tempfile.TemporaryDirectory(prefix="opendash-check-plans-") as tmp:
            root = Path(tmp)
            fixture(root)
            catalog = root / "docs/plans/MODULE_CATALOG_V1.md"
            roadmap = root / "docs/plans/ROADMAP_V1.md"
            if location == "catalog" and name == "duplicate":
                with catalog.open("a", encoding="utf-8") as out:
                    out.write("| **CNT-01** | duplicate | — | Planned | — |\n")
            elif location == "roadmap":
                with roadmap.open("a", encoding="utf-8") as out:
                    out.write("Unknown dependency CNT-99\n")
            elif location == "inverted_range":
                with roadmap.open("a", encoding="utf-8") as out:
                    out.write("Inverted dependency CNT-05–01\n")
            elif location == "plans_readme":
                (root / "docs/plans/README.md").write_text("There are 25 modules.\n", encoding="utf-8")
            elif location == "readme":
                (root / "README.md").write_text("[Missing](docs/no-such-file.md)\n", encoding="utf-8")
            elif location == "catalog" and name == "evidence":
                catalog.write_text(
                    catalog.read_text(encoding="utf-8").replace(
                        " [test](../verification/test.md)", ""
                    ),
                    encoding="utf-8",
                )
            elif location == "roadmap_evidence":
                roadmap.write_text(
                    roadmap.read_text(encoding="utf-8").replace(
                        " [test](../verification/test.md)", ""
                    ),
                    encoding="utf-8",
                )
            errors = check(root)
            if expected is None and errors:
                raise AssertionError(f"{name}: expected clean fixture, got {errors}")
            if expected is not None and not any(expected in issue for issue in errors):
                raise AssertionError(f"{name}: expected {expected!r}, got {errors}")
    print(f"PASS check-plans --self-test: {len(cases)} fixtures (valid + {len(cases) - 1} failures)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    problems = check(args.root)
    if problems:
        for problem in problems:
            print(f"FAIL {problem}", file=sys.stderr)
        print(f"FAIL check-plans: {len(problems)} issue(s)", file=sys.stderr)
        return 1
    print("PASS check-plans: markdown links, module IDs/counts, verified evidence")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
