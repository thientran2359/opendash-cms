#!/usr/bin/env python3
"""#376 native isolated C3 route authority/API/SQLite/Chrome acceptance.

Usage: python3 cms/scripts/content_foundation_m7_s17_t01_e2e.py \
  --binary cms/target/release/opendash --artifact-dir /tmp/c3-evidence
"""
from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import pathlib
import platform
import sqlite3
import subprocess
import tempfile
import traceback
from urllib.parse import urlsplit

from playwright.sync_api import sync_playwright
from m7_beta_menu_picker_e2e import admin_login, launch_server, stop_server
from content_foundation_m4_s12_t01_e2e import api, fetch


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(condition: bool, label: str) -> None:
    assert condition, label


def no_redirect(ctx, base: str, path: str, expected: int, location: str | None = None):
    response = ctx.request.fetch(base + path, method="GET", max_redirects=0, timeout=20000)
    check(response.status == expected,
          f"{path}: expected {expected}, got {response.status}: {response.text()[:200]}")
    if location is not None:
        check(response.headers.get("location") == location,
              f"{path}: redirect {response.headers.get('location')!r}, expected {location!r}")
    return response


def permalink_input(current: dict, *, post: str | None = None,
                    cpt: dict | None = None, taxonomy: dict | None = None) -> dict:
    return {
        "post_structure": post or current["post_structure"],
        "content_type_bases": {**current["content_type_bases"], **(cpt or {})},
        "taxonomy_bases": {**current["taxonomy_bases"], **(taxonomy or {})},
    }


def run(binary: pathlib.Path, artifacts: pathlib.Path) -> dict:
    checks: list[str] = []
    with tempfile.TemporaryDirectory(prefix="c3-native-", dir=artifacts) as temp:
        root = pathlib.Path(temp) / "site"
        root.mkdir(mode=0o700)
        proc, base = launch_server(binary, root)
        check(":18964" not in base and base.startswith("http://127.0.0.1:"),
              "must use isolated random loopback port")
        db = root / "database/opendash.sqlite"
        try:
            with sync_playwright() as playwright:
                chrome = playwright.chromium.launch(channel="chrome", headless=True)
                chrome_version = chrome.version
                try:
                    ctx = chrome.new_context(viewport={"width": 1280, "height": 800})
                    guest = chrome.new_context()
                    api(ctx, base, "POST", "/auth/setup", 201,
                        {"username": "c3_admin", "password": "Native-Isolated-C3-Test-2026!"})
                    csrf = admin_login(ctx, base, "c3_admin", "Native-Isolated-C3-Test-2026!")
                    fetch(guest, base, "GET", "/api/admin/settings/permalinks", 401)
                    guest.close()
                    snapshot = fetch(ctx, base, "GET", "/api/admin/settings/permalinks", 200)
                    state = snapshot.json()
                    check(state["post_structure"] == "/content/post/%postname%", "legacy default")
                    check(snapshot.headers.get("cache-control") == "no-store", "no-store")
                    check(snapshot.headers.get("etag") == f'"v{state["version"]}"', "version ETag")
                    checks.append("legacy_default_auth_and_etag")
                    public_settings = api(ctx, base, "GET", "/api/admin/settings/public")
                    api(ctx, base, "PUT", "/api/admin/settings/public", 200,
                        {"public_origin": base,
                         "content_language": public_settings["content_language"],
                         "indexing_enabled": True},
                        csrf, public_settings["version"])

                    post = api(ctx, base, "POST", "/api/admin/content-types/post/entries", 201,
                               {"title": "C3 Alpha", "slug": "alpha", "values": {}}, csrf)
                    draft = api(ctx, base, "POST", "/api/admin/content-types/post/entries", 201,
                                {"title": "C3 Draft", "slug": "private", "values": {}}, csrf)
                    post = api(ctx, base, "POST",
                               f'/api/admin/content-types/post/entries/{post["id"]}/publication',
                               200, {"status": "published"}, csrf, post["version"])
                    check(post["publication_status"] == "published", "post publication")
                    no_redirect(ctx, base, "/content/post/alpha", 200)
                    no_redirect(ctx, base, "/content/post/private", 404)
                    checks.append("legacy_live_and_draft_private")

                    cpt = api(ctx, base, "POST", "/api/admin/content-types", 201,
                              {"key": "books_c3", "label": "Books", "fields": []}, csrf)
                    termdef = api(ctx, base, "POST", "/api/admin/taxonomy/definitions", 201,
                                  {"key": "genre_c3", "label": "Genre", "hierarchical": False}, csrf)
                    term = api(ctx, base, "POST", "/api/admin/taxonomy/terms", 201,
                               {"kind": "genre_c3", "name": "Fiction", "slug": "fiction"}, csrf)
                    api(ctx, base, "PUT", "/api/admin/taxonomy/eligibility/books_c3", 200,
                        {"taxonomies": ["genre_c3"], "expected_version": cpt["version"]},
                        csrf, cpt["version"])
                    book = api(ctx, base, "POST",
                               "/api/admin/content-types/books_c3/entries", 201,
                               {"title": "C3 Book", "slug": "novel", "values": {},
                                "term_ids": [term["id"]]}, csrf)
                    no_redirect(ctx, base, "/content/books_c3/novel", 200)
                    checks.append("legacy_cpt_and_taxonomy_created")

                    no_match = fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 428,
                                     permalink_input(state), csrf)
                    check(no_match.status == 428, "missing If-Match")
                    for bad in ("/blog/%date%", "/api/%postname%",
                                "/blog/%postname%/%postname%", "/a/../%postname%"):
                        fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 400,
                              permalink_input(state, post=bad), csrf, state["version"])
                    fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 400,
                          permalink_input(state, cpt={"page": "pages-new"}), csrf,
                          state["version"])
                    stale_state = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    check(stale_state == state, "invalid forms must leave config unchanged")
                    checks.append("invalid_formats_and_428_rollback")

                    candidate = permalink_input(state, post="/blog/%postname%",
                                                cpt={"books_c3": "projects"},
                                                taxonomy={"genre_c3": "genres"})
                    saved = api(ctx, base, "PUT", "/api/admin/settings/permalinks", 200,
                                candidate, csrf, state["version"])
                    check(saved["version"] == state["version"] + 1, "CAS version bump")
                    no_redirect(ctx, base, "/blog/alpha", 200)
                    no_redirect(ctx, base, "/content/post/alpha", 301, "/blog/alpha")
                    no_redirect(ctx, base, "/projects/novel", 200)
                    no_redirect(ctx, base, "/content/books_c3/novel", 301, "/projects/novel")
                    archive_html = no_redirect(ctx, base, "/genres/fiction", 200).text()
                    check("C3 Book" in archive_html, "custom taxonomy must filter associated public CPT")
                    no_redirect(ctx, base, "/taxonomy/genre_c3/fiction", 301, "/genres/fiction")
                    no_redirect(ctx, base, "/blog/private", 404)
                    checks.append("post_cpt_term_direct_canonical_redirect_and_privacy")
                    article_html = no_redirect(ctx, base, "/blog/alpha", 200).text()
                    check(f'<link rel="canonical" href="{base}/blog/alpha"' in article_html,
                          "entry SEO canonical must use configured post URL")
                    check(f'<meta property="og:url" content="{base}/blog/alpha"' in article_html,
                          "OG URL must equal canonical resolver")
                    sitemap_index = no_redirect(ctx, base, "/sitemap.xml", 200).text()
                    check("sitemaps/1.xml" in sitemap_index, "sitemap index must include first part")
                    sitemap_part = no_redirect(ctx, base, "/sitemaps/1.xml", 200).text()
                    for path in ("/blog/alpha", "/projects/novel", "/genres/fiction"):
                        check(f"{base}{path}" in sitemap_part,
                              f"sitemap must use C3 canonical path {path}")
                    check(f"{base}/content/post/alpha" not in sitemap_part,
                          "sitemap must not list legacy redirect source")
                    check(f"{base}/blog/private" not in sitemap_part,
                          "sitemap must not list draft")
                    checks.append("entry_seo_og_and_sitemap_share_canonical_authority")

                    term = api(ctx, base, "PATCH",
                               f'/api/admin/taxonomy/terms/{term["id"]}', 200,
                               {"slug": "new-fiction"}, csrf, term["version"])
                    no_redirect(ctx, base, "/genres/fiction", 301, "/genres/new-fiction")
                    changed_archive = no_redirect(ctx, base, "/genres/new-fiction", 200).text()
                    check("C3 Book" in changed_archive,
                          "term rename must retain stable membership and content")
                    checks.append("term_slug_redirect_by_stable_id_and_membership")
                    for invalid_path in (
                        "/blog/%2e%2e/alpha",
                        "/blog/%ZZ",
                        "/projects//novel",
                        "/content/post/%2Falpha",
                        "/genres/new-fiction?taxonomy_key=genre_c3&term_id=1&category_id=1",
                    ):
                        check(no_redirect_raw(base, invalid_path) == 400,
                              f"raw request-target must fail 400: {invalid_path}")
                    checks.append("malformed_encoding_traversal_and_mixed_filters_400")

                    fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 412,
                          permalink_input(state, post="/%postname%"), csrf, state["version"])
                    check(api(ctx, base, "GET", "/api/admin/settings/permalinks") == saved,
                          "stale 412 must not change settings")
                    checks.append("stale_cas_412_atomic")

                    # New custom route takes precedence over generated path;
                    # old generated path must redirect directly to the final target.
                    route = api(ctx, base, "PUT",
                                f'/api/admin/content-types/post/entries/{post["id"]}/route', 200,
                                {"custom_path": "/showcase/alpha"}, csrf, post["version"])
                    no_redirect(ctx, base, "/showcase/alpha", 200)
                    no_redirect(ctx, base, "/blog/alpha", 301, "/showcase/alpha")
                    no_redirect(ctx, base, "/content/post/alpha", 301, "/showcase/alpha")
                    checks.append("custom_path_precedence_and_single_hop_redirect")
                    route = api(ctx, base, "PUT",
                                f'/api/admin/content-types/post/entries/{post["id"]}/route', 200,
                                {"custom_path": None}, csrf, route["entry_version"])
                    no_redirect(ctx, base, "/blog/alpha", 200)
                    no_redirect(ctx, base, "/showcase/alpha", 301, "/blog/alpha")
                    checks.append("custom_path_reset_to_configured_format")

                    # Public Builder list must link canonical paths supplied by storage.
                    home = api(ctx, base, "GET", "/api/admin/builder/homepage")
                    document = {"schema_version": 1, "sections": [{
                        "id": "c3-section", "children": [{
                            "id": "c3-row", "children": [{
                                "id": "c3-column", "children": [{
                                    "id": "c3-list", "type": "core/list",
                                    "props": {"type_key": "post", "page_size": 6}
                                }]
                            }]
                        }]
                    }]}
                    api(ctx, base, "PUT", "/api/admin/builder/homepage", 200,
                        document, csrf, home["version"])
                    page = ctx.new_page()
                    page.goto(base, wait_until="domcontentloaded")
                    check(page.locator('a[href="/blog/alpha"]').count() > 0,
                          "Builder list link must use canonical path")
                    check(page.locator('a[href="/blog/private"]').count() == 0,
                          "Draft must not appear in Builder")
                    page.screenshot(path=str(artifacts / "c3-builder-post-list.png"), full_page=True)
                    page.close()
                    checks.append("native_chrome_builder_canonical_and_draft_exclusion")

                    # Another entry may not steal any canonical/redirect claim.
                    page_entry = api(ctx, base, "POST",
                                     "/api/admin/content-types/page/entries", 201,
                                     {"title": "C3 Page", "slug": "page-c3", "values": {}}, csrf)
                    api(ctx, base, "PUT",
                        f'/api/admin/content-types/page/entries/{page_entry["id"]}/route', 200,
                        {"custom_path": "/collision/alpha"}, csrf, page_entry["version"])
                    before = api(ctx, base, "GET", "/api/admin/settings/permalinks")
                    fetch(ctx, base, "PUT", "/api/admin/settings/permalinks", 409,
                          permalink_input(before, post="/collision/%postname%"),
                          csrf, before["version"])
                    check(api(ctx, base, "GET", "/api/admin/settings/permalinks") == before,
                          "409 collision must rollback full namespace")
                    no_redirect(ctx, base, "/blog/alpha", 200)
                    checks.append("collision_409_keeps_public_and_db_state")

                    termdef = api(ctx, base, "PATCH",
                                  "/api/admin/taxonomy/definitions/genre_c3", 200,
                                  {"public": False}, csrf, termdef["version"])
                    no_redirect(ctx, base, "/genres/new-fiction", 404)
                    no_redirect(ctx, base, "/genres/fiction", 404)
                    termdef = api(ctx, base, "PATCH",
                                  "/api/admin/taxonomy/definitions/genre_c3", 200,
                                  {"public": True}, csrf, termdef["version"])
                    no_redirect(ctx, base, "/genres/new-fiction", 200)
                    checks.append("taxonomy_public_toggle_no_private_history_leak")

                    # Database foreign keys and all migration rows on the real site.
                    with sqlite3.connect(db) as conn:
                        check(conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok",
                              "SQLite integrity")
                        check(not conn.execute("PRAGMA foreign_key_check").fetchall(),
                              "SQLite foreign keys")
                        check(conn.execute("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=40 AND success=1")
                              .fetchone()[0] == 1, "migration 0040 applied")
                    checks.append("sqlite_integrity_fk_and_migration")
                finally:
                    chrome.close()
        finally:
            stop_server(proc)

        proc, base = launch_server(binary, root)
        try:
            check(no_redirect_raw(base, "/blog/alpha") == 200,
                  "restart must retain post canonical")
            check(no_redirect_raw(base, "/content/post/alpha") == 301,
                  "restart must retain stable ID redirect")
            checks.append("restart_preserves_resolver_without_flush")
        finally:
            stop_server(proc)
    return {"result": "PASS", "checks": checks, "chrome_version": chrome_version,
            "port_18964_used": False}


def no_redirect_raw(base: str, path: str) -> int:
    # Playwright/WHATWG URL parsing normalizes encoded dot segments BEFORE
    # sending the request. Use HTTPConnection to send the exact request-target
    # bytes and test the server's path parser rather than the browser's URL API.
    origin = urlsplit(base)
    connection = http.client.HTTPConnection(origin.hostname, origin.port, timeout=15)
    try:
        connection.request("GET", path)
        response = connection.getresponse()
        response.read()
        return response.status
    finally:
        connection.close()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--artifact-dir", required=True, type=pathlib.Path)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    artifacts = args.artifact_dir.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    repo = pathlib.Path(__file__).resolve().parents[2]
    receipt = {
        "source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "dirty_paths": subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True).splitlines(),
        "binary_sha256": digest(binary), "runner_sha256": digest(pathlib.Path(__file__).resolve()),
        "platform": platform.platform(), "architecture": platform.machine(),
    }
    try:
        receipt.update(run(binary, artifacts))
        receipt["screenshot_sha256"] = {p.name: digest(p) for p in artifacts.glob("*.png")}
    except Exception as error:
        receipt.update({"result": "FAIL", "error": str(error), "traceback": traceback.format_exc()})
    (artifacts / "receipt.json").write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(receipt, indent=2, ensure_ascii=False))
    if receipt["result"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
