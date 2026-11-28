#!/usr/bin/env python3
"""Playwright E2E acceptance suite for Entry Revisions, Diff & Server Autosave (CUJ 1-6)."""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import re
import sqlite3
import subprocess
import tempfile
import traceback
from urllib.parse import quote

from playwright.sync_api import Page, expect, sync_playwright
from e2e_helpers import (
    admin_login,
    api_json,
    launch_server,
    make_png,
    set_cms_locale,
    stop_server,
)


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def accept_next_confirm(page: Page, expected_substring: str | None = None):
    seen: list[str] = []

    def on_dialog(dialog):
        seen.append(dialog.message)
        dialog.accept()

    page.once("dialog", on_dialog)

    def verify() -> None:
        assert len(seen) == 1, f"Expected 1 confirm dialog, got {seen}"
        if expected_substring:
            assert expected_substring.lower() in seen[0].lower(), seen[0]

    return verify


def builder_doc(heading: str, extra_copy: str | None = None) -> dict:
    children = [
        {
            "id": "hd-1",
            "type": "core/heading",
            "props": {"level": 2, "text": heading},
        }
    ]
    if extra_copy:
        children.append(
            {"id": "p-1", "type": "core/text", "props": {"text": extra_copy}}
        )
    return {
        "schema_version": 1,
        "sections": [
            {
                "id": "sec-1",
                "children": [
                    {
                        "id": "row-1",
                        "children": [{"id": "col-1", "children": children}],
                    }
                ],
            }
        ],
    }


def schema_token(ctx, base: str, type_key: str) -> str:
    cfg = api_json(
        ctx,
        "GET",
        base,
        f"/api/admin/content-types/{quote(type_key)}/effective-editor-config",
    )
    return cfg["schema_token"]


def create_composite(
    ctx, base: str, csrf: str, type_key: str, title: str, slug: str, values: dict
) -> dict:
    tok = schema_token(ctx, base, type_key)
    res = api_json(
        ctx,
        "POST",
        base,
        f"/api/admin/content-types/{quote(type_key)}/entries/composite",
        {
            "expected_schema_token": tok,
            "title": title,
            "slug": slug,
            "values": values,
        },
        csrf,
        expected=201,
    )
    return res["entry"]


def update_composite(
    ctx,
    base: str,
    csrf: str,
    type_key: str,
    entry_id: int,
    version: int,
    title: str,
    slug: str,
    values: dict,
) -> dict:
    tok = schema_token(ctx, base, type_key)
    res = api_json(
        ctx,
        "PUT",
        base,
        f"/api/admin/content-types/{quote(type_key)}/entries/{entry_id}/composite",
        {
            "expected_version": version,
            "expected_schema_token": tok,
            "title": title,
            "slug": slug,
            "values": values,
        },
        csrf,
        expected=200,
    )
    return res["entry"]


def upload_png(ctx, base: str, csrf: str, filename: str, seed: int = 42) -> dict:
    resp = ctx.request.fetch(
        base + "/api/admin/media",
        method="POST",
        headers={
            "Origin": base,
            "X-CSRF-Token": csrf,
            "Content-Type": "image/png",
            "X-File-Name": quote(filename),
        },
        data=make_png(seed=seed, width=4, height=4),
        timeout=20000,
    )
    assert resp.status == 201, (filename, resp.status, resp.text()[:240])
    return resp.json()


def delete_media(ctx, base: str, csrf: str, media_id: str) -> None:
    info = ctx.request.get(base + f"/api/admin/media/{quote(media_id)}")
    assert info.status == 200, (media_id, info.status, info.text()[:200])
    etag = info.headers["etag"]
    resp = ctx.request.fetch(
        base + f"/api/admin/media/{quote(media_id)}",
        method="DELETE",
        headers={
            "Origin": base,
            "X-CSRF-Token": csrf,
            "If-Match": etag,
        },
        timeout=20000,
    )
    assert resp.status in (200, 202), (media_id, resp.status, resp.text()[:240])


def click_primary_save_button(page: Page) -> None:
    btn = (
        page.get_by_test_id("single-edit-move-to-trash")
        .locator("..")
        .locator('button[class*="bg-od-accent"]')
    )
    expect(btn).to_be_enabled(timeout=15000)
    btn.click()


def run(binary: pathlib.Path, artifacts: pathlib.Path, checks: list[str]) -> dict:
    parent_dir = artifacts.parent.resolve()
    with tempfile.TemporaryDirectory(
        prefix="revisions-autosave-e2e-", dir=str(parent_dir)
    ) as temporary:
        root = pathlib.Path(temporary).resolve() / "site"
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    errors: list[str] = []
                    ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                    page = ctx.new_page()
                    page.on("pageerror", lambda err: errors.append(str(err)))

                    admin_pw = "Revisions-Admin-Pass-12345!"
                    editor_pw = "Revisions-Editor-Pass-12345!"
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/auth/setup",
                        {"username": "rev_admin", "password": admin_pw},
                        expected=201,
                    )
                    csrf = admin_login(ctx, base, "rev_admin", admin_pw, cms_locale="en")
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/users",
                        {"username": "rev_editor", "password": editor_pw, "role": "editor"},
                        csrf,
                        expected=201,
                    )

                    # ------------------------------------------------------------------
                    # CUJ 1: Single Edit diff, snapshot tab, restore (v1->v2->v3->v4)
                    # ------------------------------------------------------------------
                    post = create_composite(
                        ctx,
                        base,
                        csrf,
                        "post",
                        "Release Plan v1",
                        "release-plan",
                        {"body": "<p>First paragraph baseline.</p>"},
                    )
                    post_id = post["id"]
                    post = update_composite(
                        ctx,
                        base,
                        csrf,
                        "post",
                        post_id,
                        post["version"],
                        "Release Plan v2",
                        "release-plan",
                        {"body": "<p>Second paragraph revised.</p>"},
                    )

                    page.goto(base + f"/admin/#/posts/{post_id}/edit", wait_until="domcontentloaded")
                    title_input = page.locator("#entry-title-input")
                    expect(title_input).to_have_value("Release Plan v2", timeout=20000)
                    expect(page.get_by_test_id("publish-box-revision-count")).to_have_text(
                        "2", timeout=15000
                    )

                    title_input.fill("Release Plan v3")
                    click_primary_save_button(page)
                    expect(page.get_by_test_id("publish-box-revision-count")).to_have_text(
                        "3", timeout=15000
                    )

                    page.get_by_test_id("publish-box-open-revisions").click()
                    modal = page.get_by_test_id("revision-history-modal")
                    expect(modal).to_be_visible(timeout=15000)
                    page.get_by_test_id("revision-item-1").click()
                    expect(page.get_by_test_id("revision-diff-view")).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("revision-diff-view")).to_contain_text(
                        "Release Plan v1"
                    )
                    page.screenshot(
                        path=str(artifacts / "revisions-modal-en.png"), full_page=True
                    )

                    page.get_by_test_id("revision-tab-snapshot").click()
                    expect(page.get_by_test_id("revision-snapshot-detail")).to_be_visible(
                        timeout=15000
                    )
                    expect(page.get_by_test_id("revision-snapshot-detail")).to_contain_text(
                        "Release Plan v1"
                    )

                    page.get_by_test_id("revision-restore-btn").click()
                    expect(modal).to_be_hidden( timeout=15000)
                    expect(page.get_by_test_id("publish-box-revision-count")).to_have_text(
                        "4", timeout=15000
                    )
                    expect(title_input).to_have_value("Release Plan v1", timeout=15000)
                    checks.append("cuj1_single_edit_diff_snapshot_and_restore")

                    # ------------------------------------------------------------------
                    # CUJ 2: Studio Mode 2 block diff, restore, editor mode notice,
                    #        Pattern CPT revisions, & navigation dirty reset (`mayLeave`)
                    # ------------------------------------------------------------------
                    landing = create_composite(
                        ctx,
                        base,
                        csrf,
                        "page",
                        "Landing Hero Editorial",
                        "landing-hero",
                        {"body": "<p>Legacy prose v1</p>"},
                    )
                    page_id = landing["id"]
                    b1 = api_json(
                        ctx,
                        "PUT",
                        base,
                        f"/api/admin/entries/page/{page_id}/builder",
                        builder_doc("Hero V1"),
                        csrf,
                        {"If-Match": f'"v{landing["version"]}"'},
                    )
                    api_json(
                        ctx,
                        "PUT",
                        base,
                        f"/api/admin/entries/page/{page_id}/builder",
                        builder_doc("Hero V2", "Added Subcopy"),
                        csrf,
                        {"If-Match": f'"v{b1["version"]}"'},
                    )

                    page.goto(
                        base + f"/admin/#/studio/entry/page/{page_id}",
                        wait_until="domcontentloaded",
                    )
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(
                        timeout=20000
                    )
                    page.get_by_test_id("studio-more-menu-btn").click()
                    page.get_by_test_id("studio-revisions-btn").click()
                    expect(modal).to_be_visible(timeout=15000)
                    page.get_by_test_id("revision-item-2").click()
                    expect(page.get_by_test_id("revision-diff-view")).to_contain_text(
                        "Added Subcopy", timeout=15000
                    )
                    page.get_by_test_id("revision-restore-btn").click()
                    expect(modal).to_be_hidden( timeout=15000)
                    expect(page.get_by_test_id("studio-dirty-badge")).to_have_count(0)

                    # Restore rev #1 (editor content_mode) -> Mode Notice -> Open in Editor
                    page.get_by_test_id("studio-more-menu-btn").click()
                    page.get_by_test_id("studio-revisions-btn").click()
                    expect(modal).to_be_visible(timeout=15000)
                    page.get_by_test_id("revision-item-1").click()
                    page.get_by_test_id("revision-restore-btn").click()
                    expect(modal).to_be_hidden( timeout=15000)
                    expect(
                        page.get_by_test_id("studio-restored-editor-mode-notice")
                    ).to_be_visible(timeout=15000)
                    page.get_by_test_id("studio-open-editor-after-restore").click()
                    expect(page.locator("#entry-title-input")).to_have_value(
                        "Landing Hero Editorial", timeout=15000
                    )

                    # Pattern CPT in Studio Mode 2 + dirty `mayLeave` via hash change
                    pat = api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/patterns",
                        {"title": "Hero Pattern", "slug": "hero-pattern"},
                        csrf,
                        expected=201,
                    )
                    pat_id = pat["id"]
                    pb1 = api_json(
                        ctx,
                        "PUT",
                        base,
                        f"/api/admin/entries/pattern/{pat_id}/builder",
                        builder_doc("Pattern V2"),
                        csrf,
                        {"If-Match": f'"v{pat["version"]}"'},
                    )
                    api_json(
                        ctx,
                        "PUT",
                        base,
                        f"/api/admin/entries/pattern/{pat_id}/builder",
                        builder_doc("Pattern V3", "Promo Line"),
                        csrf,
                        {"If-Match": f'"v{pb1["version"]}"'},
                    )

                    page.goto(
                        base + f"/admin/#/studio/entry/pattern/{pat_id}",
                        wait_until="domcontentloaded",
                    )
                    expect(page.get_by_test_id("studio-mode2-sidebar")).to_be_visible(
                        timeout=20000
                    )
                    page.get_by_test_id("studio-more-menu-btn").click()
                    page.get_by_test_id("studio-revisions-btn").click()
                    expect(modal).to_be_visible(timeout=15000)
                    page.get_by_test_id("revision-item-2").click()
                    expect(page.get_by_test_id("revision-diff-view")).to_contain_text(
                        "Promo Line", timeout=15000
                    )
                    page.get_by_test_id("revision-restore-btn").click()
                    expect(modal).to_be_hidden( timeout=15000)

                    # Dirty Studio via "Add block" -> "Empty section", leave via hash (`mayLeave`)
                    page.get_by_test_id("studio-mode2-sidebar").get_by_role(
                        "button", name=re.compile(r"Add block")
                    ).click()
                    page.get_by_role("button", name=re.compile(r"Empty section")).click()
                    expect(page.get_by_test_id("studio-dirty-badge")).to_be_visible(timeout=10000)
                    verify_leave = accept_next_confirm(page)
                    page.evaluate("window.location.hash = '#/posts'")
                    verify_leave()
                    expect(page.get_by_test_id("studio-workspace")).to_have_count(
                        0, timeout=15000
                    )
                    expect(page.get_by_test_id("admin-layout")).to_be_visible(timeout=15000)
                    # Leaving Posts -> Pages must NOT re-prompt for stale Studio dirty state
                    page.get_by_role("navigation", name="Admin sections").locator(
                        'button[data-menu-id="pages"]'
                    ).click()
                    expect(page).to_have_url(re.compile(r"#/pages$"), timeout=10000)
                    checks.append("cuj2_studio_mode2_diff_restore_pattern_and_mayleave")

                    # ------------------------------------------------------------------
                    # CUJ 3: Pattern Editor 403 & 3-Axis Field ACL Isolation
                    # ------------------------------------------------------------------
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types",
                        {
                            "key": "secret_doc",
                            "label": "Secret Docs",
                            "fields": [
                                {"key": "headline", "label": "Headline", "type": "text"},
                                {
                                    "key": "admin_secret",
                                    "label": "Admin Secret",
                                    "type": "text",
                                    "acl": {
                                        "min_view_role": "admin",
                                        "min_edit_role": "admin",
                                        "public_exposure": "private",
                                    },
                                },
                            ],
                            "settings": {
                                "public": False,
                                "has_archive": False,
                                "publication": "draft_publish",
                                "supports": ["editor"],
                            },
                        },
                        csrf,
                        expected=201,
                    )
                    sdoc = create_composite(
                        ctx,
                        base,
                        csrf,
                        "secret_doc",
                        "ACL Doc v1",
                        "acl-doc",
                        {"headline": "Public A", "admin_secret": "TOP_SECRET_ALPHA"},
                    )
                    update_composite(
                        ctx,
                        base,
                        csrf,
                        "secret_doc",
                        sdoc["id"],
                        sdoc["version"],
                        "ACL Doc v2",
                        "acl-doc",
                        {"headline": "Public B", "admin_secret": "TOP_SECRET_BETA"},
                    )

                    editor_ctx = browser.new_context(viewport={"width": 1280, "height": 800})
                    try:
                        ed_csrf = admin_login(editor_ctx, base, "rev_editor", editor_pw)
                        for method, path, payload in [
                            (
                                "GET",
                                f"/api/admin/content-types/pattern/entries/{pat_id}/revisions",
                                None,
                            ),
                            (
                                "GET",
                                f"/api/admin/content-types/pattern/entries/{pat_id}/autosave",
                                None,
                            ),
                            (
                                "PUT",
                                f"/api/admin/content-types/pattern/entries/{pat_id}/autosave",
                                {
                                    "source_surface": "studio",
                                    "base_version": 4,
                                    "client_seq": 1,
                                    "builder_document": builder_doc("Nope"),
                                },
                            ),
                        ]:
                            hdrs = {}
                            if method != "GET":
                                hdrs["Origin"] = base
                                hdrs["X-CSRF-Token"] = ed_csrf
                            if payload is not None:
                                hdrs["Content-Type"] = "application/json"
                            r403 = editor_ctx.request.fetch(
                                base + path,
                                method=method,
                                headers=hdrs,
                                data=json.dumps(payload) if payload is not None else None,
                                timeout=20000,
                            )
                            assert r403.status == 403, (
                                method,
                                path,
                                r403.status,
                                r403.text()[:200],
                            )

                        ed_revs = api_json(
                            editor_ctx,
                            "GET",
                            base,
                            f"/api/admin/content-types/secret_doc/entries/{sdoc['id']}/revisions",
                        )
                        rev_1_id = ed_revs["items"][1]["id"]
                        ed_det = api_json(
                            editor_ctx,
                            "GET",
                            base,
                            f"/api/admin/content-types/secret_doc/entries/{sdoc['id']}/revisions/{rev_1_id}",
                        )
                        assert ed_det["has_hidden_admin_fields"] is True, ed_det
                        ed_blob = json.dumps(ed_det)
                        assert "TOP_SECRET_ALPHA" not in ed_blob and "admin_secret" not in ed_blob, ed_blob
                        ed_cmp = api_json(
                            editor_ctx,
                            "GET",
                            base,
                            f"/api/admin/content-types/secret_doc/entries/{sdoc['id']}/revisions/compare?from={rev_1_id}&to=current",
                        )
                        assert ed_cmp["diff"]["has_hidden_admin_changes"] is True, ed_cmp
                        cmp_blob = json.dumps(ed_cmp)
                        assert "TOP_SECRET" not in cmp_blob and "admin_secret" not in cmp_blob, cmp_blob

                        ad_det = api_json(
                            ctx,
                            "GET",
                            base,
                            f"/api/admin/content-types/secret_doc/entries/{sdoc['id']}/revisions/{rev_1_id}",
                        )
                        assert "TOP_SECRET_ALPHA" in json.dumps(ad_det), ad_det
                    finally:
                        editor_ctx.close()
                    checks.append("cuj3_pattern_editor_403_and_field_acl_isolation")

                    # ------------------------------------------------------------------
                    # CUJ 4: Real 15s Timer Autosave, User Isolation, Stale Base &
                    #        401 RAM Draft Precedence
                    # ------------------------------------------------------------------
                    page.goto(base + f"/admin/#/posts/{post_id}/edit", wait_until="domcontentloaded")
                    expect(title_input).to_have_value("Release Plan v1", timeout=20000)
                    title_input.fill("Autosave Live Draft 15s")
                    expect(page.get_by_test_id("editor-autosave-status")).to_contain_text(
                        "Autosaved", timeout=25000
                    )

                    as_admin = api_json(
                        ctx,
                        "GET",
                        base,
                        f"/api/admin/content-types/post/entries/{post_id}/autosave",
                    )
                    assert as_admin["autosave"] is not None
                    assert as_admin["autosave"]["source_surface"] == "editor"
                    assert as_admin["autosave"]["title"] == "Autosave Live Draft 15s"

                    iso_ctx = browser.new_context()
                    try:
                        admin_login(iso_ctx, base, "rev_editor", editor_pw)
                        as_ed = api_json(
                            iso_ctx,
                            "GET",
                            base,
                            f"/api/admin/content-types/post/entries/{post_id}/autosave",
                        )
                        assert as_ed["autosave"] is None, as_ed
                    finally:
                        iso_ctx.close()

                    cur_post = api_json(
                        ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_id}"
                    )
                    update_composite(
                        ctx,
                        base,
                        csrf,
                        "post",
                        post_id,
                        cur_post["version"],
                        "Server Bumped Version",
                        "release-plan",
                        {"body": "<p>External edit while away.</p>"},
                    )
                    page.reload(wait_until="domcontentloaded")
                    banner = page.get_by_test_id("autosave-recovery-banner")
                    expect(banner).to_be_visible(timeout=20000)
                    expect(banner).to_contain_text("v", timeout=10000)
                    page.get_by_test_id("autosave-banner-compare").click()
                    expect(modal).to_be_visible(timeout=15000)
                    expect(page.get_by_test_id("revision-compare-to")).to_have_value(
                        "autosave", timeout=10000
                    )
                    page.get_by_test_id("revision-modal-close").click()
                    expect(modal).to_be_hidden( timeout=10000)

                    page.get_by_test_id("autosave-banner-restore").click()
                    expect(title_input).to_have_value("Autosave Live Draft 15s", timeout=10000)
                    click_primary_save_button(page)
                    expect(page.get_by_test_id("publish-box-revision-count")).to_have_text(
                        "6", timeout=15000
                    )
                    as_cleared = api_json(
                        ctx,
                        "GET",
                        base,
                        f"/api/admin/content-types/post/entries/{post_id}/autosave",
                    )
                    assert as_cleared["autosave"] is None, as_cleared

                    # 401 session expiry RAM draft precedence over server autosave
                    cur_post2 = api_json(
                        ctx, "GET", base, f"/api/admin/content-types/post/entries/{post_id}"
                    )
                    api_json(
                        ctx,
                        "PUT",
                        base,
                        f"/api/admin/content-types/post/entries/{post_id}/autosave",
                        {
                            "source_surface": "editor",
                            "base_version": cur_post2["version"],
                            "client_seq": 50,
                            "title": "Older Server Autosave",
                        },
                        csrf,
                        expected=200,
                    )
                    ctx.clear_cookies()
                    title_input.fill("Newer Local 401 Draft")
                    click_primary_save_button(page)
                    login_user = page.locator("#auth-main #auth-username")
                    expect(login_user).to_be_visible(timeout=15000)
                    login_user.fill("rev_admin")
                    page.locator("#auth-main #auth-password").fill(admin_pw)
                    page.locator("#auth-main #auth-login").click()
                    expect(title_input).to_have_value("Newer Local 401 Draft", timeout=20000)
                    click_primary_save_button(page)
                    expect(page.get_by_test_id("publish-box-revision-count")).to_have_text(
                        "7", timeout=15000
                    )
                    csrf = admin_login(ctx, base, "rev_admin", admin_pw, cms_locale="en")
                    page.reload(wait_until="domcontentloaded")
                    checks.append("cuj4_real_autosave_15s_isolation_stale_and_401_draft")

                    # ------------------------------------------------------------------
                    # CUJ 5: Missing Media 409 Confirm & Sanitize, Zero Media Pins
                    # ------------------------------------------------------------------
                    img = upload_png(ctx, base, csrf, "rev-cover.png", seed=77)
                    api_json(
                        ctx,
                        "POST",
                        base,
                        "/api/admin/content-types",
                        {
                            "key": "story",
                            "label": "Stories",
                            "fields": [
                                {"key": "cover", "label": "Cover", "type": "media", "required": False},
                                {"key": "summary", "label": "Summary", "type": "text"},
                            ],
                            "settings": {
                                "public": False,
                                "has_archive": False,
                                "publication": "draft_publish",
                                "supports": ["editor"],
                            },
                        },
                        csrf,
                        expected=201,
                    )
                    story = create_composite(
                        ctx,
                        base,
                        csrf,
                        "story",
                        "Story v1 With Media",
                        "story-media",
                        {"cover": img["id"], "summary": "With cover"},
                    )
                    update_composite(
                        ctx,
                        base,
                        csrf,
                        "story",
                        story["id"],
                        story["version"],
                        "Story v2 Media Removed",
                        "story-media",
                        {"cover": None, "summary": "No cover"},
                    )
                    delete_media(ctx, base, csrf, img["id"])

                    page.goto(
                        base + f"/admin/#/cpt/story/{story['id']}/edit",
                        wait_until="domcontentloaded",
                    )
                    expect(page.get_by_test_id("publish-box-revision-count")).to_have_text(
                        "2", timeout=20000
                    )
                    page.get_by_test_id("publish-box-open-revisions").click()
                    expect(modal).to_be_visible(timeout=15000)
                    page.get_by_test_id("revision-item-1").click()
                    page.get_by_test_id("revision-restore-btn").click()
                    confirm_box = page.get_by_test_id("revision-missing-refs-confirm")
                    expect(confirm_box).to_be_visible(timeout=15000)
                    page.get_by_test_id("revision-missing-refs-cancel").click()
                    expect(confirm_box).to_have_count(0, timeout=10000)
                    expect(modal).to_be_visible()

                    page.get_by_test_id("revision-restore-btn").click()
                    expect(confirm_box).to_be_visible(timeout=15000)
                    page.get_by_test_id("revision-missing-refs-proceed").click()
                    expect(modal).to_be_hidden( timeout=15000)
                    expect(page.get_by_test_id("publish-box-revision-count")).to_have_text(
                        "3", timeout=15000
                    )
                    expect(page.locator("#entry-title-input")).to_have_value(
                        "Story v1 With Media", timeout=15000
                    )
                    checks.append("cuj5_missing_media_preflight_confirm_and_zero_pins")

                    # ------------------------------------------------------------------
                    # CUJ 6: Trash wipes autosaves, Vietnamese UI + 390px mobile,
                    #        sqlite_schema <= 256, zero pageerrors
                    # ------------------------------------------------------------------
                    trash_post = create_composite(
                        ctx,
                        base,
                        csrf,
                        "post",
                        "Trash Wipes Autosave",
                        "trash-wipes-autosave",
                        {"body": "<p>Trash test.</p>"},
                    )
                    api_json(
                        ctx,
                        "PUT",
                        base,
                        f"/api/admin/content-types/post/entries/{trash_post['id']}/autosave",
                        {
                            "source_surface": "editor",
                            "base_version": trash_post["version"],
                            "client_seq": 1,
                            "title": "Should Be Wiped On Trash",
                        },
                        csrf,
                        expected=200,
                    )
                    page.goto(
                        base + f"/admin/#/posts/{trash_post['id']}/edit",
                        wait_until="domcontentloaded",
                    )
                    expect(page.get_by_test_id("single-edit-move-to-trash")).to_be_visible(
                        timeout=20000
                    )
                    verify_trash = accept_next_confirm(page, "trash")
                    page.get_by_test_id("single-edit-move-to-trash").click()
                    verify_trash()
                    expect(page).to_have_url(re.compile(r"#/posts"), timeout=15000)

                    trashed_resp = ctx.request.get(
                        base
                        + f"/api/admin/content-types/post/entries/{trash_post['id']}/autosave"
                    )
                    assert trashed_resp.status == 409, trashed_resp.status
                    page.get_by_test_id("entry-status-tab-trash").click()
                    restore_trash_btn = page.get_by_test_id(
                        f"entry-restore-btn-{trash_post['id']}"
                    )
                    expect(restore_trash_btn).to_be_visible(timeout=15000)
                    restore_trash_btn.click()
                    expect(restore_trash_btn).to_have_count(0, timeout=15000)
                    untrashed_as = api_json(
                        ctx,
                        "GET",
                        base,
                        f"/api/admin/content-types/post/entries/{trash_post['id']}/autosave",
                    )
                    assert untrashed_as["autosave"] is None, untrashed_as

                    # Vietnamese UI + 390px mobile
                    set_cms_locale(ctx, base, csrf, "vi")
                    page.set_viewport_size({"width": 390, "height": 844})
                    page.goto(
                        base + f"/admin/#/posts/{post_id}/edit",
                        wait_until="domcontentloaded",
                    )
                    page.reload(wait_until="domcontentloaded")
                    expect(page.get_by_test_id("publish-box-open-revisions")).to_be_visible(
                        timeout=20000
                    )
                    page.get_by_test_id("publish-box-open-revisions").click()
                    expect(modal).to_be_visible(timeout=15000)
                    expect(modal).to_contain_text("Lịch sử phiên bản", timeout=10000)
                    expect(page.get_by_test_id("revision-tab-diff")).to_contain_text(
                        "So sánh thay đổi"
                    )
                    expect(page.get_by_test_id("revision-restore-btn")).to_contain_text(
                        "Khôi phục #"
                    )
                    page.screenshot(
                        path=str(artifacts / "revisions-modal-vi-mobile.png"),
                        full_page=True,
                    )

                    with sqlite3.connect(root / "database/opendash.sqlite") as db:
                        schema_count = db.execute(
                            "SELECT COUNT(*) FROM sqlite_schema"
                        ).fetchone()[0]
                    assert schema_count <= 256, f"sqlite_schema exceeded 256: {schema_count}"
                    checks.append("cuj6_trash_wipe_vi_mobile_and_schema_budget")

                    assert not errors, errors
                    checks.append("chrome_page_errors_none")
                    return {
                        "chrome_version": browser.version,
                        "sqlite_schema_count": schema_count,
                    }
                finally:
                    browser.close()
        finally:
            stop_server(server)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(mode=0o700, parents=True, exist_ok=True)
    checks: list[str] = []
    result = {
        "task": "revisions_autosave_e2e",
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary_sha256": digest(binary),
        "platform": platform.platform(),
        "checks": checks,
        "status": "FAIL",
    }
    try:
        result.update(run(binary, artifacts, checks))
        result["status"] = "PASS"
        return 0
    except Exception as error:
        result["error"] = str(error)
        result["traceback"] = traceback.format_exc(limit=8)
        return 1
    finally:
        result["screenshots_sha256"] = {
            path.name: digest(path) for path in sorted(artifacts.glob("*.png"))
        }
        (artifacts / "receipt.json").write_text(
            json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    raise SystemExit(main())
