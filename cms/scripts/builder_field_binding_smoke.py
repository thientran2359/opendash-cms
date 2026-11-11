"""M4-S05-T01 actual HTTP/SQLite field binding acceptance; disposable site only.

python3 cms/scripts/builder_field_binding_smoke.py --binary cms/target/debug/opendash
"""
import argparse
import html
import json
import sqlite3
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from builder_homepage_smoke import checked, element, get, preview, rows, save, tree
from content_entries_smoke import api
from content_types_smoke import login, request, running
from media_smoke import raw_request, upload_image


def bind(type_key, slug, key, target, *, title=False):
    return {
        "type_key": type_key,
        "entry_slug": slug,
        "source": {"kind": "title"} if title else {"kind": "field", "key": key},
        "target": target,
    }


def post(base, path, auth, payload):
    return json.loads(checked(api(base, "POST", path, payload, auth), 201, path)[1])


def publication(base, auth, type_key, item, status):
    return json.loads(checked(api(base, "POST", f"/api/admin/content-types/{type_key}/entries/{item['id']}/publication",
                                  {"status": status}, {**auth, "If-Match": f'"v{item["version"]}"'}),
                              200, f"{type_key} {status}")[1])


def check(binary):
    with TemporaryDirectory(prefix="opendash-m4-field-binding-") as folder:
        db = Path(folder) / "site" / "database" / "cms.sqlite"
        doc = None
        with running(binary, db) as base:
            checked(api(base, "POST", "/api/admin/builder/homepage/preview", {}), 401, "guest preview")
            assert request(base, "/auth/setup", {"username": "admin", "password": "password123"})[0] == 201
            admin = login(base, "admin", "password123")
            public_image = upload_image(base, admin, seed=55)["id"]
            draft_image = upload_image(base, admin, seed=56)["id"]
            checked(raw_request(base, "GET", f"/media/{draft_image}/file"), 404, "draft image private")
            checked(api(base, "POST", "/api/admin/content-types", {
                "key": "projects", "label": "Projects",
                "fields": [
                    {"key": "summary", "label": "Summary", "type": "short_text"},
                    {"key": "body", "label": "Body", "type": "rich_text"},
                    {"key": "link", "label": "Link", "type": "short_text"},
                    {"key": "cover", "label": "Cover", "type": "image"},
                ],
            }, admin), 201, "create CPT")
            custom = post(base, "/api/admin/content-types/projects/entries", admin, {
                "slug": "sample", "title": "Project <sample>",
                "values": {"summary": "See & learn", "body": "<script>alert('XSS')</script><b>rich</b>",
                           "link": "javascript:alert(1)", "cover": public_image},
            })
            assert custom["publication_status"] == "published"
            draft = post(base, "/api/admin/content-types/post/entries", admin, {
                "slug": "secret-post", "title": "DRAFT PRIVATE TITLE",
                "values": {"body": "A secret body", "featured_image": draft_image},
            })
            page = post(base, "/api/admin/content-types/page/entries", admin, {
                "slug": "about-us", "title": "DRAFT PAGE TITLE", "values": {},
            })
            assert draft["publication_status"] == "draft"
            doc = tree(
                element("core/heading", {"level": 2, "text": "", "binding": bind("post", "secret-post", None, "text", title=True)}, "bound-heading"),
                element("core/text", {"text": "", "binding": bind("projects", "sample", "body", "text")}, "bound-rich"),
                element("core/image", {"media_id": "", "alt": "Bound image", "binding": bind("projects", "sample", "cover", "media_id")}, "bound-cover"),
                element("core/button", {"label": "See", "url": "", "binding": bind("projects", "sample", "link", "url")}, "bound-link"),
                element("core/button", {"label": "", "url": "/safe", "binding": bind("projects", "sample", "summary", "label")}, "bound-label"),
                element("core/text", {"text": "", "binding": bind("projects", "sample", "gone", "text")}, "bound-missing"),
                element("core/text", {"text": "", "binding": bind("projects", "sample", "cover", "text")}, "bound-incompatible"),
                element("core/image", {"media_id": "", "alt": "Draft image", "binding": bind("post", "secret-post", "featured_image", "media_id")}, "draft-image"),
                element("core/text", {"text": "", "binding": bind("page", "about-us", None, "text", title=True)}, "page-heading"),
            )
            original_rows = rows(db)
            checked(preview(base, {}, doc), 401, "guest preview")
            checked(api(base, "POST", "/api/admin/users", {
                "username": "editor", "password": "password123", "role": "editor",
            }, admin), 201, "create editor")
            editor = login(base, "editor", "password123")
            checked(preview(base, editor, doc), 200, "editor draft preview")
            checked(preview(base, {"Cookie": admin["Cookie"]}, doc), 403, "csrf missing")
            checked(preview(base, {**admin, "Origin": "https://evil.test"}, doc), 403, "origin reject")
            rendered = json.loads(checked(preview(base, admin, doc), 200, "preview draft")[1])
            preview_html = rendered["html"]
            assert "DRAFT PRIVATE TITLE" in preview_html and "DRAFT PAGE TITLE" in preview_html
            assert f"/api/admin/media/{draft_image}/file" in preview_html
            assert "&lt;script&gt;alert" in preview_html and "<script>alert" not in preview_html
            assert "&lt;b&gt;rich&lt;/b&gt;" in preview_html
            codes = {item["node_id"]: item["code"] for item in rendered["diagnostics"]}
            assert codes == {"bound-link": "binding_url_rejected", "bound-missing": "binding_unavailable",
                             "bound-incompatible": "binding_incompatible"}, codes
            assert rows(db) == original_rows and original_rows[2] == []
            checked(raw_request(base, "GET", f"/media/{draft_image}/file"), 404, "preview cannot publish draft media")

            for wrong in (
                bind("projects", "sample", "body", "url"),
                {**bind("projects", "sample", "body", "text"), "where": "select *"},
                bind("projects", "../escape", "body", "text"),
            ):
                invalid = tree(element("core/text", {"text": "", "binding": wrong}, "invalid-binding"))
                checked(preview(base, admin, invalid), 400, "binding invalid")
            checked(save(base, admin, doc, 1), 200, "Save binds without promoting draft")
            stored = get(base, admin)
            assert stored["document"] == doc and stored["version"] == 2
            assert rows(db)[2] == [], "bound image must not grant static media ACL"
            raw = checked(raw_request(base, "GET", "/"), 200, "public before publishing")[1].decode()
            assert "DRAFT PRIVATE TITLE" not in raw and "DRAFT PAGE TITLE" not in raw
            assert draft_image not in raw and "<script>alert" not in raw
            assert f"/media/{public_image}/file" in raw
            assert "See &amp; learn" in raw
            assert "javascript:" not in raw
            checked(raw_request(base, "GET", f"/media/{draft_image}/file"), 404, "bound draft image still private")
            checked(save(base, admin, doc, 1), 412, "stale no partial save")
            assert get(base, admin) == stored

            published_post = publication(base, admin, "post", draft, "published")
            published_page = publication(base, admin, "page", page, "published")
            raw = checked(raw_request(base, "GET", "/"), 200, "public after publish")[1].decode()
            assert "DRAFT PRIVATE TITLE" in raw and "DRAFT PAGE TITLE" in raw
            assert f"/media/{draft_image}/file" in raw
            checked(raw_request(base, "GET", f"/media/{draft_image}/file"), 200, "published entry image available")
            publication(base, admin, "post", published_post, "draft")
            publication(base, admin, "page", published_page, "draft")
            raw = checked(raw_request(base, "GET", "/"), 200, "public after unpublish")[1].decode()
            assert "DRAFT PRIVATE TITLE" not in raw and draft_image not in raw and "DRAFT PAGE TITLE" not in raw
            checked(raw_request(base, "GET", f"/media/{draft_image}/file"), 404, "unpublished entry image private again")
            # Change an already saved entry field: binding is resolved at display time.
            updated_values = dict(custom["values"], link="/legal?ok=1")
            updated = json.loads(checked(api(base, "PUT", f"/api/admin/content-types/projects/entries/{custom['id']}",
                                             {"slug": "sample", "title": custom["title"], "values": updated_values},
                                             {**admin, "If-Match": f'"v{custom["version"]}"'}),
                                         200, "edit bound URL")[1])
            assert updated["version"] == custom["version"] + 1
            public = checked(raw_request(base, "GET", "/"), 200, "updated bound URL")[1].decode()
            assert 'href="/legal?ok=1"' in public and "javascript:" not in public
            # Saved source resolution must not search arbitrary slugs or serve stale values.
            moved = json.loads(checked(api(base, "PUT",
                f"/api/admin/content-types/projects/entries/{updated['id']}",
                {"slug": "renamed-project", "title": updated["title"], "values": updated["values"]},
                {**admin, "If-Match": f'"v{updated["version"]}"'}),
                200, "rename CPT source")[1])
            renamed_public = checked(raw_request(base, "GET", "/"), 200, "missing old slug")[1].decode()
            assert "&lt;script&gt;alert" not in renamed_public
            assert 'href="/legal?ok=1"' not in renamed_public
            restored = json.loads(checked(api(base, "PUT",
                f"/api/admin/content-types/projects/entries/{moved['id']}",
                {"slug": "sample", "title": moved["title"], "values": moved["values"]},
                {**admin, "If-Match": f'"v{moved["version"]}"'}),
                200, "restore CPT slug")[1])
            assert restored["slug"] == "sample"
            with closing(sqlite3.connect(db)) as conn:
                assert conn.execute("PRAGMA integrity_check").fetchone() == ("ok",)
                assert conn.execute("PRAGMA foreign_key_check").fetchall() == []
        # Restart: persisted binding descriptor, refreshed source and publication gate.
        with running(binary, db) as base:
            reopened = login(base, "admin", "password123")
            assert get(base, reopened)["document"] == doc
            raw = checked(raw_request(base, "GET", "/"), 200, "restart persisted bindings")[1].decode()
            assert 'href="/legal?ok=1"' in raw and "DRAFT PRIVATE TITLE" not in raw
    print("PASS M4-S05 field binding: Post/Page/CPT, draft Preview/public ACL, safe rich/URL, image, stale, restart")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    check(args.binary.resolve())
