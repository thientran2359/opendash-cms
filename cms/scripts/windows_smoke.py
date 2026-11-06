"""Run the Windows CMS slice over HTTP, or write a sample theme for the builder."""

import argparse
from contextlib import contextmanager
import io
import json
import os
from pathlib import Path
from queue import Queue
import subprocess
from tempfile import TemporaryDirectory
from threading import Thread
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from zipfile import ZIP_DEFLATED, ZipFile


def sample_theme(descriptor="block.json"):
    archive = io.BytesIO()
    with ZipFile(archive, "w", ZIP_DEFLATED) as theme:
        theme.writestr(
            descriptor,
            json.dumps({
                "id": "sample/banner",
                "fields": {"title": "Tiêu đề", "note": "Mô tả"},
            }),
        )
        theme.writestr(
            "block.html",
            "<article><h2>{{ values.title }}</h2><p>{{ values.note }}</p></article>",
        )
    return archive.getvalue()


@contextmanager
def running(binary, database):
    env = dict(os.environ, OPENDASH_PORT="0", OPENDASH_SITE_ROOT=str(database.parent), OPENDASH_DB_PATH=str(database))
    with subprocess.Popen(
        [str(binary)], env=env, stdout=subprocess.PIPE, text=True, encoding="utf-8"
    ) as process:
        output = Queue()
        Thread(target=lambda: output.put(process.stdout.readline()), daemon=True).start()
        try:
            line = output.get(timeout=20).strip()
            assert line.startswith("OpenDash: http://127.0.0.1:") and line.endswith("/builder"), line
            yield line.removeprefix("OpenDash: ").removesuffix("/builder")
        finally:
            process.terminate()


def request(base, path, data=None, content_type=None, headers=None):
    req_headers = {}
    if content_type:
        req_headers["Content-Type"] = content_type
    if headers:
        req_headers.update(headers)
    req = Request(base + path, data=data, headers=req_headers)
    try:
        with urlopen(req, timeout=5) as response:
            set_cookie = response.headers.get("Set-Cookie")
            return response.status, response.read().decode("utf-8"), set_cookie
    except HTTPError as error:
        with error:
            set_cookie = error.headers.get("Set-Cookie")
            return error.code, error.read().decode("utf-8"), set_cookie


def verify(binary):
    block = {"type": "sample/banner", "values": {"title": "Dự án A < B", "note": "{{ 7 * 7 }}"}}
    body = json.dumps(block, ensure_ascii=False).encode()
    admin_setup = json.dumps({"username": "admin", "password": "password123"}).encode()

    with TemporaryDirectory(prefix="opendash-smoke-") as folder:
        database = Path(folder) / "cms.sqlite"
        with running(binary, database) as base:
            # Shell builder and public page accessible without auth
            assert request(base, "/builder")[0] == 200
            assert request(base, "/")[0] == 200

            # Admin endpoints protected
            assert request(base, "/registry")[0] == 401
            assert request(base, "/page")[0] == 401

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

            # Check /auth/me
            me_status, me_body, _ = request(base, "/auth/me", headers=read_headers)
            assert me_status == 200
            assert json.loads(me_body)["user"]["username"] == "admin"

            # Check registry with auth
            assert json.loads(request(base, "/registry", headers=read_headers)[1]) == {}

            # Theme installation
            assert (
                request(
                    base,
                    "/theme",
                    sample_theme("../block.json"),
                    "application/zip",
                    headers=auth_headers,
                )[0]
                == 400
            )
            assert (
                request(
                    base,
                    "/theme",
                    sample_theme(),
                    "application/zip",
                    headers=auth_headers,
                )[0]
                == 201
            )
            assert json.loads(request(base, "/registry", headers=read_headers)[1]) == {
                "sample/banner": {"note": "Mô tả", "title": "Tiêu đề"}
            }

            # Preview
            status, preview, _ = request(
                base, "/preview", body, "application/json", headers=auth_headers
            )
            assert status == 200 and "Dự án A &lt; B" in preview and "{{ 7 * 7 }}" in preview

            # Public page still unchanged before save
            assert request(base, "/")[1] == "<p>Chưa lưu block.</p>"

            # Save
            save_status, save_res, _ = request(
                base, "/save", body, "application/json", headers=auth_headers
            )
            assert save_status == 200 and save_res == preview
            assert request(base, "/")[1] == preview

            # Rejected save input
            assert (
                request(
                    base,
                    "/save",
                    b'{"type":"sample/banner","values":{}}',
                    "application/json",
                    headers=auth_headers,
                )[0]
                == 400
            )
            # Duplicate theme
            assert (
                request(
                    base,
                    "/theme",
                    sample_theme(),
                    "application/zip",
                    headers=auth_headers,
                )[0]
                == 409
            )
            assert request(base, "/")[1] == preview

        # Restart and verify persistence
        with running(binary, database) as base:
            assert json.loads(request(base, "/page", headers=read_headers)[1]) == block
            assert request(base, "/")[1] == preview

    print("PASS: ZIP install, preview isolation, save, rejected inputs, and SQLite persistence after restart")


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
