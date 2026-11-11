"""M3-S26-T01 Windows Dashboard API smoke: one aggregate, RBAC, publication transitions.

All writes use an isolated, disposable SQLite site root. From cms/:
    python scripts/dashboard_smoke.py --binary target/debug/opendash.exe
"""
import argparse
import json
from pathlib import Path
from tempfile import TemporaryDirectory

from content_entries_smoke import api
from windows_smoke import request, running


DASHBOARD = "/api/admin/dashboard"
EXPECTED_ZERO = {
    "post": {"draft": 0, "published": 0},
    "page": {"draft": 0, "published": 0},
}


def authorize(base: str, username: str, password: str):
    status, raw, cookie = request(
        base, "/auth/login",
        json.dumps({"username": username, "password": password}).encode(), "application/json",
    )
    assert status == 200 and cookie, (status, raw)
    return {"Cookie": cookie.split(";")[0], "X-CSRF-Token": json.loads(raw)["csrf_token"]}


def check_counts(base: str, headers, expected):
    status, body, meta = api(base, "GET", DASHBOARD, headers=headers)
    assert status == 200, (status, body)
    assert meta.get("Cache-Control") == "no-store"
    assert meta.get("Content-Type", "").startswith("application/json")
    data = json.loads(body)
    assert data == expected, (data, expected)
    assert set(data) == {"post", "page"}
    assert set(data["post"]) == set(data["page"]) == {"draft", "published"}
    assert all(type(value) is int and value >= 0
               for group in data.values() for value in group.values())
    assert all(key not in body.lower() for key in ('username', '"id"', 'token', 'password', 'diagnostic'))
    return data


def create_content(base: str, headers, kind: str, slug: str):
    status, body, _ = api(
        base, "POST", f"/api/admin/content-types/{kind}/entries",
        {
            "slug": slug,
            "title": f"{kind} {slug}",
            "values": {"body": "Content snapshot", "featured_image": None},
        },
        headers,
    )
    assert status == 201, (kind, status, body)
    record = json.loads(body)
    assert record["publication_status"] == "draft" and record["version"] == 1
    return record


def change_status(base, headers, kind, record, status):
    reply = api(
        base, "POST",
        f"/api/admin/content-types/{kind}/entries/{record['id']}/publication",
        {"status": status},
        {**headers, "If-Match": f'"v{record["version"]}"'},
    )
    assert reply[0] == 200, (reply[0], reply[1])
    changed = json.loads(reply[1])
    assert changed["publication_status"] == status
    assert changed["version"] == record["version"] + 1
    return changed


def verify(binary: Path):
    with TemporaryDirectory(prefix="opendash-dashboard-smoke-") as folder:
        database = Path(folder) / "site.sqlite"
        with running(binary, database) as base:
            assert api(base, "GET", DASHBOARD)[0] == 401
            assert request(
                base, "/auth/setup",
                json.dumps({"username": "dashadmin", "password": "admin-password-1"}).encode(),
                "application/json",
            )[0] == 201
            admin = authorize(base, "dashadmin", "admin-password-1")
            check_counts(base, admin, EXPECTED_ZERO)

            status, body, _ = api(
                base, "POST", "/api/admin/users",
                {"username": "dasheditor", "password": "editor-password-1", "role": "editor"},
                admin,
            )
            assert status == 201, (status, body)
            editor = authorize(base, "dasheditor", "editor-password-1")
            check_counts(base, editor, EXPECTED_ZERO)

            # Built-in entries start as drafts and are counted regardless of page size.
            post_draft = create_content(base, editor, "post", "post-draft")
            post_published = create_content(base, admin, "post", "post-published")
            page_draft = create_content(base, admin, "page", "page-draft")
            page_published = create_content(base, editor, "page", "page-published")
            expected = {"post": {"draft": 2, "published": 0},
                        "page": {"draft": 2, "published": 0}}
            check_counts(base, admin, expected)

            post_published = change_status(base, editor, "post", post_published, "published")
            page_published = change_status(base, admin, "page", page_published, "published")
            expected = {"post": {"draft": 1, "published": 1},
                        "page": {"draft": 1, "published": 1}}
            check_counts(base, admin, expected)
            check_counts(base, editor, expected)

            # A custom CPT remains excluded even when it is published.
            status, body, _ = api(
                base, "POST", "/api/admin/content-types",
                {"key": "portfolio", "label": "Dự án", "fields": [
                    {"key": "caption", "label": "Mô tả", "type": "short_text"},
                ]}, admin,
            )
            assert status == 201, (status, body)
            status, body, _ = api(
                base, "POST", "/api/admin/content-types/portfolio/entries",
                {"slug": "portfolio-example", "title": "CPT ignored",
                 "values": {"caption": "Không tính vào Dashboard"}}, editor,
            )
            assert status == 201, (status, body)
            check_counts(base, editor, expected)

            post_published = change_status(base, editor, "post", post_published, "draft")
            page_published = change_status(base, admin, "page", page_published, "draft")
            check_counts(base, editor, {
                "post": {"draft": 2, "published": 0},
                "page": {"draft": 2, "published": 0},
            })
            # Republish both again: verifies stale browser totals must be refreshed.
            change_status(base, admin, "post", post_published, "published")
            change_status(base, editor, "page", page_published, "published")
            check_counts(base, admin, expected)
            check_counts(base, editor, expected)

            # Logging out invalidates the authenticated route, not other sessions.
            status, _, _ = api(base, "POST", "/auth/logout", headers=editor)
            assert status == 204
            assert api(base, "GET", DASHBOARD, headers=editor)[0] == 401
            check_counts(base, admin, expected)

        with running(binary, database) as base:
            restored = authorize(base, "dashadmin", "admin-password-1")
            check_counts(base, restored, expected)
    print("PASS: Dashboard zero, guest 401, Admin/Editor, draft/published, CPT exclusion, transitions, restart")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opendash.exe"))
    args = parser.parse_args()
    verify(args.binary.resolve())
