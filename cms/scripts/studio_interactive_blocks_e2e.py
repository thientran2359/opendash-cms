#!/usr/bin/env python3
"""Playwright Chrome E2E acceptance suite for Spec A3 interactive & starter blocks."""
from __future__ import annotations

import argparse
import json
import pathlib
import platform
import tempfile
import traceback

from playwright.sync_api import expect, sync_playwright
from e2e_helpers import admin_login, api_json, launch_server, make_png, stop_server

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_BIN = REPO_ROOT / "cms/target/debug/opendash"
VIDEO_FIXTURE = REPO_ROOT / "cms/fixtures/m7/video-avc-aac.mp4"


def upload_png(ctx, base: str, csrf: str, name: str, seed: int) -> str:
    res = ctx.request.post(
        f"{base}/api/admin/media",
        headers={"X-CSRF-Token": csrf, "Origin": base, "Content-Type": "image/png", "X-File-Name": name},
        data=make_png(seed=seed, width=4, height=4),
    )
    assert res.status == 201, (res.status, res.text())
    return res.json()["id"]


def upload_video(ctx, base: str, csrf: str) -> str:
    data = VIDEO_FIXTURE.read_bytes()
    res = ctx.request.post(
        f"{base}/api/admin/media/video",
        headers={"X-CSRF-Token": csrf, "Origin": base, "Content-Type": "video/mp4"},
        data=data,
    )
    assert res.status == 201, (res.status, res.text())
    return res.json()["id"]


def create_and_publish_page(ctx, base: str, csrf: str, title: str, slug: str, doc: dict) -> int:
    created = api_json(
        ctx, "POST", base, "/api/admin/content-types/page/entries",
        {"title": title, "slug": slug, "values": {}}, csrf, expected=201,
    )
    pid = created["id"]
    api_json(
        ctx, "PUT", base, f"/api/admin/entries/page/{pid}/builder",
        doc, csrf, {"If-Match": '"v1"'}, expected=200,
    )
    api_json(
        ctx, "POST", base, f"/api/admin/content-types/page/entries/{pid}/publication",
        {"status": "published"}, csrf, {"If-Match": '"v2"'}, expected=200,
    )
    return pid


def select_tree_path(page, node_ids: list[str]) -> None:
    for nid in node_ids[:-1]:
        item = page.locator(f'button[role="treeitem"][data-node-id="{nid}"]')
        expect(item).to_be_visible(timeout=10000)
        if item.get_attribute("aria-expanded") == "false":
            item.locator("xpath=preceding-sibling::button[1]").click()
    target = page.locator(f'button[role="treeitem"][data-node-id="{node_ids[-1]}"]')
    expect(target).to_be_visible(timeout=10000)
    target.click()


def run_suite(binary: pathlib.Path, checks: list[str]) -> dict:
    with tempfile.TemporaryDirectory(prefix="studio-a3-e2e-") as tmpdir:
        root = pathlib.Path(tmpdir).resolve() / "site"
        root.mkdir(mode=0o700)
        server, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1280, "height": 840})
                    ctx.grant_permissions(["clipboard-read", "clipboard-write"], origin=base)
                    page = ctx.new_page()
                    page_errors: list[str] = []
                    bad_responses: list[tuple[int, str]] = []
                    csp_violations: list[str] = []
                    page.on("pageerror", lambda err: page_errors.append(str(err)))
                    page.on("response", lambda r: bad_responses.append((r.status, r.url)) if r.status >= 400 and not r.url.endswith("/favicon.ico") else None)
                    page.add_init_script("document.addEventListener('securitypolicyviolation', e => { window.__odCspV = (window.__odCspV || []); window.__odCspV.push(e.violatedDirective + ':' + e.blockedURI); });")

                    admin_pw = "A3-Interactive-Pass-12345!"
                    api_json(ctx, "POST", base, "/auth/setup", {"username": "a3_admin", "password": admin_pw}, expected=201)
                    csrf = admin_login(ctx, base, "a3_admin", admin_pw, cms_locale="en")

                    img1 = upload_png(ctx, base, csrf, "s1.png", 17)
                    img2 = upload_png(ctx, base, csrf, "s2.png", 93)
                    img3 = upload_png(ctx, base, csrf, "s3.png", 171)
                    vid1 = upload_video(ctx, base, csrf)
                    form = api_json(
                        ctx, "POST", base, "/api/admin/contact/forms",
                        {"slug": "a3-lead", "title": "Lead Form", "schema": {"version": 1, "fields": [
                            {"key": "email", "label": "Email", "kind": "email", "required": True},
                            {"key": "message", "label": "Message", "kind": "textarea", "required": True},
                        ]}},
                        csrf, expected=201,
                    )
                    form_id = form["id"]

                    # 1. Starter-only marketing page -> 0 blocks JS, form-action 'none' (checked before header drawer override)
                    starter_doc = {"schema_version": 1, "blocks": [{"id": "mkt-sec", "block_id": "core/section", "children": [
                        {"id": "mkt-ib", "block_id": "opendash-starter/icon-box", "props": {"title": "Fast Core"}},
                        {"id": "mkt-tm", "block_id": "opendash-starter/testimonial", "props": {"quote": "Great CMS", "author": "Minh Anh", "rating": 5}},
                        {"id": "mkt-sb", "block_id": "opendash-starter/stats-bar", "props": {"columns": {"desktop": "3", "mobile": "2"}, "items": [{"value": "100+", "label": "Sites"}, {"value": "99%", "label": "Uptime"}]}},
                        {"id": "mkt-ls", "block_id": "opendash-starter/logo-strip", "props": {"columns": {"desktop": "4", "mobile": "3"}, "items": [{"name": "Partner One"}, {"name": "Partner Two"}]}},
                        {"id": "mkt-fl", "block_id": "opendash-starter/feature-list", "props": {"columns": {"desktop": "2", "mobile": "1"}, "items": [{"title": "CSP Ready"}]}},
                    ]}]}
                    create_and_publish_page(ctx, base, csrf, "Starter Showcase", "starter-showcase", starter_doc)
                    st_resp = ctx.request.get(f"{base}/starter-showcase")
                    assert st_resp.status == 200 and "/assets/opendash-blocks.js" not in st_resp.text()
                    assert "form-action 'none'" in st_resp.headers.get("content-security-policy", "")
                    page.goto(f"{base}/starter-showcase", wait_until="domcontentloaded")
                    for cls in ["od-starter-icon-box", "od-starter-testimonial", "od-starter-stats-bar", "od-starter-logo-strip", "od-starter-feature-list"]:
                        expect(page.locator(f".{cls}")).to_be_visible()
                    assert page.locator(".od-star-filled").count() == 5
                    checks.append("starter_5_marketing_blocks_zero_js_and_csp")

                    # Header override with left drawer popup (tests display: contents & part:header)
                    api_json(
                        ctx, "PUT", base, "/api/admin/template-overrides",
                        {"target": {"kind": "part", "part_key": "header"}, "document": {"schema_version": 1, "blocks": [{
                            "id": "hdr-sec", "block_id": "core/section",
                            "children": [{
                                "id": "hdr-stk", "block_id": "core/stack", "props": {"direction": "row"},
                                "children": [
                                    {"id": "hdr-brand", "block_id": "core/site-brand"},
                                    {"id": "hdr-btn", "block_id": "core/button", "props": {"cta": {"label": "Open Drawer", "url": "#mobile-drawer", "new_tab": False, "nofollow": False}}},
                                    {"id": "hdr-drw", "block_id": "core/popup", "props": {"popup_id": "mobile-drawer", "position": "left", "full_height": True, "backdrop_style": "dark"}, "children": [
                                        {"id": "drw-stk", "block_id": "core/stack", "children": [
                                            {"id": "drw-lnk", "block_id": "core/button", "props": {"cta": {"label": "Jump Target", "url": "#section-target", "new_tab": False, "nofollow": False}}}
                                        ]}
                                    ]},
                                ],
                            }],
                        }]}},
                        csrf, expected=201,
                    )

                    # Secondary target page for cross-page hash link non-hijack test
                    create_and_publish_page(ctx, base, csrf, "Other Page", "other-page", {"schema_version": 1, "blocks": [
                        {"id": "oth-sec", "block_id": "core/section", "children": [{"id": "oth-h", "block_id": "core/heading", "props": {"level": "h1", "text": "Other Destination"}}]}
                    ]})

                    # 2. Interactive showcase page (no contact-form so CSP keeps form-action 'none')
                    inter_doc = {"schema_version": 1, "blocks": [
                        {"id": "sec-inter", "block_id": "core/section", "children": [
                            {"id": "tbs-main", "block_id": "core/tabs", "props": {"style": "pills"}, "children": [
                                {"id": "tp-one", "block_id": "core/tab-panel", "props": {"title": "Overview"}, "children": [
                                    {"id": "tx-one", "block_id": "core/text", "props": {"content": "<p>Overview content</p>"}}
                                ]},
                                {"id": "tp-two", "block_id": "core/tab-panel", "props": {"title": "Specs"}, "children": [
                                    {"id": "tbs-nest", "block_id": "core/tabs", "children": [
                                        {"id": "tp-n1", "block_id": "core/tab-panel", "props": {"title": "Inner A"}, "children": [{"id": "tx-n1", "block_id": "core/text", "props": {"content": "<p>Inner A body</p>"}}]},
                                        {"id": "tp-n2", "block_id": "core/tab-panel", "props": {"title": "Inner B"}, "children": [{"id": "tx-n2", "block_id": "core/text", "meta": {"html_id": "deep-anchor"}, "props": {"content": "<p>Inner B deep body</p>"}}]},
                                    ]},
                                ]},
                            ]},
                            {"id": "sld-hero", "block_id": "core/slider", "props": {"slides_per_view": {"desktop": "1", "tablet": "1", "mobile": "1"}, "loop": True, "autoplay_seconds": 3}, "children": [
                                {"id": "sl-one", "block_id": "core/slide", "children": [{"id": "im-sl1", "block_id": "core/image", "props": {"image": img1, "alt": "Slide 1 img"}}]},
                                {"id": "sl-two", "block_id": "core/slide", "children": [{"id": "im-sl2", "block_id": "core/image", "props": {"image": img2, "alt": "Slide 2 img"}}]},
                                {"id": "sl-thr", "block_id": "core/slide", "children": [{"id": "hd-sl3", "block_id": "core/heading", "props": {"level": "h3", "text": "Slide Three"}}]},
                            ]},
                            {"id": "gal-main", "block_id": "core/gallery", "props": {"images": [img1, img2, img3], "captions": "Alpha\nBeta\nGamma", "lightbox": True, "show_captions": True}},
                            {"id": "shr-main", "block_id": "core/social-share", "props": {"show_copy_link": True, "custom_url": ""}},
                            {"id": "btn-vid", "block_id": "core/button", "props": {"cta": {"label": "Watch Video", "url": "#video-modal", "new_tab": False, "nofollow": False}}},
                            {"id": "btn-lck", "block_id": "core/button", "props": {"cta": {"label": "Locked Popup", "url": "#locked-modal", "new_tab": False, "nofollow": False}}},
                            {"id": "btn-oth", "block_id": "core/button", "props": {"cta": {"label": "Other Page Drawer", "url": "/other-page#mobile-drawer", "new_tab": False, "nofollow": False}}},
                            {"id": "pop-vid", "block_id": "core/popup", "props": {"popup_id": "video-modal", "position": "center", "open_on_hash": True}, "children": [
                                {"id": "vid-in-pop", "block_id": "core/video", "props": {"video": vid1, "controls": True, "muted": True}},
                                {"id": "img-in-pop", "block_id": "core/image", "props": {"image": img3, "alt": "Popup pic"}},
                            ]},
                            {"id": "pop-lck", "block_id": "core/popup", "props": {"popup_id": "locked-modal", "position": "right", "close_on_backdrop": False, "show_close_button": False}, "children": [
                                {"id": "lck-txt", "block_id": "core/text", "props": {"content": "<p>Must use close btn</p>"}}
                            ]},
                        ]},
                        {"id": "sec-target", "block_id": "core/section", "meta": {"html_id": "section-target"}, "children": [
                            {"id": "tgt-hd", "block_id": "core/heading", "props": {"level": "h2", "text": "Target Section"}}
                        ]},
                    ]}
                    inter_id = create_and_publish_page(ctx, base, csrf, "Interactive Showcase", "interactive-showcase", inter_doc)

                    # Check /interactive-showcase
                    int_resp = ctx.request.get(f"{base}/interactive-showcase")
                    assert int_resp.status == 200
                    assert "form-action 'none'" in int_resp.headers.get("content-security-policy", "")
                    page.goto(f"{base}/interactive-showcase", wait_until="domcontentloaded")

                    # Drawer in part:header uses display: contents & closes on internal #section link
                    drw_host = page.locator('[data-od-popup-host][data-od-popup-id="mobile-drawer"]')
                    assert drw_host.evaluate("el => getComputedStyle(el).display") == "contents"
                    drw_dlg = page.locator('dialog[data-od-popup-dialog="mobile-drawer"]')
                    page.locator('a[href="#mobile-drawer"]').first.click()
                    expect(drw_dlg).to_have_attribute("open", "")
                    drw_dlg.locator('a[href="#section-target"]').click()
                    expect(drw_dlg).not_to_have_attribute("open", "")
                    checks.append("popup_header_drawer_display_contents_and_anchor_close")

                    # Tabs: click, ArrowLeft, Home/End
                    main_tabs = page.locator("#tbs-main, [data-od-tabs-root]").first
                    tab0 = main_tabs.locator('[role="tab"][data-od-tab-index="0"]').first
                    tab1 = main_tabs.locator('[role="tab"][data-od-tab-index="1"]').first
                    tab1.click()
                    expect(tab1).to_have_attribute("aria-selected", "true")
                    expect(main_tabs.locator('[data-od-panel-index="1"]').first).to_be_visible()
                    tab1.press("ArrowLeft")
                    expect(tab0).to_have_attribute("aria-selected", "true")
                    tab0.press("End")
                    expect(tab1).to_have_attribute("aria-selected", "true")
                    tab1.press("Home")
                    expect(tab0).to_have_attribute("aria-selected", "true")
                    checks.append("tabs_click_and_keyboard_navigation")

                    # Slider: arrows, dots, fetchpriority="low" on slide >= 2, autoplay toggle, reduced-motion
                    sld = page.locator("[data-od-slider-root]").first
                    assert sld.locator('[data-od-slide-index="1"] img').first.get_attribute("fetchpriority") == "low"
                    sld.locator('[data-od-slide-dir="next"]').click()
                    expect(sld.locator("[data-od-slider-status]")).to_contain_text("2")
                    toggle = sld.locator("[data-od-slider-toggle]")
                    toggle.click()
                    expect(toggle).to_have_attribute("aria-pressed", "true")
                    page.emulate_media(reduced_motion="reduce")
                    page.reload(wait_until="domcontentloaded")
                    expect(page.locator("[data-od-slider-root]").first.locator("[data-od-slider-toggle]")).to_have_attribute("aria-pressed", "true")
                    page.emulate_media(reduced_motion="no-preference")
                    checks.append("slider_nav_fetchpriority_autoplay_and_reduced_motion")

                    # Gallery Lightbox: open, ArrowRight, Esc close + focus return, backdrop click close
                    first_lb_trig = page.locator("[data-od-lb-trigger]").first
                    lb_dlg = page.locator("dialog.od-lightbox-dialog").first
                    first_lb_trig.click()
                    assert lb_dlg.evaluate("el => el.open") is True
                    expect(lb_dlg.locator("[data-od-lb-counter]")).to_have_text("1 / 3")
                    lb_dlg.locator("[data-od-lb-next]").click()
                    expect(lb_dlg.locator("[data-od-lb-counter]")).to_have_text("2 / 3")
                    page.keyboard.press("Escape")
                    expect(lb_dlg).not_to_have_attribute("open", "")
                    assert first_lb_trig.evaluate("el => document.activeElement === el")
                    first_lb_trig.click()
                    page.mouse.click(5, 5)
                    expect(lb_dlg).not_to_have_attribute("open", "")
                    checks.append("gallery_lightbox_nav_esc_backdrop_and_focus")

                    # Social Share: origin hydration + copy link status outside button
                    fb_href = page.locator("a[data-od-share-href]").first.get_attribute("href")
                    assert fb_href and fb_href.startswith("https://") and "%3A%2F%2F127.0.0.1" in fb_href
                    page.locator("button[data-od-copy-url]").click()
                    expect(page.locator(".od-copy-tip")).not_to_be_empty()
                    checks.append("social_share_origin_and_copy_status")

                    # Video Popup: open via #id, play, close -> pauses video; fetchpriority="low" on img inside popup
                    vid_dlg = page.locator('dialog[data-od-popup-dialog="video-modal"]')
                    assert vid_dlg.locator("img").first.get_attribute("fetchpriority") == "low"
                    page.locator('a[href="#video-modal"]').click()
                    expect(vid_dlg).to_have_attribute("open", "")
                    vid_dlg.evaluate("async el => { const v = el.querySelector('video'); if (v) await v.play().catch(() => {}); }")
                    vid_dlg.locator("[data-od-popup-close]").click()
                    expect(vid_dlg).not_to_have_attribute("open", "")
                    assert vid_dlg.evaluate("el => !el.open && el.querySelector('video').paused") is True
                    checks.append("popup_video_modal_pauses_on_close_and_fetchpriority")

                    # Locked backdrop popup
                    lck_dlg = page.locator('dialog[data-od-popup-dialog="locked-modal"]')
                    page.locator('a[href="#locked-modal"]').click()
                    expect(lck_dlg).to_have_attribute("open", "")
                    page.mouse.click(5, 5)
                    assert lck_dlg.evaluate("el => el.open") is True
                    lck_dlg.locator("[data-od-popup-close]").click()
                    expect(lck_dlg).not_to_have_attribute("open", "")
                    checks.append("popup_close_on_backdrop_false_enforced")

                    # Cross-page hash link NOT hijacked + open_on_hash + nested #deep-anchor tab reveal
                    page.locator('a[href="/other-page#mobile-drawer"]').click()
                    expect(page).to_have_url(f"{base}/other-page#mobile-drawer")
                    page.goto(f"{base}/interactive-showcase#deep-anchor", wait_until="domcontentloaded")
                    expect(page.locator("#deep-anchor")).to_be_visible()
                    page.goto(f"{base}/interactive-showcase#video-modal", wait_until="domcontentloaded")
                    assert page.locator('dialog[data-od-popup-dialog="video-modal"]').evaluate("el => el.open") is True
                    checks.append("popup_cross_page_open_on_hash_and_deep_tab_hash")

                    # 3. Contact form inside Popup submission
                    form_doc = {"schema_version": 1, "blocks": [{"id": "cf-sec", "block_id": "core/section", "children": [
                        {"id": "cf-btn", "block_id": "core/button", "props": {"cta": {"label": "Open Form", "url": "#lead-popup", "new_tab": False, "nofollow": False}}},
                        {"id": "cf-pop", "block_id": "core/popup", "props": {"popup_id": "lead-popup"}, "children": [
                            {"id": "cf-blk", "block_id": "core/contact-form", "props": {"form_id": str(form_id), "submit_label": "Send Lead"}}
                        ]},
                    ]}]}
                    create_and_publish_page(ctx, base, csrf, "Contact Popup", "contact-popup-page", form_doc)
                    page.goto(f"{base}/contact-popup-page", wait_until="domcontentloaded")
                    cf_dlg = page.locator('dialog[data-od-popup-dialog="lead-popup"]')
                    page.locator('a[href="#lead-popup"]').click()
                    assert cf_dlg.evaluate("el => el.open") is True
                    cf_dlg.locator('input[name="email"]').fill("lead@example.com")
                    cf_dlg.locator('textarea[name="message"]').fill("Need demo")
                    cf_dlg.locator('button[type="submit"]').click()
                    page.wait_for_load_state("domcontentloaded")
                    expect(page.locator("body")).not_to_contain_text("404")
                    checks.append("popup_contact_form_submit")

                    csp_violations.extend(page.evaluate("() => window.__odCspV || []"))
                    assert not bad_responses, bad_responses
                    bad_responses.clear()

                    # 4. Studio Preview reveal for nested tab, slider, & popup without focus theft or blocks JS
                    page.goto(f"{base}/admin/#/studio/entry/page/{inter_id}", wait_until="domcontentloaded")
                    expect(page.get_by_test_id("studio-builder-core")).to_be_visible(timeout=20000)
                    frame = page.frame_locator("#canvas")
                    expect(frame.locator('[data-od-popup-host][data-od-popup-id="video-modal"]')).to_be_attached(timeout=15000)
                    assert frame.locator('script[src*="opendash-blocks.js"]').count() == 0

                    select_tree_path(page, ["sec-inter", "tbs-main", "tp-two", "tbs-nest", "tp-n2", "tx-n2"])
                    expect(frame.locator('[data-node-id="tp-two"]')).not_to_have_attribute("hidden", "until-found")
                    expect(frame.locator('[data-node-id="tp-n2"]')).not_to_have_attribute("hidden", "until-found")

                    select_tree_path(page, ["sec-inter", "sld-hero", "sl-two", "im-sl2"])
                    expect(frame.locator('[data-node-id="im-sl2"]')).to_have_attribute("data-od-selected", "1")

                    select_tree_path(page, ["sec-inter", "pop-vid", "img-in-pop"])
                    pop_host_prev = frame.locator('[data-od-popup-host][data-od-popup-id="video-modal"]')
                    expect(pop_host_prev).to_have_attribute("data-od-popup-preview-open", "1")
                    assert pop_host_prev.locator("dialog").evaluate("el => el.hasAttribute('open')") is True
                    assert page.evaluate("() => !!(document.activeElement && document.activeElement.id !== 'canvas')")

                    select_tree_path(page, ["sec-inter"])
                    expect(pop_host_prev).not_to_have_attribute("data-od-popup-preview-open", "1")
                    assert pop_host_prev.locator("dialog").evaluate("el => el.hasAttribute('open')") is False
                    checks.append("studio_preview_reveal_and_close_popup_no_focus_theft")

                    csp_violations.extend(page.evaluate("() => window.__odCspV || []"))
                    assert not page_errors, page_errors
                    assert not csp_violations, csp_violations
                    # Pre-existing previewMediaRewrite skips <video src> inside opaque allow-scripts iframe (401 on /api/admin/media/{vid1}/file)
                    unexpected_http = [(s, u) for (s, u) in bad_responses if not (s == 401 and f"/api/admin/media/{vid1}/" in u)]
                    assert not unexpected_http, unexpected_http
                    checks.append("zero_public_http_and_csp_errors")
                    return {"chrome_version": browser.version}
                finally:
                    browser.close()
        finally:
            stop_server(server)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, default=DEFAULT_BIN)
    args = parser.parse_args()
    checks: list[str] = []
    result = {"task": "studio_interactive_blocks_e2e", "platform": platform.platform(), "checks": checks, "status": "FAIL"}
    try:
        binary = args.binary.resolve(strict=True)
        result.update(run_suite(binary, checks))
        result["status"] = "PASS"
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0
    except Exception as error:
        result["error"] = str(error)
        result["traceback"] = traceback.format_exc(limit=8)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
