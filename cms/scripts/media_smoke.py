"""M3.5 Media Library end-to-end smoke tests using an isolated site and SQLite.

From repository root:
    python3 cms/scripts/media_smoke.py --binary cms/target/debug/opendash
"""

import argparse
import hashlib
import json
import sqlite3
import struct
import zlib
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from content_types_smoke import login, request, running


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


def raw_request(base, method, path, body=None, headers=None):
    req = Request(base + path, method=method, data=body, headers=headers or {})
    try:
        with urlopen(req, timeout=20) as response:
            return response.status, response.read(), response.headers
    except HTTPError as error:
        with error:
            return error.code, error.read(), error.headers


def upload_image(base, auth, seed=42, filename="image.png"):
    status, body, _ = raw_request(base, "POST", "/api/admin/media", make_png(seed), {
        **auth,
        "Content-Type": "image/png",
        "X-File-Name": filename,
    })
    assert status == 201, (status, body)
    return json.loads(body)


def smoke(binary):
    with TemporaryDirectory(prefix="opendash-m3-5-media-") as folder:
        site = Path(folder) / "site"
        db = site / "database" / "opendash.sqlite"
        with running(binary, db) as base:
            endpoint = "/api/admin/media"
            assert raw_request(base, "GET", endpoint)[0] == 401
            assert raw_request(base, "POST", endpoint, make_png(), {"Content-Type": "image/png"})[0] == 401
            assert request(base, "/auth/setup", {"username": "admin", "password": "password123"})[0] == 201
            admin = login(base, "admin", "password123")
            assert json.loads(raw_request(base, "GET", endpoint, headers=admin)[1]) == []
            assert raw_request(base, "POST", endpoint, make_png(), {**admin, "X-CSRF-Token": "wrong", "Content-Type": "image/png"})[0] == 403
            assert raw_request(base, "POST", endpoint, make_png(), {"Cookie": admin["Cookie"], "Content-Type": "image/png"})[0] == 403
            assert raw_request(base, "POST", endpoint, make_png(), {**admin, "Content-Type": "image/png", "Origin": "http://evil.invalid"})[0] == 403
            assert raw_request(base, "POST", endpoint, make_png(), {**admin, "Content-Type": "image/jpeg"})[0] == 415
            assert raw_request(base, "POST", endpoint, b"<svg><script>bad</script></svg>", {**admin, "Content-Type": "image/png"})[0] == 400
            assert raw_request(base, "POST", endpoint, make_png()[:20], {**admin, "Content-Type": "image/png"})[0] == 400
            assert raw_request(base, "POST", endpoint, make_png(7, width=6001), {**admin, "Content-Type": "image/png"})[0] == 400
            assert raw_request(base, "POST", endpoint, b"x" * (8 * 1024 * 1024 + 1), {**admin, "Content-Type": "image/png"})[0] == 413

            asset = upload_image(base, admin, seed=11, filename="my%20image.png")
            media_id = asset["id"]
            assert len(media_id) == 64 and asset["width"] == 2 and asset["height"] == 2
            assert asset["thumb_width"] == 2 and asset["thumb_height"] == 2
            assert asset["mime_type"] == "image/png"
            assert raw_request(base, "GET", endpoint + "?limit=0", headers=admin)[0] == 400
            assert raw_request(base, "GET", f"{endpoint}/{media_id}/file")[0] == 401
            assert raw_request(base, "GET", f"{endpoint}/{media_id}/thumbnail")[0] == 401

            status, original, headers = raw_request(base, "GET", f"{endpoint}/{media_id}/file", headers=admin)
            assert status == 200 and original.startswith(b"\x89PNG\r\n\x1a\n")
            assert hashlib.sha256(original).hexdigest() == media_id
            assert headers["Content-Type"] == "image/png"
            assert headers["X-Content-Type-Options"] == "nosniff"
            assert "no-store" in headers["Cache-Control"]
            status, thumb, headers = raw_request(base, "GET", f"{endpoint}/{media_id}/thumbnail", headers=admin)
            assert status == 200 and thumb.startswith(b"\x89PNG\r\n\x1a\n")
            assert headers["Content-Type"] == "image/png"
            assert raw_request(base, "GET", f"{endpoint}/{'0' * 64}/thumbnail", headers=admin)[0] == 404
            assert raw_request(base, "GET", f"{endpoint}/../../etc/passwd", headers=admin)[0] in (400, 404)

            assert upload_image(base, admin, seed=11)["id"] == media_id  # deduplicate identical bytes
            status, body, _ = raw_request(base, "GET", endpoint + "?limit=50&offset=0", headers=admin)
            assert status == 200 and len(json.loads(body)) == 1
            assert request(base, "/api/admin/users", {"username": "editor", "password": "password123"}, admin)[0] == 201
            editor = login(base, "editor", "password123")
            second = upload_image(base, editor, seed=19)
            assert second["id"] != media_id
            assert len(json.loads(raw_request(base, "GET", endpoint, headers=editor)[1])) == 2

            # M3.3 now verifies media existence, not just shape of an opaque string.
            schema = {"key": "portfolio", "label": "Dự án", "fields": [
                {"key": "cover", "label": "Ảnh bìa", "type": "image", "required": True},
                {"key": "gallery", "label": "Thư viện", "type": "gallery"},
            ]}
            assert request(base, "/api/admin/content-types", schema, admin)[0] == 201
            entry_path = "/api/admin/content-types/portfolio/entries"
            entry = {"title": "Album mẫu", "slug": "album-mau", "values": {"cover": media_id, "gallery": [media_id, second["id"]]}}
            assert request(base, entry_path, entry, editor)[0] == 201
            missing = {**entry, "slug": "missing-image", "values": {"cover": "a" * 64}}
            assert request(base, entry_path, missing, editor)[0] == 400
            assert raw_request(base, "GET", f"{endpoint}/{'a' * 64}/file", headers=admin)[0] == 404

        with sqlite3.connect(db) as connection:
            assert connection.execute("SELECT COUNT(*) FROM media_assets").fetchone() == (2,)
            assert connection.execute("SELECT success FROM _sqlx_migrations WHERE version = 6").fetchone() == (1,)
            original_path = site / "uploads" / "media" / media_id[:2] / f"{media_id}.png"
            thumb_path = site / "uploads" / "media" / media_id[:2] / f"{media_id}.thumb.png"
            assert hashlib.sha256(original_path.read_bytes()).hexdigest() == media_id
            assert thumb_path.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")

        with running(binary, db) as base:
            editor = login(base, "editor", "password123")
            status, body, _ = raw_request(base, "GET", "/api/admin/media", headers=editor)
            assert status == 200 and len(json.loads(body)) == 2
            status, thumb, _ = raw_request(base, "GET", f"/api/admin/media/{media_id}/thumbnail", headers=editor)
            assert status == 200 and thumb.startswith(b"\x89PNG")
            entries = request(base, "/api/admin/content-types/portfolio/entries", headers=editor)
            assert entries[0] == 200 and json.loads(entries[1])[0]["values"]["cover"] == media_id


if __name__ == "__main__":
    cms_dir = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=cms_dir / "target/debug/opendash")
    args = parser.parse_args()
    smoke(args.binary.resolve())
    print("M3.5 MEDIA SMOKE: PASS (upload, format/size, auth, thumbs, refs, restart)")
