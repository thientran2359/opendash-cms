"""M3.2 CPT API smoke test against a real OpenDash binary and temporary SQLite.

Run from the project root:
    python3 cms/scripts/content_types_smoke.py --binary cms/target/debug/opendash
"""

import argparse
import json
import os
import sqlite3
import subprocess
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing, contextmanager
from pathlib import Path
from queue import Queue
from tempfile import TemporaryDirectory
from threading import Thread
from urllib.error import HTTPError
from urllib.request import Request, urlopen


@contextmanager
def running(binary: Path, db: Path):
    env = dict(
        os.environ,
        OPENDASH_PORT="0",
        OPENDASH_SITE_ROOT=str(db.parent.parent),
        OPENDASH_DB_PATH=str(db),
    )
    with subprocess.Popen(
        [str(binary)], env=env, stdout=subprocess.PIPE, text=True, encoding="utf-8"
    ) as process:
        output = Queue()
        Thread(target=lambda: output.put(process.stdout.readline()), daemon=True).start()
        try:
            line = output.get(timeout=30).strip()
            assert line.startswith("OpenDash: http://127.0.0.1:") and line.endswith("/builder"), line
            yield line.removeprefix("OpenDash: ").removesuffix("/builder")
        finally:
            process.terminate()


def request(base, path, body=None, headers=None):
    headers = dict(headers or {})
    data = None
    if body is not None:
        data = json.dumps(body, ensure_ascii=False).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = Request(base + path, data=data, headers=headers)
    try:
        with urlopen(req, timeout=10) as response:
            return response.status, response.read().decode("utf-8"), response.headers.get("Set-Cookie")
    except HTTPError as error:
        with error:
            return error.code, error.read().decode("utf-8"), error.headers.get("Set-Cookie")


def login(base, username, password):
    status, body, cookie = request(base, "/auth/login", {"username": username, "password": password})
    assert status == 200, (status, body)
    return {
        "Cookie": cookie.split(";")[0],
        "X-CSRF-Token": json.loads(body)["csrf_token"],
    }


def sample_content_type(key="portfolio_item"):
    return {
        "key": key,
        "label": "Dự án",
        "fields": [
            {"key": "summary", "label": "Tóm tắt", "type": "long_text", "required": False},
            {
                "key": "status",
                "label": "Trạng thái",
                "type": "select",
                "required": True,
                "options": [
                    {"value": "new", "label": "Mới"},
                    {"value": "done", "label": "Đã hoàn tất"},
                ],
            },
        ],
    }


def test_fresh_site(binary):
    with TemporaryDirectory(prefix="opendash-content-") as folder:
        db = Path(folder) / "site" / "database" / "opendash.sqlite"
        with running(binary, db) as base:
            endpoint = "/api/admin/content-types"
            assert request(base, endpoint)[0] == 401
            admin = {"username": "admin", "password": "password123"}
            assert request(base, "/auth/setup", admin)[0] == 201
            auth = login(base, **admin)
            builtins = json.loads(request(base, endpoint, headers=auth)[1])
            assert [item["descriptor"]["key"] for item in builtins] == ["page", "post"], builtins
            assert all(item["owner"] == {"kind": "core"} and item["status"] == "active" for item in builtins)
            assert request(base, endpoint, sample_content_type("post"), auth)[0] == 409
            assert request(base, endpoint, sample_content_type("page"), auth)[0] == 409

            definition = sample_content_type()
            assert request(base, endpoint, definition)[0] == 401
            assert request(base, endpoint, definition, {"Cookie": auth["Cookie"]})[0] == 403
            assert request(base, endpoint, definition, {**auth, "X-CSRF-Token": "wrong"})[0] == 403
            assert request(base, endpoint, definition, {**auth, "Origin": "http://evil.invalid"})[0] == 403

            invalid = sample_content_type("Not-Snake")
            assert request(base, endpoint, invalid, auth)[0] == 400
            invalid_field = sample_content_type()
            invalid_field["fields"][1]["options"] = []
            assert request(base, endpoint, invalid_field, auth)[0] == 400

            status, body, _ = request(base, endpoint, definition, auth)
            assert status == 201, (status, body)
            created = json.loads(body)
            assert created["owner"] == {"kind": "user"}
            assert created["status"] == "active"
            assert created["descriptor"]["fields"][1]["options"][1]["value"] == "done"
            assert request(base, endpoint, definition, auth)[0] == 409

            second = sample_content_type("news_item")
            with ThreadPoolExecutor(max_workers=4) as executor:
                statuses = list(executor.map(lambda _: request(base, endpoint, second, auth)[0], range(4)))
            assert statuses.count(201) == 1 and statuses.count(409) == 3, statuses

            status, listing, _ = request(base, endpoint, headers=auth)
            assert status == 200
            keys = [item["descriptor"]["key"] for item in json.loads(listing)]
            assert keys == ["news_item", "page", "portfolio_item", "post"], keys

            editor = {"username": "editor", "password": "password123"}
            assert request(base, "/api/admin/users", editor, auth)[0] == 201
            editor_auth = login(base, **editor)
            assert request(base, endpoint, headers=editor_auth)[0] == 200
            assert request(base, endpoint, sample_content_type("editor_type"), editor_auth)[0] == 403
            assert request(base, endpoint, {"key": "huge", "label": "X" * 600_000}, auth)[0] == 413

        with closing(sqlite3.connect(db)) as conn, conn:
            assert conn.execute("SELECT COUNT(*) FROM content_types").fetchone()[0] == 4
            assert conn.execute(
                "SELECT owner_kind, owner_id, provider_status FROM content_types WHERE key = 'portfolio_item'"
            ).fetchone() == ("user", None, "active")
            try:
                conn.execute(
                    "INSERT INTO content_types (key, definition_json, owner_kind) VALUES (?, ?, 'user')",
                    ("broken", "not json"),
                )
            except sqlite3.IntegrityError:
                pass
            else:
                raise AssertionError("Database accepted an invalid JSON descriptor")

            # Provider removal must never cascade into CPT deletion. Simulate a
            # formerly plugin-owned inactive descriptor, preserving its fields.
            conn.execute(
                "UPDATE content_types SET owner_kind = 'plugin', owner_id = 'missing-plugin', "
                "provider_status = 'provider_inactive' WHERE key = 'portfolio_item'"
            )
            conn.commit()

        with running(binary, db) as base:
            auth = login(base, "admin", "password123")
            status, body, _ = request(base, "/api/admin/content-types", headers=auth)
            assert status == 200
            items = {item["descriptor"]["key"]: item for item in json.loads(body)}
            assert len(items) == 4
            assert items["portfolio_item"]["owner"] == {"kind": "plugin", "id": "missing-plugin"}
            assert items["portfolio_item"]["status"] == "provider_inactive"
            assert items["portfolio_item"]["descriptor"]["fields"][1]["type"] == "select"


def test_upgrading_legacy_database(binary, fixture):
    with TemporaryDirectory(prefix="opendash-content-upgrade-") as folder:
        db = Path(folder) / "site" / "database" / "opendash.sqlite"
        db.parent.mkdir(parents=True)
        # The committed fixture is immutable; read it without writing WAL/SHM
        # sidecar files beside the source database in the repository.
        with closing(sqlite3.connect(f"{fixture.as_uri()}?mode=ro&immutable=1", uri=True)) as source:
            with closing(sqlite3.connect(db)) as target:
                source.backup(target)
                before = target.execute("SELECT block_id, values_json FROM home_page WHERE id = 1").fetchone()

        with running(binary, db) as base:
            admin = {"username": "admin", "password": "password123"}
            assert request(base, "/auth/setup", admin)[0] == 201
            auth = login(base, **admin)
            assert request(base, "/api/admin/content-types", sample_content_type(), auth)[0] == 201
            assert request(base, "/")[0] == 200

        with closing(sqlite3.connect(db)) as conn, conn:
            after = conn.execute("SELECT block_id, values_json FROM home_page WHERE id = 1").fetchone()
            assert after == before, (before, after)
            assert conn.execute("SELECT COUNT(*) FROM content_types").fetchone()[0] == 3
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version = 4").fetchone() == (1,)
            assert conn.execute("SELECT success FROM _sqlx_migrations WHERE version = 8").fetchone() == (1,)


if __name__ == "__main__":
    cms_dir = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=cms_dir / "target/debug/opendash")
    parser.add_argument("--baseline-db", type=Path, default=cms_dir / "fixtures/baseline_0001.sqlite")
    args = parser.parse_args()
    test_fresh_site(args.binary.resolve())
    test_upgrading_legacy_database(args.binary.resolve(), args.baseline_db.resolve())
    print("M3.2 CONTENT TYPES SMOKE: PASS (auth, CSRF, validation, persistence, legacy upgrade)")
