#!/usr/bin/env python3
"""M7-S16-T01: isolated native macOS Chrome acceptance for two upload controls.

Never uses the owner's site; --binary and --artifact-dir are required. The first
geometry assertion intentionally reproduces the original grid overlap on the
baseline binary and captures a FAIL receipt/screenshot instead of hiding it.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import pathlib
import platform
import struct
import subprocess
import tempfile
import traceback
import zlib
from typing import Any

from playwright.sync_api import expect, sync_playwright

from m7_beta_menu_picker_e2e import admin_login, api_json, launch_server, stop_server


JPEG_BASE64 = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAsICAoIBwsKCQoNDAsNERwSEQ8PESIZGhQcKSQrKigkJyctMkA3LTA9MCcnOEw5PUNFSElIKzZPVU5GVEBHSEX/2wBDAQwNDREPESESEiFFLicuRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUVFRUX/wAARCAAkADADASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAX/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFgEBAQEAAAAAAAAAAAAAAAAAAAMF/8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAwDAQACEQMRAD8AjgJM4AAAAAAAAAAAAAAAB//Z"
WEBP_BASE64 = "UklGRkQAAABXRUJQVlA4IDgAAABQAwCdASowACQAPqlUpU4mJKOiI4gAwBUJZwDMHArTD1SggAD+6NbEhvyzm/kMA6yWNEfKWQAAAA=="


def png_blob() -> bytes:
    def chunk(tag: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + tag + body + struct.pack(
            ">I", zlib.crc32(tag + body) & 0xFFFFFFFF
        )

    rows = (b"\x00" + bytes((162, 105, 73)) * 48) * 36
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 48, 36, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows))
        + chunk(b"IEND", b"")
    )


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def assert_disjoint(first: dict[str, float], second: dict[str, float], width: int) -> None:
    for rect in (first, second):
        assert rect["width"] >= 40 and rect["height"] >= 36, rect
        assert rect["x"] >= -1 and rect["x"] + rect["width"] <= width + 1, (width, rect)
    overlap_x = min(first["x"] + first["width"], second["x"] + second["width"]) - max(first["x"], second["x"])
    overlap_y = min(first["y"] + first["height"], second["y"] + second["height"]) - max(first["y"], second["y"])
    assert overlap_x <= 0 or overlap_y <= 0, (
        f"Upload controls overlap at {width}px: {first!r} vs {second!r}"
    )


def file_chooser_upload(page: Any, button: Any, file: pathlib.Path, key: str | None = None) -> None:
    button.scroll_into_view_if_needed()
    if key:
        button.focus()
        assert button.evaluate("(node) => node === document.activeElement"), "Upload button is not keyboard-focusable"
    with page.expect_file_chooser(timeout=12000) as chooser_info:
        if key:
            page.keyboard.press(key)
        else:
            button.click()
    chooser_info.value.set_files(str(file))


def verify_layout(page: Any, artifacts: pathlib.Path, widths: tuple[int, ...], checks: list[str]) -> None:
    for width in widths:
        page.set_viewport_size({"width": width, "height": 900})
        page.locator(".od-media-upload-actions").first.wait_for(timeout=15000)
        buttons = page.locator(".od-media-upload")
        assert buttons.count() == 2, "Image and video controls not independent"
        first, second = (buttons.nth(i) for i in range(2))
        expect(first).to_be_visible()
        expect(second).to_be_visible()
        first.scroll_into_view_if_needed()
        page.screenshot(path=str(artifacts / f"upload-{width}.png"), full_page=True)
        a = first.bounding_box()
        b = second.bounding_box()
        assert a is not None and b is not None, f"{width}: invisible button"
        assert_disjoint(a, b, width)
        assert first.get_attribute("type") == second.get_attribute("type") == "button"
        assert first.is_enabled() and second.is_enabled()
        # Both controls must open a native picker via pointer and keyboard at
        # each breakpoint, not merely occupy separate DOM rectangles.
        with page.expect_file_chooser(timeout=12000) as image_chooser:
            first.click()
        image_chooser.value.set_files([])
        second.focus()
        assert second.evaluate("(node) => node === document.activeElement")
        with page.expect_file_chooser(timeout=12000) as video_chooser:
            page.keyboard.press("Space")
        video_chooser.value.set_files([])
        checks.append(f"nonoverlap_pointer_and_keyboard_{width}px")


def perform(binary: pathlib.Path, artifacts: pathlib.Path, checks: list[str]) -> dict[str, Any]:
    # macOS /tmp and /var are symlinks; OpenDash deliberately rejects
    # symlinked site roots. Keep disposable roots next to private artifacts.
    with tempfile.TemporaryDirectory(prefix="m7-s16-t01-", dir=str(artifacts.parent)) as tmp:
        site = pathlib.Path(tmp) / "site"
        site.mkdir(mode=0o700)
        samples = pathlib.Path(tmp) / "samples"
        samples.mkdir()
        images: list[pathlib.Path] = []
        for ext, blob in (
            ("png", png_blob()),
            ("jpg", base64.b64decode(JPEG_BASE64)),
            ("webp", base64.b64decode(WEBP_BASE64)),
        ):
            path = samples / f"coffee-{ext}.{ext}"
            path.write_bytes(blob)
            images.append(path)
        invalid = samples / "not-image.txt"
        invalid.write_text("not a photograph", encoding="utf-8")
        oversize = samples / "oversize.png"
        oversize.write_bytes(b"not-png" + b"0" * (8 * 1024 * 1024 + 1))
        fixture = pathlib.Path(__file__).resolve().parents[1] / "fixtures/m7/video-avc-aac.mp4"
        assert fixture.exists(), f"Missing supported MP4 fixture: {fixture}"

        proc, base = launch_server(binary, site)
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1440, "height": 900})
                    page = ctx.new_page()
                    page_errors: list[str] = []
                    page.on("pageerror", lambda error: page_errors.append(str(error)))
                    api_json(ctx, "POST", base, "/auth/setup", {
                        "username": "media_agent", "password": "Local-Media-Private-12345!",
                    }, expected=201)
                    csrf = admin_login(ctx, base, "media_agent", "Local-Media-Private-12345!")
                    page.goto(base + "/admin/#/media", wait_until="domcontentloaded")
                    expect(page.locator("#od-media-title")).to_be_visible(timeout=25000)
                    expect(page.get_by_role("button", name="Tải lại")).to_be_enabled(timeout=15000)

                    # The baseline has both labels stacked on grid-row:1/grid-column:1.
                    # Capture first; a missing wrapper is a clear baseline FAIL too.
                    page.screenshot(path=str(artifacts / "baseline-or-new-desktop.png"), full_page=True)
                    controls = page.locator(".od-media-upload")
                    assert controls.count() == 2
                    a, b = controls.nth(0).bounding_box(), controls.nth(1).bounding_box()
                    assert a is not None and b is not None
                    assert_disjoint(a, b, 1440)
                    checks.append("baseline_overlap_absent")

                    verify_layout(page, artifacts, (1440, 768, 390), checks)
                    page.set_viewport_size({"width": 1440, "height": 900})
                    image_button = page.get_by_role("button", name="+ Tải ảnh lên", exact=True)
                    video_button = page.get_by_role("button", name="+ Tải video MP4", exact=True)
                    assert image_button.count() == video_button.count() == 1
                    assert page.locator("#od-media-files").get_attribute("accept") == "image/jpeg,image/png,image/webp"
                    assert page.locator("#od-media-video-file").get_attribute("accept") == "video/mp4,.mp4"
                    checks.append("distinct_button_names_and_file_accept")

                    file_chooser_upload(page, image_button, images[0])
                    expect(page.get_by_role("status").filter(has_text="Đã tải thành công 1 ảnh")).to_be_visible(timeout=25000)
                    checks.append("image_png_pointer_upload")
                    file_chooser_upload(page, image_button, images[1], key="Enter")
                    expect(page.locator(".od-media-card")).to_have_count(2, timeout=25000)
                    checks.append("image_jpeg_keyboard_enter")
                    file_chooser_upload(page, image_button, images[2], key="Space")
                    expect(page.locator(".od-media-card")).to_have_count(3, timeout=25000)
                    checks.append("image_webp_keyboard_space")

                    # Cancel file chooser without losing existing items, then
                    # reselect exactly the same file after input value was reset.
                    with page.expect_file_chooser() as chooser_info:
                        image_button.click()
                    chooser_info.value.set_files([])
                    expect(page.locator(".od-media-card")).to_have_count(3)
                    file_chooser_upload(page, image_button, images[2])
                    expect(page.get_by_role("status").filter(has_text="Đã tải thành công 1 ảnh")).to_be_visible(timeout=25000)
                    checks.append("file_chooser_cancel_same_file_reselect")

                    file_chooser_upload(page, video_button, fixture, key="Enter")
                    expect(page.get_by_role("status").filter(has_text="Đã tải thành công video")).to_be_visible(timeout=30000)
                    page.get_by_role("button", name="Video", exact=True).click()
                    expect(page.locator(".od-media-video-placeholder")).to_have_count(1, timeout=15000)
                    checks.append("mp4_separate_keyboard_upload")
                    page.get_by_role("button", name="Tất cả", exact=True).click()
                    expect(page.locator(".od-media-card")).to_have_count(4, timeout=15000)
                    page.screenshot(path=str(artifacts / "both-kinds-persisted.png"), full_page=True)

                    before = api_json(ctx, "GET", base, "/api/admin/media?limit=36&offset=0")
                    assert len(before) == 4, before
                    assert {item["kind"] for item in before} == {"image", "video"}
                    page.locator("#od-media-files").set_input_files(str(invalid))
                    expect(page.get_by_role("alert")).to_contain_text("Chỉ hỗ trợ ảnh", timeout=12000)
                    page.locator("#od-media-files").set_input_files(str(oversize))
                    expect(page.get_by_role("alert")).to_contain_text("8 MiB", timeout=12000)
                    assert len(api_json(ctx, "GET", base, "/api/admin/media?limit=36&offset=0")) == 4
                    checks.append("wrong_type_and_oversize_visible_no_ghost_media")

                    # Backend enforcement is independent of the visible button.
                    unauth = playwright.request.new_context()
                    try:
                        assert unauth.get(base + "/api/admin/media").status == 401
                        res = unauth.post(base + "/api/admin/media", headers={
                            "Origin": base, "Content-Type": "image/png", "X-File-Name": "denied.png",
                        }, data=images[0].read_bytes())
                        assert res.status in (401, 403), res.status
                    finally:
                        unauth.dispose()
                    bad_csrf = ctx.request.post(base + "/api/admin/media", headers={
                        "Origin": base, "X-CSRF-Token": "invalid", "Content-Type": "image/png",
                        "X-File-Name": "bad-csrf.png",
                    }, data=images[0].read_bytes())
                    assert bad_csrf.status in (401, 403), bad_csrf.status
                    assert len(api_json(ctx, "GET", base, "/api/admin/media?limit=36&offset=0")) == 4
                    checks.append("401_and_csrf_denied_no_ghost_media")
                    assert not page_errors, page_errors
                    checks.append("chrome_pageerrors_none")
                    chrome_version = browser.version
                finally:
                    browser.close()
        finally:
            stop_server(proc)

        # Same exact site, new process; all supported kinds are durable.
        proc, base2 = launch_server(binary, site)
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context()
                    login = admin_login(ctx, base2, "media_agent", "Local-Media-Private-12345!")
                    assert login
                    page = ctx.new_page()
                    page.goto(base2 + "/admin/#/media", wait_until="domcontentloaded")
                    expect(page.locator(".od-media-card")).to_have_count(4, timeout=20000)
                    page.get_by_role("button", name="Ảnh", exact=True).click()
                    expect(page.locator(".od-media-card")).to_have_count(3, timeout=20000)
                    page.get_by_role("button", name="Video", exact=True).click()
                    expect(page.locator(".od-media-card")).to_have_count(1, timeout=20000)
                    checks.append("restart_persists_correct_image_video_kinds")
                finally:
                    browser.close()
        finally:
            stop_server(proc)
    return {"chrome_version": chrome_version}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--artifact-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(mode=0o700, parents=True, exist_ok=True)
    checks: list[str] = []
    result: dict[str, Any] = {
        "task": "M7-S16-T01",
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary_sha256": sha256(binary),
        "platform": platform.platform(),
        "checks": checks,
        "status": "FAIL",
        "not_run": ["owner localhost:18964", "Windows/Linux", "full Beta/human dogfood"],
    }
    try:
        result.update(perform(binary, artifacts, checks))
        result["status"] = "PASS"
        return 0
    except Exception as error:
        result["error"] = str(error)
        result["traceback"] = traceback.format_exc(limit=8)
        return 1
    finally:
        result["screenshots_sha256"] = {
            path.name: sha256(path) for path in sorted(artifacts.glob("*.png"))
        }
        (artifacts / "receipt.json").write_text(
            json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    raise SystemExit(main())
