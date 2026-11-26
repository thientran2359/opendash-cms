#!/usr/bin/env python3
"""Slice S04 real Chrome Playwright E2E: bounded current-page Bulk Actions,
broadened draft_publish CPT publication gating, single-taxonomy add/remove/replace
preserving unrelated taxonomies & fields, mixed-locale blocking, Trash/Restore/
Admin-only Purge + immediate public-restore confirmation, mixed 200/412 explicit
reconcile & retry without replaying successes, post-commit network-drop uncertain
reconciliation, EN/VI localization, and 390px mobile no-overflow.

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


def accept_once(page, accept: bool = True, expected_substr: str | None = None):
    dialogs: list[tuple[str, str]] = []

    def handler(dialog):
        dialogs.append((dialog.type, dialog.message))
        dialog.accept() if accept else dialog.dismiss()

    page.once("dialog", handler)

    def verify():
        assert len(dialogs) == 1, f"expected 1 dialog, got {dialogs}"
        if expected_substr is not None:
            assert expected_substr.lower() in dialogs[0][1].lower(), (
                f"expected {expected_substr!r} in dialog {dialogs[0][1]!r}"
            )
        return dialogs[0][1]

    return verify


def ui_login(page, base: str, username: str, password: str) -> None:
    page.goto(f"{base}/admin/", wait_until="domcontentloaded")
    page.locator("#auth-main #auth-username").fill(username)
    page.locator("#auth-main #auth-password").fill(password)
    page.locator("#auth-main #auth-login").click()
    expect(page.locator("#auth-main")).to_be_hidden(timeout=20000)


def run(binary: pathlib.Path, artifact_dir: pathlib.Path) -> None:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    checks: dict[str, str] = {}
    with tempfile.TemporaryDirectory(prefix="opendash-s04-e2e-", dir=artifact_dir.parent) as td:
        root = pathlib.Path(td).resolve()
        proc, base = launch_server(binary, root)
        try:
            with sync_playwright() as p:
                browser = p.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1280, "height": 900})
                    admin_pw = "S04-Admin-Password-123!"
                    editor_pw = "S04-Editor-Password-123!"
                    api_json(ctx, "POST", base, "/auth/setup", {"username": "admin", "password": admin_pw}, expected=201)
                    csrf = admin_login(ctx, base, "admin", admin_pw)
                    set_cms_locale(ctx, base, csrf, "en")

                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/users",
                        {"username": "editor", "password": editor_pw, "role": "editor"},
                        csrf,
                        expected=201,
                    )

                    # Enable multilingual and add `en` for mixed-locale bulk guard checks.
                    loc_resp = ctx.request.get(f"{base}/api/admin/locales")
                    assert loc_resp.status == 200, loc_resp.text()
                    loc_etag = loc_resp.headers["etag"]
                    ml_resp = ctx.request.put(
                        f"{base}/api/admin/locales/multilingual",
                        data=json.dumps({"enabled": True}),
                        headers={
                            "Content-Type": "application/json",
                            "Origin": base,
                            "X-CSRF-Token": csrf,
                            "If-Match": loc_etag,
                        },
                    )
                    assert ml_resp.status == 200, ml_resp.text()
                    loc_etag = ml_resp.headers["etag"]
                    add_en = ctx.request.post(
                        f"{base}/api/admin/locales",
                        data=json.dumps({"code": "en", "label": "English"}),
                        headers={
                            "Content-Type": "application/json",
                            "Origin": base,
                            "X-CSRF-Token": csrf,
                            "If-Match": loc_etag,
                        },
                    )
                    assert add_en.status == 201, add_en.text()

                    # Create custom draft_publish CPT ("article") and immediate public CPT ("catalog").
                    art_ct = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types",
                        {
                            "key": "article",
                            "label": "Articles",
                            "settings": {
                                "public": True,
                                "has_archive": True,
                                "publication": "draft_publish",
                                "supports": ["editor"],
                            },
                            "fields": [{"key": "summary", "label": "Summary", "type": "short_text"}],
                        },
                        csrf,
                        expected=201,
                    )
                    art_ct_ver = int(art_ct["version"])
                    api_json(
                        ctx,
                        "PUT",
                        base,
                        "/api/admin/taxonomy/eligibility/article",
                        {"taxonomies": ["category", "tag"], "expected_version": art_ct_ver},
                        csrf,
                        {"If-Match": f'"v{art_ct_ver}"'},
                    )
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types",
                        {
                            "key": "catalog",
                            "label": "Catalog",
                            "settings": {
                                "public": True,
                                "has_archive": True,
                                "publication": "immediate",
                                "supports": ["editor"],
                            },
                            "fields": [{"key": "sku", "label": "SKU", "type": "short_text"}],
                        },
                        csrf,
                        expected=201,
                    )

                    # Terms in vi.
                    cat_vi = api_json(
                        ctx, "POST", base, "/api/admin/taxonomy/terms",
                        {"kind": "category", "name": "Article Cat", "slug": "art-cat", "locale": "vi"},
                        csrf, expected=201,
                    )
                    tag_keep = api_json(
                        ctx, "POST", base, "/api/admin/taxonomy/terms",
                        {"kind": "tag", "name": "KeepTag", "slug": "keep-tag", "locale": "vi"},
                        csrf, expected=201,
                    )
                    tag_bulk = api_json(
                        ctx, "POST", base, "/api/admin/taxonomy/terms",
                        {"kind": "tag", "name": "BulkTag", "slug": "bulk-tag", "locale": "vi"},
                        csrf, expected=201,
                    )

                    art_tok = api_json(ctx, "GET", base, "/api/admin/content-types/article/entry-table?limit=1")["schema_token"]
                    articles = []
                    for i in (1, 2):
                        entry = api_json(
                            ctx,
                            "POST",
                            base,
                            "/api/admin/content-types/article/entries/composite",
                            {
                                "expected_schema_token": art_tok,
                                "title": f"Bulk Article {i}",
                                "slug": f"bulk-article-{i}",
                                "locale": "vi",
                                "publication_status": "draft",
                                "values": {"summary": f"Summary {i}"},
                                "term_ids": [cat_vi["id"], tag_keep["id"]],
                            },
                            csrf,
                            expected=201,
                        )["entry"]
                        articles.append(entry)

                    cat_tok = api_json(ctx, "GET", base, "/api/admin/content-types/catalog/entry-table?limit=1")["schema_token"]
                    catalog_entry = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types/catalog/entries/composite",
                        {
                            "expected_schema_token": cat_tok,
                            "title": "Catalog Public One",
                            "slug": "catalog-public-one",
                            "locale": "vi",
                            "values": {"sku": "SKU-100"},
                        },
                        csrf,
                        expected=201,
                    )["entry"]

                    post_tok = api_json(ctx, "GET", base, "/api/admin/content-types/post/entry-table?limit=1")["schema_token"]
                    post_vi_a = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types/post/entries/composite",
                        {
                            "expected_schema_token": post_tok,
                            "title": "Mixed Post VI A",
                            "slug": "mixed-post-vi-a",
                            "locale": "vi",
                            "publication_status": "draft",
                            "values": {"body": "<p>A</p>"},
                        },
                        csrf,
                        expected=201,
                    )["entry"]
                    post_vi_b = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types/post/entries/composite",
                        {
                            "expected_schema_token": post_tok,
                            "title": "Mixed Post VI B",
                            "slug": "mixed-post-vi-b",
                            "locale": "vi",
                            "publication_status": "draft",
                            "values": {"body": "<p>B</p>"},
                        },
                        csrf,
                        expected=201,
                    )["entry"]
                    post_vi_uncertain = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types/post/entries/composite",
                        {
                            "expected_schema_token": post_tok,
                            "title": "Uncertain Drop Post",
                            "slug": "uncertain-drop-post",
                            "locale": "vi",
                            "publication_status": "draft",
                            "values": {"body": "<p>U</p>"},
                        },
                        csrf,
                        expected=201,
                    )["entry"]
                    post_en = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types/post/entries/composite",
                        {
                            "expected_schema_token": post_tok,
                            "title": "Mixed Post EN",
                            "slug": "mixed-post-en",
                            "locale": "en",
                            "publication_status": "draft",
                            "values": {"body": "<p>EN</p>"},
                        },
                        csrf,
                        expected=201,
                    )["entry"]

                    page = ctx.new_page()

                    # 1. Broadened draft_publish CPT ("article") exposes Publish/Move to Draft,
                    #    while immediate CPT ("catalog") hides them.
                    page.goto(f"{base}/admin/#/cpt/article?locale=vi", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("bulk-publish-btn")).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("bulk-draft-btn")).to_be_visible()
                    for art in articles:
                        page.get_by_label(f"Select item #{art['id']}").check()
                    verify = accept_once(page, True, "publish")
                    page.get_by_test_id("bulk-publish-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)
                    for art in articles:
                        expect(page.locator(f'li[data-bulk-id="{art["id"]}"][data-bulk-status="succeeded"]')).to_be_visible()
                        cur = api_json(ctx, "GET", base, f"/api/admin/content-types/article/entries/{art['id']}")
                        assert cur["publication_status"] == "published", cur

                    page.goto(f"{base}/admin/#/cpt/catalog?locale=vi", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("entry-list-catalog")).to_be_visible(timeout=20000)
                    assert page.get_by_test_id("bulk-publish-btn").count() == 0
                    assert page.get_by_test_id("bulk-draft-btn").count() == 0
                    checks["broadened_cpt_publication_and_immediate_cpt_gate"] = "PASS"

                    # 2. Bulk Single-Taxonomy Add + Replace/Clear preserving category & fields.
                    page.goto(f"{base}/admin/#/cpt/article?locale=vi", wait_until="domcontentloaded")
                    for art in articles:
                        page.get_by_label(f"Select item #{art['id']}").check()
                    page.get_by_test_id("bulk-taxonomy-toggle-btn").click()
                    expect(page.get_by_test_id("bulk-taxonomy-panel")).to_be_visible()
                    page.get_by_test_id("bulk-taxonomy-key").select_option("tag")
                    expect(page.get_by_test_id(f"bulk-taxonomy-term-{tag_bulk['id']}")).to_be_visible(timeout=15000)
                    page.get_by_test_id(f"bulk-taxonomy-term-{tag_bulk['id']}").check()
                    verify = accept_once(page, True)
                    page.get_by_test_id("bulk-taxonomy-apply-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)
                    for art in articles:
                        cur = api_json(ctx, "GET", base, f"/api/admin/content-types/article/entries/{art['id']}")
                        assert set(cur["term_ids"]) == {cat_vi["id"], tag_keep["id"], tag_bulk["id"]}, cur

                    # Clear all tags via Replace with 0 terms selected -> explicit clear confirm -> category untouched!
                    for art in articles:
                        page.get_by_label(f"Select item #{art['id']}").check()
                    page.get_by_test_id("bulk-taxonomy-toggle-btn").click()
                    page.get_by_test_id("bulk-taxonomy-key").select_option("tag")
                    page.get_by_test_id("bulk-taxonomy-op").select_option("replace")
                    verify = accept_once(page, True, "clear")
                    page.get_by_test_id("bulk-taxonomy-apply-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)
                    for i, art in enumerate(articles, start=1):
                        cur = api_json(ctx, "GET", base, f"/api/admin/content-types/article/entries/{art['id']}")
                        assert cur["term_ids"] == [cat_vi["id"]], cur
                        assert cur["values"]["summary"] == f"Summary {i}", cur
                    page.screenshot(path=str(artifact_dir / "s04-bulk-taxonomy.png"), full_page=True)
                    checks["bulk_single_taxonomy_add_and_replace_clear_preserves_other_taxonomy"] = "PASS"

                    # 3. Mixed-locale selection blocked on All Locales listing.
                    page.goto(f"{base}/admin/#/posts", wait_until="domcontentloaded")
                    expect(page.get_by_label(f"Select item #{post_vi_a['id']}")).to_be_visible(timeout=20000)
                    page.get_by_label(f"Select item #{post_vi_a['id']}").check()
                    page.get_by_label(f"Select item #{post_en['id']}").check()
                    page.get_by_test_id("bulk-move-to-trash-btn").click()
                    expect(page.get_by_test_id("bulk-block-alert")).to_be_visible()
                    assert api_json(ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_vi_a['id']}")["version"] == 1
                    checks["mixed_locale_selection_blocked"] = "PASS"

                    # 4. Bulk Trash -> Bulk Restore -> Immediate Public Restore confirm -> Editor vs Admin Bulk Purge.
                    page.goto(f"{base}/admin/#/cpt/article?locale=vi", wait_until="domcontentloaded")
                    for art in articles:
                        page.get_by_label(f"Select item #{art['id']}").check()
                    verify = accept_once(page, True)
                    page.get_by_test_id("bulk-move-to-trash-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)

                    page.goto(f"{base}/admin/#/cpt/article?locale=vi&status=trash", wait_until="domcontentloaded")
                    for art in articles:
                        expect(page.get_by_label(f"Select item #{art['id']}")).to_be_visible(timeout=20000)
                        page.get_by_label(f"Select item #{art['id']}").check()
                    expect(page.get_by_test_id("bulk-restore-btn")).to_be_visible()
                    expect(page.get_by_test_id("bulk-purge-btn")).to_be_visible()
                    verify = accept_once(page, True)
                    page.get_by_test_id("bulk-restore-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)
                    for art in articles:
                        cur = api_json(ctx, "GET", base, f"/api/admin/content-types/article/entries/{art['id']}")
                        assert cur["publication_status"] == "draft", cur

                    # Trash immediate CPT catalog entry and bulk-restore with explicit public-restore confirmation.
                    page.goto(f"{base}/admin/#/cpt/catalog?locale=vi", wait_until="domcontentloaded")
                    page.get_by_label(f"Select item #{catalog_entry['id']}").check()
                    verify = accept_once(page, True)
                    page.get_by_test_id("bulk-move-to-trash-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)
                    page.goto(f"{base}/admin/#/cpt/catalog?locale=vi&status=trash", wait_until="domcontentloaded")
                    page.get_by_label(f"Select item #{catalog_entry['id']}").check()
                    verify = accept_once(page, True, "public")
                    page.get_by_test_id("bulk-restore-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)
                    cat_after = api_json(ctx, "GET", base, f"/api/admin/content-types/catalog/entries/{catalog_entry['id']}")
                    assert cat_after["publication_status"] == "published", cat_after

                    # Trash articles again; Editor sees Bulk Restore but NO Bulk Purge.
                    page.goto(f"{base}/admin/#/cpt/article?locale=vi", wait_until="domcontentloaded")
                    for art in articles:
                        page.get_by_label(f"Select item #{art['id']}").check()
                    verify = accept_once(page, True)
                    page.get_by_test_id("bulk-move-to-trash-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)

                    ed_ctx = browser.new_context(viewport={"width": 1280, "height": 900})
                    ed_page = ed_ctx.new_page()
                    ui_login(ed_page, base, "editor", editor_pw)
                    ed_page.goto(f"{base}/admin/#/cpt/article?locale=vi&status=trash", wait_until="domcontentloaded")
                    expect(ed_page.get_by_test_id("bulk-restore-btn")).to_be_visible(timeout=20000)
                    assert ed_page.get_by_test_id("bulk-purge-btn").count() == 0
                    ed_ctx.close()

                    # Admin permanently purges both trashed articles via Bulk Purge.
                    page.goto(f"{base}/admin/#/cpt/article?locale=vi&status=trash", wait_until="domcontentloaded")
                    for art in articles:
                        page.get_by_label(f"Select item #{art['id']}").check()
                    verify = accept_once(page, True, "permanently")
                    page.get_by_test_id("bulk-purge-btn").click()
                    verify()
                    expect(page.get_by_test_id("bulk-results-panel")).to_be_visible(timeout=20000)
                    for art in articles:
                        r = ctx.request.get(f"{base}/api/admin/content-types/article/entries/{art['id']}")
                        assert r.status == 404, (art["id"], r.status)
                    checks["bulk_trash_restore_immediate_confirm_and_admin_only_purge"] = "PASS"

                    # 5. Mixed 200 + 412 + explicit Reconcile & Retry without replaying succeeded row.
                    page.goto(f"{base}/admin/#/posts?locale=vi&status=draft", wait_until="domcontentloaded")
                    expect(page.get_by_label(f"Select item #{post_vi_a['id']}")).to_be_visible(timeout=20000)
                    page.get_by_label(f"Select item #{post_vi_a['id']}").check()
                    page.get_by_label(f"Select item #{post_vi_b['id']}").check()

                    # Concurrently bump post_vi_b title via PATCH so its version advances while remaining draft!
                    api_json(
                        ctx,
                        "PATCH",
                        base,
                        f"/api/admin/content-types/post/entries/{post_vi_b['id']}/metadata",
                        {"expected_schema_token": post_tok, "title": "Mixed Post VI B Updated"},
                        csrf,
                        {"If-Match": f'"v{post_vi_b["version"]}"'},
                    )
                    verify = accept_once(page, True, "publish")
                    page.get_by_test_id("bulk-publish-btn").click()
                    verify()
                    expect(page.locator(f'li[data-bulk-id="{post_vi_a["id"]}"][data-bulk-status="succeeded"]')).to_be_visible(timeout=20000)
                    expect(page.locator(f'li[data-bulk-id="{post_vi_b["id"]}"][data-bulk-status="stale"]')).to_be_visible()
                    a_ver_after_first = api_json(ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_vi_a['id']}")["version"]
                    assert a_ver_after_first == 2, a_ver_after_first

                    # Explicit Reconcile & Retry remaining retries ONLY post_vi_b (never replays post_vi_a!).
                    expect(page.get_by_test_id("bulk-reconcile-retry-btn")).to_be_visible(timeout=20000)
                    verify = accept_once(page, True, "publish")
                    page.get_by_test_id("bulk-reconcile-retry-btn").click()
                    verify()
                    expect(page.locator(f'li[data-bulk-id="{post_vi_b["id"]}"][data-bulk-status="succeeded"]')).to_be_visible(timeout=20000)
                    assert api_json(ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_vi_a['id']}")["version"] == 2
                    b_after = api_json(ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_vi_b['id']}")
                    assert b_after["publication_status"] == "published" and b_after["version"] == 3, b_after
                    checks["mixed_200_412_reconcile_retry_without_replaying_success"] = "PASS"

                    # 6. Post-commit network drop -> `uncertain` classification & unresolved reconciliation notice.
                    page.goto(f"{base}/admin/#/posts?locale=vi", wait_until="domcontentloaded")
                    expect(page.get_by_label(f"Select item #{post_vi_uncertain['id']}")).to_be_visible(timeout=20000)
                    page.get_by_label(f"Select item #{post_vi_uncertain['id']}").check()

                    intercepted = {"done": False}

                    def drop_after_commit(route):
                        if not intercepted["done"]:
                            intercepted["done"] = True
                            # Forward to real server so DB actually commits, then drop the HTTP response!
                            route.fetch()
                            route.abort("failed")
                        else:
                            route.continue_()

                    pub_pattern = f"**/api/admin/content-types/post/entries/{post_vi_uncertain['id']}/publication"
                    page.route(pub_pattern, drop_after_commit)
                    verify = accept_once(page, True, "publish")
                    page.get_by_test_id("bulk-publish-btn").click()
                    verify()
                    expect(page.locator(f'li[data-bulk-id="{post_vi_uncertain["id"]}"][data-bulk-status="uncertain"]')).to_be_visible(timeout=20000)
                    expect(page.get_by_test_id("bulk-unresolved-uncertain-notice")).to_be_visible(timeout=20000)
                    page.unroute(pub_pattern)
                    page.screenshot(path=str(artifact_dir / "s04-uncertain-reconcile.png"), full_page=True)
                    checks["post_commit_network_drop_uncertain_reconciliation"] = "PASS"

                    # 7. Vietnamese localization + 390px mobile no horizontal overflow.
                    set_cms_locale(ctx, base, csrf, "vi")
                    page.set_viewport_size({"width": 390, "height": 844})
                    page.goto(f"{base}/admin/#/posts?locale=vi", wait_until="domcontentloaded")
                    page.reload(wait_until="domcontentloaded")
                    expect(page.get_by_test_id("bulk-taxonomy-toggle-btn")).to_contain_text("Phân loại", timeout=20000)
                    page.get_by_label(f"Chọn mục #{post_vi_a['id']}").check()
                    page.get_by_test_id("bulk-taxonomy-toggle-btn").click()
                    expect(page.get_by_test_id("bulk-taxonomy-panel")).to_be_visible()
                    body_text = page.locator("body").inner_text()
                    assert "{{" not in body_text and "}}" not in body_text, "raw i18n placeholder leaked"
                    scroll_w = page.evaluate("() => document.documentElement.scrollWidth")
                    assert scroll_w <= 390, f"mobile horizontal overflow: scrollWidth={scroll_w}"
                    page.screenshot(path=str(artifact_dir / "s04-vi-mobile-390.png"), full_page=True)
                    checks["vi_localization_and_mobile_390_no_overflow"] = "PASS"
                finally:
                    browser.close()
        finally:
            stop_server(proc)

    summary = {
        "binary": str(binary),
        "binary_sha256": sha(binary),
        "checks": checks,
        "status": "PASS",
    }
    (artifact_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description="Run S04 Bulk Actions Chrome E2E suite.")
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--artifact-dir", required=True, type=pathlib.Path)
    args = parser.parse_args()
    try:
        run(args.binary.resolve(), args.artifact_dir.resolve())
    except Exception:
        traceback.print_exc()
        raise SystemExit(1)


if __name__ == "__main__":
    main()
