"""M4-S04-T01 entry→category/tag contract on disposable SQLite sites.

From repository root:
  python3 cms/scripts/entry_terms_smoke.py --binary cms/target/debug/opendash

Use --pre-entry-binary with a real 0012-era binary to verify forward migration,
preservation of Post/Page/CPT and snapshot rollback with the matching binary.
Never connect to the user's actual CMS database.
"""

import argparse
import json
import shutil
import sqlite3
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
from pathlib import Path
from tempfile import TemporaryDirectory

from content_entries_smoke import api
from content_types_smoke import login, request, running


TERMS = "/api/admin/taxonomy/terms"
ENTRIES = "/api/admin/content-types"


def expect(result, code, label):
    assert result[0] == code, (label, result[0], code, result[1][:500])
    return result


def read(base, path, auth):
    return json.loads(expect(api(base, "GET", path, headers=auth), 200, path)[1])


def add_term(base, auth, kind, name, slug, parent=None):
    payload = {"kind": kind, "name": name, "slug": slug}
    if parent is not None:
        payload["parent_id"] = parent
    return json.loads(expect(api(base, "POST", TERMS, payload, auth), 201, "add term")[1])


def new_entry(base, auth, type_key, slug, term_ids=None):
    entry = {
        "slug": slug,
        "title": "Entry " + slug,
        "values": {"summary": "giữ nội dung"} if type_key == "portfolio" else {},
    }
    if term_ids is not None:
        entry["term_ids"] = term_ids
    return json.loads(expect(api(base, "POST", f"{ENTRIES}/{type_key}/entries", entry, auth),
                             201, f"create {type_key}")[1])


def update(base, auth, item, type_key, *, term_ids=None, omit=False, title=None, slug=None):
    payload = {"slug": slug or item["slug"], "title": title or item["title"], "values": item["values"]}
    if not omit:
        payload["term_ids"] = term_ids
    return api(base, "PUT", f"{ENTRIES}/{type_key}/entries/{item['id']}", payload,
               {**auth, "If-Match": f'"v{item["version"]}"'})


def entry_path(type_key, id_):
    return f"{ENTRIES}/{type_key}/entries/{id_}"


def list_entries(base, auth, type_key, **filters):
    from urllib.parse import urlencode

    query = urlencode(filters)
    return read(base, f"{ENTRIES}/{type_key}/entries" + ("?" + query if query else ""), auth)


def ensure_membership(db, entry_id, wanted):
    with closing(sqlite3.connect(db)) as conn:
        actual = sorted(row[0] for row in conn.execute(
            "SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id", (entry_id,)))
    assert actual == sorted(wanted), (entry_id, actual, wanted)


def test_fresh(binary):
    with TemporaryDirectory(prefix="opendash-m4-s04-") as folder:
        db = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(binary, db) as base:
            expect(request(base, "/auth/setup", {"username": "admin", "password": "password123"}),
                   201, "setup")
            admin = login(base, "admin", "password123")
            expect(request(base, "/api/admin/users", {
                "username": "editor", "password": "password123", "role": "editor",
            }, admin), 201, "editor")
            editor = login(base, "editor", "password123")
            expect(request(base, "/api/admin/content-types", {
                "key": "portfolio", "label": "Dự án",
                "fields": [{"key": "summary", "label": "Tóm tắt", "type": "short_text"}],
            }, admin), 201, "custom type")
            assert api(base, "GET", f"{ENTRIES}/post/entries")[0] == 401
            assert api(base, "POST", f"{ENTRIES}/post/entries", {
                "title": "No login", "slug": "no-login", "values": {}, "term_ids": [],
            })[0] == 401
            root = add_term(base, admin, "category", "Tin tức", "tin-tuc")
            child = add_term(base, admin, "category", "Địa phương", "dia-phuong", root["id"])
            other = add_term(base, admin, "category", "Khác", "khac")
            tag = add_term(base, admin, "tag", "Khẩn", "khan")
            second_tag = add_term(base, admin, "tag", "Tổng hợp", "tong-hop")

            posts = [
                new_entry(base, admin, "post", "root-post", [root["id"], tag["id"]]),
                new_entry(base, editor, "post", "child-post", [child["id"], tag["id"]]),
                new_entry(base, admin, "post", "other-post", [other["id"], second_tag["id"]]),
                new_entry(base, admin, "post", "empty-post"),
            ]
            page = new_entry(base, editor, "page", "page-entry", [root["id"], second_tag["id"]])
            custom = new_entry(base, editor, "portfolio", "portfolio-entry", [child["id"], tag["id"]])
            for item, expected in zip(
                posts + [page, custom],
                [[root["id"], tag["id"]], [child["id"], tag["id"]],
                 [other["id"], second_tag["id"]], [], [root["id"], second_tag["id"]],
                 [child["id"], tag["id"]]],
            ):
                assert sorted(item["term_ids"]) == sorted(expected), item
                ensure_membership(db, item["id"], expected)
            assert list_entries(base, editor, "portfolio")[0]["term_ids"] == custom["term_ids"]
            assert read(base, entry_path("page", page["id"]), editor)["term_ids"] == page["term_ids"]

            # Slug changes cannot orphan the site-owned link relation (by entry ID).
            moved_page = json.loads(expect(
                update(base, editor, page, "page", omit=True, slug="renamed-page"),
                200, "rename page slug preserves links")[1])
            assert moved_page["id"] == page["id"]
            assert moved_page["slug"] == "renamed-page"
            assert set(moved_page["term_ids"]) == {root["id"], second_tag["id"]}
            ensure_membership(db, moved_page["id"], moved_page["term_ids"])

            # Two simultaneous editors changing content and links under the
            # same If-Match must yield precisely one complete winner.
            choices = ([root["id"], tag["id"]], [other["id"], second_tag["id"]])
            with ThreadPoolExecutor(max_workers=2) as pool:
                results = list(pool.map(
                    lambda item: update(
                        base, admin if item[0] == 0 else editor, moved_page, "page",
                        title=f"Parallel {item[0]}", term_ids=choices[item[0]],
                    ),
                    [(0,), (1,)],
                ))
            assert sorted(result[0] for result in results) == [200, 412], results
            winner = json.loads(next(result[1] for result in results if result[0] == 200))
            saved_page = read(base, entry_path("page", page["id"]), editor)
            assert saved_page == winner
            assert saved_page["version"] == moved_page["version"] + 1
            assert set(saved_page["term_ids"]) in (set(ids) for ids in choices)
            ensure_membership(db, saved_page["id"], saved_page["term_ids"])

            # Old clients omit term_ids; the existing membership must remain.
            untouched = json.loads(expect(update(base, editor, posts[0], "post", omit=True,
                                                 title="Legacy save"),
                                          200, "legacy update")[1])
            assert sorted(untouched["term_ids"]) == sorted(posts[0]["term_ids"])
            ensure_membership(db, posts[0]["id"], posts[0]["term_ids"])
            assert untouched["version"] == posts[0]["version"] + 1

            # Explicit [] must clear both kinds as one CAS operation.
            clear = json.loads(expect(update(base, editor, untouched, "post", term_ids=[]),
                                      200, "explicit clear")[1])
            assert clear["term_ids"] == [] and clear["version"] == untouched["version"] + 1
            ensure_membership(db, posts[0]["id"], [])
            expect(update(base, editor, untouched, "post", term_ids=[tag["id"]]),
                   412, "stale CAS leaves membership unchanged")
            ensure_membership(db, posts[0]["id"], [])

            # Validation errors never change content, version or link set.
            before = read(base, entry_path("post", posts[1]["id"]), editor)
            for ids, code, label in [
                ([tag["id"], tag["id"]], 400, "duplicate"),
                ([0], 400, "zero"),
                ([-9], 400, "negative"),
                ([99999999], 400, "missing"),
                ([root["id"], "bad"], 422, "noninteger rejected by typed JSON extraction"),
            ]:
                result = update(base, editor, before, "post", term_ids=ids, title="bad " + label)
                expect(result, code, label)
                persisted = read(base, entry_path("post", before["id"]), admin)
                assert persisted == before, (label, persisted, before)
            expect(api(base, "PUT", entry_path("post", before["id"]), {
                "slug": before["slug"], "title": "No CSRF", "values": before["values"],
                "term_ids": [],
            }, {"Cookie": editor["Cookie"], "If-Match": f'"v{before["version"]}"'}),
                   403, "missing CSRF")
            expect(api(base, "PUT", entry_path("post", before["id"]), {
                "slug": before["slug"], "title": "Missing version", "values": before["values"],
                "term_ids": [],
            }, editor), 428, "missing If-Match cannot change term membership")
            assert read(base, entry_path("post", before["id"]), admin) == before
            expect(api(base, "PUT", entry_path("post", before["id"]), {
                "slug": before["slug"], "title": "Bad Origin", "values": before["values"],
                "term_ids": [],
            }, {**editor, "Origin": "http://evil.invalid",
                "If-Match": f'"v{before["version"]}"'}), 403, "bad Origin")

            # Exact matching, descendants, AND category+tag and pagination.
            # Delete/clear did not affect the child-post's relationships.
            exact = list_entries(base, editor, "post", category_id=root["id"])
            assert exact == [], exact
            desc = list_entries(base, editor, "post", category_id=root["id"],
                                include_descendants="true")
            assert [item["id"] for item in desc] == [posts[1]["id"]], desc
            joined = list_entries(base, editor, "post", category_id=root["id"],
                                  tag_id=tag["id"], include_descendants="true")
            assert [item["id"] for item in joined] == [posts[1]["id"]]
            assert list_entries(base, editor, "post", category_id=root["id"],
                                tag_id=second_tag["id"], include_descendants="true") == []
            page1 = list_entries(base, admin, "post", tag_id=tag["id"], limit=1, offset=0)
            page2 = list_entries(base, admin, "post", tag_id=tag["id"], limit=1, offset=1)
            assert len(page1) == 1 and page2 == [] and page1[0]["id"] == posts[1]["id"]
            for invalid in (
                "?category_id=0", "?tag_id=-1", "?category_id=not-a-number",
                "?include_descendants=other", "?tag_id=9999999",
                f"?category_id={tag['id']}", f"?tag_id={root['id']}",
                "?include_descendants=true",
            ):
                expect(api(base, "GET", f"{ENTRIES}/post/entries" + invalid, headers=editor),
                       400, invalid)

            # Reparent/rename preserve membership; DELETE terms still linked 409.
            renamed = json.loads(expect(
                api(base, "PATCH", f"{TERMS}/{child['id']}", {
                    "name": "Đổi tên", "slug": "doi-ten", "parent_id": other["id"],
                }, {**admin, "If-Match": '"v1"'}), 200, "rename/reparent category")[1])
            assert renamed["id"] == child["id"]
            ensure_membership(db, posts[1]["id"], posts[1]["term_ids"])
            assert list_entries(base, admin, "post", category_id=root["id"],
                                include_descendants="true") == []
            assert [item["id"] for item in list_entries(
                base, editor, "post", category_id=other["id"], include_descendants="true")
                ] == [posts[2]["id"], posts[1]["id"]]
            expect(api(base, "DELETE", f"{TERMS}/{tag['id']}", headers={
                **admin, "If-Match": '"v1"'}), 409, "linked tag cannot delete")

            # Publication follows same entry version and doesn't expose drafts.
            for target in (posts[1], posts[2]):
                status = expect(api(base, "POST", entry_path("post", target["id"]) + "/publication",
                                    {"status": "published"},
                                    {**admin, "If-Match": f'"v{target["version"]}"'}),
                                200, "publish")
                assert json.loads(status[1])["term_ids"] == target["term_ids"]
            published = list_entries(base, admin, "post", category_id=other["id"],
                                     include_descendants="true")
            assert len(published) == 2
            preview = read(base, entry_path("post", posts[1]["id"]), editor)
            expect(api(base, "DELETE", entry_path("post", preview["id"]),
                       headers={**editor, "If-Match": f'"v{preview["version"]}"'}),
                   204, "delete entry cascades links")
            ensure_membership(db, posts[1]["id"], [])
            assert read(base, TERMS + "?kind=category", admin)["total"] == 3
            assert len(list_entries(base, admin, "post", category_id=other["id"],
                                    include_descendants="true")) == 1

            # Limits need distinct valid IDs of each kind to avoid duplicate masking.
            extra_categories = [add_term(base, admin, "category", f"C{i}", f"extra-cat-{i}")["id"]
                                for i in range(30)]
            extra_tags = [add_term(base, admin, "tag", f"T{i}", f"extra-tag-{i}")["id"]
                          for i in range(63)]
            newest = read(base, entry_path("post", posts[2]["id"]), admin)
            expect(update(base, admin, newest, "post", term_ids=[
                root["id"], child["id"], other["id"], *extra_categories,
            ]), 400, "33 categories")
            expect(update(base, admin, newest, "post", term_ids=[
                tag["id"], second_tag["id"], *extra_tags,
            ]), 400, "65 tags")
            ensure_membership(db, newest["id"], newest["term_ids"])

            # A term delete competing with linking that same term must not
            # produce a dangling FK, regardless of which writer wins.
            race_tag = add_term(base, admin, "tag", "Race", "race-tag")
            race_entry = read(base, entry_path("post", posts[3]["id"]), admin)
            with ThreadPoolExecutor(max_workers=2) as executor:
                deletion = executor.submit(
                    api, base, "DELETE", f"{TERMS}/{race_tag['id']}", None,
                    {**admin, "If-Match": '"v1"'})
                linking = executor.submit(
                    update, base, editor, race_entry, "post", term_ids=[race_tag["id"]])
                delete_code = deletion.result()[0]
                link_code = linking.result()[0]
            assert (delete_code, link_code) in ((204, 400), (409, 200)), (
                "delete/link race", delete_code, link_code)
            after_race = read(base, entry_path("post", race_entry["id"]), admin)
            if link_code == 200:
                assert after_race["term_ids"] == [race_tag["id"]]
            else:
                assert after_race["term_ids"] == []

        with closing(sqlite3.connect(db)) as con:
            assert con.execute("SELECT success FROM _sqlx_migrations WHERE version=14").fetchone() == (1,)
            assert con.execute("PRAGMA foreign_key_check").fetchall() == []
            assert con.execute("SELECT COUNT(*) FROM entry_terms WHERE entry_id=?", (posts[1]["id"],)).fetchone() == (0,)
            # Disabled CPT providers must never cascade-delete site-owned
            # entries or memberships. Update remains allowed under M3 policy.
            con.execute(
                "UPDATE content_types SET owner_kind='plugin', owner_id='disabled-provider', "
                "provider_status='provider_inactive' WHERE key='portfolio'"
            )
            con.commit()

        with running(binary, db) as base:
            editor = login(base, "editor", "password123")
            persisted = read(base, entry_path("portfolio", custom["id"]), editor)
            assert sorted(persisted["term_ids"]) == sorted(custom["term_ids"])
            refreshed = json.loads(expect(
                update(base, editor, persisted, "portfolio", omit=True,
                       title="Inactive provider retained links"),
                200, "inactive provider update")[1])
            assert sorted(refreshed["term_ids"]) == sorted(custom["term_ids"])
        print("PASS entry_terms fresh: CRUD/CAS/ACL/FK, Post/Page/CPT, pagination/descendants, limits, restart")


def test_upgrade(binary, old_binary):
    with TemporaryDirectory(prefix="opendash-m4-s04-upgrade-") as folder:
        db = Path(folder) / "site" / "database" / "opendash.sqlite"
        snapshot = Path(folder) / "baseline-0012.sqlite"
        with running(old_binary, db) as base:
            expect(request(base, "/auth/setup", {
                "username": "admin", "password": "password123",
            }), 201, "setup M4-P09 binary")
            admin = login(base, "admin", "password123")
            category = add_term(base, admin, "category", "Before upgrade", "before-upgrade")
            post = new_entry(base, admin, "post", "before-upgrade-post")
            page = new_entry(base, admin, "page", "before-upgrade-page")
            expect(request(base, "/api/admin/content-types", {
                "key": "portfolio", "label": "Portfolio",
                "fields": [{"key": "summary", "label": "Summary", "type": "short_text"}],
            }, admin), 201, "custom type")
            cpt = new_entry(base, admin, "portfolio", "before-upgrade-cpt")
            assert category and post and page and cpt
        with closing(sqlite3.connect(db)) as con:
            assert con.execute("SELECT MAX(version) FROM _sqlx_migrations").fetchone() == (12,)
            before = con.execute(
                "SELECT id,content_type_key,slug,title,version FROM content_entries ORDER BY id"
            ).fetchall()
            with closing(sqlite3.connect(snapshot)) as dest:
                con.backup(dest)
        with running(binary, db) as base:
            admin = login(base, "admin", "password123")
            for type_key, item in (("post", post), ("page", page), ("portfolio", cpt)):
                saved = json.loads(expect(update(base, admin, item, type_key,
                                                 term_ids=[category["id"]]), 200, "attach upgrade")[1])
                assert saved["term_ids"] == [category["id"]]
        with closing(sqlite3.connect(db)) as con:
            assert con.execute("SELECT success FROM _sqlx_migrations WHERE version=14").fetchone() == (1,)
            assert con.execute("SELECT COUNT(*) FROM entry_terms").fetchone() == (3,)
            after = con.execute(
                "SELECT id,content_type_key,slug,title,version FROM content_entries ORDER BY id"
            ).fetchall()
            assert [(r[0], r[1], r[2], r[3]) for r in after] == [
                (r[0], r[1], r[2], r[3]) for r in before
            ]
        for ext in ("", "-shm", "-wal"):
            candidate = Path(str(db) + ext)
            if candidate.exists():
                candidate.unlink()
        shutil.copy2(snapshot, db)
        with running(old_binary, db) as base:
            admin = login(base, "admin", "password123")
            assert len(read(base, entry_path("post", post["id"]), admin)["values"]) == 0
        with closing(sqlite3.connect(db)) as con:
            assert con.execute("SELECT MAX(version) FROM _sqlx_migrations").fetchone() == (12,)
            assert con.execute(
                "SELECT id,content_type_key,slug,title,version FROM content_entries ORDER BY id"
            ).fetchall() == before
        print("PASS entry_terms 0012→0014 dev migration + Post/Page/CPT + SQLite snapshot rollback")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash"))
    parser.add_argument("--pre-entry-binary", type=Path, default=None)
    args = parser.parse_args()
    test_fresh(args.binary.resolve())
    if args.pre_entry_binary is not None:
        test_upgrade(args.binary.resolve(), args.pre_entry_binary.resolve())
