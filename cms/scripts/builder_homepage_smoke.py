"""M4-S02-T01 Builder homepage HTTP/SQLite acceptance on disposable sites.

  python cms/scripts/builder_homepage_smoke.py --binary <opendash.exe>
  python cms/scripts/builder_homepage_smoke.py --binary <new.exe> --m3-binary <M3.exe>

Only temporary site folders and SQLite files are created. The optional M3
binary is used to prove an untouched pre-migration snapshot can be restored.
"""

import argparse
import json
import shutil
import sqlite3
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from media_smoke import raw_request, upload_image
from themes_smoke import (
    activate, auth_login, install, json_request, request, setup, theme_zip, themes,
)
from windows_smoke import running


PATH = "/api/admin/builder/homepage"
PREVIEW = PATH + "/preview"


def checked(result, status, context):
    assert result[0] == status, (context, status, result[0], str(result[1])[:500])
    return result


def get(base, auth):
    result = checked(request(base, "GET", PATH, headers=auth), 200, "GET homepage")
    state = json.loads(result[1])
    assert result[2]["ETag"] == f'"v{state["version"]}"'
    assert result[2]["Cache-Control"] == "no-store"
    return state


def preview(base, auth, document):
    return json_request(base, "POST", PREVIEW, document, auth)


def save(base, auth, document, version):
    return json_request(base, "PUT", PATH, document, {
        **auth, "If-Match": f'"v{version}"',
    })


def element(kind, props, node_id):
    return {"id": node_id, "type": kind, "props": props}


def heading(value="Public heading"):
    return element("core/heading", {"level": 2, "text": value}, "heading-root")


def paragraph(value="Draft body"):
    return element("core/text", {"text": value}, "paragraph-root")


def image(media_id):
    return element("core/image", {"media_id": media_id, "alt": 'Ocean & "<reef>"'}, "image-root")


def tree(*elements):
    return {
        "schema_version": 1,
        "sections": [{"id": "section-root", "children": [{
            "id": "row-root", "children": [{
                "id": "column-root", "children": list(elements),
            }],
        }]}],
    }


def rows(db):
    with closing(sqlite3.connect(db)) as conn:
        builder = conn.execute(
            "SELECT version, document_json FROM builder_homepage WHERE id=1"
        ).fetchone()
        old = conn.execute(
            "SELECT block_id, values_json FROM home_page WHERE id=1"
        ).fetchone()
        acl = conn.execute(
            "SELECT media_id FROM builder_homepage_media ORDER BY media_id"
        ).fetchall()
        return builder, old, acl


def test_new_site(binary):
    with TemporaryDirectory(prefix="opendash-m4-builder-empty-") as directory:
        db = Path(directory) / "site" / "database" / "cms.sqlite"
        with running(binary, db) as base:
            checked(request(base, "GET", PATH), 401, "guest GET")
            checked(preview(base, {}, tree(heading())), 401, "guest Preview")
            checked(save(base, {}, tree(heading()), 1), 401, "guest Save")
            auth = setup(base)
            initial = get(base, auth)
            assert initial == {
                "document": {"schema_version": 1, "sections": []},
                "source": "empty", "version": 1,
            }
            original_rows = rows(db)
            assert original_rows == ((1, None), None, []), original_rows
            draft = tree(heading('Title <script>"&</script>'), paragraph("Draft only"))
            data = json.loads(checked(preview(base, auth, draft), 200, "Draft Preview")[1])
            assert "&lt;script&gt;" in data["html"] and "<script>" not in data["html"]
            assert data["diagnostics"] == []
            assert "Draft only" not in checked(request(base, "GET", "/"), 200, "public old")[1]
            assert rows(db) == original_rows, "Open/Preview must not write"

            asset = upload_image(base, auth, seed=51)
            media_id = asset["id"]
            media_url = f"/media/{media_id}/file"
            private_url = f"/api/admin/media/{media_id}/file"
            document = tree(heading(), image(media_id), paragraph("Saved body"))
            checked(raw_request(base, "GET", media_url), 404, "private before preview")
            data = json.loads(checked(preview(base, auth, document), 200, "Image Preview")[1])
            assert private_url in data["html"]
            assert media_url not in data["html"].replace(private_url, "")
            checked(raw_request(base, "GET", media_url), 404, "private after preview")
            assert rows(db) == original_rows

            checked(preview(base, {"Cookie": auth["Cookie"]}, draft),
                    403, "Preview CSRF")
            checked(preview(base, {**auth, "Origin": "http://evil.invalid"}, draft),
                    403, "Preview Origin")
            checked(json_request(base, "PUT", PATH, document, auth),
                    428, "Missing If-Match")
            checked(json_request(base, "PUT", PATH, document, {
                **auth, "If-Match": 'W/"v1"',
            }), 400, "Weak ETag")
            checked(json_request(base, "PUT", PATH, document, {
                **auth, "If-Match": '"v1"', "Origin": "http://evil.invalid",
            }), 403, "Save Origin")
            checked(save(base, {"Cookie": auth["Cookie"]}, document, 1),
                    403, "Save CSRF")
            checked(request(base, "POST", PREVIEW, b"{}", {
                **auth, "Content-Type": "text/plain",
            }), 415, "Preview MIME")
            # DefaultBodyLimit rejects excessive JSON before deserialization.
            oversized = {"schema_version": 1, "sections": [], "unused": "x" * 270000}
            checked(preview(base, auth, oversized), 413, "Body limit")
            invalid = tree(element("plugin/unknown", {"value": "x"}, "unknown-node"))
            checked(preview(base, auth, invalid), 400, "Unknown type")
            invalid = tree(element("core/button", {
                "label": "Bad", "url": "javascript:alert(1)",
            }, "button-root"))
            checked(preview(base, auth, invalid), 400, "Unsafe link")
            checked(preview(base, auth, tree(image("0" * 64))), 400, "Missing Media")
            # A valid document over 64 KiB must work; this route is capped at
            # 256 KiB independently from the global 64 KiB legacy ZIP limit.
            large = tree(*[
                element("core/text", {"text": "x" * 15000}, f"large-text-{index}")
                for index in range(5)
            ])
            checked(preview(base, auth, large), 200, "Valid 75 KiB draft")
            assert rows(db) == original_rows, "Invalid requests must be read-only"

            checked(json_request(base, "POST", "/api/admin/users", {
                "username": "buildereditor", "password": "password123", "role": "editor",
            }, auth), 201, "Create Editor")
            editor_result = checked(json_request(base, "POST", "/auth/login", {
                "username": "buildereditor", "password": "password123",
            }), 200, "Editor login")
            editor = {
                "Cookie": editor_result[2]["Set-Cookie"].split(";")[0],
                "X-CSRF-Token": json.loads(editor_result[1])["csrf_token"],
            }
            assert get(base, editor)["version"] == 1
            checked(preview(base, editor, document), 200, "Editor preview")
            result = checked(save(base, auth, document, 1), 200, "First conversion")
            data = json.loads(result[1])
            assert data["source"] == "layout" and data["version"] == 2
            assert result[2]["ETag"] == '"v2"'
            assert media_url in data["html"] and data["diagnostics"] == []
            checked(save(base, editor, draft, 1), 412, "Other editor stale")
            checked(request(base, "GET", "/page", headers=auth), 409, "Legacy GET closed")
            checked(json_request(base, "POST", "/save", {
                "type": "missing/block", "values": {},
            }, auth), 409, "Every legacy writer closed")
            assert get(base, auth)["document"] == document
            assert rows(db)[0][0] == 2 and rows(db)[2] == [(media_id,)]
            public = checked(request(base, "GET", "/"), 200, "public after Save")
            assert "Saved body" in public[1] and media_url in public[1]
            assert "script-src 'none'" in public[2]["Content-Security-Policy"]
            checked(raw_request(base, "GET", media_url), 200, "Saved image public")
            checked(raw_request(base, "GET", f"/media/{media_id}/thumbnail"),
                    200, "Saved thumbnail")

            changed = tree(paragraph("Reordered"), heading("After save"))
            checked(save(base, editor, changed, 2), 200, "Remove public Image")
            assert rows(db)[2] == []
            checked(raw_request(base, "GET", media_url), 404, "Public ACL revoked")
            checked(raw_request(base, "GET", private_url, headers=auth), 200, "Admin media retained")
            assert get(base, auth)["document"] == changed
            assert json.loads(rows(db)[0][1]) == changed
        with running(binary, db) as base:
            auth = auth_login(base)
            state = get(base, auth)
            assert state["version"] == 3 and state["document"] == changed
            assert "Reordered" in checked(request(base, "GET", "/"), 200, "Restart")[1]
            checked(raw_request(base, "GET", media_url), 404, "ACL after restart")
        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute(
                "SELECT success FROM _sqlx_migrations WHERE version=13"
            ).fetchone() == (1,)


def test_legacy_theme_switch(binary):
    with TemporaryDirectory(prefix="opendash-m4-builder-m3-") as directory:
        db = Path(directory) / "site" / "database" / "cms.sqlite"
        with running(binary, db) as base:
            auth = setup(base)
            a = theme_zip("qa-a/banner", '<article data-theme="A">{{ values.title }}</article>',
                          fields={"title": "Title"})
            b = theme_zip("qa-b/banner", '<article data-theme="B">{{ values.title }}</article>',
                          fields={"title": "Title"}, alias="qa-a/banner")
            c = theme_zip("qa-c/banner", '<article data-theme="C">{{ values.title }}</article>',
                          fields={"title": "Title"})
            for package in (a, b, c):
                checked(install(base, auth, package), 201, "Install theme")
            old = {"type": "qa-a/banner", "values": {"title": "Legacy original"}}
            checked(json_request(base, "POST", "/save", old, auth), 200, "M3 save")
            state = get(base, auth)
            assert state["source"] == "legacy" and state["version"] == 2
            leaf = state["document"]["sections"][0]["children"][0]["children"][0]["children"][0]
            assert leaf["type"] == "legacy/block"
            assert leaf["props"] == {"block_id": "qa-a/banner", "values": old["values"]}
            assert get(base, auth)["document"] == state["document"]
            assert json.loads(checked(request(base, "GET", "/page", headers=auth), 200, "M3 GET")[1]) == old
            untouched = rows(db)
            checked(preview(base, auth, state["document"]), 200, "Legacy Preview")
            assert rows(db) == untouched

            edited = {"type": "qa-a/banner", "values": {"title": "Legacy updated"}}
            checked(json_request(base, "POST", "/save", edited, auth), 200, "Legacy Save increments version")
            assert get(base, auth)["version"] == 3
            checked(save(base, auth, state["document"], 2), 412, "Stale before conversion")
            state = get(base, auth)
            updated_leaf = state["document"]["sections"][0]["children"][0]["children"][0]["children"][0]
            assert updated_leaf["id"] == leaf["id"] and updated_leaf["props"]["values"] == edited["values"]
            checked(save(base, auth, state["document"], 3), 200, "Convert M3")
            assert themes(base, auth)["homepage_compatible"] is None
            assert rows(db)[1] == (
                "qa-a/banner", json.dumps(edited["values"], separators=(",", ":")),
            ), "Legacy data retained"
            checked(json_request(base, "POST", "/save", edited, auth), 409, "Legacy Save 409")
            checked(request(base, "GET", "/page", headers=auth), 409, "Legacy GET 409")
            assert get(base, auth)["version"] == 4
            assert 'data-theme="A"' in checked(request(base, "GET", "/"), 200, "A")[1]
            checked(activate(base, auth, "qa-a/banner", 1), 200, "Activate A")
            checked(activate(base, auth, "qa-b/banner", 2), 200, "Activate B")
            assert 'data-theme="B"' in checked(request(base, "GET", "/"), 200, "B")[1]
            assert "Legacy updated" in request(base, "GET", "/")[1]

            combined = json.loads(json.dumps(state["document"]))
            combined["sections"][0]["children"][0]["children"][0]["children"].append(heading("Core visible"))
            checked(save(base, auth, combined, 4), 200, "Save mixed layout")
            checked(activate(base, auth, "qa-c/banner", 3), 200, "Activate incompatible C")
            public = checked(request(base, "GET", "/"), 200, "C")
            assert 'data-theme="B"' not in public[1]
            assert "Legacy updated" not in public[1] and "Core visible" in public[1]
            data = json.loads(checked(preview(base, auth, combined), 200, "Diagnostic C")[1])
            assert data["diagnostics"] == [
                {"node_id": updated_leaf["id"], "code": "unsupported_by_theme"},
            ], data
            assert get(base, auth)["document"] == combined
            checked(activate(base, auth, "qa-a/banner", 4), 200, "Activate A again")
            assert 'data-theme="A"' in checked(request(base, "GET", "/"), 200, "A restored")[1]
            assert get(base, auth)["version"] == 5
            assert rows(db)[1][0] == "qa-a/banner"

            # Two concurrent Save requests on the *same* expected version
            # must serialize under SQLite; exactly one wins, no lost update.
            with ThreadPoolExecutor(max_workers=2) as executor:
                results = list(executor.map(
                    lambda value: save(base, auth, tree(heading(value)), 5)[0],
                    ("Editor one", "Editor two"),
                ))
            assert sorted(results) == [200, 412], results
            assert get(base, auth)["version"] == 6

            # Theme activation and Builder Save race under the SQLite writer.
            # At completion the public homepage must reflect one coherent
            # active theme/document snapshot, never a stale mixed template.
            with ThreadPoolExecutor(max_workers=2) as executor:
                jobs = [
                    executor.submit(save, base, auth, combined, 6),
                    executor.submit(activate, base, auth, "qa-c/banner", 5),
                ]
                outcomes = [job.result() for job in jobs]
            assert [result[0] for result in outcomes] == [200, 200], outcomes
            final = checked(request(base, "GET", "/"), 200, "Theme/Save race public")[1]
            assert "Core visible" in final and "Legacy updated" not in final, final
            assert get(base, auth)["document"] == combined


def test_m3_snapshot_restore(binary, m3_binary):
    with TemporaryDirectory(prefix="opendash-m4-builder-restore-") as folder:
        site = Path(folder) / "site"
        db = site / "database" / "cms.sqlite"
        snapshot = Path(folder) / "m3-snapshot"
        legacy = {"type": "qa-a/banner", "values": {"title": "M3 snapshot"}}
        with running(m3_binary, db) as base:
            auth = setup(base)
            package = theme_zip("qa-a/banner", '<p data-m3="true">{{ values.title }}</p>',
                                fields={"title": "Title"})
            checked(install(base, auth, package), 201, "M3 theme")
            checked(json_request(base, "POST", "/save", legacy, auth), 200, "M3 homepage")
        shutil.copytree(site, snapshot)
        with running(binary, db) as base:
            auth = auth_login(base)
            state = get(base, auth)
            assert state["source"] == "legacy"
            assert state["document"]["sections"]
            checked(save(base, auth, state["document"], state["version"]), 200, "Upgrade Save")
            assert 'data-m3="true"' in checked(request(base, "GET", "/"), 200, "M4 public")[1]
        # Restore the *copy* with the original M3 binary. Never launch an M3
        # binary against a database already migrated to 0013.
        restored = snapshot / "database" / "cms.sqlite"
        with running(m3_binary, restored) as base:
            auth = auth_login(base)
            assert json.loads(checked(request(base, "GET", "/page", headers=auth), 200, "Restored M3")[1]) == legacy
            assert 'data-m3="true"' in checked(request(base, "GET", "/"), 200, "Restored public")[1]


def test_unknown_persisted_element(binary):
    """An unknown historical payload may round-trip but never be introduced/edited."""
    with TemporaryDirectory(prefix="opendash-m4-builder-unknown-") as directory:
        db = Path(directory) / "site" / "database" / "cms.sqlite"
        with running(binary, db) as base:
            admin = setup(base)
            checked(save(base, admin, {"schema_version": 1, "sections": []}, 1),
                    200, "Seed persisted layout")
        unknown = tree(element("other/missing", {"historical": "retain & <tag>"}, "unknown-node"))
        with closing(sqlite3.connect(db)) as connection, connection:
            connection.execute(
                "UPDATE builder_homepage SET document_json = ?, version = 3 WHERE id = 1",
                (json.dumps(unknown),),
            )
        with running(binary, db) as base:
            admin = auth_login(base)
            state = get(base, admin)
            assert state["document"] == unknown and state["version"] == 3
            diagnostics = json.loads(checked(preview(base, admin, unknown), 200,
                                            "Preserved unknown Preview")[1])["diagnostics"]
            assert diagnostics == [{"node_id": "unknown-node", "code": "descriptor_missing"}]
            assert "historical" not in checked(request(base, "GET", "/"), 200, "Unknown public")[1]
            changed = json.loads(json.dumps(unknown))
            leaf = changed["sections"][0]["children"][0]["children"][0]["children"][0]
            leaf["props"]["historical"] = "tampered"
            checked(save(base, admin, changed, 3), 400, "Unknown cannot be edited")
            reordered = tree(paragraph("New sibling"), unknown["sections"][0]["children"][0]["children"][0]["children"][0])
            checked(save(base, admin, reordered, 3), 400, "Unknown cannot be repositioned")
            checked(save(base, admin, unknown, 3), 200, "Unknown unchanged Save")
            assert get(base, admin)["document"] == unknown
            assert get(base, admin)["version"] == 4


def main(binary, m3_binary):
    test_new_site(binary)
    print("PASS Builder: empty Open, Preview read-only, security, CAS, image ACL, restart")
    test_legacy_theme_switch(binary)
    print("PASS Builder: M3 projection, 409/412, A-B-C-A, diagnostics, concurrent Save")
    test_unknown_persisted_element(binary)
    print("PASS Builder: unresolved historical payload round-trip without mutation")
    if m3_binary:
        test_m3_snapshot_restore(binary, m3_binary)
        print("PASS Builder: M3 snapshot restore with the actual M3 binary")
    else:
        print("NOT_RUN: M3 binary snapshot restore (--m3-binary not provided)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--m3-binary", type=Path)
    args = parser.parse_args()
    main(args.binary.resolve(), args.m3_binary.resolve() if args.m3_binary else None)
