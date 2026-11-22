#!/usr/bin/env python3
"""Validate OpenDash EGM documentation and design sitemap consistency.

Python 3.11+ standard library only. Run from any directory:
    python cms/scripts/check-plans.py
    python cms/scripts/check-plans.py --self-test

Checks:
1. All active Markdown links across README.md, AGENTS.md, docs/ and .agents/
   (excluding .agents/archive/) are relative and resolve to existing paths.
2. docs/designs/SITEMAP.md exists, contains valid design table rows, and every
   design document in docs/designs/*.md (except SITEMAP.md) is registered in
   docs/designs/SITEMAP.md.
3. Bold '**...verified...**' status claims in SITEMAP.md or catalog tables link
   directly to docs/verification/*.md evidence when claiming verified status.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys
import tempfile
from urllib.parse import unquote, urlsplit


MARKDOWN_LINK = re.compile(r"\]\(([^)]+)\)")
FENCE = re.compile(r"^\s*(?:`{3,}|~{3,})")
STATUS_BOLD = re.compile(r"^\*\*([^*]+)\*\*")
EGM_STATUS_LINE = re.compile(r"^\s*(?:[-*]\s+)?\*\*Status:\*\*\s*(.+?)\s*$", re.I)
EGM_META_LINE = re.compile(r"^\s*(?:[-*]\s+)?\*\*([^*:]+):\*\*\s*(.*?)\s*$")
EGM_PLACEHOLDER = re.compile(r"\b(?:TODO|TBD)\b", re.I)
EGM_UNRESOLVED_OQ = re.compile(r"(?:\bOQ-\d+\b|^\s*[-*]\s+\[ \])", re.M)
EGM_SLICE_HEADING = re.compile(
    r"^(#{3,4})\s+Slice\s+([A-Za-z0-9][A-Za-z0-9_-]*)\s*[-–—:]\s*(.+?)\s*$", re.I
)
EGM_REQUIRED_PROVENANCE = ("approved by", "approval evidence", "goldfish review")
EGM_REQUIRED_SLICE_FIELDS = (
    "**Dependencies:**",
    "**File Ownership:**",
    "**Acceptance Criteria:**",
    "**Rollback:**",
)


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
    return [
        cell.replace(r"\|", "|").strip()
        for cell in re.split(r"(?<!\\)\|", line.strip().strip("|"))
    ]


def links_in(text: str) -> list[str]:
    return [
        match.group(1).strip().split(" ", 1)[0].strip("<>")
        for match in MARKDOWN_LINK.finditer(text)
    ]


def claims_verified(cell: str) -> bool:
    """Only a bold status label containing 'verified' requires a verification link."""
    match = STATUS_BOLD.match(cell)
    return bool(match and "verified" in match.group(1).lower())


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


def check(root: Path) -> list[str]:
    root = root.resolve()
    docs = root / "docs"
    designs_dir = docs / "designs"
    sitemap = designs_dir / "SITEMAP.md"
    errors: list[str] = []

    if not sitemap.is_file():
        return ["Missing docs/designs/SITEMAP.md"]

    # 1. Check Markdown links in official docs, active agent guidance and root entry points.
    agent_dir = root / ".agents"
    agent_files = (
        [
            file
            for file in agent_dir.rglob("*.md")
            if "archive" not in file.relative_to(agent_dir).parts
        ]
        if agent_dir.is_dir()
        else []
    )
    files = sorted(docs.rglob("*.md")) + sorted(agent_files)
    github_dir = root / ".github"
    if github_dir.is_dir():
        files += sorted(github_dir.rglob("*.md"))
    cms_dir = root / "cms"
    if cms_dir.is_dir():
        files += sorted(
            f
            for f in cms_dir.rglob("README.md")
            if "node_modules" not in f.parts and "target" not in f.parts
        )
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
                    errors.append(
                        f"{file.relative_to(root)}:{number}: absolute Markdown link {link}"
                    )
                    continue
                target = (file.parent / unquote(parsed.path)).resolve()
                if not target.exists():
                    errors.append(
                        f"{file.relative_to(root)}:{number}: broken Markdown link {link}"
                    )

    # 2. Check SITEMAP.md table rows and ensure all docs/designs/*.md are registered.
    sitemap_targets: set[Path] = set()
    sitemap_rows = 0
    for number, line in content_lines(sitemap):
        cells = table_cells(line)
        if len(cells) < 4:
            continue
        if cells[0].lower() in {"design document", "baseline specification / contract"} or set(cells[0]) <= {"-", " "}:
            continue
        row_links = links_in(cells[0])
        if not row_links:
            errors.append(
                f"{sitemap.relative_to(root)}:{number}: table row missing Markdown link in first column"
            )
            continue
        sitemap_rows += 1
        for link in row_links:
            parsed = urlsplit(link)
            if not parsed.scheme and not parsed.netloc and parsed.path and not parsed.path.startswith("/"):
                sitemap_targets.add((sitemap.parent / unquote(parsed.path)).resolve())
        for cell in cells[1:]:
            if claims_verified(cell) and not is_verification_link(sitemap, line, root):
                errors.append(
                    f"{sitemap.relative_to(root)}:{number}: verified status "
                    "missing direct docs/verification/*.md evidence link"
                )

    if sitemap_rows == 0:
        errors.append("No design table rows found in docs/designs/SITEMAP.md")

    for design_file in sorted(designs_dir.glob("*.md")):
        if design_file.name == "SITEMAP.md":
            continue
        if design_file.resolve() not in sitemap_targets:
            errors.append(
                f"{design_file.relative_to(root)}: design document not registered in docs/designs/SITEMAP.md"
            )
        if design_file.name != "README.md":
            errors.extend(check_egm_design_structure(design_file, root))

    return errors


def check_egm_design_structure(design_file: Path, root: Path) -> list[str]:
    """Enforce EGM Approved gate: provenance, Detailed Implementation Plan, Slice Contract, Verification Plan, and no open TODO/TBD/OQ."""
    errors: list[str] = []
    rel = design_file.relative_to(root)
    active_lines = [line for _, line in content_lines(design_file)]
    meta: dict[str, str] = {}
    for line in active_lines:
        if line.startswith("## "):
            break
        match = EGM_META_LINE.match(line)
        if match:
            meta[match.group(1).strip().casefold()] = match.group(2).strip()
    if meta.get("status", "").casefold() != "approved":
        return errors

    for key in EGM_REQUIRED_PROVENANCE:
        val = meta.get(key, "")
        if not val or EGM_PLACEHOLDER.search(val):
            errors.append(
                f"{rel}: Approved EGM design missing valid '**{key.title()}:**' provenance metadata"
            )

    text = "\n".join(active_lines)
    if "## Detailed Implementation Plan" not in text:
        errors.append(f"{rel}: Approved EGM design missing '## Detailed Implementation Plan'")
    if "### Verification Plan" not in text:
        errors.append(f"{rel}: Approved EGM design missing '### Verification Plan'")
    if EGM_PLACEHOLDER.search(text):
        errors.append(f"{rel}: Approved EGM design contains unresolved TODO/TBD placeholder")

    # Check Open Questions section if present
    in_oq = False
    oq_lines: list[str] = []
    for line in active_lines:
        if line.startswith("## "):
            in_oq = line.strip().casefold() == "## open questions"
            continue
        if in_oq:
            oq_lines.append(line)
    if oq_lines and EGM_UNRESOLVED_OQ.search("\n".join(oq_lines)):
        errors.append(f"{rel}: Approved EGM design still has unresolved Open Questions (OQ-xx / unchecked items)")

    # Check per-slice contract if slices are defined inside Detailed Implementation Plan
    in_impl = False
    impl_lines: list[str] = []
    for line in active_lines:
        if line.startswith("## "):
            in_impl = line.strip() == "## Detailed Implementation Plan"
            continue
        if in_impl:
            impl_lines.append(line)
    slice_starts: list[tuple[int, int, str]] = []
    for idx, line in enumerate(impl_lines):
        m = EGM_SLICE_HEADING.match(line)
        if m:
            slice_starts.append((idx, len(m.group(1)), m.group(2)))
    for i, (start, level, slice_id) in enumerate(slice_starts):
        stop = len(impl_lines)
        for j in range(start + 1, len(impl_lines)):
            if impl_lines[j].startswith("#") and len(impl_lines[j].split(" ", 1)[0]) <= level:
                stop = j
                break
        slice_body = "\n".join(impl_lines[start:stop])
        for req_field in EGM_REQUIRED_SLICE_FIELDS:
            if req_field not in slice_body:
                errors.append(
                    f"{rel}: Slice {slice_id} missing required contract field '{req_field}'"
                )
    return errors


def fixture(root: Path) -> None:
    designs = root / "docs" / "designs"
    designs.mkdir(parents=True, exist_ok=True)
    verification = root / "docs" / "verification"
    verification.mkdir(parents=True, exist_ok=True)
    (verification / "test.md").write_text("# Real test evidence\n", encoding="utf-8")
    (designs / "README.md").write_text("# EGM Designs Guide\n", encoding="utf-8")
    (designs / "feature-a.md").write_text(
        "# Feature A\n\n"
        "* **Status:** Approved\n"
        "* **Approved By:** Owner\n"
        "* **Approval Evidence:** Owner chat 2026-11-20 + PR #1\n"
        "* **Goldfish Review:** 5-agent swarm · 2026-11-20 · reviewed baseline\n\n"
        "## Detailed Implementation Plan\n\n"
        "### Slice S01: Core\n\n"
        "- **Dependencies:** None\n"
        "- **File Ownership:** `cms/crates/opendash-core/src/lib.rs`\n"
        "- **Acceptance Criteria:**\n  - [ ] Core compiles\n"
        "- **Rollback:** Revert commit\n\n"
        "### Verification Plan\n\n- Run tests\n",
        encoding="utf-8",
    )
    (designs / "SITEMAP.md").write_text(
        "# Design Sitemap\n\n"
        "| Design Document | Date | Author | Status | Components | Description |\n"
        "| --- | --- | --- | --- | --- | --- |\n"
        "| [Guide](README.md) | 2026-11-20 | Owner | Approved | `docs/designs/` | Guide |\n"
        "| [Feature A](feature-a.md) | 2026-11-20 | Owner | **Verified** [test](../verification/test.md) | `cms/` | Spec |\n",
        encoding="utf-8",
    )
    (root / "README.md").write_text(
        "[Sitemap](docs/designs/SITEMAP.md)\n", encoding="utf-8"
    )


def self_test() -> None:
    cases = [
        ("valid", None),
        ("missing_sitemap", "Missing docs/designs/SITEMAP.md"),
        ("empty_sitemap", "No design table rows found"),
        ("unregistered_design", "not registered in docs/designs/SITEMAP.md"),
        ("missing_row_link", "table row missing Markdown link"),
        ("broken_link", "broken Markdown link"),
        ("absolute_link", "absolute Markdown link"),
        ("missing_evidence", "missing direct docs/verification"),
        ("approved_missing_impl", "missing '## Detailed Implementation Plan'"),
        ("approved_missing_verify", "missing '### Verification Plan'"),
        ("approved_open_oq", "unresolved Open Questions"),
        ("approved_missing_provenance", "provenance metadata"),
        ("approved_incomplete_slice", "missing required contract field"),
    ]
    for name, expected in cases:
        with tempfile.TemporaryDirectory(prefix="opendash-check-plans-") as tmp:
            root = Path(tmp)
            fixture(root)
            sitemap = root / "docs/designs/SITEMAP.md"
            feature = root / "docs/designs/feature-a.md"
            if name == "missing_sitemap":
                sitemap.unlink()
            elif name == "empty_sitemap":
                sitemap.write_text("# Empty Sitemap\n", encoding="utf-8")
            elif name == "unregistered_design":
                (root / "docs/designs/orphan.md").write_text("# Orphan\n", encoding="utf-8")
            elif name == "missing_row_link":
                with sitemap.open("a", encoding="utf-8") as out:
                    out.write("| Plain text without link | 2026-11-20 | Owner | Draft | `cms/` | — |\n")
            elif name == "broken_link":
                (root / "README.md").write_text("[Missing](docs/no-such-file.md)\n", encoding="utf-8")
            elif name == "absolute_link":
                (root / "README.md").write_text("[Abs](/docs/designs/SITEMAP.md)\n", encoding="utf-8")
            elif name == "missing_evidence":
                sitemap.write_text(
                    sitemap.read_text(encoding="utf-8").replace(
                        " [test](../verification/test.md)", ""
                    ),
                    encoding="utf-8",
                )
            elif name == "approved_missing_impl":
                feature.write_text(
                    "# Feature A\n\n* **Status:** Approved\n* **Approved By:** Owner\n"
                    "* **Approval Evidence:** PR #1\n* **Goldfish Review:** 3-agent\n\n## Overview\n\nText.\n",
                    encoding="utf-8",
                )
            elif name == "approved_missing_verify":
                feature.write_text(
                    "# Feature A\n\n* **Status:** Approved\n* **Approved By:** Owner\n"
                    "* **Approval Evidence:** PR #1\n* **Goldfish Review:** 3-agent\n\n"
                    "## Detailed Implementation Plan\n\nFiles.\n",
                    encoding="utf-8",
                )
            elif name == "approved_open_oq":
                feature.write_text(
                    "# Feature A\n\n* **Status:** Approved\n* **Approved By:** Owner\n"
                    "* **Approval Evidence:** PR #1\n* **Goldfish Review:** 3-agent\n\n"
                    "## Open Questions\n\n| OQ-01 | Unresolved |\n\n"
                    "## Detailed Implementation Plan\n\n### Verification Plan\n\nTests.\n",
                    encoding="utf-8",
                )
            elif name == "approved_missing_provenance":
                feature.write_text(
                    "# Feature A\n\n* **Status:** Approved\n\n## Detailed Implementation Plan\n\n### Verification Plan\n\nTests.\n",
                    encoding="utf-8",
                )
            elif name == "approved_incomplete_slice":
                feature.write_text(
                    "# Feature A\n\n* **Status:** Approved\n* **Approved By:** Owner\n"
                    "* **Approval Evidence:** PR #1\n* **Goldfish Review:** 3-agent\n\n"
                    "## Detailed Implementation Plan\n\n### Slice S01: Core\n\n- Only files\n\n"
                    "### Verification Plan\n\nTests.\n",
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
    print("PASS check-plans: markdown links, EGM design sitemap coverage, verified evidence")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
