#!/usr/bin/env python3
"""Tests for offline GitHub Issue rendering and gate validation."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest


FILE = Path(__file__).with_name("prepare-github-issue.py")
SPEC = importlib.util.spec_from_file_location("prepare_github_issue", FILE)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def example(*, g3="READY", g2="AGREED", overview="Người quản trị cần danh mục để phân loại bài viết."):
    return f"""# Living Module Brief

## G3 — Tasks

### M3-S07-T01 — Tạo danh mục phân cấp trong Admin
- **Module:** CNT-03
- **Capability:** CNT-03.1
- **Track:** Full-Track
- **G2:** {g2}
- **G3:** {g3}

#### Tổng quan và giá trị
{overview}

#### Mục tiêu và hành vi quan sát được
Người quản trị có thể tạo danh mục có cha–con trong Admin.

#### Phạm vi công việc
- API tạo và chỉnh sửa danh mục.
- UI biểu diễn quan hệ cha–con.

#### Ngoài phạm vi
- Không thay đổi menu điều hướng website.

#### Phụ thuộc và điều kiện chặn
- Cần hợp đồng CNT-01 và FND-02 đã duyệt.

#### Kỹ thuật và file ownership
- Chỉ sửa routes/taxonomy.rs và storage/taxonomy.rs theo quyền sở hữu được giao.

#### Nghiệm thu
- [ ] Có thể tạo danh mục cha và danh mục con.
- [ ] Phân quyền và restart hoạt động đúng.

#### Kiểm thử, rủi ro và rollback
- Test HTTP/SQLite trên DB fixture và kiểm tra negative cases.
"""


class ParseTaskTests(unittest.TestCase):
    def test_rendered_issue_is_readable_and_traceable(self):
        task = MODULE.parse_task(example(), "M3-S07-T01")
        payload = MODULE.issue_payload(task, "docs/plans/modules/CNT-03-taxonomy.md", "a" * 40, "b" * 40, "forge")
        self.assertEqual(payload["title"], "[M3-S07-T01] Tạo danh mục phân cấp trong Admin")
        self.assertIn("Người quản trị cần danh mục", payload["body"])
        self.assertIn("## Tiêu chí nghiệm thu", payload["body"])
        self.assertIn("- [ ] Có thể tạo", payload["body"])
        self.assertIn("blob/" + "a" * 40, payload["body"])
        self.assertEqual(payload["labels"], ["type:implementation", "status:ready", "module:CNT-03", "milestone:M3", "agent:forge"])

    def test_refuse_not_ready_and_unapproved_g2(self):
        for kwargs in ({"g3": "PROPOSED"}, {"g2": "PROPOSED"}):
            with self.subTest(kwargs=kwargs), self.assertRaises(MODULE.TaskError):
                MODULE.parse_task(example(**kwargs), "M3-S07-T01")

    def test_refuse_historical_g3_table_only(self):
        table = "# CNT-03\n## G3 — candidates\n| Task | Mục tiêu |\n| --- | --- |\n| `M3-S07-T01` | Tạo danh mục |\n"
        with self.assertRaisesRegex(MODULE.TaskError, "đúng một heading"):
            MODULE.parse_task(table, "M3-S07-T01")

    def test_refuse_duplicate_headings(self):
        with self.assertRaisesRegex(MODULE.TaskError, "đúng một heading"):
            MODULE.parse_task(example() + "\n### M3-S07-T01 — Trùng task\n", "M3-S07-T01")

    def test_refuse_placeholder_and_incomplete_acceptance(self):
        with self.assertRaisesRegex(MODULE.TaskError, "chưa viết xong"):
            MODULE.parse_task(example(overview="TODO ..."), "M3-S07-T01")
        with self.assertRaisesRegex(MODULE.TaskError, "hai checklist"):
            MODULE.parse_task(example().replace("- [ ] Phân quyền", "- Phân quyền"), "M3-S07-T01")
        with self.assertRaisesRegex(MODULE.TaskError, "đánh dấu nghiệm thu"):
            MODULE.parse_task(example().replace("- [ ] Có thể", "- [x] Có thể"), "M3-S07-T01")

    def test_source_path_must_be_planning_markdown(self):
        for source in ("../secrets.env", "docs/README.md", "docs/plans/templates/SLICE_TASK.md", "/tmp/task.md"):
            with self.subTest(source=source), self.assertRaises(MODULE.TaskError):
                MODULE.source_path(source)
        self.assertEqual(MODULE.source_path(r"docs\plans\modules\CNT-03-taxonomy.md"), "docs/plans/modules/CNT-03-taxonomy.md")

    def test_fast_track_requires_reason_when_no_g2(self):
        doc = example().replace("**Track:** Full-Track", "**Track:** Fast-Track").replace("**G2:** AGREED", "**G2:** NOT_APPLICABLE")
        with self.assertRaisesRegex(MODULE.TaskError, "Lý do Fast-Track"):
            MODULE.parse_task(doc, "M3-S07-T01")
        doc = doc.replace("- **G3:** READY", "- **Lý do Fast-Track:** Chỉ thay đổi giao diện theo API hiện tại.\n- **G3:** READY")
        self.assertEqual(MODULE.parse_task(doc, "M3-S07-T01")["metadata"]["g2"], "NOT_APPLICABLE")

    def test_can_read_approved_blob_without_worktree_text(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "docs" / "plans" / "modules" / "CNT-03-taxonomy.md"
            path.parent.mkdir(parents=True)
            path.write_text(example(), encoding="utf-8")
            subprocess.run(["git", "init", "-q", str(root)], check=True, capture_output=True)
            subprocess.run(["git", "-C", str(root), "add", "docs/plans/modules/CNT-03-taxonomy.md"], check=True, capture_output=True)
            subprocess.run(
                ["git", "-C", str(root), "-c", "user.name=Test", "-c", "user.email=test@example.org", "commit", "-qm", "approved G3"],
                check=True, capture_output=True,
            )
            path.write_text(example(g3="PROPOSED"), encoding="utf-8")
            previous = MODULE.ROOT
            try:
                MODULE.ROOT = root
                content, sha = MODULE.read_task_source("docs/plans/modules/CNT-03-taxonomy.md", "HEAD")
                self.assertRegex(sha, r"^[0-9a-f]{40}$")
                self.assertIn("**G3:** READY", content)
                self.assertNotIn("**G3:** PROPOSED", content)
                MODULE.parse_task(content, "M3-S07-T01")
            finally:
                MODULE.ROOT = previous


if __name__ == "__main__":
    unittest.main()
