#!/usr/bin/env python3
"""EGM CPT Builder S01: real Chrome smoke on a disposable native OpenDash site.

Uses the existing random-loopback-port launcher and a fresh temporary site root.
Never targets the owner's running port 18964 or any existing site database.

Usage:
  python3 cms/scripts/egm_cpt_builder_s01_e2e.py \
    --binary /path/to/pinned/opendash --artifact-dir /path/to/private/results
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from m4_s11_t01_page_builder_e2e import new_heading
from m7_beta_menu_picker_e2e import api_json, admin_login, launch_server, stop_server


TYPE_KEY = "project"
TITLE = "Project A Builder Smoke"
SLUG = "s01-project-a"
BODY = "<p>Original <strong>Project A</strong> rich-text body.</p>"
DESIGN = "PROJECT A BUILDER AST S01"
SETTINGS_HASH = "#/settings/content-types"


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def builder_path(entry_id: int) -> str:
    return f"/api/admin/entries/{TYPE_KEY}/{entry_id}/builder"


def entry_path(entry_id: int) -> str:
    return f"/api/admin/content-types/{TYPE_KEY}/entries/{entry_id}"


def editor_url(base: str, entry_id: int) -> str:
    return f"{base}/admin/#/cpt/{TYPE_KEY}/{entry_id}/edit"


def schema_from_server(context, base: str) -> dict:
    entries = api_json(context, "GET", base, "/api/admin/content-types")
    return next(row for row in entries if row["descriptor"]["key"] == TYPE_KEY)


def open_type_settings(page, base: str):
    page.goto(base + "/admin/" + SETTINGS_HASH, wait_until="domcontentloaded")
    page.get_by_role("button", name="Project (project)", exact=True).click()
    form = page.get_by_role("form", name="Cấu hình loại nội dung")
    expect(form.get_by_label("Visual Builder (builder)")).to_be_visible(timeout=20000)
    return form


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    assert binary.is_file(), f"Missing native binary: {binary}"
    artifacts.mkdir(parents=True, exist_ok=True)
    checks: list[str] = []
    js_errors: list[str] = []
    source_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    with tempfile.TemporaryDirectory(prefix="opendash-egm-cpt-builder-s01-") as temporary:
        root = pathlib.Path(temporary).resolve() / "site"
        root.mkdir(mode=0o700)
        server = None
        try:
            # Exactly one isolated native process; launcher selects a free
            # loopback port and points both DB and content root at this fixture.
            server, base = launch_server(binary, root)
            assert not base.endswith(":18964"), base
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    context = browser.new_context(viewport={"width": 1440, "height": 900})
                    try:
                        page = context.new_page()
                        page.on("pageerror", lambda error: js_errors.append(str(error)))
                        api_json(context, "POST", base, "/auth/setup", {
                            "username": "s01_admin", "password": "S01-Only-Test-Password-123!"
                        }, expected=201)
                        admin_login(context, base, "s01_admin", "S01-Only-Test-Password-123!")

                        # Create project through Admin, including a user-owned
                        # rich_text body and explicitly enabled Builder support.
                        page.goto(base + "/admin/" + SETTINGS_HASH, wait_until="domcontentloaded")
                        page.get_by_role("button", name="+ Tạo loại nội dung mới").click()
                        page.locator("#content-type-key").fill(TYPE_KEY)
                        page.locator("#content-type-label").fill("Project")
                        page.locator("#new-cpt-publication").select_option("draft_publish")
                        page.locator("#new-cpt-builder").check()
                        page.get_by_role("button", name="+ Thêm field").click()
                        page.locator('[id^="schema-field-key-"]').fill("body")
                        page.locator('[id^="schema-field-label-"]').fill("Body")
                        page.locator('[id^="schema-field-type-"]').select_option("rich_text")
                        page.get_by_role("button", name="Tạo loại nội dung", exact=True).click()
                        expect(page.get_by_role("button", name="Project (project)", exact=True)).to_be_visible(
                            timeout=20000
                        )
                        project = schema_from_server(context, base)
                        assert project["descriptor"]["settings"]["publication"] == "draft_publish", project
                        assert "builder" in project["descriptor"]["settings"]["supports"], project
                        assert any(
                            field["key"] == "body" and field["type"] == "rich_text"
                            for field in project["descriptor"]["fields"]
                        ), project
                        checks.append("chrome_create_project_with_rich_text_body_and_builder_support")

                        # Draft publication stays private even after Builder Save.
                        page.goto(base + "/admin/#/cpt/project/new", wait_until="domcontentloaded")
                        expect(page.locator("#content-field-body")).to_be_visible(timeout=20000)
                        page.locator("#entry-title-input").fill(TITLE)
                        page.locator("#entry-slug-input").fill(SLUG)
                        page.locator("#content-field-body").fill(BODY)
                        page.get_by_role("group", name="Lưu nháp và xuất bản").get_by_role(
                            "button", name="Lưu nháp"
                        ).click()
                        expect(page).to_have_url(re.compile(r"#/cpt/project/[1-9]\d*/edit$"), timeout=20000)
                        match = re.search(r"#/cpt/project/([1-9]\d*)/edit$", page.url)
                        assert match, page.url
                        entry_id = int(match.group(1))
                        initial_entry = api_json(context, "GET", base, entry_path(entry_id))
                        assert initial_entry["values"]["body"] == BODY, initial_entry
                        assert initial_entry["publication_status"] == "draft", initial_entry
                        assert initial_entry["content_mode"] == "editor", initial_entry
                        assert context.request.get(base + "/content/project/" + SLUG).status == 404
                        checks.append("chrome_create_project_A_private_draft_with_original_body")

                        # Before a document exists, Editor cannot select Builder
                        # as content source. It can open the exact scoped editor.
                        mode = page.locator("#entry-content-mode")
                        expect(mode).to_have_value("editor", timeout=20000)
                        expect(mode.locator('option[value="builder"]')).to_have_count(1)
                        expect(mode.locator('option[value="builder"]')).to_be_disabled()
                        expect(page.get_by_role("button", name="Mở Visual Builder")).to_be_visible()
                        page.get_by_role("button", name="Mở Visual Builder").click()
                        scoped_hash = f"#/builder?template=entry&type=project&entry={entry_id}"
                        expect(page).to_have_url(base + "/admin/" + scoped_hash, timeout=20000)
                        workspace = page.get_by_test_id("builder-workspace")
                        expect(workspace).to_contain_text("Đã mở", timeout=20000)
                        builder_initial = api_json(context, "GET", base, builder_path(entry_id))
                        assert builder_initial["entry_id"] == entry_id, builder_initial
                        assert builder_initial["type_key"] == TYPE_KEY, builder_initial
                        assert builder_initial["content_mode"] == "editor", builder_initial
                        assert builder_initial["document"] is None, builder_initial
                        checks.append("builder_option_requires_document_and_opens_exact_project_scope")

                        # Save AST using the actual Builder controls. Confirmation
                        # must retain original rich text as a separate field.
                        new_heading(page, DESIGN)
                        expect(workspace).to_contain_text("Chưa lưu", timeout=20000)
                        prompts: list[str] = []

                        def accept_confirm(dialog):
                            prompts.append(dialog.message)
                            dialog.accept()

                        page.on("dialog", accept_confirm)
                        page.locator("#save").click()
                        expect(workspace).to_contain_text("Đã lưu", timeout=20000)
                        built = api_json(context, "GET", base, builder_path(entry_id))
                        assert built["version"] == builder_initial["version"] + 1, built
                        assert built["content_mode"] == "builder", built
                        assert DESIGN in json.dumps(built["document"], ensure_ascii=False), built
                        after_builder = api_json(context, "GET", base, entry_path(entry_id))
                        assert after_builder["values"]["body"] == BODY, after_builder
                        assert after_builder["publication_status"] == "draft", after_builder
                        assert after_builder["content_mode"] == "builder", after_builder
                        assert context.request.get(base + "/content/project/" + SLUG).status == 404
                        checks.append("chrome_builder_save_ast_preserves_body_and_unpublished_status")
                        page.screenshot(path=str(artifacts / "project-a-builder.png"), full_page=True)

                        # Editor↔Builder is a source selection, not a destructive
                        # migration. Saving Editor mode preserves both values.
                        page.goto(editor_url(base, entry_id), wait_until="domcontentloaded")
                        mode = page.locator("#entry-content-mode")
                        expect(mode).to_have_value("builder", timeout=20000)
                        expect(page.locator("#content-field-body")).to_have_value(BODY, timeout=20000)
                        mode.select_option("editor")
                        page.get_by_role("group", name="Lưu nháp và xuất bản").get_by_role(
                            "button", name="Lưu nháp"
                        ).click()
                        expect(page.get_by_text("Đã lưu thành công", exact=False)).to_be_visible(
                            timeout=20000
                        )
                        editor_saved = api_json(context, "GET", base, entry_path(entry_id))
                        after_switch = api_json(context, "GET", base, builder_path(entry_id))
                        assert editor_saved["content_mode"] == "editor", editor_saved
                        assert editor_saved["publication_status"] == "draft", editor_saved
                        assert editor_saved["values"]["body"] == BODY, editor_saved
                        assert after_switch["document"] == built["document"], after_switch
                        assert after_switch["version"] == built["version"] + 1, after_switch
                        page.reload(wait_until="domcontentloaded")
                        expect(page.locator("#entry-content-mode")).to_have_value("editor", timeout=20000)
                        expect(page.locator("#content-field-body")).to_have_value(BODY, timeout=20000)
                        assert context.request.get(base + "/content/project/" + SLUG).status == 404
                        checks.append("chrome_switch_back_editor_preserves_rich_text_ast_and_draft")

                        # Disable per-CPT Builder in Admin: its affordance and API
                        # disappear, while the private AST remains stored.
                        form = open_type_settings(page, base)
                        form.get_by_label("Visual Builder (builder)").uncheck()
                        with page.expect_response(
                            lambda response: response.request.method == "PUT"
                            and response.url.endswith("/api/admin/content-types/project"),
                            timeout=20000,
                        ) as disable_response:
                            form.get_by_role("button", name="Lưu cấu hình").click()
                        assert disable_response.value.status == 200, disable_response.value.text()
                        expect(form.get_by_label("Visual Builder (builder)")).not_to_be_checked(
                            timeout=20000
                        )
                        disabled = schema_from_server(context, base)
                        assert "builder" not in disabled["descriptor"]["settings"]["supports"], disabled
                        denied = context.request.get(base + builder_path(entry_id))
                        assert denied.status == 403, (denied.status, denied.text()[:260])
                        page.goto(editor_url(base, entry_id), wait_until="domcontentloaded")
                        expect(page.locator("#entry-title-input")).to_have_value(TITLE, timeout=20000)
                        expect(page.get_by_role("button", name="Mở Visual Builder")).to_have_count(0)
                        expect(page.locator("#entry-content-mode")).to_have_count(0)
                        expect(page.locator("#content-field-body")).to_have_value(BODY, timeout=20000)
                        assert api_json(context, "GET", base, entry_path(entry_id))["publication_status"] == "draft"
                        checks.append("disabled_cpt_builder_hides_button_and_denies_builder_api_403")

                        # Verify disabled support did not delete the old document
                        # by re-enabling the same CPT without editing its entry.
                        form = open_type_settings(page, base)
                        form.get_by_label("Visual Builder (builder)").check()
                        with page.expect_response(
                            lambda response: response.request.method == "PUT"
                            and response.url.endswith("/api/admin/content-types/project"),
                            timeout=20000,
                        ) as enable_response:
                            form.get_by_role("button", name="Lưu cấu hình").click()
                        assert enable_response.value.status == 200, enable_response.value.text()
                        expect(form.get_by_label("Visual Builder (builder)")).to_be_checked(
                            timeout=20000
                        )
                        restored = api_json(context, "GET", base, builder_path(entry_id))
                        assert restored["document"] == built["document"], restored
                        assert restored["content_mode"] == "editor", restored
                        assert api_json(context, "GET", base, entry_path(entry_id))["values"]["body"] == BODY
                        checks.append("reenable_builder_restores_same_ast_with_editor_body_intact")

                        assert not js_errors, f"Chrome JS errors: {js_errors}"
                    finally:
                        context.close()
                finally:
                    browser.close()
        finally:
            if server is not None:
                stop_server(server)
    return {
        "status": "PASS",
        "task": "EGM CPT Builder S01 / Issue #429",
        "source_sha": source_sha,
        "binary_sha256": digest(binary),
        "browser": "Chrome (channel=chrome, headless)",
        "checks": checks,
        "chrome_js_errors": js_errors,
        "isolation": "one temporary site root; random loopback port; no live port 18964",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    try:
        result = run(binary, artifacts)
    except Exception as error:
        result = {
            "status": "FAIL",
            "task": "EGM CPT Builder S01 / Issue #429",
            "error": f"{type(error).__name__}: {error}",
            "traceback": traceback.format_exc(),
        }
    (artifacts / "egm_cpt_builder_s01_receipt.json").write_text(
        json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(result, ensure_ascii=False, indent=2), flush=True)
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
