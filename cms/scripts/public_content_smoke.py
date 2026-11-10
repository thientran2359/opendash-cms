"""M3.6 end-to-end public CPT/themed HTML/media smoke, using temporary SQLite.

Usage from repository root:
    python3 cms/scripts/public_content_smoke.py --binary cms/target/debug/opendash
"""

import argparse
import io
import json
import os
import sqlite3
import subprocess
from pathlib import Path
from tempfile import TemporaryDirectory
from zipfile import ZIP_DEFLATED, ZipFile

from content_types_smoke import login, request, running
from content_entries_smoke import api
from media_smoke import raw_request, upload_image


def require(response, code, label):
    assert response[0] == code, (label, response[0], response[1][:500])
    return response


def theme_zip(block_id, with_content=False, invalid=False):
    out = io.BytesIO()
    with ZipFile(out, "w", ZIP_DEFLATED) as archive:
        archive.writestr("block.json", json.dumps({
            "id": block_id, "fields": {"title": "Tiêu đề", "note": "Mô tả"},
        }))
        archive.writestr("block.html", "<article><h2>{{ values.title }}</h2><p>{{ values.note }}</p></article>")
        if with_content:
            archive.writestr(
                "content-list.html",
                '<!doctype html><html><h1>THEME ARCHIVE {{ content_type.label }}</h1>'
                '{% for item in entries %}<a href="{{ item.url }}">{{ item.title }}</a>{% endfor %}'
                '{% if next_url %}<a href="{{ next_url }}">Tiếp</a>{% endif %}</html>',
            )
            archive.writestr(
                "content-single.html",
                '<!doctype html><html><h1>THEME SINGLE {{ entry.title }}</h1>'
                '<p>{{ entry.values.body }}</p>'
                '{% for f in entry.fields %}{% if f.image_url %}<img src="{{ f.image_url }}" alt="{{ f.label }}">{% endif %}{% endfor %}</html>',
            )
        if invalid:
            archive.writestr("content-single.html", "{% invalid directive %}")
    return out.getvalue()


def build_entry(slug, cover=None, gallery=None, notes="") -> dict:
    values = {
        "body": '<img src=x onerror="window.__xss=1"><script>alert(1)</script> {{7*7}}',
        "notes": notes,
    }
    if cover:
        values["cover"] = cover
    if gallery:
        values["photos"] = gallery
    return {"title": 'Dự án <img src=x onerror="alert(2)">', "slug": slug, "values": values}


def test_public(binary: Path) -> None:
    with TemporaryDirectory(prefix="opendash-m3-6-public-") as folder:
        site = Path(folder) / "site"
        db = site / "database" / "opendash.sqlite"
        listing = "/content/projects"
        detail = listing + "/first-entry"
        with running(binary, db) as base:
            require(raw_request(base, "GET", "/"), 200, "Homepage remains public")
            require(raw_request(base, "GET", listing), 404, "Missing CPT archive")
            require(raw_request(base, "GET", detail), 404, "Missing CPT entry")
            require(raw_request(base, "GET", "/content/Invalid!"), 404, "Invalid CPT key")
            require(raw_request(base, "GET", listing + "/../bad"), 404, "Invalid slug")

            require(request(base, "/auth/setup", {"username": "admin", "password": "password123"}), 201, "Setup")
            auth = login(base, "admin", "password123")
            img = upload_image(base, auth, seed=10)
            unused = upload_image(base, auth, seed=19)
            gallery = upload_image(base, auth, seed=23)
            a, b, c = img["id"], unused["id"], gallery["id"]
            require(raw_request(base, "GET", f"/media/{a}/file"), 404, "Unreferenced upload private")
            require(raw_request(base, "GET", f"/media/{b}/thumbnail"), 404, "Unreferenced thumb private")
            require(raw_request(base, "GET", f"/api/admin/media/{a}/file", headers=auth), 200, "Admin media remains readable")

            schema = {
                "key": "projects", "label": 'Dự án & <"x">',
                "fields": [
                    {"key": "body", "label": "Nội dung", "type": "rich_text"},
                    {"key": "notes", "label": "Ghi chú", "type": "long_text"},
                    {"key": "cover", "label": "Ảnh bìa", "type": "image"},
                    {"key": "photos", "label": "Album", "type": "gallery"},
                ],
            }
            require(request(base, "/api/admin/content-types", schema, auth), 201, "CPT")
            empty = require(raw_request(base, "GET", listing), 200, "Empty archive")
            assert b"Ch\xc6\xb0a c\xc3\xb3 n\xe1\xbb\x99i dung" in empty[1]
            require(raw_request(base, "GET", listing + "?page=0"), 400, "Invalid archive page")
            require(raw_request(base, "GET", listing + "?page=10001"), 400, "Huge archive page")

            entry_path = "/api/admin/content-types/projects/entries"
            saved = require(api(base, "POST", entry_path, build_entry("first-entry", a, [c], notes=b), auth), 201, "Save public entry")
            first_id = json.loads(saved[1])["id"]
            item_path = f"{entry_path}/{first_id}"
            response = require(raw_request(base, "GET", detail), 200, "Public single no cookie")
            html = response[1].decode()
            assert 'script-src \'none\'' in response[2]["Content-Security-Policy"]
            assert "img-src 'self'" in response[2]["Content-Security-Policy"]
            assert response[2]["X-Content-Type-Options"] == "nosniff"
            assert "&lt;script&gt;alert(1)" in html and "{{7*7}}" in html
            assert "&lt;img src=x onerror=" in html and "<script>alert(1)</script>" not in html
            assert f"/media/{a}/file" in html.replace("&#x2f;", "/")
            assert "Ảnh bìa" in html and "Album" in html
            archive = require(raw_request(base, "GET", listing), 200, "Archive with entry")
            assert b"first-entry" in archive[1]
            require(raw_request(base, "GET", f"/media/{a}/file"), 200, "Referenced cover is public")
            require(raw_request(base, "GET", f"/media/{a}/thumbnail"), 200, "Referenced cover thumbnail")
            require(raw_request(base, "GET", f"/media/{c}/file"), 200, "Gallery image public")
            require(raw_request(base, "GET", f"/media/{b}/file"), 404, "Media ID in text must not publish")

            # Uploading a theme must not activate its templates until its
            # home_page block is selected via the existing Builder Save API.
            zip_data = theme_zip("sample/banner", with_content=True)
            require(raw_request(base, "POST", "/theme", zip_data, {**auth, "Content-Type": "application/zip"}), 201, "Install themed CPT ZIP")
            assert b"THEME SINGLE" not in require(raw_request(base, "GET", detail), 200, "No home means fallback")[1]
            body = {"type": "sample/banner", "values": {"title": "Trang chủ", "note": "Builder không đổi"}}
            require(api(base, "POST", "/save", body, auth), 200, "Activate theme by saving home block")
            assert "Builder không đổi" in require(
                raw_request(base, "GET", "/"), 200, "Builder root preserved"
            )[1].decode("utf-8")
            themed = require(raw_request(base, "GET", detail), 200, "Theme single")[1].decode()
            assert "THEME SINGLE" in themed and "&lt;script&gt;alert(1)" in themed
            assert "<script>alert(1)</script>" not in themed
            themed_archive = require(raw_request(base, "GET", listing), 200, "Theme archive")[1].decode()
            assert "THEME ARCHIVE" in themed_archive
            require(raw_request(base, "POST", "/theme", zip_data, {**auth, "Content-Type": "application/zip"}), 409, "No overwrite same theme")
            require(raw_request(base, "POST", "/theme", theme_zip("bad/template", invalid=True), {**auth, "Content-Type": "application/zip"}), 400, "Reject template syntax")

            # An extra saved entry keeps the same asset public even if the
            # first entry removes its own reference.
            second = require(api(base, "POST", entry_path, build_entry("second-entry", a), auth), 201, "Second reference")
            second_id = json.loads(second[1])["id"]
            edited = build_entry("first-entry", notes=b)
            edited["title"] = "Đã cập nhật ngay"
            require(api(base, "PUT", item_path, edited, {**auth, "If-Match": '"v1"'}), 200, "Live update")
            assert "Đã cập nhật ngay" in require(raw_request(base, "GET", detail), 200, "Updated public HTML")[1].decode()
            require(raw_request(base, "GET", f"/media/{a}/file"), 200, "Asset still referenced by second entry")
            require(raw_request(base, "GET", f"/media/{c}/file"), 404, "Unreferenced gallery becomes private")
            require(api(base, "DELETE", f"{entry_path}/{second_id}", headers={**auth, "If-Match": '"v1"'}), 204, "Delete second reference")
            require(raw_request(base, "GET", f"/media/{a}/file"), 404, "Final ref removed")
            require(raw_request(base, "GET", detail + "-absent"), 404, "Unknown slug")

            # Existing two-file theme remains installable and becomes a safe
            # fallback when active, without affecting persisted CPT/entries.
            require(raw_request(base, "POST", "/theme", theme_zip("old/banner"), {**auth, "Content-Type": "application/zip"}), 201, "Legacy two-file theme")
            old = {"type": "old/banner", "values": {"title": "Old", "note": "Old block"}}
            require(api(base, "POST", "/save", old, auth), 200, "Switch home block")
            old_html = require(raw_request(base, "GET", detail), 200, "Built-in after theme switch")[1]
            assert b"THEME SINGLE" not in old_html and b"<!doctype html>" in old_html
            # Archive pagination is a bounded read, no uncontrolled full dump.
            for idx in range(21):
                require(api(base, "POST", entry_path, {
                    "title": f"Entry {idx}", "slug": f"entry-{idx}", "values": {},
                }, auth), 201, "Create paginated entry")
            assert "Trang sau" in require(raw_request(base, "GET", listing), 200, "Page1")[1].decode()
            assert "Trang trước" in require(raw_request(base, "GET", listing + "?page=2"), 200, "Page2")[1].decode()

        with sqlite3.connect(db) as con:
            assert con.execute("SELECT success FROM _sqlx_migrations WHERE version = 7").fetchone() == (1,)
            assert con.execute("SELECT content_single_template IS NOT NULL FROM theme_blocks WHERE id='sample/banner'").fetchone() == (1,)
            assert con.execute("SELECT content_single_template IS NULL FROM theme_blocks WHERE id='old/banner'").fetchone() == (1,)
            con.execute("UPDATE content_types SET owner_kind='plugin', owner_id='inactive-provider', provider_status='provider_inactive' WHERE key='projects'")
            con.commit()

        with running(binary, db) as base:
            assert "Đã cập nhật ngay" in require(raw_request(base, "GET", detail), 200, "Inactive CPT public after restart")[1].decode()
            require(raw_request(base, "GET", f"/media/{a}/file"), 404, "Public media ACL after restart")
            auth = login(base, "admin", "password123")
            require(raw_request(base, "GET", f"/api/admin/media/{a}/file", headers=auth), 200, "Admin media still exists")


def test_post_page_publication(binary: Path) -> None:
    """Real SQLx migration, draft privacy, version CAS and media ACL on copies."""
    with TemporaryDirectory(prefix="opendash-m3-s20-") as folder:
        site = Path(folder) / "site"
        db = site / "database" / "opendash.sqlite"
        post_list = "/api/admin/content-types/post/entries"
        with running(binary, db) as base:
            require(request(base, "/auth/setup", {"username": "admin", "password": "password123"}),
                    201, "Setup for publication")
            admin = login(base, "admin", "password123")
            require(request(base, "/api/admin/users",
                            {"username": "editor", "password": "password123"}, admin),
                    201, "Create Editor")
            editor = login(base, "editor", "password123")
            schema_response = json.loads(require(api(base, "GET", "/api/admin/content-types",
                                                     headers=admin), 200, "Built-in schemas")[1])
            schemas = {item["descriptor"]["key"]: item for item in schema_response}
            assert schemas["post"]["owner"] == schemas["page"]["owner"] == {"kind": "core"}
            for reserved in ("post", "page"):
                require(request(base, "/api/admin/content-types",
                                {"key": reserved, "label": "Hijack"}, admin),
                        409, "Reserved key cannot be claimed")

            draft_media = upload_image(base, admin, seed=33)["id"]
            private_media = upload_image(base, admin, seed=44)["id"]
            post_input = {
                "title": "<script>alert('x')</script>",
                "slug": "secret-draft",
                "values": {
                    "body": "<img src=x onerror=alert(1)> {{7*7}}",
                    "featured_image": draft_media,
                },
            }
            created = require(api(base, "POST", post_list, post_input, admin), 201, "Create private draft")
            record = json.loads(created[1])
            assert record["publication_status"] == "draft" and record["version"] == 1
            item_path = f"{post_list}/{record['id']}"
            publish_path = item_path + "/publication"
            preview_path = item_path + "/preview"
            require(api(base, "POST", "/api/admin/content-types/page/entries",
                        {"title": "Private Page", "slug": "hidden-page", "values": {}}, editor),
                    201, "Page starts private")
            require(raw_request(base, "GET", "/content/post/secret-draft"), 404, "Draft single 404")
            require(raw_request(base, "GET", "/content/page/hidden-page"), 404, "Draft page 404")
            require(raw_request(base, "GET", f"/media/{draft_media}/file"), 404, "Draft-only image 404")
            require(raw_request(base, "GET", f"/media/{draft_media}/thumbnail"), 404, "Draft-only thumb 404")
            require(api(base, "GET", preview_path), 401, "Guest preview denied")
            preview = require(api(base, "GET", preview_path, headers=editor), 200, "Editor private preview")
            assert preview[2]["Cache-Control"] == "no-store"
            assert "script-src 'none'" in preview[2]["Content-Security-Policy"]
            assert "&lt;script&gt;" in preview[1] and "<script>alert('x')</script>" not in preview[1]
            assert "/api/admin/media/" in preview[1].replace("&#x2f;", "/")
            assert f"/media/{draft_media}/file" not in preview[1].replace("/api/admin/media/", "/private-admin/")

            # Even many recent drafts must not occupy a public LIMIT/OFFSET slot.
            for index in range(21):
                require(api(base, "POST", post_list,
                            {"title": f"Draft {index}", "slug": f"draft-{index}", "values": {}}, editor),
                        201, "Batch drafts")
            archive = require(raw_request(base, "GET", "/content/post"), 200, "Private archive")
            assert b"secret-draft" not in archive[1] and b"draft-20" not in archive[1]
            assert b"Trang sau" not in archive[1]

            require(api(base, "POST", publish_path, {"status": "published"}), 401, "Guest publish denied")
            require(api(base, "POST", publish_path, {"status": "published"},
                        {"Cookie": admin["Cookie"], "If-Match": '"v1"'}), 403, "Publish needs CSRF")
            require(api(base, "POST", publish_path, {"status": "published"},
                        {**admin, "Origin": "http://evil.invalid", "If-Match": '"v1"'}),
                    403, "Publish Origin")
            require(api(base, "POST", publish_path, {"status": "published"}, admin),
                    428, "Publish requires ETag")
            require(api(base, "POST", publish_path, {"status": "scheduled"},
                        {**admin, "If-Match": '"v1"'}), 400, "Invalid publish status")
            require(api(base, "POST", publish_path, {"status": "published"},
                        {**admin, "If-Match": '"v0"'}), 400, "Bad If-Match")
            require(api(base, "POST", "/api/admin/content-types/invalid/entries/1/publication",
                        {"status": "published"}, {**admin, "If-Match": '"v1"'}),
                    400, "Legacy non-core publish denied")
            published = require(api(base, "POST", publish_path, {"status": "published"},
                                    {**editor, "If-Match": '"v1"'}), 200, "Editor publishes")
            assert json.loads(published[1])["publication_status"] == "published"
            assert published[2]["ETag"] == '"v2"'
            require(api(base, "POST", publish_path, {"status": "draft"},
                        {**admin, "If-Match": '"v1"'}), 412, "Stale CAS")
            require(api(base, "POST", f"{post_list}/999999/publication",
                        {"status": "published"}, {**admin, "If-Match": '"v1"'}),
                    404, "Unknown ID")
            require(raw_request(base, "GET", "/content/post/secret-draft"), 200, "Published single")
            public_list = require(raw_request(base, "GET", "/content/post"), 200, "Published list")[1]
            assert b"secret-draft" in public_list and b"draft-20" not in public_list
            assert b"Trang sau" not in public_list
            require(raw_request(base, "GET", f"/media/{draft_media}/file"), 200, "Published image 200")
            require(raw_request(base, "GET", f"/media/{draft_media}/thumbnail"), 200, "Published thumb 200")

            edited = dict(post_input, title="Published Save")
            updated = require(api(base, "PUT", item_path, edited,
                                  {**admin, "If-Match": '"v2"'}), 200, "Save preserves published")
            assert json.loads(updated[1])["publication_status"] == "published"
            assert updated[2]["ETag"] == '"v3"'
            unpublished = require(api(base, "POST", publish_path, {"status": "draft"},
                                      {**admin, "If-Match": '"v3"'}), 200, "Unpublish")
            assert json.loads(unpublished[1])["publication_status"] == "draft"
            require(raw_request(base, "GET", "/content/post/secret-draft"), 404, "Unpublished 404")
            require(raw_request(base, "GET", f"/media/{draft_media}/file"), 404, "Last public image ref revoked")

            # A legacy public gallery sharing an asset keeps that asset public.
            gallery_type = {
                "key": "gallery_legacy", "label": "Gallery",
                "fields": [{"key": "photos", "label": "Ảnh", "type": "gallery"}],
            }
            require(request(base, "/api/admin/content-types", gallery_type, admin),
                    201, "Legacy gallery schema")
            legacy_list = "/api/admin/content-types/gallery_legacy/entries"
            legacy = require(api(base, "POST", legacy_list, {
                "title": "Shared asset", "slug": "legacy-gallery",
                "values": {"photos": [draft_media, private_media]},
            }, admin), 201, "Legacy gallery Save public")
            assert json.loads(legacy[1])["publication_status"] == "published"
            require(raw_request(base, "GET", f"/media/{draft_media}/file"), 200, "Shared SHA stays public")
            require(raw_request(base, "GET", f"/media/{private_media}/thumbnail"), 200, "Public gallery thumbnail")
            legacy_id = json.loads(legacy[1])["id"]
            # Negative ACL fixture: a draft gallery must not pass the OR
            # branch of media_is_referenced even if its JSON contains the ID.
            with sqlite3.connect(db) as con:
                con.execute(
                    "UPDATE content_entries SET publication_status='draft' WHERE id=?",
                    (legacy_id,),
                )
                con.commit()
            require(raw_request(base, "GET", "/content/gallery_legacy/legacy-gallery"),
                    404, "Draft gallery single private")
            require(raw_request(base, "GET", f"/media/{draft_media}/file"),
                    404, "Draft gallery may not bypass publication AND")
            require(raw_request(base, "GET", f"/media/{private_media}/thumbnail"),
                    404, "Draft gallery thumb not public")
            with sqlite3.connect(db) as con:
                con.execute(
                    "UPDATE content_entries SET publication_status='published' WHERE id=?",
                    (legacy_id,),
                )
                con.commit()
            require(raw_request(base, "GET", f"/media/{draft_media}/file"),
                    200, "Republished gallery reference")
            require(api(base, "DELETE", f"{legacy_list}/{legacy_id}",
                        headers={**admin, "If-Match": '"v1"'}), 204, "Delete last gallery")
            require(raw_request(base, "GET", f"/media/{draft_media}/file"), 404, "Draft-only after gallery delete")
            require(raw_request(base, "GET", f"/media/{private_media}/thumbnail"), 404, "Private after gallery delete")
            require(raw_request(base, "GET", "/"), 200, "Legacy Builder homepage intact")

        with sqlite3.connect(db) as con:
            assert con.execute("SELECT success FROM _sqlx_migrations WHERE version = 8").fetchone() == (1,)
            assert con.execute("SELECT publication_status FROM content_entries WHERE id = ?",
                               (record["id"],)).fetchone() == ("draft",)
            assert con.execute("SELECT owner_kind, owner_id FROM content_types WHERE key='post'").fetchone() == ("core", None)
            try:
                con.execute(
                    "INSERT INTO content_entries(content_type_key,slug,title,values_json) "
                    "VALUES('post','unsafe-direct-write','Unsafe default','{}')"
                )
            except sqlite3.IntegrityError:
                pass
            else:
                raise AssertionError("Direct Post INSERT must not inherit published default")

        with running(binary, db) as base:
            admin = login(base, "admin", "password123")
            restarted = require(api(base, "GET", item_path, headers=admin), 200, "Restart preserves draft")
            assert json.loads(restarted[1])["publication_status"] == "draft"
            require(raw_request(base, "GET", "/content/post/secret-draft"), 404, "Restart draft private")

        # Reconstruct a disposable pre-0008 database with a legacy 'post'
        # collision. Actual SQLx startup must reject it *before* migration.
        collision_db = Path(folder) / "collision" / "database" / "opendash.sqlite"
        collision_db.parent.mkdir(parents=True)
        with sqlite3.connect(db) as source, sqlite3.connect(collision_db) as target:
            source.backup(target)
            target.execute("DROP INDEX idx_content_entries_published_type_id")
            target.execute("DROP TRIGGER content_entries_core_insert_draft")
            target.execute("ALTER TABLE content_entries DROP COLUMN publication_status")
            target.execute("DELETE FROM _sqlx_migrations WHERE version=8")
            target.execute("UPDATE content_types SET owner_kind='user' WHERE key='post'")
            target.commit()
        env = dict(os.environ, OPENDASH_PORT="0",
                   OPENDASH_SITE_ROOT=str(collision_db.parent.parent),
                   OPENDASH_DB_PATH=str(collision_db))
        result = subprocess.run([str(binary)], env=env, capture_output=True, text=True, timeout=15)
        assert result.returncode != 0 and "M3-S20 preflight" in result.stderr, result.stderr
        with sqlite3.connect(collision_db) as con:
            assert con.execute("SELECT owner_kind FROM content_types WHERE key='post'").fetchone() == ("user",)
            assert con.execute("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=8").fetchone() == (0,)
            assert "publication_status" not in [r[1] for r in con.execute("PRAGMA table_info(content_entries)")]
            # A superficially Core-owned key with a mismatched descriptor is
            # also a collision; preserve it without any automatic rewrite.
            con.execute(
                "UPDATE content_types SET owner_kind='core', definition_json=? WHERE key='post'",
                (json.dumps({"key": "post", "label": "Unexpected", "fields": []}),),
            )
            con.commit()
        result = subprocess.run([str(binary)], env=env, capture_output=True, text=True, timeout=15)
        assert result.returncode != 0 and "M3-S20 preflight" in result.stderr, result.stderr
        with sqlite3.connect(collision_db) as con:
            assert con.execute("SELECT json_extract(definition_json,'$.label') "
                               "FROM content_types WHERE key='post'").fetchone() == ("Unexpected",)
            assert con.execute("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=8").fetchone() == (0,)

        # Positive legacy 0007 -> 0008 path on a separate DB copy.
        # Existing non-Core entries must become published; Core seed is fresh.
        upgrade_db = Path(folder) / "upgrade" / "database" / "opendash.sqlite"
        upgrade_db.parent.mkdir(parents=True)
        with sqlite3.connect(db) as source, sqlite3.connect(upgrade_db) as target:
            source.backup(target)
            target.execute("DELETE FROM content_entries WHERE content_type_key IN ('post','page')")
            target.execute("DELETE FROM content_types WHERE key IN ('post','page')")
            target.execute(
                "INSERT INTO content_entries(content_type_key, slug, title, values_json) "
                "VALUES('gallery_legacy','legacy-before-0008','Legacy retained','{}')"
            )
            target.execute("DROP INDEX idx_content_entries_published_type_id")
            target.execute("DROP TRIGGER content_entries_core_insert_draft")
            target.execute("ALTER TABLE content_entries DROP COLUMN publication_status")
            target.execute("DELETE FROM _sqlx_migrations WHERE version=8")
            target.commit()
        with running(binary, upgrade_db) as base:
            require(raw_request(base, "GET", "/content/gallery_legacy/legacy-before-0008"),
                    200, "Legacy content remains public through 0008 upgrade")
            auth = login(base, "admin", "password123")
            seeded = json.loads(require(api(base, "GET", "/api/admin/content-types",
                                             headers=auth), 200, "Core re-seed on upgrade")[1])
            assert {"post", "page"}.issubset({x["descriptor"]["key"] for x in seeded})
            new_draft = json.loads(require(
                api(base, "POST", post_list,
                    {"title": "New draft", "slug": "new-after-upgrade", "values": {}}, auth),
                201, "Post after upgrade starts draft",
            )[1])
            assert new_draft["publication_status"] == "draft"
        with sqlite3.connect(upgrade_db) as con:
            assert con.execute("SELECT success FROM _sqlx_migrations WHERE version=8").fetchone() == (1,)
            assert con.execute("SELECT publication_status FROM content_entries "
                               "WHERE slug='legacy-before-0008'").fetchone() == ("published",)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(__file__).resolve().parents[1] / "target/debug/opendash")
    options = parser.parse_args()
    test_public(options.binary.resolve())
    test_post_page_publication(options.binary.resolve())
    print("M3.6 PUBLIC CONTENT SMOKE: PASS (CPT archive/single, theme templates, XSS, media ACL, pagination, restart)")
