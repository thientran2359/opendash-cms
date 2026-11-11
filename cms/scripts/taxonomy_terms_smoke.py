"""M4-S03-T01 category/tag HTTP and SQLite acceptance on disposable sites.

From repository root:
  python3 cms/scripts/taxonomy_terms_smoke.py --binary cms/target/debug/opendash

Optional --pre-m4-binary points to an actual M3 executable for migration and
snapshot rollback, with an isolated database containing Post/Page/CPT data.
This script never touches the user's active site.
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


def checked(result, expected, context):
    assert result[0] == expected, (context, expected, result[0], result[1][:350])
    return result


def get(base, auth=None, query=""):
    return api(base, "GET", TERMS + query, headers=auth)


def create(base, auth, kind, name, slug, parent_id=None):
    return api(base, "POST", TERMS, {
        "kind": kind, "name": name, "slug": slug, "parent_id": parent_id,
    }, headers=auth)


def patch_term(base, auth, term_id, version=None, **changes):
    headers = {
        **(auth or {}),
        **({"If-Match": f'"v{version}"'} if version is not None else {}),
    }
    return api(base, "PATCH", f"{TERMS}/{term_id}", changes, headers=headers)


def delete_term(base, auth, term_id, version=None):
    return api(base, "DELETE", f"{TERMS}/{term_id}", headers={
        **(auth or {}),
        **({"If-Match": f'"v{version}"'} if version is not None else {}),
    })


def term_created(result, context):
    _, data, headers = checked(result, 201, context)
    term = json.loads(data)
    assert headers.get("ETag") == f'"v{term["version"]}"', (context, headers)
    assert term["version"] == 1 and term["id"] > 0
    assert headers.get("Cache-Control") == "no-store"
    return term


def smoke(binary: Path):
    with TemporaryDirectory(prefix="opendash-m4-s03-terms-") as folder:
        db = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(binary, db) as base:
            checked(get(base), 401, "guest GET")
            checked(create(base, {}, "category", "Guest", "guest"), 401, "guest create")
            checked(api(base, "DELETE", f"{TERMS}/1"), 401, "guest delete")
            checked(request(base, "/auth/setup", {
                "username": "termadmin", "password": "password123",
            }), 201, "setup")
            admin = login(base, "termadmin", "password123")
            checked(request(base, "/api/admin/users", {
                "username": "termeditor", "password": "password123", "role": "editor",
            }, admin), 201, "editor setup")
            editor = login(base, "termeditor", "password123")
            checked(get(base, editor), 200, "editor read")
            checked(create(base, editor, "tag", "Editor", "editor"), 403, "editor create")
            checked(patch_term(base, editor, 1, 1, name="Edit"), 403, "editor patch")
            checked(delete_term(base, editor, 1, 1), 403, "editor delete")

            checked(create(base, {"Cookie": admin["Cookie"]}, "category", "A", "a"),
                    403, "missing CSRF")
            checked(create(base, {**admin, "X-CSRF-Token": "wrong"},
                           "category", "A", "a"), 403, "invalid CSRF")
            checked(api(base, "POST", TERMS, {
                "kind": "category", "name": "A", "slug": "a",
            }, {**admin, "Origin": "https://evil.example"}), 403, "invalid Origin")

            root = term_created(create(base, admin, "category", "  Tin tức  ",
                                       "tin-tuc"), "root")
            assert root["name"] == "Tin tức" and root["parent_id"] is None
            child = term_created(create(base, admin, "category", "Báo mới",
                                        "bao-moi", root["id"]), "child")
            tag = term_created(create(base, admin, "tag", "Báo mới",
                                      "bao-moi"), "flat tag same slug as category")
            assert tag["parent_id"] is None
            checked(create(base, admin, "category", "Duplicate", "bao-moi"),
                    409, "duplicate within kind")
            checked(create(base, admin, "tag", "Duplicate", "bao-moi"),
                    409, "duplicate tag")
            checked(create(base, admin, "tag", "Invalid", "has-parent",
                           root["id"]), 400, "tag must be flat")
            checked(create(base, admin, "category", "Invalid", "wrong-parent",
                           tag["id"]), 400, "category parent must be category")
            checked(create(base, admin, "category", "Invalid", "unknown-parent",
                           999999), 400, "unknown parent")
            for name, slug in (("", "empty-name"), (" " * 4, "blank"),
                               ("X" * 121, "long-name"), ("Valid", "Bad Slug"),
                               ("Valid", "x" * 65), ("Valid", "")):
                checked(create(base, admin, "category", name, slug),
                        400, f"invalid term {slug[:20]}")

            status, payload, headers = checked(get(base, admin), 200, "list all")
            data = json.loads(payload)
            assert data["total"] == 3 and len(data["items"]) == 3
            assert headers.get("Cache-Control") == "no-store"
            assert [(v["name"], v["id"]) for v in data["items"]] == sorted(
                (v["name"], v["id"]) for v in data["items"])
            assert json.loads(checked(get(base, editor, "?kind=tag"), 200,
                                      "editor tag filter")[1])["total"] == 1
            assert json.loads(checked(get(base, admin, "?kind=category&limit=1&offset=1"),
                                      200, "pagination")[1])["total"] == 2
            for invalid_query in ("?kind=invalid", "?limit=101", "?limit=0",
                                  "?offset=-1", "?offset=foo"):
                checked(get(base, admin, invalid_query), 400, "query " + invalid_query)

            checked(patch_term(base, admin, child["id"], name="Khác"), 428, "missing If-Match")
            checked(api(base, "PATCH", f"{TERMS}/{child['id']}", {"name": "Khác"}, {
                **admin, "If-Match": "v1",
            }), 400, "malformed If-Match")
            result = checked(patch_term(base, admin, child["id"], 1,
                                        name="  Bản tin  ", slug="ban-tin"), 200, "rename slug")
            updated = json.loads(result[1])
            assert updated["id"] == child["id"] and updated["version"] == 2
            assert updated["name"] == "Bản tin" and updated["slug"] == "ban-tin"
            assert result[2].get("ETag") == '"v2"'
            checked(patch_term(base, admin, child["id"], 1, name="stale"),
                    412, "stale If-Match")
            checked(patch_term(base, admin, root["id"], 1, parent_id=child["id"]),
                    409, "direct cycle")
            checked(patch_term(base, admin, tag["id"], 1, parent_id=root["id"]),
                    400, "tag cannot be reparented")
            checked(api(base, "PATCH", f"{TERMS}/{tag['id']}",
                        {"kind": "category"}, {**admin, "If-Match": '"v1"'}),
                    400, "kind immutable")
            checked(delete_term(base, admin, root["id"], 1), 409, "cannot delete with child")

            # A category root is depth 1. Creating a ninth level must fail.
            parent = child
            levels = [root, child]
            for depth in range(3, 9):
                parent = term_created(create(base, admin, "category", f"Level {depth}",
                                             f"level-{depth}", parent["id"]),
                                      f"depth {depth}")
                levels.append(parent)
            checked(create(base, admin, "category", "Too deep", "level-9",
                           parent["id"]), 409, "depth 9 prohibited")
            moving = term_created(create(base, admin, "category", "Moving",
                                         "moving"), "moving branch")
            checked(patch_term(base, admin, moving["id"], 1,
                               parent_id=levels[-1]["id"]), 409,
                    "moving a subtree to depth 9")
            checked(patch_term(base, admin, root["id"], 1,
                               parent_id=levels[2]["id"]), 409,
                    "reparent creates indirect cycle")

            # Two stale-independent reparent operations cannot form a cycle.
            left = term_created(create(base, admin, "category", "Left", "left"), "left")
            right = term_created(create(base, admin, "category", "Right", "right"), "right")
            with ThreadPoolExecutor(max_workers=2) as executor:
                attempts = list(executor.map(
                    lambda spec: patch_term(
                        base, admin, spec[0], 1, parent_id=spec[1])[0],
                    ((left["id"], right["id"]), (right["id"], left["id"])),
                ))
            assert sorted(attempts) == [200, 409], attempts

            checked(delete_term(base, admin, tag["id"]), 428, "delete If-Match")
            checked(delete_term(base, admin, tag["id"], 1), 204, "delete tag")
            checked(delete_term(base, admin, tag["id"], 1), 404, "deleted ID")
            checked(patch_term(base, admin, 999999, 1, name="missing"), 404, "missing ID")

        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version=12").fetchone() == (1,)
            assert conn.execute("SELECT COUNT(*) FROM taxonomy_terms WHERE kind='tag'").fetchone() == (0,)
            rows = conn.execute("SELECT id,parent_id FROM taxonomy_terms WHERE kind='category'").fetchall()
            parents = dict(rows)
            for category in parents:
                seen = set()
                here = category
                while here is not None:
                    assert here not in seen, f"cycle found at {category}"
                    seen.add(here)
                    here = parents.get(here)
                assert len(seen) <= 8, (category, len(seen))

        with running(binary, db) as base:
            admin = login(base, "termadmin", "password123")
            entries = json.loads(checked(get(base, admin), 200, "restart")[1])["items"]
            assert any(item["id"] == child["id"] and item["slug"] == "ban-tin"
                       and item["version"] == 2 for item in entries)
            assert len(entries) == len(parents)
        print("PASS: fresh taxonomy ACL/CSRF/Origin, kind/slug, pagination, CAS, cycles/depth, race, restart")


def upgrade_and_restore(binary: Path, pre_m4_binary: Path):
    """Run pre-0012 release on a copy, migrate, then restore the saved snapshot."""
    with TemporaryDirectory(prefix="opendash-m4-s03-upgrade-") as folder:
        db = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(pre_m4_binary, db) as base:
            checked(request(base, "/auth/setup", {
                "username": "upgradeadmin", "password": "password123",
            }), 201, "pre-M4 setup")
            admin = login(base, "upgradeadmin", "password123")
            checked(request(base, "/api/admin/content-types", {
                "key": "upgrade_cpt", "label": "Upgraded CPT",
                "fields": [{"key": "summary", "label": "Summary", "type": "short_text"}],
            }, admin), 201, "pre-M4 CPT")
            for key in ("post", "page", "upgrade_cpt"):
                checked(request(base, f"/api/admin/content-types/{key}/entries", {
                    "title": "Giữ nguyên " + key,
                    "slug": "preserved-" + key.replace("_", "-"),
                    "values": {"summary": "Preserved"} if key == "upgrade_cpt" else {},
                }, admin), 201, "pre-M4 " + key)
        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version=11").fetchone() == (1,)
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version=12").fetchone() is None
            before = conn.execute("SELECT id,content_type_key,title,slug FROM content_entries ORDER BY id").fetchall()
            assert len(before) == 3
            snapshot = Path(folder) / "pre-0012-snapshot.sqlite"
            with closing(sqlite3.connect(snapshot)) as dest:
                conn.backup(dest)

        with running(binary, db) as base:
            admin = login(base, "upgradeadmin", "password123")
            term_created(create(base, admin, "category", "Sau nâng cấp", "sau-nang-cap"),
                         "upgrade term")
            checked(get(base, admin), 200, "upgrade term read")
            for key in ("post", "page", "upgrade_cpt"):
                checked(api(base, "GET", f"/api/admin/content-types/{key}/entries",
                            headers=admin), 200, "upgrade entry " + key)
        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute("SELECT id,content_type_key,title,slug FROM content_entries ORDER BY id").fetchall() == before
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version=12").fetchone() == (1,)

        # A proper rollback restores pre-migration SQLite and runs with the matching binary.
        for suffix in ("", "-wal", "-shm"):
            candidate = Path(str(db) + suffix)
            if candidate.exists():
                candidate.unlink()
        shutil.copy2(snapshot, db)
        with running(pre_m4_binary, db) as base:
            admin = login(base, "upgradeadmin", "password123")
            checked(api(base, "GET", "/api/admin/content-types/post/entries",
                        headers=admin), 200, "rollback pre-M4 binary")
        with closing(sqlite3.connect(db)) as conn:
            assert conn.execute("SELECT id,content_type_key,title,slug FROM content_entries ORDER BY id").fetchall() == before
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version=12").fetchone() is None
        print("PASS: pre-M4 Post/Page/CPT migration, snapshot and matching-binary rollback")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("cms/target/debug/opendash"))
    parser.add_argument("--pre-m4-binary", type=Path,
                        help="Optional actual pre-0012 release binary for migration+rollback")
    args = parser.parse_args()
    smoke(args.binary.resolve())
    if args.pre_m4_binary:
        upgrade_and_restore(args.binary.resolve(), args.pre_m4_binary.resolve())
