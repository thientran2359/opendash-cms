#!/usr/bin/env python3
"""Slice S03 real Chrome Playwright E2E: Quick Edit inline form, site-timezone
published_at, single-taxonomy delta (add/remove/replace + confirm), 412 conflict
diff + explicit rebase, simultaneous Single Edit + Quick Edit drafts for the same
entry across 401/Cancel without overwrite/clear, trashed-target 409 guard, EN/VI,
and mobile 390px viewport.

Always runs against a disposable temp site root and ephemeral loopback port;
never touches port 18964.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from m7_beta_menu_picker_e2e import api_json, admin_login, launch_server, set_cms_locale, stop_server


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def accept_once(page, accept: bool = True):
    dialogs = []

    def handler(dialog):
        dialogs.append((dialog.type, dialog.message))
        dialog.accept() if accept else dialog.dismiss()

    page.once("dialog", handler)

    def verify():
        assert len(dialogs) == 1, f"expected 1 dialog, got {dialogs}"

    return verify


def run(binary: pathlib.Path, artifact_dir: pathlib.Path) -> None:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    checks: dict[str, str] = {}
    with tempfile.TemporaryDirectory(prefix="opendash-s03-e2e-", dir=artifact_dir.parent) as td:
        root = pathlib.Path(td).resolve()
        proc, base = launch_server(binary, root)
        try:
            with sync_playwright() as p:
                browser = p.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1280, "height": 900})
                    admin_pw = "S03-Admin-Password-123!"
                    api_json(ctx, "POST", base, "/auth/setup", {"username": "admin", "password": admin_pw}, expected=201)
                    csrf = admin_login(ctx, base, "admin", admin_pw)

                    # Set timezone to Asia/Ho_Chi_Minh (+07:00) and start in English UI for assertions.
                    gen = api_json(ctx, "GET", base, "/api/admin/settings/general")
                    api_json(
                        ctx,
                        "PUT",
                        base,
                        "/api/admin/settings/general",
                        {
                            "site_title": gen["site_title"],
                            "tagline": gen["tagline"],
                            "timezone": "Asia/Ho_Chi_Minh",
                            "cms_locale": "en",
                        },
                        csrf,
                        {"If-Match": f'"v{gen["version"]}"'},
                    )

                    # Create categories & tags in default locale (vi).
                    cat_a = api_json(
                        ctx, "POST", base, "/api/admin/taxonomy/terms",
                        {"kind": "category", "name": "News", "slug": "news", "locale": "vi"},
                        csrf, expected=201,
                    )
                    tag_a = api_json(
                        ctx, "POST", base, "/api/admin/taxonomy/terms",
                        {"kind": "tag", "name": "AlphaTag", "slug": "alpha-tag", "locale": "vi"},
                        csrf, expected=201,
                    )
                    tag_b = api_json(
                        ctx, "POST", base, "/api/admin/taxonomy/terms",
                        {"kind": "tag", "name": "BetaTag", "slug": "beta-tag", "locale": "vi"},
                        csrf, expected=201,
                    )

                    table_snap = api_json(
                        ctx, "GET", base, "/api/admin/content-types/post/entry-table?limit=1",
                    )
                    schema_token = table_snap["schema_token"]

                    # Create draft post with body + category + AlphaTag.
                    created = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types/post/entries/composite",
                        {
                            "expected_schema_token": schema_token,
                            "title": "Quick Edit Target",
                            "slug": "quick-edit-target",
                            "locale": "vi",
                            "publication_status": "draft",
                            "values": {"body": "<p>Preserved Body Content</p>"},
                            "term_ids": [cat_a["id"], tag_a["id"]],
                            "primary_category_term_id": cat_a["id"],
                        },
                        csrf,
                        expected=201,
                    )["entry"]
                    post_id = created["id"]

                    page = ctx.new_page()

                    # 1. Inline Quick Edit: publish + timezone published_at + add BetaTag (EN UI).
                    page.goto(f"{base}/admin/#/posts", wait_until="domcontentloaded")
                    expect(page.get_by_test_id(f"entry-quick-edit-btn-{post_id}")).to_have_text("Quick Edit")
                    page.get_by_test_id(f"entry-quick-edit-btn-{post_id}").click()
                    expect(page.get_by_test_id(f"quick-edit-form-{post_id}")).to_be_visible()

                    page.get_by_test_id("quick-edit-title").fill("Quick Edited Title")
                    page.get_by_test_id("quick-edit-status").select_option("published")
                    page.get_by_test_id("quick-edit-published-at").fill("2025-05-10T14:30:00")
                    page.get_by_test_id("quick-edit-taxonomy-key").select_option("tag")
                    expect(page.get_by_test_id(f"quick-edit-term-{tag_b['id']}")).to_be_visible()
                    page.get_by_test_id(f"quick-edit-term-{tag_b['id']}").check()
                    page.get_by_test_id("quick-edit-save-btn").click()

                    expect(page.get_by_test_id("quick-edit-saved-notice")).to_be_visible()
                    after_save = api_json(ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_id}")
                    assert after_save["title"] == "Quick Edited Title", after_save
                    assert after_save["publication_status"] == "published", after_save
                    assert after_save["published_at"] == "2025-05-10 07:30:00", after_save
                    assert after_save["values"]["body"] == "<p>Preserved Body Content</p>", after_save
                    assert set(after_save["term_ids"]) == {cat_a["id"], tag_a["id"], tag_b["id"]}, after_save
                    checks["quick_edit_publish_date_and_taxonomy_add"] = "PASS"

                    # 2. Replace tags with explicit confirmation (clears tags, preserves category!).
                    page.get_by_test_id(f"entry-quick-edit-btn-{post_id}").click()
                    page.get_by_test_id("quick-edit-taxonomy-key").select_option("tag")
                    page.get_by_test_id("quick-edit-taxonomy-op").select_option("replace")
                    expect(page.get_by_test_id("quick-edit-taxonomy-terms")).to_be_visible()
                    verify_replace_dlg = accept_once(page, accept=True)
                    page.get_by_test_id("quick-edit-save-btn").click()
                    verify_replace_dlg()
                    expect(page.get_by_test_id("quick-edit-saved-notice")).to_be_visible()
                    after_clear = api_json(ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_id}")
                    assert after_clear["term_ids"] == [cat_a["id"]], after_clear
                    checks["quick_edit_taxonomy_replace_clear_confirm"] = "PASS"

                    # 3. 412 conflict + changed-fields diff + explicit rebase!
                    page.get_by_test_id(f"entry-quick-edit-btn-{post_id}").click()
                    page.get_by_test_id("quick-edit-title").fill("Local Rebase Candidate")
                    api_json(
                        ctx,
                        "PATCH",
                        base,
                        f"/api/admin/content-types/post/entries/{post_id}/metadata",
                        {"expected_schema_token": schema_token, "title": "Concurrent Server Title"},
                        csrf,
                        {"If-Match": f'"v{after_clear["version"]}"'},
                    )
                    page.get_by_test_id("quick-edit-save-btn").click()
                    expect(page.get_by_test_id("quick-edit-conflict-412")).to_be_visible()
                    expect(page.get_by_test_id("quick-edit-conflict-diffs")).to_contain_text(
                        "Concurrent Server Title"
                    )
                    expect(page.get_by_test_id("quick-edit-save-btn")).to_be_disabled()
                    page.get_by_test_id("quick-edit-rebase-btn").click()
                    expect(page.get_by_test_id("quick-edit-title")).to_have_value("Local Rebase Candidate")
                    page.get_by_test_id("quick-edit-save-btn").click()
                    expect(page.get_by_test_id("quick-edit-saved-notice")).to_be_visible()
                    after_rebase = api_json(ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_id}")
                    assert after_rebase["title"] == "Local Rebase Candidate", after_rebase
                    checks["quick_edit_412_diff_and_explicit_rebase"] = "PASS"

                    # 4. Simultaneous Full Single Edit draft + Quick Edit draft for the SAME entry ID across 401 & Cancel!
                    # Use SPA navigation (clicking links/buttons) so in-memory RAM EditorDraftStore persists across routes.
                    page.locator(f'tr[data-entry-id="{post_id}"] button', has_text="Edit").first.click()
                    title_input = page.locator("#entry-title-input")
                    expect(title_input).to_be_visible()
                    title_input.fill("Full Single Edit Unsaved Draft")
                    verify_leave = accept_once(page, accept=True)
                    page.get_by_role("link", name="All posts").click()
                    verify_leave()

                    expect(page.get_by_test_id(f"entry-quick-edit-btn-{post_id}")).to_be_visible()
                    page.get_by_test_id(f"entry-quick-edit-btn-{post_id}").click()
                    page.get_by_test_id("quick-edit-title").fill("Quick Edit Unsaved Draft")
                    page.get_by_test_id("quick-edit-slug").fill("quick-unsaved-slug")

                    # Expire session -> click Save Quick Edit -> 401 quarantines BOTH drafts -> sign back in!
                    ctx.clear_cookies()
                    page.get_by_test_id("quick-edit-save-btn").click()
                    login_user = page.locator("#auth-main #auth-username")
                    expect(login_user).to_be_visible()
                    login_user.fill("admin")
                    page.locator("#auth-main #auth-password").fill(admin_pw)
                    page.locator("#auth-main #auth-login").click()

                    # Quick Edit draft restores automatically from distinct `quick_edit:` namespace!
                    expect(page.get_by_test_id(f"quick-edit-form-{post_id}")).to_be_visible()
                    expect(page.get_by_test_id("quick-edit-restored-alert")).to_be_visible()
                    expect(page.get_by_test_id("quick-edit-title")).to_have_value("Quick Edit Unsaved Draft")
                    expect(page.get_by_test_id("quick-edit-slug")).to_have_value("quick-unsaved-slug")

                    # Cancel Quick Edit -> MUST NOT clear the Full Single Edit draft for the same entry!
                    verify_cancel = accept_once(page, accept=True)
                    page.get_by_test_id("quick-edit-cancel-btn").click()
                    verify_cancel()
                    expect(page.get_by_test_id(f"quick-edit-form-{post_id}")).to_have_count(0)

                    page.locator(f'tr[data-entry-id="{post_id}"] button', has_text="Edit").first.click()
                    expect(page.locator("#entry-title-input")).to_have_value("Full Single Edit Unsaved Draft")
                    checks["simultaneous_single_edit_and_quick_edit_401_cancel_isolation"] = "PASS"

                    # 5. Trashed-target 409 guard in Quick Edit preserves recoverable text!
                    verify_leave2 = accept_once(page, accept=True)
                    page.get_by_role("link", name="All posts").click()
                    verify_leave2()
                    csrf = api_json(ctx, "GET", base, "/auth/me")["csrf_token"]
                    fresh_entry = api_json(ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_id}")
                    page.get_by_test_id(f"entry-quick-edit-btn-{post_id}").click()
                    page.get_by_test_id("quick-edit-title").fill("Recoverable Before Trash")
                    api_json(
                        ctx,
                        "POST",
                        base,
                        f"/api/admin/content-types/post/entries/{post_id}/trash",
                        {},
                        csrf,
                        {"If-Match": f'"v{fresh_entry["version"]}"'},
                    )
                    page.get_by_test_id("quick-edit-save-btn").click()
                    expect(page.get_by_test_id("quick-edit-trashed-guard")).to_be_visible()
                    expect(page.get_by_test_id("quick-edit-recoverable-text")).to_contain_text(
                        "Recoverable Before Trash"
                    )
                    checks["quick_edit_trashed_target_409_recoverable"] = "PASS"

                    # 6. Switch to Vietnamese UI + mobile 390px viewport -> localized labels & zero horizontal overflow!
                    trashed_table = api_json(
                        ctx, "GET", base, "/api/admin/content-types/post/entry-table?status=trash",
                    )
                    t_ver = next(i["version"] for i in trashed_table["items"] if i["id"] == post_id)
                    api_json(
                        ctx,
                        "POST",
                        base,
                        f"/api/admin/content-types/post/entries/{post_id}/restore",
                        {},
                        csrf,
                        {"If-Match": f'"v{t_ver}"'},
                    )
                    set_cms_locale(ctx, base, csrf, "vi")
                    verify_cancel_trash = accept_once(page, accept=True)
                    page.get_by_test_id("quick-edit-cancel-btn").click()
                    verify_cancel_trash()
                    page.set_viewport_size({"width": 390, "height": 844})
                    page.reload(wait_until="domcontentloaded")
                    expect(page.get_by_test_id(f"entry-quick-edit-btn-{post_id}")).to_have_text("Sửa nhanh")
                    page.get_by_test_id(f"entry-quick-edit-btn-{post_id}").click()
                    expect(page.get_by_test_id(f"quick-edit-form-{post_id}")).to_be_visible()
                    overflow = page.evaluate(
                        "document.documentElement.scrollWidth > document.documentElement.clientWidth"
                    )
                    assert not overflow, "mobile 390px Quick Edit should not overflow horizontally"
                    page.screenshot(path=str(artifact_dir / "s03_quick_edit_vi_mobile_390.png"))
                    checks["vi_localization_and_mobile_390_no_overflow"] = "PASS"
                finally:
                    browser.close()

            summary = {
                "status": "PASS",
                "binary": str(binary),
                "binary_sha256": sha(binary),
                "checks": checks,
            }
            (artifact_dir / "summary.json").write_text(
                json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
            )
            print(json.dumps(summary, indent=2, ensure_ascii=False))
        finally:
            stop_server(proc)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True, type=pathlib.Path)
    ap.add_argument("--artifact-dir", required=True, type=pathlib.Path)
    args = ap.parse_args()
    try:
        run(args.binary.resolve(), args.artifact_dir.resolve())
        return 0
    except Exception:
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
