"""M3.3 entry CRUD live HTTP/SQLite smoke tests; never use the real site DB.

From repository root:
    python3 cms/scripts/content_entries_smoke.py --binary cms/target/debug/opendash
"""

import argparse
import json
import sqlite3
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from content_types_smoke import login, request, running


def api(base, method, path, body=None, headers=None):
    headers = dict(headers or {})
    data = None if body is None else json.dumps(body, ensure_ascii=False).encode("utf-8")
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = Request(base + path, data=data, headers=headers, method=method)
    try:
        with urlopen(req, timeout=10) as response:
            return response.status, response.read().decode("utf-8"), response.headers
    except HTTPError as error:
        with error:
            return error.code, error.read().decode("utf-8"), error.headers


def definition(key="portfolio"):
    return {
        "key": key,
        "label": "Dự án",
        "fields": [
            {"key": "caption", "label": "Chú thích", "type": "short_text", "required": True},
            {"key": "description", "label": "Mô tả", "type": "long_text"},
            {"key": "html", "label": "Nội dung", "type": "rich_text"},
            {"key": "price", "label": "Giá", "type": "number", "required": True},
            {"key": "active", "label": "Kích hoạt", "type": "boolean", "required": True},
            {"key": "date", "label": "Ngày", "type": "date"},
            {
                "key": "state", "label": "Trạng thái", "type": "select", "required": True,
                "options": [{"value": "open", "label": "Đang mở"}, {"value": "closed", "label": "Đã đóng"}],
            },
            {"key": "cover", "label": "Ảnh", "type": "image"},
            {"key": "gallery", "label": "Album", "type": "gallery"},
        ],
    }


def entry(slug="demo-project"):
    return {
        "slug": slug,
        "title": "Dự án đầu tiên",
        "values": {
            "caption": "Giới thiệu", "description": "Dòng một\nDòng hai",
            "html": "<strong>Chưa render</strong>", "price": 0, "active": False,
            "date": "2024-02-29", "state": "open",
            "cover": "asset_1", "gallery": ["asset_1", "asset_2"],
        },
    }


def assert_status(got, expected, description):
    assert got[0] == expected, (description, got[0], got[1])


def check_crud(binary):
    with TemporaryDirectory(prefix="opendash-m3-3-") as folder:
        db = Path(folder) / "database" / "opendash.sqlite"
        list_path = "/api/admin/content-types/portfolio/entries"
        with running(binary, db) as base:
            assert_status(api(base, "GET", list_path), 401, "Anonymous list")
            assert_status(api(base, "POST", list_path, entry()), 401, "Anonymous create")
            assert_status(api(base, "POST", list_path, entry(), {"X-CSRF-Token": "fake"}), 401, "No session")
            assert_status(api(base, "GET", "/api/admin/content-types/missing/entries"), 401, "Auth first")

            admin_credentials = {"username": "admin", "password": "password123"}
            assert_status(request(base, "/auth/setup", admin_credentials), 201, "Setup")
            admin = login(base, **admin_credentials)
            assert_status(request(base, "/api/admin/content-types", definition(), admin), 201, "Create CPT")
            assert_status(request(base, "/api/admin/content-types", definition("news"), admin), 201, "Create second CPT")
            assert_status(api(base, "GET", "/api/admin/content-types/missing/entries", headers=admin), 404, "Missing CPT")
            assert_status(api(base, "GET", list_path, headers=admin), 200, "Empty list")
            assert_status(api(base, "POST", list_path, entry(), {"Cookie": admin["Cookie"]}), 403, "No CSRF")
            assert_status(api(base, "POST", list_path, entry(), {**admin, "Origin": "http://evil.invalid"}), 403, "Bad Origin")
            assert_status(api(base, "POST", list_path, entry(), admin), 201, "Create entry")
            created_status, created_body, created_headers = api(base, "GET", list_path, headers=admin)
            assert created_status == 200 and len(json.loads(created_body)) == 1

            invalid_cases = [
                {**entry("bad-unknown"), "values": {**entry()["values"], "unknown": 1}},
                {**entry("bad-required"), "values": {k: v for k, v in entry()["values"].items() if k != "active"}},
                {**entry("bad-select"), "values": {**entry()["values"], "state": "other"}},
                {**entry("bad-number"), "values": {**entry()["values"], "price": "100"}},
                {**entry("bad-bool"), "values": {**entry()["values"], "active": "false"}},
                {**entry("bad-date"), "values": {**entry()["values"], "date": "2025-02-29"}},
                {**entry("bad-short"), "values": {**entry()["values"], "caption": "a" * 513}},
                {**entry("bad-image"), "values": {**entry()["values"], "cover": "../bad"}},
                {**entry("bad-gallery"), "values": {**entry()["values"], "gallery": ["asset_1", "asset_1"]}},
                {**entry("bad-slug"), "slug": "Invalid Slug"},
            ]
            for invalid in invalid_cases:
                assert_status(api(base, "POST", list_path, invalid, admin), 400, "Invalid entry")
            assert_status(api(base, "POST", list_path, entry(), admin), 409, "Duplicate slug")

            editor_credentials = {"username": "editor", "password": "password123"}
            assert_status(request(base, "/api/admin/users", editor_credentials, headers=admin), 201, "Create editor")
            editor = login(base, **editor_credentials)
            assert_status(api(base, "POST", list_path, entry("editor-project"), editor), 201, "Editor may create")
            assert_status(api(base, "POST", "/api/admin/content-types", definition("forbidden"), editor), 403, "Editor may not create CPT")
            cross_type = api(base, "POST", "/api/admin/content-types/news/entries", entry(), editor)
            assert_status(cross_type, 201, "Same slug allowed in different CPT")

            first = json.loads(api(base, "GET", list_path + "?limit=100&offset=0", headers=admin)[1])
            assert len(first) == 2 and first[0]["slug"] == "editor-project", first
            assert_status(api(base, "GET", list_path + "?limit=0", headers=admin), 400, "Invalid pagination")
            assert_status(api(base, "GET", list_path + "?offset=-1", headers=admin), 400, "Negative pagination")
            assert len(json.loads(api(base, "GET", list_path + "?limit=1&offset=1", headers=editor)[1])) == 1

            target_id = first[1]["id"]
            item = f"{list_path}/{target_id}"
            get_status, get_body, get_headers = api(base, "GET", item, headers=editor)
            assert get_status == 200 and get_headers["ETag"] == '"v1"'
            assert json.loads(get_body)["values"]["active"] is False
            assert_status(api(base, "PUT", item, entry(), editor), 428, "Missing If-Match")
            assert_status(api(base, "PUT", item, entry(), {**editor, "If-Match": '"v00"'}), 400, "Invalid ETag")
            assert_status(api(base, "PUT", item, entry(), {**editor, "If-Match": '"v1"', "X-CSRF-Token": "wrong"}), 403, "Bad CSRF update")
            assert_status(api(base, "DELETE", item, headers={**editor, "If-Match": '"v1"', "X-CSRF-Token": "wrong"}), 403, "Bad CSRF delete")

            updated = entry()
            updated["title"] = "Đã sửa bởi Editor"
            updated["values"]["html"] = "<script>not publicly rendered</script>"
            put = api(base, "PUT", item, updated, {**editor, "If-Match": '"v1"'})
            assert_status(put, 200, "First update")
            assert put[2]["ETag"] == '"v2"'
            assert_status(api(base, "PUT", item, updated, {**editor, "If-Match": '"v1"'}), 412, "Stale update")
            assert_status(api(base, "DELETE", item, headers={**admin, "If-Match": '"v1"'}), 412, "Stale deletion")
            assert_status(api(base, "PUT", item, entry("editor-project"), {**admin, "If-Match": '"v2"'}), 409, "Update slug collision")
            assert json.loads(api(base, "GET", item, headers=admin)[1])["version"] == 2

            concurrent = entry()
            concurrent["title"] = "Parallel edit"
            with ThreadPoolExecutor(max_workers=2) as pool:
                outcomes = list(pool.map(lambda _: api(base, "PUT", item, concurrent, {**admin, "If-Match": '"v2"'})[0], range(2)))
            assert sorted(outcomes) == [200, 412], outcomes
            assert_status(api(base, "GET", item, headers=admin), 200, "Get after concurrent update")
            assert api(base, "GET", item, headers=admin)[2]["ETag"] == '"v3"'

            other = next(record for record in first if record["slug"] == "editor-project")
            other_url = f"{list_path}/{other['id']}"
            assert_status(api(base, "DELETE", other_url, headers={**editor, "If-Match": '"v1"'}), 204, "Editor delete")
            assert_status(api(base, "GET", other_url, headers=editor), 404, "Deleted entry missing")
            assert_status(api(base, "DELETE", other_url, headers={**admin, "If-Match": '"v1"'}), 404, "Deleting nonexistent")

            assert_status(api(base, "GET", "/api/admin/content-types/news/entries", headers=admin), 200, "Second CPT list")
            assert_status(api(base, "GET", list_path + "/999999", headers=admin), 404, "Missing entry")

        with sqlite3.connect(db) as con:
            con.execute("PRAGMA foreign_keys = ON")
            assert con.execute("SELECT COUNT(*) FROM content_entries WHERE content_type_key = 'portfolio'").fetchone()[0] == 1
            assert con.execute("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 5 AND success = 1").fetchone()[0] == 1
            try:
                con.execute("DELETE FROM content_types WHERE key = 'portfolio'")
            except sqlite3.IntegrityError:
                pass
            else:
                raise AssertionError("CPT deletion with saved entries must be restricted")
            con.execute(
                "UPDATE content_types SET owner_kind = 'plugin', owner_id = 'no-longer-installed', "
                "provider_status = 'provider_inactive' WHERE key = 'portfolio'"
            )
            con.commit()

        with running(binary, db) as base:
            editor = login(base, **editor_credentials)
            record = api(base, "GET", item, headers=editor)
            assert_status(record, 200, "Persisted entry after restart")
            assert json.loads(record[1])["version"] == 3
            update = entry()
            update["title"] = "Editor vẫn sửa được CPT nguồn inactive"
            assert_status(api(base, "PUT", item, update, {**editor, "If-Match": '"v3"'}), 200, "Inactive provider editable")
            assert json.loads(api(base, "GET", item, headers=editor)[1])["title"] == update["title"]


def check_legacy_upgrade(binary, fixture):
    with TemporaryDirectory(prefix="opendash-m3-3-upgrade-") as folder:
        db = Path(folder) / "database" / "opendash.sqlite"
        db.parent.mkdir(parents=True)
        with closing(sqlite3.connect(f"{fixture.as_uri()}?mode=ro&immutable=1", uri=True)) as src:
            with closing(sqlite3.connect(db)) as dest:
                src.backup(dest)
                before = dest.execute("SELECT block_id, values_json FROM home_page WHERE id = 1").fetchone()
        with running(binary, db) as base:
            credentials = {"username": "admin", "password": "password123"}
            assert_status(request(base, "/auth/setup", credentials), 201, "Upgrade setup")
            admin = login(base, **credentials)
            assert_status(request(base, "/api/admin/content-types", definition(), admin), 201, "Upgrade CPT")
            assert_status(api(base, "POST", "/api/admin/content-types/portfolio/entries", entry(), admin), 201, "Upgrade entry")
            assert_status(api(base, "GET", "/api/admin/content-types/portfolio/entries", headers=admin), 200, "Upgrade list")
        with sqlite3.connect(db) as con:
            assert con.execute("SELECT block_id, values_json FROM home_page WHERE id = 1").fetchone() == before
            assert con.execute("SELECT COUNT(*) FROM content_entries").fetchone() == (1,)
            assert con.execute("SELECT success FROM _sqlx_migrations WHERE version = 5").fetchone() == (1,)


if __name__ == "__main__":
    cms_dir = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=cms_dir / "target/debug/opendash")
    parser.add_argument("--baseline-db", type=Path, default=cms_dir / "fixtures/baseline_0001.sqlite")
    args = parser.parse_args()
    check_crud(args.binary.resolve())
    check_legacy_upgrade(args.binary.resolve(), args.baseline_db.resolve())
    print("M3.3 CONTENT ENTRIES SMOKE: PASS (CRUD, RBAC, CSRF, fields, CAS, restart, upgrade)")
