#!/usr/bin/env python3
"""E2E verification of CUJ 1-4 for ACF Content Structure V1 in real Chrome Playwright."""
from __future__ import annotations

import argparse
import json
import pathlib
import sqlite3
import tempfile
import traceback
from contextlib import closing

from playwright.sync_api import expect, sync_playwright
from e2e_helpers import admin_login, api_json, launch_server, set_cms_locale, stop_server


def default_binary() -> pathlib.Path:
    target = pathlib.Path(__file__).resolve().parents[1] / "target"
    release = target / "release/opendash"
    debug = target / "debug/opendash"
    if release.exists():
        return release
    return debug


def run(binary: pathlib.Path, artifact_dir: pathlib.Path) -> None:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    checks: dict[str, str] = {}
    with tempfile.TemporaryDirectory(prefix="opendash-acf-e2e-", dir=artifact_dir.parent) as td:
        root = pathlib.Path(td).resolve()
        db_path = root / "database/opendash.sqlite"
        proc, base = launch_server(binary, root)
        try:
            with sync_playwright() as pw:
                browser = pw.chromium.launch(channel="chrome", headless=True)
                try:
                    ctx = browser.new_context(viewport={"width": 1360, "height": 960})
                    pw_admin = "Acf-Admin-Pass-2026!"
                    api_json(ctx, "POST", base, "/auth/setup", {"username": "admin", "password": pw_admin}, expected=201)
                    csrf = admin_login(ctx, base, "admin", pw_admin)
                    set_cms_locale(ctx, base, csrf, "en")

                    page = ctx.new_page()
                    page.set_default_timeout(20000)

                    def go(route: str) -> None:
                        page.goto(f"{base}/admin#{route}")

                    go("/dashboard")
                    expect(page.get_by_test_id("admin-layout")).to_be_visible()

                    # Verify Migration 0063 schema budget (< 256 sqlite_schema objects, no forbidden tables).
                    with closing(sqlite3.connect(db_path)) as conn:
                        total_objs = conn.execute(
                            "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'"
                        ).fetchone()[0]
                        assert total_objs < 256, f"sqlite_schema count {total_objs} >= 256"
                        for tbl in ("content_types", "content_field_groups", "taxonomy_definitions", "taxonomy_terms", "custom_options_pages"):
                            cnt = conn.execute("SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name=?", (tbl,)).fetchone()[0]
                            assert cnt == 1, f"missing expected table {tbl}"
                    checks["migration_budget_and_tables"] = "PASS"

                    # ---------------------------------------------------------
                    # CUJ 1: Clean-Slate CPT CRUD + Dynamic Sidebar + Excerpt/Attributes + Delete <dialog>
                    # ---------------------------------------------------------
                    go("/content-structure/content-types")
                    expect(page.get_by_test_id("content-type-schema-settings")).to_be_visible()
                    page.get_by_role("button", name="+ Create content type", exact=True).click()
                    cpt_form = page.locator("form:has(#content-type-key)")
                    cpt_form.locator("#content-type-key").fill("portfolio_case")
                    cpt_form.locator("#content-type-label").fill("Portfolio Cases")
                    cpt_form.locator("#content-type-singular").fill("Portfolio Case")
                    cpt_form.get_by_label("Hierarchical parent-child tree (hierarchical)").check()
                    cpt_form.get_by_label("Excerpt (excerpt)").check()
                    cpt_form.get_by_label("Menu order (menu_order)").check()
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/content-types") and r.request.method == "POST") as cpt_res:
                        cpt_form.get_by_role("button", name="Create content type", exact=True).click()
                    assert cpt_res.value.status == 201, cpt_res.value.text()

                    # Create an empty disposable CPT and verify native <dialog> cancel vs confirm delete.
                    page.get_by_role("button", name="+ Create content type", exact=True).click()
                    page.locator("#content-type-key").fill("scratch_item")
                    page.locator("#content-type-label").fill("Scratch Items")
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/content-types") and r.request.method == "POST") as scr_res:
                        page.get_by_role("button", name="Create content type", exact=True).click()
                    assert scr_res.value.status == 201, scr_res.value.text()

                    page.get_by_role("button", name="Scratch Items (scratch_item)").click()
                    page.get_by_test_id("open-delete-cpt-btn").click()
                    dlg = page.get_by_test_id("delete-cpt-dialog")
                    expect(dlg).to_be_visible()
                    dlg.get_by_role("button", name="Cancel", exact=True).click()
                    expect(dlg).to_be_hidden()
                    page.get_by_test_id("open-delete-cpt-btn").click()
                    expect(dlg).to_be_visible()
                    with page.expect_response(lambda r: "/api/admin/content-types/scratch_item" in r.url and r.request.method == "DELETE") as del_res:
                        page.get_by_test_id("confirm-delete-cpt-btn").click()
                    assert del_res.value.status in (200, 204), del_res.value.text()
                    expect(page.get_by_role("button", name="Scratch Items (scratch_item)")).to_have_count(0)

                    # Dynamic sidebar entry for Portfolio Cases should be visible.
                    expect(page.locator("nav button", has_text="Portfolio Cases")).to_be_visible()
                    checks["cuj1_cpt_and_delete_dialog"] = "PASS"

                    # ---------------------------------------------------------
                    # CUJ 2: Standalone Field Group Builder + DNF Location Rules + Editor
                    # ---------------------------------------------------------
                    go("/content-structure/field-groups")
                    expect(page.get_by_test_id("field-groups-list-page")).to_be_visible()
                    page.get_by_test_id("new-field-group-btn").click()
                    fg_form = page.get_by_test_id("field-group-editor")
                    expect(fg_form).to_be_visible()
                    fg_form.get_by_label("Group title *").fill("Case Study Meta")
                    fg_form.get_by_label("Local key *").fill("case_study_meta")
                    loc = page.get_by_test_id("location-rules-builder")
                    loc.get_by_label("Rule value").select_option("portfolio_case")
                    card = fg_form.locator('[data-testid^="field-builder-card-"]').first
                    card.get_by_label("Field label *").fill("Client Organization")
                    card.get_by_label("Field key *").fill("client_org")
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/content-field-groups") and r.request.method == "POST") as fg_res:
                        page.get_by_test_id("save-field-group-btn").click()
                    assert fg_res.value.status == 201, fg_res.value.text()

                    go("/content-structure/field-groups")
                    expect(page.locator('tr[data-group-id="user:case_study_meta"]')).to_be_visible()

                    # Open new Portfolio Case entry -> verify custom field group + Excerpt + Page Attributes render cleanly
                    go("/cpt/portfolio_case/new")
                    expect(page.locator("#entry-title-input")).to_be_visible()
                    expect(page.locator('[data-panel-id="core:excerpt"]')).to_be_visible()
                    expect(page.locator('[data-panel-id="core:page_attributes"]')).to_be_visible()
                    expect(page.locator('[data-panel-id="user:case_study_meta"]')).to_be_visible()
                    page.locator("#entry-title-input").fill("Cloud Migration Case")
                    page.locator('[data-panel-id="user:case_study_meta"]').get_by_label("Client Organization").fill("Acme Corp")
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/content-types/portfolio_case/entries/composite") and r.request.method == "POST") as entry_res:
                        page.get_by_role("button", name="Create new", exact=True).click()
                    assert entry_res.value.status == 201, entry_res.value.text()
                    entry_id = entry_res.value.json()["entry"]["id"]
                    page.reload(wait_until="domcontentloaded")
                    expect(page.locator('[data-panel-id="user:case_study_meta"]').get_by_label("Client Organization")).to_have_value("Acme Corp")
                    checks["cuj2_field_group_and_entry"] = f"PASS (entry #{entry_id})"

                    # ---------------------------------------------------------
                    # CUJ 3: Taxonomy Settings + Term Meta + SEO + Editor Inline Create (<dialog>)
                    # ---------------------------------------------------------
                    go("/content-structure/taxonomies")
                    expect(page.get_by_test_id("taxonomy-definitions-page")).to_be_visible()
                    page.get_by_role("button", name="+ Add taxonomy", exact=True).click()
                    page.get_by_label("Key *").fill("topic_tag")
                    page.get_by_label("Display name *").fill("Topic Tags")
                    page.get_by_label("Allow parent/child hierarchy (up to 8 levels)").check()
                    page.locator("#tax-selection-mode").select_option("single")
                    page.locator("#tax-picker-style").select_option("select")
                    page.locator("#tax-rewrite-base").fill("topics")
                    page.get_by_label("Allow Editors to create new terms").check()
                    page.get_by_test_id("taxonomy-settings-panel").get_by_label("Portfolio Cases (portfolio_case)").check()
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/taxonomy/definitions") and r.request.method == "POST") as tax_res:
                        page.get_by_role("button", name="Save definition", exact=True).click()
                    assert tax_res.value.status == 201, tax_res.value.text()

                    # Create a Term Field Group targeting taxonomy_term == topic_tag
                    go("/content-structure/field-groups")
                    page.get_by_test_id("new-field-group-btn").click()
                    fg_form = page.get_by_test_id("field-group-editor")
                    fg_form.get_by_label("Group title *").fill("Topic Extra")
                    fg_form.get_by_label("Local key *").fill("topic_extra")
                    loc = page.get_by_test_id("location-rules-builder")
                    loc.get_by_role("button", name="Taxonomy Term", exact=True).click()
                    loc.get_by_label("Rule value").select_option("topic_tag")
                    card = fg_form.locator('[data-testid^="field-builder-card-"]').first
                    card.get_by_label("Field label *").fill("Tagline")
                    card.get_by_label("Field key *").fill("tagline")
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/content-field-groups") and r.request.method == "POST") as tfg_res:
                        page.get_by_test_id("save-field-group-btn").click()
                    assert tfg_res.value.status == 201, tfg_res.value.text()

                    # Open Taxonomy Term editor -> create Term with description, sort_order, Term Meta & SEO
                    go("/taxonomy/topic_tag/terms")
                    expect(page.get_by_test_id("term-edit-page")).to_be_visible()
                    page.locator("#term-name").fill("Design Systems")
                    page.locator("#term-sort-order").fill("7")
                    page.locator("#term-description").fill("Core UI component taxonomy")
                    expect(page.get_by_test_id("term-meta-field-groups")).to_be_visible()
                    page.get_by_test_id("term-meta-field-groups").get_by_label("Tagline").fill("Pixel Precision")
                    page.locator("#term-seo-title").fill("Design Systems SEO")
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/taxonomy/terms") and r.request.method == "POST") as term_res:
                        page.get_by_test_id("term-edit-page").get_by_role("button", name="Create new", exact=True).click()
                    assert term_res.value.status == 201, term_res.value.text()
                    created_term_id = term_res.value.json()["id"]
                    go(f"/taxonomy/topic_tag/terms/{created_term_id}/edit")
                    page.reload(wait_until="domcontentloaded")
                    expect(page.locator("#term-description")).to_have_value("Core UI component taxonomy")
                    expect(page.get_by_test_id("term-meta-field-groups").get_by_label("Tagline")).to_have_value("Pixel Precision")
                    expect(page.locator("#term-seo-title")).to_have_value("Design Systems SEO")

                    # Create Editor user and test inline term creation & picker_style='select'
                    editor_pw = "Editor-Pass-2026!"
                    api_json(
                        ctx, "POST", base, "/api/admin/users",
                        {"username": "acf_editor", "password": editor_pw, "role": "editor"},
                        csrf, expected=201,
                    )
                    ed_ctx = browser.new_context(viewport={"width": 1360, "height": 960})
                    ed_page = ed_ctx.new_page()
                    ed_page.set_default_timeout(20000)
                    ed_page.goto(f"{base}/admin#/dashboard")
                    ed_page.locator("#auth-username").fill("acf_editor")
                    ed_page.locator("#auth-password").fill(editor_pw)
                    with ed_page.expect_response(lambda r: r.url.endswith("/auth/login") and r.request.method == "POST") as ed_login:
                        ed_page.locator("#auth-login").click()
                    assert ed_login.value.status == 200
                    ed_page.goto(f"{base}/admin#/cpt/portfolio_case/new")
                    expect(ed_page.get_by_test_id("term-select-topic_tag")).to_be_visible()
                    ed_page.get_by_test_id("inline-add-term-topic_tag").click()
                    tdlg = ed_page.get_by_test_id("inline-create-term-dialog-topic_tag")
                    expect(tdlg).to_be_visible()
                    tdlg.locator("#inline-term-name").fill("Architecture")
                    with ed_page.expect_response(lambda r: r.url.endswith("/api/admin/taxonomy/terms") and r.request.method == "POST") as iterm:
                        tdlg.get_by_role("button", name="Create term", exact=True).click()
                    assert iterm.value.status == 201, iterm.value.text()
                    new_term_id = str(iterm.value.json()["id"])
                    expect(ed_page.get_by_test_id("term-select-topic_tag")).to_have_value(new_term_id)
                    checks["cuj3_taxonomy_term_meta_and_inline_create"] = "PASS"

                    # ---------------------------------------------------------
                    # CUJ 4: Custom Options Pages + Multilingual <LocaleTabs> + RBAC + <dialog> Delete
                    # ---------------------------------------------------------
                    go("/settings/multilingual")
                    with page.expect_response(lambda r: "/multilingual" in r.url and r.request.method in ("PUT", "POST")):
                        page.get_by_test_id("multilingual-enabled-checkbox").click()
                    expect(page.get_by_test_id("add-locale-btn")).to_be_visible()
                    page.get_by_test_id("add-locale-btn").click()
                    page.get_by_test_id("new-locale-code").fill("en")
                    page.get_by_test_id("new-locale-label").fill("English")
                    page.get_by_test_id("add-locale-form").locator("button[type=submit]").click()
                    expect(page.get_by_test_id("locale-row-en")).to_be_visible()

                    go("/content-structure/options-pages")
                    expect(page.get_by_test_id("options-pages-list-page")).to_be_visible()
                    page.get_by_test_id("add-options-page-btn").click()
                    opt_form = page.get_by_test_id("options-page-form")
                    opt_form.locator("#opt-page-title").fill("Brand Config")
                    opt_form.locator("#opt-parent-menu").select_option("settings")
                    opt_form.locator("#opt-min-role").select_option("editor")
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/options-pages") and r.request.method == "POST") as op_res:
                        opt_form.get_by_role("button", name="Save options page", exact=True).click()
                    assert op_res.value.status == 201, op_res.value.text()

                    # Create Field Group targeting options_page == custom:brand_config
                    go("/content-structure/field-groups")
                    page.get_by_test_id("new-field-group-btn").click()
                    fg_form = page.get_by_test_id("field-group-editor")
                    fg_form.get_by_label("Group title *").fill("Brand Settings Fields")
                    fg_form.get_by_label("Local key *").fill("brand_settings_fields")
                    loc = page.get_by_test_id("location-rules-builder")
                    loc.get_by_role("button", name="Options Page", exact=True).click()
                    loc.get_by_role("button", name="custom:brand_config", exact=True).click()
                    card1 = fg_form.locator('[data-testid^="field-builder-card-"]').first
                    card1.get_by_label("Field label *").fill("Hotline")
                    card1.get_by_label("Field key *").fill("hotline")

                    page.get_by_test_id("add-field-btn").click()
                    card2 = fg_form.locator('[data-testid^="field-builder-card-"]').nth(1)
                    card2.locator('button[aria-expanded]').click()
                    card2.get_by_label("Field label *").fill("Banner Title")
                    card2.get_by_label("Field key *").fill("banner_title")
                    card2.get_by_role("tab", name="Presentation & Access", exact=True).click()
                    card2.get_by_label("Translatable (multilingual)").check()

                    with page.expect_response(lambda r: r.url.endswith("/api/admin/content-field-groups") and r.request.method == "POST") as ofg_res:
                        page.get_by_test_id("save-field-group-btn").click()
                    assert ofg_res.value.status == 201, ofg_res.value.text()

                    # Edit Options Page values on vi & en locales via <LocaleTabs>
                    go("/options/brand_config")
                    opt_view = page.get_by_test_id("custom-options-page-view")
                    expect(opt_view).to_be_visible()
                    opt_view.get_by_label("Hotline").fill("1900-8888")
                    opt_view.get_by_label("Banner Title").fill("Xin chao Brand")
                    with page.expect_response(lambda r: "/api/admin/options-pages/brand_config/values" in r.url and r.request.method == "PUT") as put_vi:
                        opt_view.get_by_role("button", name="Save options", exact=True).click()
                    assert put_vi.value.status == 200, put_vi.value.text()

                    page.get_by_test_id("options-page-locale-tabs-en").click()
                    expect(opt_view.get_by_text("Fallback (vi)")).to_be_visible()
                    expect(opt_view.get_by_label("Hotline")).to_be_disabled()
                    opt_view.get_by_label("Banner Title").fill("Hello Brand EN")
                    with page.expect_response(lambda r: "/api/admin/options-pages/brand_config/values?locale=en" in r.url and r.request.method == "PUT") as put_en:
                        opt_view.get_by_role("button", name="Save options", exact=True).click()
                    assert put_en.value.status == 200, put_en.value.text()

                    # Verify Editor can open and view Custom Options Page
                    ed_page.on("dialog", lambda d: d.accept())
                    ed_page.goto(f"{base}/admin#/options/brand_config")
                    ed_page.reload(wait_until="domcontentloaded")
                    expect(ed_page.get_by_test_id("custom-options-page-view")).to_be_visible()
                    expect(ed_page.get_by_test_id("custom-options-page-view").get_by_label("Banner Title")).to_have_value("Xin chao Brand")
                    ed_ctx.close()

                    # Create & delete disposable custom options page via native <dialog>
                    go("/content-structure/options-pages")
                    page.get_by_test_id("add-options-page-btn").click()
                    opt_form.locator("#opt-page-title").fill("Temp Options Page")
                    with page.expect_response(lambda r: r.url.endswith("/api/admin/options-pages") and r.request.method == "POST") as tmp_op:
                        opt_form.get_by_role("button", name="Save options page", exact=True).click()
                    assert tmp_op.value.status == 201
                    tmp_row = page.locator('article[data-options-page-key="temp_options_page"]')
                    tmp_row.get_by_role("button", name="Delete", exact=True).click()
                    odlg = page.locator('dialog[aria-labelledby="delete-options-page-title"]')
                    expect(odlg).to_be_visible()
                    odlg.get_by_role("checkbox").check()
                    with page.expect_response(lambda r: "/api/admin/options-pages/temp_options_page" in r.url and r.request.method == "DELETE") as del_op:
                        odlg.get_by_role("button", name="Confirm delete", exact=True).click()
                    assert del_op.value.status == 204
                    expect(tmp_row).to_have_count(0)

                    page.screenshot(path=str(artifact_dir / "acf_e2e_options_multilingual.png"))
                    checks["cuj4_options_page_multilingual_rbac"] = "PASS"
                finally:
                    browser.close()
        finally:
            stop_server(proc)

    summary = {"status": "PASS", "binary": str(binary), "checks": checks}
    (artifact_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", type=pathlib.Path, default=default_binary())
    ap.add_argument("--artifact-dir", type=pathlib.Path, default=pathlib.Path("cms/target/acf-e2e"))
    args = ap.parse_args()
    try:
        run(args.binary.resolve(), args.artifact_dir.resolve())
        return 0
    except Exception:
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
