#!/usr/bin/env python3
"""Deterministic Playwright browser E2E runner for OpenDash M7 (S08-T01 / Windows & macOS)."""

import argparse
import json
import pathlib
import sys
from playwright.sync_api import sync_playwright


def launch_browser(p):
    for channel in ("chrome", "msedge", None):
        try:
            if channel:
                return p.chromium.launch(channel=channel, headless=True)
            return p.chromium.launch(headless=True)
        except Exception:
            continue
    raise RuntimeError("No Chromium/Chrome/Edge browser available for Playwright")


def main() -> int:
    parser = argparse.ArgumentParser(description="OpenDash M7 deterministic browser E2E runner")
    parser.add_argument("--base-url", required=True, help="Running server base URL, e.g. http://127.0.0.1:18766")
    parser.add_argument("--site-root", required=True, help="Isolated site root directory")
    parser.add_argument("--artifact-dir", required=True, help="Directory to write JSON receipt and artifacts")
    args = parser.parse_args()

    base_url = args.base_url.rstrip("/")
    artifact_dir = pathlib.Path(args.artifact_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    repo_root = pathlib.Path(__file__).resolve().parents[2]
    mp4_fixture = repo_root / "cms" / "fixtures" / "m7" / "video-avc-aac.mp4"
    if not mp4_fixture.exists():
        raise FileNotFoundError(f"Missing MP4 fixture: {mp4_fixture}")

    with sync_playwright() as p:
        browser = launch_browser(p)
        context = browser.new_context()
        page = context.new_page()
        page_errors = []
        console_errors = []

        page.on("pageerror", lambda err: page_errors.append(str(err)))

        def on_console(msg):
            if msg.type == "error":
                txt = msg.text
                url = (msg.location or {}).get("url", "")
                if ("401" in txt and "/auth/me" in url) or ("404" in txt and "favicon.ico" in url):
                    return
                console_errors.append(f"{txt} ({url})")

        page.on("console", on_console)

        # 1. Verify unauthenticated /api/admin/settings/general returns 401
        unauth = page.request.get(f"{base_url}/api/admin/settings/general")
        assert unauth.status == 401, f"Expected 401 before auth, got {unauth.status}"

        # 2. Setup or login admin
        setup_resp = page.request.post(
            f"{base_url}/auth/setup",
            headers={"Origin": base_url, "Content-Type": "application/json"},
            data=json.dumps({"username": "admin", "password": "StrongPass123!"}),
        )
        assert setup_resp.status in (201, 409), f"Unexpected /auth/setup status: {setup_resp.status}"
        login_resp = page.request.post(
            f"{base_url}/auth/login",
            headers={"Origin": base_url, "Content-Type": "application/json"},
            data=json.dumps({"username": "admin", "password": "StrongPass123!"}),
        )
        assert login_resp.status == 200, f"Login failed: {login_resp.status}"
        csrf = login_resp.json()["csrf_token"]

        # 3. Verify Settings, Menus, and Contact Forms views load cleanly in Admin UI
        for route in ("settings", "taxonomy", "themes", "media", "builder"):
            page.goto(f"{base_url}/admin#/{route}", wait_until="networkidle")

        # 4. Upload deterministic MP4 fixture via Media Library UI
        page.goto(f"{base_url}/admin#/media", wait_until="networkidle")
        page.locator("#od-media-video-file").set_input_files(str(mp4_fixture))
        page.locator(".od-media-video-placeholder").first.wait_for(timeout=10000)
        page.get_by_role("button", name="Video", exact=True).click()
        page.locator(".od-media-video-placeholder").first.click()
        page.locator("video").first.wait_for(timeout=5000)

        # 5. Add core/video in Builder, pick uploaded MP4, and save homepage
        page.goto(f"{base_url}/admin#/builder", wait_until="networkidle")
        page.locator("#builder-element-type").select_option("core/video")
        page.get_by_role("button", name="+ Section").click()
        page.locator("button[title='Thêm row']").first.click()
        page.locator("button[title='Thêm column']").first.click()
        page.locator("button[title='Thêm element']").first.click()
        page.get_by_role("button", name="Chọn video MP4 trong thư viện").click()
        page.locator("div[role='dialog'] button:has-text('MP4')").first.click()
        page.locator("div[role='dialog']").get_by_role("button", name="Dùng ảnh đã chọn").click()
        page.get_by_role("button", name="Lưu homepage").click()
        page.locator("text=Đã lưu và cập nhật website công khai.").wait_for(timeout=10000)

        # 6. Verify 412 stale If-Match on homepage PUT
        hp = page.request.get(f"{base_url}/api/admin/builder/homepage")
        assert hp.status == 200
        doc = hp.json()["document"]
        stale = page.request.put(
            f"{base_url}/api/admin/builder/homepage",
            headers={
                "Origin": base_url,
                "Content-Type": "application/json",
                "x-csrf-token": csrf,
                "If-Match": '"v1"',
            },
            data=json.dumps(doc),
        )
        assert stale.status == 412, f"Expected 412 stale If-Match, got {stale.status}"

        # 7. Visit public homepage, verify <video> and Range 206 seek
        page.goto(f"{base_url}/", wait_until="networkidle")
        pub_video = page.locator("video")
        assert pub_video.count() >= 1, "Expected <video> on public homepage"
        src = pub_video.first.get_attribute("src")
        assert src and "/media/" in src and src.endswith("/file"), f"Unexpected video src: {src}"

        r206 = page.request.get(f"{base_url}{src}", headers={"Range": "bytes=0-63"})
        assert r206.status == 206, f"Expected 206 Partial Content, got {r206.status}"
        assert r206.headers.get("accept-ranges") == "bytes"

        # 8. Configure public_origin and verify sitemap.xml and robots.txt
        pub_get = page.request.get(f"{base_url}/api/admin/settings/public")
        assert pub_get.status == 200
        pub_etag = pub_get.headers.get("etag")
        pub_put = page.request.put(
            f"{base_url}/api/admin/settings/public",
            headers={
                "Origin": base_url,
                "Content-Type": "application/json",
                "x-csrf-token": csrf,
                "If-Match": pub_etag,
            },
            data=json.dumps({
                "public_origin": base_url,
                "content_language": "vi",
                "indexing_enabled": True,
            }),
        )
        assert pub_put.status == 200, f"Expected 200 on public settings, got {pub_put.status}"
        assert page.request.get(f"{base_url}/sitemap.xml").status == 200
        assert page.request.get(f"{base_url}/robots.txt").status == 200

        assert not page_errors, f"Page errors: {page_errors}"
        assert not console_errors, f"Console errors: {console_errors}"

        receipt = {
            "ok": True,
            "base_url": base_url,
            "site_root": args.site_root,
            "video_src": src,
            "range_status": r206.status,
            "page_errors": page_errors,
            "console_errors": console_errors,
        }
        (artifact_dir / "m7_browser_e2e_receipt.json").write_text(
            json.dumps(receipt, indent=2), encoding="utf-8"
        )
        print(json.dumps(receipt))
        browser.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
