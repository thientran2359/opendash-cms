#!/usr/bin/env python3
"""Shared stdlib helpers for isolated OpenDash native E2E and benchmark scripts."""

from __future__ import annotations

import json
import os
import pathlib
import re
import socket
import struct
import subprocess
import time
import urllib.request
import zlib


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_health(base: str, process: subprocess.Popen) -> None:
    for _ in range(150):
        if process.poll() is not None:
            raise AssertionError(f"Native OpenDash exited early: {process.returncode}")
        try:
            with urllib.request.urlopen(base + "/api/health", timeout=1) as response:
                if response.status == 200:
                    return
        except (OSError, TimeoutError):
            time.sleep(0.1)
    raise AssertionError("Native OpenDash health did not become ready")


def launch_server(binary: pathlib.Path, root: pathlib.Path) -> tuple[subprocess.Popen, str]:
    root = root.resolve()
    port = free_port()
    assert port != 18964, "Reserved review port"
    base = f"http://127.0.0.1:{port}"
    env = os.environ.copy()
    env["OPENDASH_SITE_ROOT"] = str(root)
    env["OPENDASH_DB_PATH"] = str(root / "database/opendash.sqlite")
    env["OPENDASH_PORT"] = str(port)
    env.pop("OPENDASH_PUBLIC_ORIGIN", None)
    proc = subprocess.Popen([str(binary)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    wait_for_health(base, proc)
    return proc, base


def stop_server(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=8)


def api_json(ctx, method: str, base: str, path: str, data=None, csrf=None, extra=None, expected=200):
    headers = dict(extra or {})
    if csrf:
        headers["X-CSRF-Token"] = csrf
    if data is not None:
        headers["Content-Type"] = "application/json"
    if method != "GET":
        headers["Origin"] = base
    response = ctx.request.fetch(
        base + path, method=method, headers=headers,
        data=json.dumps(data) if data is not None else None,
        timeout=20000,
    )
    assert response.status == expected, (method, path, response.status, response.text()[:260])
    if not response.body():
        return {}
    return response.json()


def set_cms_locale(ctx, base: str, csrf: str, locale: str) -> bool:
    """Explicit locale fixture for a disposable site; preserves content locale."""
    assert re.fullmatch(r"http://127\.0\.0\.1:\d+", base) and not base.endswith(":18964"), base
    bootstrap = api_json(ctx, "GET", base, "/api/i18n/admin")
    if bootstrap["configured_locale"] == locale:
        assert bootstrap["effective_locale"] == locale
        return False
    endpoint = "/api/admin/settings/general"
    current = ctx.request.get(base + endpoint)
    assert current.status == 200, current.text()
    data = {k: current.json()[k] for k in ("site_title", "tagline", "timezone")}
    data["cms_locale"] = locale
    saved = api_json(ctx, "PUT", base, endpoint, data, csrf,
                     {"If-Match": current.headers["etag"]})
    assert saved["cms_locale"] == locale
    actual = api_json(ctx, "GET", base, "/api/i18n/admin")
    assert actual["configured_locale"] == actual["effective_locale"] == locale
    return True


def admin_login(ctx, base: str, username: str, password: str, *, cms_locale: str | None = None) -> str:
    body = api_json(ctx, "POST", base, "/auth/login", {"username": username, "password": password})
    if cms_locale is not None:
        assert body["user"]["role"] == "admin"
        set_cms_locale(ctx, base, body["csrf_token"], cms_locale)
    return body["csrf_token"]


def make_png(seed=42, width=2, height=2):
    def chunk(kind, payload):
        return struct.pack(">I", len(payload)) + kind + payload + struct.pack(
            ">I", zlib.crc32(kind + payload) & 0xFFFFFFFF
        )
    # Fresh non-palette RGBA image, no external image libraries required.
    row = b"\x00" + bytes((seed, (seed * 3) % 256, (seed * 7) % 256, 255)) * width
    pixels = row * height
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(pixels))
        + chunk(b"IEND", b"")
    )
