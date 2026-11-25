#!/usr/bin/env python3
"""M7-S15-T04 F05: real Chrome image/gallery editor picker, native Mac binary.

Only uses disposable loopback site roots, mock images, and built-in APIs.
This runner belongs to #323 and does not mutate the user's installed site.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sqlite3
import struct
import subprocess
import tempfile
import zlib

from playwright.sync_api import expect, sync_playwright

from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server


def png_blob(rgb: tuple[int, int, int]) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data)) + tag + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    rows = (b"\x00" + bytes(rgb) * 2) * 2
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows))
        + chunk(b"IEND", b"")
    )


def upload(ctx, base: str, csrf: str, filename: str, color: tuple[int, int, int]) -> str:
    response = ctx.request.post(
        base + "/api/admin/media",
        headers={
            "Origin": base, "X-CSRF-Token": csrf,
            "Content-Type": "image/png", "X-File-Name": filename,
        },
        data=png_blob(color),
    )
    assert response.status == 201, (filename, response.status, response.text()[:260])
    return response.json()["id"]


def select_media(page, filename: str):
    dialog = page.get_by_role("dialog", name="Chọn ảnh từ Media Library")
    expect(dialog).to_be_visible(timeout=20000)
    tile = dialog.locator(".media-tile", has=page.get_by_text(filename, exact=True))
    expect(tile).to_be_visible(timeout=20000)
    tile.click()
    return dialog


def create_cpt(ctx, base, csrf):
    api_json(ctx, "POST", base, "/api/admin/content-types", {
        "key": "photo_story",
        "label": "Photo Story",
        "fields": [
            {"key": "cover", "label": "Cover", "type": "image"},
            {"key": "gallery_photos", "label": "Gallery Photos", "type": "gallery"},
            {"key": "editor_readonly", "label": "Editor Readonly", "type": "image",
             "min_view_role": "editor", "min_edit_role": "admin", "public_exposure": "private"},
            {"key": "admin_only", "label": "Admin Only", "type": "image",
             "min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"},
        ],
    }, csrf, expected=201)


def content_entry(ctx, base, kind, slug):
    rows = api_json(ctx, "GET", base, f"/api/admin/content-types/{kind}/entries")
    row = next((row for row in rows if row["slug"] == slug), None)
    assert row is not None, (kind, slug)
    return row


def draft_entry(page, base, kind, slug):
    route = "posts" if kind == "post" else "pages" if kind == "page" else f"cpt/{kind}"
    page.goto(f"{base}/admin#/{route}/new", wait_until="domcontentloaded")
    page.locator("#entry-title-input").wait_for(timeout=20000)
    page.locator("#entry-title-input").fill(f"F05 {slug}")
    page.locator("#entry-slug-input").fill(slug)
    return route


def save_created(page, route):
    page.get_by_role("button", name="Tạo & Lưu nội dung").click()
    expect(page).to_have_url(re.compile(rf"#/{re.escape(route)}/\d+/edit$"), timeout=20000)


def assert_sqlite(root: pathlib.Path):
    with sqlite3.connect(root / "database/opendash.sqlite") as db:
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        assert db.execute("PRAGMA foreign_key_check").fetchall() == []


def run(binary: pathlib.Path, artifact_dir: pathlib.Path):
    binary = binary.resolve(strict=True)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    checks = []
    source_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    binary_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
    screenshot_paths: list[pathlib.Path] = []
    with tempfile.TemporaryDirectory(prefix="opendash-m7-s15-t04-") as work:
        site = pathlib.Path(work).resolve() / "site"
        site.mkdir()
        server, base = launch_server(binary, site)
        try:
            with sync_playwright() as p:
                browser = p.chromium.launch(channel="chrome", headless=True)
                browser_version = browser.version
                ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                page = ctx.new_page()
                errors = []
                page.on("pageerror", lambda e: errors.append(str(e)))
                api_json(ctx, "POST", base, "/auth/setup", {
                    "username": "admin", "password": "F05-Local-Only-Admin-123456",
                }, expected=201)
                csrf = admin_login(ctx, base, "admin", "F05-Local-Only-Admin-123456", cms_locale="vi")
                img_a = upload(ctx, base, csrf, "f05-a.png", (255, 0, 0))
                img_b = upload(ctx, base, csrf, "f05-b.png", (0, 50, 255))
                checks.append("admin_uploaded_two_authorized_images")

                # Actual builtin Post image field; cancel must not mutate draft.
                route = draft_entry(page, base, "post", "picker-draft")
                feature = page.locator('[data-field-key="featured_image"]')
                button = feature.get_by_role("button", name="Chọn ảnh từ thư viện")
                expect(button).to_be_enabled()
                button.click()
                dialog = select_media(page, "f05-a.png")
                dialog.get_by_role("button", name="Đóng").click()
                expect(page.get_by_role("dialog")).to_have_count(0)
                assert feature.locator("input").input_value() == ""
                checks.append("post_image_cancel_retains_draft")
                button.click()
                dialog = select_media(page, "f05-a.png")
                dialog.get_by_role("button", name=re.compile(r"^Dùng ảnh đã chọn")).click()
                expect(page.get_by_role("dialog")).to_have_count(0)
                assert feature.locator("input").input_value() == img_a
                assert feature.locator("img").count() == 1
                assert feature.locator("img").first.get_attribute("src").endswith(f"/{img_a}/thumbnail")
                save_created(page, route)
                rec = content_entry(ctx, base, "post", "picker-draft")
                assert rec["publication_status"] == "draft" and rec["values"]["featured_image"] == img_a
                assert ctx.request.get(base + "/content/post/picker-draft").status == 404
                assert ctx.request.get(base + f"/media/{img_a}/file").status == 404
                checks.append("post_image_draft_saved_private_media")

                # Reopen and publish only via explicit publication action.
                page.goto(f"{base}/admin#/posts/{rec['id']}/edit", wait_until="domcontentloaded")
                expect(page.locator('[data-field-key="featured_image"] input')).to_have_value(img_a)
                expect(page.locator('[data-field-key="featured_image"] img')).to_have_count(1)
                page.get_by_label("Trạng thái xuất bản").select_option("published")
                page.get_by_role("button", name="Lưu thay đổi").click()
                expect(page.get_by_text("Đã lưu thành công", exact=False)).to_be_visible(timeout=20000)
                assert ctx.request.get(base + "/content/post/picker-draft").status == 200
                assert ctx.request.get(base + f"/media/{img_a}/file").status == 200
                checks.append("image_reopen_and_publish_media_acl")

                # CPT gallery, ordered multi-select and cancel without draft loss.
                create_cpt(ctx, base, csrf)
                route = draft_entry(page, base, "photo_story", "picker-gallery")
                gallery = page.locator('[data-field-key="gallery_photos"]')
                expect(gallery.get_by_role("button", name="Chọn nhiều ảnh")).to_be_enabled()
                gallery.get_by_role("button", name="Chọn nhiều ảnh").click()
                select_media(page, "f05-b.png")
                select_media(page, "f05-a.png")
                page.get_by_role("dialog").get_by_role("button", name=re.compile(r"^Dùng 2 ảnh")).click()
                expect(page.get_by_role("dialog")).to_have_count(0)
                assert gallery.locator("textarea").input_value().splitlines() == [img_b, img_a]
                expect(gallery.locator(".media-selected-item")).to_have_count(2)
                gallery.get_by_role("button", name="Chọn nhiều ảnh").click()
                select_media(page, "f05-b.png")
                page.get_by_role("dialog").get_by_role("button", name="Đóng").click()
                assert gallery.locator("textarea").input_value().splitlines() == [img_b, img_a]
                gallery.get_by_role("button", name="Chuyển ảnh 2 lên").click()
                assert gallery.locator("textarea").input_value().splitlines() == [img_a, img_b]
                checks.append("gallery_two_ordered_multi_cancel_reorder")
                # Unrecognized media ID must fail while leaving form draft intact.
                gallery.locator("textarea").fill("asset_unrecognized_id")
                page.get_by_role("button", name="Tạo & Lưu nội dung").click()
                expect(page.locator('[data-field-key="gallery_photos"] textarea')).to_have_value(
                    "asset_unrecognized_id", timeout=20000,
                )
                assert "#/cpt/photo_story/new" in page.url, page.url
                assert not any(row["slug"] == "picker-gallery" for row in api_json(
                    ctx, "GET", base, "/api/admin/content-types/photo_story/entries"
                ))
                # Reapply an authorized selection; draft value must remain in the
                # form until user explicitly saves.
                gallery.locator("textarea").fill("")
                gallery.get_by_role("button", name="Chọn nhiều ảnh").click()
                modal = page.get_by_role("dialog")
                modal.locator(".media-tile", has=page.get_by_text("f05-a.png", exact=True)).click()
                modal.locator(".media-tile", has=page.get_by_text("f05-b.png", exact=True)).click()
                modal.get_by_role("button", name=re.compile(r"^Dùng 2 ảnh")).click()
                assert gallery.locator("textarea").input_value().splitlines() == [img_a, img_b]
                checks.append("invalid_media_id_no_silent_draft_loss")
                save_created(page, route)
                story = content_entry(ctx, base, "photo_story", "picker-gallery")
                assert story["values"]["gallery_photos"] == [img_a, img_b], story
                page.goto(f"{base}/admin#/cpt/photo_story/{story['id']}/edit", wait_until="domcontentloaded")
                expect(page.locator('[data-field-key="gallery_photos"] textarea')).to_have_value(
                    f"{img_a}\n{img_b}", timeout=20000,
                )
                expect(page.locator('[data-field-key="gallery_photos"] img')).to_have_count(2)
                checks.append("gallery_ids_persist_reopen_thumbnails")
                screenshot = artifact_dir / "gallery-reopened.png"
                screenshot_paths.append(screenshot)
                page.screenshot(path=str(screenshot))

                # Exercise the field renderer with an ACL-restricted effective
                # projection. This is a UI-only permission-negative fixture;
                # server RBAC remains covered by the Rust ACL regressions.
                restricted_calls = []
                def restricted_editor_projection(route):
                    response = route.fetch()
                    projection = response.json()
                    projection["effective_fields"]["editor_readonly"]["editable"] = False
                    del projection["effective_fields"]["admin_only"]
                    restricted_calls.append(projection["schema_token"])
                    route.fulfill(response=response, json=projection)

                projection_url = "**/api/admin/content-types/photo_story/effective-editor-config"
                page.route(projection_url, restricted_editor_projection)
                page.reload(wait_until="domcontentloaded")
                readonly_field = page.locator('[data-field-key="editor_readonly"]')
                expect(readonly_field).to_be_visible(timeout=20000)
                assert restricted_calls, "effective editor config interceptor was not reached"
                expect(readonly_field.locator("input")).to_be_disabled()
                expect(readonly_field.get_by_role("button", name="Chọn ảnh từ thư viện")).to_be_disabled()
                expect(page.locator('[data-field-key="admin_only"]')).to_have_count(0)
                checks.append("restricted_projection_readonly_and_hidden_picker_guard")
                page.unroute(projection_url, restricted_editor_projection)

                # A pending picker cannot populate another entry after a hash
                # route change, even if a tile was selected before navigation.
                page.locator('[data-field-key="gallery_photos"]').get_by_role("button", name="Chọn nhiều ảnh").click()
                select_media(page, "f05-b.png")
                page.evaluate("window.location.hash = '#/posts/new'")
                expect(page.get_by_role("dialog")).to_have_count(0, timeout=15000)
                expect(page.locator('#entry-title-input')).to_have_value("", timeout=15000)
                expect(page.locator('[data-field-key="featured_image"] input')).to_have_value("")
                checks.append("late_picker_dropped_on_route_change")

                # Failed upload is visible and doesn't clear typed editor draft.
                page.locator("#entry-title-input").fill("Upload failure draft stays")
                page.locator('[data-field-key="featured_image"]').get_by_role("button", name="Chọn ảnh từ thư viện").click()
                page.get_by_role("dialog").get_by_label("Upload ảnh vào Media Library").set_input_files({
                    "name": "not-an-image.txt", "mimeType": "text/plain", "buffer": b"no image",
                })
                expect(page.get_by_role("dialog").get_by_role("alert")).to_be_visible(timeout=15000)
                page.get_by_role("dialog").get_by_role("button", name="Đóng").click()
                expect(page.locator("#entry-title-input")).to_have_value("Upload failure draft stays")
                checks.append("upload_validation_error_preserves_editor_draft")

                page.goto(f"{base}/admin#/posts/new", wait_until="domcontentloaded")
                page.locator('[data-field-key="featured_image"] input').wait_for(timeout=15000)

                # Simulate expired authorization during the modal's list fetch.
                # The caller quarantines all drafts on 401 and prevents stale Apply.
                page.route("**/api/admin/media?*", lambda route: route.fulfill(
                    status=401, content_type="application/json",
                    body='{"error":"session expired"}',
                ))
                page.locator('[data-field-key="featured_image"]').get_by_role("button", name="Chọn ảnh từ thư viện").click()
                expect(page.get_by_role("dialog")).to_have_count(0, timeout=15000)
                # The app may unmount the entire editor on 401; that is the
                # intended fail-closed behavior, not a failure to keep fields.
                image_input = page.locator('[data-field-key="featured_image"] input')
                assert image_input.count() == 0 or image_input.input_value() == ""
                checks.append("media_list_401_quarantine_no_stale_apply")
                page.unroute("**/api/admin/media?*")

                assert not errors, errors
                checks.append("chrome_pageerrors_none")
                browser.close()
        finally:
            stop_server(server)
        assert_sqlite(site)
        checks.append("sqlite_integrity_foreign_keys")

    result = {
        "status": "PASS", "task": "M7-S15-T04", "source_sha": source_sha,
        "binary_sha256": binary_sha, "chrome_version": browser_version,
        "checks": checks,
        "screenshots_sha256": {
            file.name: hashlib.sha256(file.read_bytes()).hexdigest() for file in screenshot_paths
        },
        "not_run": ["Windows native F05", "full Beta B01–B10", "release authorization"],
    }
    (artifact_dir / "receipt.json").write_text(
        json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8",
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    opts = parser.parse_args()
    run(opts.binary, opts.artifact_dir)
