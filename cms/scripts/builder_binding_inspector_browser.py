"""M4-S05-T02: real Chrome Builder Field Binding Inspector E2E on disposable SQLite.

Run after npm Admin build then cargo build (the executable embeds Admin assets):
  python cms/scripts/builder_binding_inspector_browser.py --binary <BINARY> --browser <CHROME>
No production database, sessions or uploads are accessed.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from tempfile import TemporaryDirectory

from playwright.sync_api import expect, sync_playwright

from builder_editor_browser import element_at, new_tree, running, save, state
from content_entries_smoke import api
from content_types_smoke import login, request
from media_smoke import raw_request, upload_image


def checked(response, expected: int, label: str):
    assert response[0] == expected, (label, response[0], response[1][:500])
    return response


def create(base: str, auth: dict, key: str, slug: str, title: str, values: dict) -> dict:
    return json.loads(checked(api(base, "POST", f"/api/admin/content-types/{key}/entries", {
        "slug": slug, "title": title, "values": values,
    }, auth), 201, f"create {key}/{slug}")[1])


def add_element(page, element_type: str):
    page.get_by_label("Element mới").select_option(element_type)
    page.get_by_role("treeitem", name=re.compile(r"Column ")).first.click()
    page.get_by_role("button", name="Thêm element vào Column").click()
    return page.locator('[role="treeitem"][data-node-id^="element-"]').last.get_attribute("data-node-id")


def choose_binding(page, type_key: str, slug: str, source: str, target: str | None = None):
    if target is not None:
        page.get_by_label("Binding target").select_option(target)
    type_picker = page.get_by_label("Binding content type")
    expect(type_picker.locator(f'option[value="{type_key}"]')).to_have_count(1)
    type_picker.select_option(type_key)
    entry_picker = page.get_by_label("Binding entry")
    expect(entry_picker.locator(f'option[value="{slug}"]')).to_have_count(1)
    entry_picker.select_option(slug)
    field_picker = page.get_by_label("Binding source field")
    field_picker.select_option(source)
    expect(page.get_by_role("button", name="Áp dụng binding")).to_be_enabled()
    page.get_by_role("button", name="Áp dụng binding").click()
    expect(page.get_by_text("Field Binding hiện có")).to_be_visible()


def check(browser_path: str, binary: Path):
    with TemporaryDirectory(prefix="opendash-m4-s05-t02-inspector-") as folder:
        database = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(binary, database) as base:
            checked(request(base, "/auth/setup", {
                "username": "bindingadmin", "password": "password123",
            }), 201, "admin setup")
            admin = login(base, "bindingadmin", "password123")
            checked(request(base, "/api/admin/users", {
                "username": "bindingeditor", "password": "password123", "role": "editor",
            }, admin), 201, "editor user")
            asset_id = upload_image(base, admin, seed=74)["id"]
            checked(api(base, "POST", "/api/admin/content-types", {
                "key": "projects", "label": "Projects",
                "fields": [
                    {"key": "summary", "label": "Summary", "type": "short_text"},
                    {"key": "body", "label": "Body", "type": "rich_text"},
                    {"key": "cover", "label": "Cover", "type": "image"},
                    {"key": "link", "label": "Link", "type": "short_text"},
                    {"key": "count", "label": "Count", "type": "number"},
                ],
            }, admin), 201, "CPT")
            project = create(base, admin, "projects", "showcase", "Public project", {
                "summary": "Learn & build",
                "body": "<script>alert('xss')</script><b>Rich</b>",
                "cover": asset_id,
                "link": "/safe",
                "count": 12,
            })
            post = create(base, admin, "post", "draft-post", "DRAFT SECRET POST", {})
            page_entry = create(base, admin, "page", "draft-page", "DRAFT PRIVATE PAGE", {})
            assert project["publication_status"] == "published"
            assert post["publication_status"] == page_entry["publication_status"] == "draft"
            # A published CPT already authorizes its own image; binding must
            # neither widen that ACL nor expose any unpublished Post/Page.
            assert raw_request(base, "GET", f"/media/{asset_id}/file")[0] == 200

            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(executable_path=browser_path)
                context = browser.new_context(viewport={"width": 1440, "height": 900})
                ui = context.new_page()
                errors = []
                ui.on("pageerror", lambda error: errors.append(str(error)))
                ui.goto(base + "/admin/#/builder")
                ui.locator("#auth-username").fill("bindingadmin")
                ui.locator("#auth-password").fill("password123")
                ui.locator("#auth-login").click()
                expect(ui.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở")
                new_tree(ui)

                # Heading ← title of a draft Post, preview works but public may not see it.
                heading_id = ui.locator('[role="treeitem"][data-node-id^="element-"]').last.get_attribute("data-node-id")
                choose_binding(ui, "post", "draft-post", "title")
                assert state(ui, base)["document"]["sections"] == [], "Inspector changes are draft-only until Save"
                ui.get_by_role("button", name="Xem trước").click()
                expect(ui.frame_locator("#canvas").locator("body")).to_contain_text("DRAFT SECRET POST")
                assert "DRAFT SECRET POST" not in ui.request.get(base + "/").text()
                ui.get_by_role("button", name="Canvas cấu trúc").click()
                save(ui)
                assert element_at(state(ui, base)["document"])["props"]["binding"] == {
                    "type_key": "post", "entry_slug": "draft-post",
                    "source": {"kind": "title"}, "target": "text",
                }
                assert "DRAFT SECRET POST" not in ui.request.get(base + "/").text()

                # Post/Page/CPT and compatibility matrix across all four core elements.
                text_id = add_element(ui, "core/text")
                choose_binding(ui, "projects", "showcase", "field:body")
                fields = ui.get_by_label("Binding source field")
                expect(fields.locator('option[value="field:cover"]')).to_have_attribute("disabled", "")
                expect(fields.locator('option[value="field:count"]')).to_be_enabled()

                image_id = add_element(ui, "core/image")
                choose_binding(ui, "projects", "showcase", "field:cover", "media_id")
                expect(ui.get_by_label("Binding source field").locator('option[value="field:body"]')).to_have_attribute("disabled", "")
                expect(ui.get_by_label("Binding source field").locator('option[value="title"]')).to_have_attribute("disabled", "")
                ui.get_by_label("Binding target").select_option("alt")
                ui.get_by_label("Binding source field").select_option("title")
                expect(ui.get_by_text("Binding cho alt yêu cầu chọn Media ID tĩnh")).to_be_visible()
                expect(ui.get_by_role("button", name="Áp dụng binding")).to_be_disabled()

                button_id = add_element(ui, "core/button")
                choose_binding(ui, "projects", "showcase", "field:link", "url")
                expect(ui.get_by_label("Binding source field").locator('option[value="title"]')).to_have_attribute("disabled", "")
                expect(ui.get_by_label("Binding source field").locator('option[value="field:body"]')).to_have_attribute("disabled", "")

                page_text_id = add_element(ui, "core/text")
                choose_binding(ui, "page", "draft-page", "title")
                ui.get_by_role("button", name="Xem trước").click()
                preview_markup = ui.locator("#canvas").get_attribute("srcdoc") or ""
                assert "DRAFT PRIVATE PAGE" in preview_markup
                assert "Rich" in preview_markup
                assert "<script>alert('xss')</script>" not in preview_markup
                assert f"data:image/png;base64," in preview_markup, "private media must resolve only in Preview"
                ui.get_by_role("button", name="Canvas cấu trúc").click()
                save(ui)
                saved = state(ui, base)
                bindings = [el["props"].get("binding") for el in
                    saved["document"]["sections"][0]["children"][0]["children"][0]["children"]]
                assert len(bindings) == 5 and all(bindings), bindings
                public = ui.request.get(base + "/").text()
                assert "Learn &amp; build" not in public  # Text binds rich body, not summary.
                assert "DRAFT SECRET POST" not in public and "DRAFT PRIVATE PAGE" not in public
                assert "<script>alert('xss')</script>" not in public
                assert "/safe" in public
                assert f"/media/{asset_id}/file" in public
                assert raw_request(base, "GET", f"/media/{asset_id}/file")[0] == 200

                # Switching to an existing bound node and editing static fields preserves binding.
                ui.reload()
                expect(ui.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở")
                ui.get_by_role("treeitem", name=re.compile(re.escape(text_id))).click()
                expect(ui.get_by_text("Field Binding hiện có")).to_be_visible()
                static_text = ui.locator('form[aria-label="Thuộc tính phần tử"] textarea')
                assert static_text.count() == 1, ui.locator('section[aria-label="Inspector"]').inner_text()
                static_text.fill("Keep static fallback")
                save(ui)
                retained = state(ui, base)["document"]
                assert retained["sections"][0]["children"][0]["children"][0]["children"][1]["props"]["binding"] == bindings[1]

                # Missing bound entry must show a non-sensitive diagnostic and never erase it.
                updated = json.loads(checked(api(base, "PUT", f"/api/admin/content-types/projects/entries/{project['id']}", {
                    "title": project["title"], "slug": "renamed-showcase", "values": project["values"],
                }, {**admin, "If-Match": f'\"v{project["version"]}\"'}), 200, "rename source")[1])
                assert updated["slug"] == "renamed-showcase"
                ui.reload()
                ui.get_by_role("treeitem", name=re.compile(re.escape(text_id))).click()
                expect(ui.get_by_text("Không tìm thấy entry trong danh sách đã tải.", exact=False)).to_be_visible()
                expect(ui.get_by_role("button", name="Áp dụng binding")).to_be_disabled()
                assert state(ui, base)["document"]["sections"][0]["children"][0]["children"][0]["children"][1]["props"]["binding"] == bindings[1]

                # Simulated lookup error must keep the selected slug and old document unchanged.
                ui.get_by_role("treeitem", name=re.compile(re.escape(image_id))).click()
                ui.get_by_label("Mô tả ảnh (alt)").fill("Uncommitted alt survives lookup")
                ui.get_by_role("treeitem", name=re.compile(re.escape(text_id))).click()
                ui.route("**/api/admin/content-types/projects/entries?*", lambda route:
                    route.fulfill(status=503, body='{"error":"temporary"}', content_type="application/json"))
                ui.get_by_role("treeitem", name=re.compile(re.escape(image_id))).click()
                expect(ui.get_by_role("alert").filter(has_text="Không thể tải entry")).to_be_visible()
                expect(ui.get_by_label("Binding entry")).to_have_value("showcase")
                expect(ui.get_by_label("Mô tả ảnh (alt)")).to_have_value("Uncommitted alt survives lookup")
                ui.unroute("**/api/admin/content-types/projects/entries?*")
                ui.get_by_role("button", name="Thử tải lại nguồn").click()
                expect(ui.get_by_label("Binding entry").locator('option[value="renamed-showcase"]')).to_have_count(1)
                expect(ui.get_by_label("Binding entry")).to_have_value("showcase")
                assert state(ui, base)["document"]["sections"][0]["children"][0]["children"][0]["children"][2]["props"]["binding"] == bindings[2]
                assert state(ui, base)["document"]["sections"][0]["children"][0]["children"][0]["children"][2]["props"]["alt"] != "Uncommitted alt survives lookup"
                save(ui)
                updated_image_props = state(ui, base)["document"]["sections"][0]["children"][0]["children"][0]["children"][2]["props"]
                assert updated_image_props["alt"] == "Uncommitted alt survives lookup"
                assert updated_image_props["binding"] == bindings[2]

                # Unbind in draft, save/reload: static props and other bindings survive.
                ui.get_by_role("treeitem", name=re.compile(re.escape(heading_id))).click()
                ui.get_by_role("button", name="Gỡ binding").click()
                assert state(ui, base)["document"]["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["binding"] == bindings[0]
                save(ui)
                ui.reload()
                current = state(ui, base)["document"]["sections"][0]["children"][0]["children"][0]["children"]
                assert "binding" not in current[0]["props"]
                assert current[0]["props"]["text"] == "Tiêu đề mới"
                assert current[1]["props"]["binding"] == bindings[1]

                # Editor uses existing auth/ACL; public cannot access Admin lookup.
                assert context.request.get(base + "/api/admin/content-types").status == 200
                editor_context = browser.new_context(viewport={"width": 390, "height": 844})
                editor = editor_context.new_page()
                editor.goto(base + "/admin/#/builder")
                editor.locator("#auth-username").fill("bindingeditor")
                editor.locator("#auth-password").fill("password123")
                editor.locator("#auth-login").click()
                expect(editor.locator('[data-testid="builder-workspace"]')).to_contain_text("Đã mở")
                editor.get_by_role("treeitem", name=re.compile(re.escape(page_text_id))).click()
                expect(editor.get_by_label("Binding content type")).to_have_value("page")
                expect(editor.get_by_label("Binding entry")).to_have_value("draft-page")
                editor.get_by_role("button", name="Xem trước").click()
                expect(editor.frame_locator("#canvas").locator("body")).to_contain_text("DRAFT PRIVATE PAGE")
                editor.get_by_role("button", name="Canvas cấu trúc").click()
                choose_binding(editor, "post", "draft-post", "title")
                save(editor)
                editor_saved = state(editor, base)["document"]["sections"][0]["children"][0]["children"][0]["children"][-1]["props"]["binding"]
                assert editor_saved == {
                    "type_key": "post", "entry_slug": "draft-post",
                    "source": {"kind": "title"}, "target": "text",
                }
                assert "DRAFT SECRET POST" not in editor.request.get(base + "/").text()
                assert not errors, errors
                editor_context.close()
                browser.close()

        print("PASS #104: Chrome Admin/Editor Post/Page/CPT Heading/Text/Image/Button, field matrix, Preview/public ACL, Save/reload/unbind, source missing and lookup error")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--browser", required=True)
    args = parser.parse_args()
    check(args.browser, args.binary.resolve())
