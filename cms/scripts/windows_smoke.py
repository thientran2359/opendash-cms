"""Run the Windows CMS slice over HTTP, or write a sample v3 theme ZIP."""

import argparse
from contextlib import contextmanager
import io
import json
import os
from pathlib import Path
from queue import Queue
import socket
import subprocess
from tempfile import TemporaryDirectory
from threading import Thread
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from zipfile import ZIP_DEFLATED, ZipFile


def sample_theme(manifest_path="theme.json"):
    archive = io.BytesIO()
    with ZipFile(archive, "w", ZIP_DEFLATED) as theme:
        theme.writestr(
            manifest_path,
            json.dumps({
                "manifest_version": 3,
                "theme_id": "sample-v3",
                "name": "Sample v3 Theme",
                "version": "1.0.0",
                "templates": [{"kind": "single", "file": "single.html"}],
            }),
        )
        theme.writestr(
            "single.html",
            "<article><h1>{{ entry.title }}</h1>{{ od_content() }}</article>",
        )
    return archive.getvalue()


@contextmanager
def running(binary, database, site_root=None):
    database = database.resolve()
    root = (site_root or (database.parent.parent if database.parent.name == "database" else database.parent)).resolve()
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    assert port != 18964, "Reserved review port"
    env = dict(os.environ, OPENDASH_PORT=str(port), OPENDASH_SITE_ROOT=str(root), OPENDASH_DB_PATH=str(database))
    env.pop("OPENDASH_PUBLIC_ORIGIN", None)
    with subprocess.Popen(
        [str(binary)], env=env, stdout=subprocess.PIPE, text=True, encoding="utf-8"
    ) as process:
        output = Queue()
        Thread(target=lambda: output.put(process.stdout.readline()), daemon=True).start()
        try:
            line = output.get(timeout=20).strip()
            base = f"http://127.0.0.1:{port}"
            assert line == f"OpenDash: {base}/builder", line
            yield base
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)


def request(base, path, data=None, content_type=None, headers=None, method=None):
    req_headers = {}
    if content_type:
        req_headers["Content-Type"] = content_type
    if headers:
        req_headers.update(headers)
    req = Request(base + path, data=data, headers=req_headers, method=method)
    try:
        with urlopen(req, timeout=5) as response:
            set_cookie = response.headers.get("Set-Cookie")
            return response.status, response.read().decode("utf-8"), set_cookie
    except HTTPError as error:
        with error:
            set_cookie = error.headers.get("Set-Cookie")
            return error.code, error.read().decode("utf-8"), set_cookie


def verify(binary):
    doc = {
        "schema_version": 1,
        "sections": [{
            "id": "smoke-sec",
            "children": [{
                "id": "smoke-row",
                "children": [{
                    "id": "smoke-col",
                    "children": [{
                        "id": "smoke-hd",
                        "type": "core/heading",
                        "props": {"level": 2, "text": "Dự án A < B {{ 7 * 7 }}"},
                    }],
                }],
            }],
        }],
    }
    body = json.dumps(doc, ensure_ascii=False).encode()
    admin_setup = json.dumps({"username": "admin", "password": "password123"}).encode()

    with TemporaryDirectory(prefix="opendash-smoke-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            # Shell builder and public page accessible without auth
            assert request(base, "/builder")[0] == 200
            assert request(base, "/")[0] == 200

            # Retired M3-M5 endpoints stay 404; active Admin endpoints require auth
            assert request(base, "/registry")[0] == 404
            assert request(base, "/page")[0] == 404
            assert request(base, "/api/admin/builder/blocks")[0] == 401
            assert request(base, "/api/admin/themes")[0] == 401

            # Setup first admin
            assert request(base, "/auth/setup", admin_setup, "application/json")[0] == 201
            # Second setup rejected
            assert request(base, "/auth/setup", admin_setup, "application/json")[0] == 409

            # Login as admin
            status, login_res, cookie_header = request(
                base, "/auth/login", admin_setup, "application/json"
            )
            assert status == 200, login_res
            assert cookie_header and "opendash_session=" in cookie_header
            login_data = json.loads(login_res)
            csrf_token = login_data["csrf_token"]
            session_cookie = cookie_header.split(";")[0].strip()

            auth_headers = {
                "Cookie": session_cookie,
                "X-CSRF-Token": csrf_token,
            }
            read_headers = {"Cookie": session_cookie}

            # Check /auth/me & retired routes remain 404 when authenticated
            me_status, me_body, _ = request(base, "/auth/me", headers=read_headers)
            assert me_status == 200
            assert json.loads(me_body)["user"]["username"] == "admin"
            assert request(base, "/registry", headers=read_headers)[0] == 404
            assert request(base, "/page", headers=read_headers)[0] == 404
            assert request(base, "/preview", body, "application/json", headers=auth_headers)[0] == 404
            assert request(base, "/save", body, "application/json", headers=auth_headers)[0] == 404

            # Active declared blocks & ThemeList shape
            blk_status, blk_body, _ = request(base, "/api/admin/builder/blocks", headers=read_headers)
            assert blk_status == 200, blk_body
            th_status, th_body, _ = request(base, "/api/admin/themes", headers=read_headers)
            assert th_status == 200, th_body
            th_json = json.loads(th_body)
            assert "revisions" in th_json and "selection" in th_json
            assert "items" not in th_json and "active_id" not in th_json

            # Theme v3 installation & activation
            assert (
                request(
                    base,
                    "/api/admin/themes",
                    sample_theme("../theme.json"),
                    "application/zip",
                    headers=auth_headers,
                )[0]
                == 400
            )
            inst_status, inst_body, _ = request(
                base,
                "/api/admin/themes",
                sample_theme(),
                "application/zip",
                headers=auth_headers,
            )
            assert inst_status == 201, inst_body
            rev_id = json.loads(inst_body)["revision_id"]
            th_json = json.loads(request(base, "/api/admin/themes", headers=read_headers)[1])
            act_status, act_body, _ = request(
                base,
                "/api/admin/themes/active",
                json.dumps({"revision_id": rev_id}).encode(),
                "application/json",
                headers={**auth_headers, "If-Match": f'"v{th_json["version"]}"'},
                method="PUT",
            )
            assert act_status == 200, act_body

            # Page entry builder preview isolation & CAS save
            page_st, page_body, _ = request(
                base,
                "/api/admin/content-types/page/entries",
                json.dumps({"title": "Smoke Page", "slug": "smoke-page", "values": {}}).encode(),
                "application/json",
                headers=auth_headers,
            )
            assert page_st == 201, page_body
            page_entry = json.loads(page_body)
            entry_builder_path = f"/api/admin/entries/page/{page_entry['id']}/builder"
            init_st, init_body, _ = request(base, entry_builder_path, headers=read_headers)
            assert init_st == 200 and json.loads(init_body)["document"] is None

            prev_st, prev_body, _ = request(
                base, entry_builder_path + "/preview", body, "application/json", headers=auth_headers
            )
            assert prev_st == 200, prev_body
            prev_html = json.loads(prev_body)["html"]
            assert "Dự án A &lt; B" in prev_html and "{{ 7 * 7 }}" in prev_html

            # Draft page stays private before save & before publication
            assert request(base, "/smoke-page")[0] == 404

            save_st, save_body, _ = request(
                base,
                entry_builder_path,
                body,
                "application/json",
                headers={**auth_headers, "If-Match": f'"v{page_entry["version"]}"'},
                method="PUT",
            )
            assert save_st == 200, save_body
            assert request(base, "/smoke-page")[0] == 404

            # Invalid AST rejected without overwriting saved doc
            assert (
                request(
                    base,
                    entry_builder_path,
                    b'{"schema_version":999,"sections":[]}',
                    "application/json",
                    headers={**auth_headers, "If-Match": f'"v{json.loads(save_body)["version"]}"'},
                    method="PUT",
                )[0]
                == 400
            )
            assert request(base, "/")[0] == 200

        # Restart and verify persistence
        with running(binary, database) as base:
            after_st, after_body, _ = request(base, entry_builder_path, headers=read_headers)
            assert after_st == 200 and json.loads(after_body)["document"] == doc

    print("PASS: Theme v3 ZIP install, entry preview isolation, CAS save, rejected inputs, retired M3-M5 404s, and SQLite persistence after restart")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opendash.exe"))
    parser.add_argument("--theme", type=Path, help="Write a sample theme ZIP instead of running the check")
    args = parser.parse_args()
    if args.theme:
        args.theme.write_bytes(sample_theme())
        print(args.theme.resolve())
    else:
        verify(args.binary.resolve())
