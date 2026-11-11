"""M3-S29-T01: macOS release acceptance on isolated SQLite sites and real Chrome.

The exact release source must match the integrated Windows P19 candidate.  The
old binary must be a *genuine* post-0008/pre-0009 macOS build; rollback always
restores an SQLite snapshot before starting that historical executable.

From the repository root, after building embedded Admin, the Wasmi ZIP and
Rust release binary:

    .venv/bin/python cms/scripts/macos_m3_e2e.py \
      --binary cms/target/release/opendash \
      --baseline-binary PATH/TO/pre-0009/opendash \
      --browser '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'

All mutations use temporary site roots; this harness never opens a live site.
"""

import argparse
from contextlib import closing
import hashlib
from importlib.metadata import version
import os
from pathlib import Path
import platform
import shutil
import sqlite3
import subprocess
import sys
from tempfile import TemporaryDirectory

from windows_m3_release_e2e import browser_flow, historical_sqlite_upgrade, snapshot
from windows_distribution_smoke import assert_migrations, release_migration_versions


PINNED_RELEASE_SHA = "d2b6bfe1e7b6c2f6895802b8ed609292747d3062"
ROOT = Path(__file__).resolve().parents[2]
CMS = ROOT / "cms"


def data_integrity(database: Path) -> dict[str, int]:
    """Check the fresh integrated site and capture persisted domain row counts."""
    with closing(sqlite3.connect(database)) as connection:
        assert connection.execute("PRAGMA integrity_check").fetchone() == ("ok",)
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []
        tables = ("home_page", "content_entries", "media_assets", "users",
                  "plugins", "plugin_kv", "site_general_settings", "site_active_theme")
        counts = {name: connection.execute(f"SELECT COUNT(*) FROM {name}").fetchone()[0]
                  for name in tables}
        for name in ("home_page", "media_assets", "site_general_settings",
                     "site_active_theme", "plugins", "plugin_kv"):
            assert counts[name] >= 1, (name, counts)
        assert counts["content_entries"] >= 2, counts
        assert counts["users"] >= 2, counts
        published = connection.execute(
            "SELECT content_type_key, publication_status FROM content_entries "
            "WHERE content_type_key IN ('post','page')"
        ).fetchall()
        assert {"post", "page"} <= {row[0] for row in published}, published
        assert all(row[1] == "published" for row in published), published
        active = connection.execute("SELECT block_id FROM site_active_theme WHERE id=1").fetchone()
        assert active == ("sample/banner",), active
        return counts


def data_fingerprints(database: Path) -> dict[str, str]:
    """Compare complete site-owned rows, including media refs and plugin KV."""
    with closing(sqlite3.connect(database)) as connection:
        tables = ("home_page", "theme_blocks", "content_types", "content_entries",
                  "media_assets", "users", "plugins", "plugin_kv",
                  "site_general_settings", "site_active_theme")
        hashes = {}
        for name in tables:
            digest = hashlib.sha256()
            for row in connection.execute(f"SELECT * FROM {name} ORDER BY rowid"):
                digest.update(repr(row).encode("utf-8"))
                digest.update(b"\n")
            hashes[name] = digest.hexdigest()
        return hashes


def verify_integrated_site(binary: Path, chrome: Path, folder: Path) -> None:
    # The same actual SQLite database carries all Admin flows, then is restarted.
    browser_flow(binary, str(chrome), folder)
    database = folder / "fresh" / "database" / "opendash.sqlite"
    assert_migrations(database, release_migration_versions())
    before = data_integrity(database)
    before_hashes = data_fingerprints(database)
    # SQLite-only backups would strand referenced uploads; include both image
    # originals and thumbnails in the restored disposable site's file tree.
    asset_root = database.parent / "uploads" / "media"
    with closing(sqlite3.connect(database)) as connection:
        media = connection.execute("SELECT id, extension FROM media_assets").fetchall()
    expected_files = []
    for identifier, extension in media:
        for filename in (f"{identifier}.{extension}", f"{identifier}.thumb.png"):
            path = asset_root / identifier[:2] / filename
            assert path.is_file(), f"Missing referenced image/thumbnail: {path}"
            expected_files.append(path.relative_to(asset_root))
    restored = folder / "restored-fresh" / "opendash.sqlite"
    restored.parent.mkdir(parents=True, exist_ok=True)
    snapshot(database, restored)
    restored_assets = restored.parent / "uploads" / "media"
    shutil.copytree(asset_root, restored_assets)
    for relative in expected_files:
        original = asset_root / relative
        copy = restored_assets / relative
        assert hashlib.sha256(original.read_bytes()).digest() == hashlib.sha256(
            copy.read_bytes()
        ).digest(), f"Restored asset mismatch: {relative}"
    after = data_integrity(restored)
    assert before == after, (before, after)
    assert before_hashes == data_fingerprints(restored), "Snapshot changed site data/refs"
    print(f"PASS macOS integrated site + SQLite/assets snapshot: {before}, "
          f"referenced files={len(expected_files)}", flush=True)


def run_regressions(binary: Path, old_binary: Path, chrome: Path) -> None:
    """Independent negative/security coverage beyond the one-site browser flow."""
    cases = [
        ("auth/RBAC/CSRF/last-admin/rollback", "windows_auth_smoke.py",
         ["--baseline-binary", str(old_binary), "--baseline-db",
          "fixtures/baseline_0001.sqlite"]),
        ("public draft/media ACL/CSP/escaping", "public_content_smoke.py", []),
        ("plugin Wasmi worker/KV", "windows_plugin_smoke.py", []),
        ("theme A-B-C + migration", "themes_smoke.py",
         ["--baseline-db", "fixtures/baseline_0001.sqlite"]),
        ("Users Chrome", "windows_users_browser.py", ["--browser", str(chrome)]),
        ("Settings Chrome", "general_settings_browser.py", ["--browser", str(chrome)]),
        ("Themes Chrome", "themes_browser.py", ["--browser", str(chrome)]),
        ("Plugins Chrome", "windows_plugins_browser.py", ["--browser", str(chrome)]),
        ("Dashboard Chrome", "dashboard_browser.py", ["--browser", str(chrome)]),
        ("Media API", "media_smoke.py", []),
        ("Wasmi failure isolation", "windows_kernel_smoke.py", []),
    ]
    environment = dict(os.environ, PYTHONIOENCODING="utf-8")
    for label, filename, flags in cases:
        cmd = [sys.executable, str(CMS / "scripts" / filename),
               "--binary", str(binary), *flags]
        print(f"RUN macOS regression: {label}", flush=True)
        subprocess.run(cmd, cwd=CMS, env=environment, check=True)
        print(f"PASS macOS regression: {label}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--baseline-binary", required=True, type=Path)
    parser.add_argument("--baseline-db", type=Path, default=CMS / "fixtures/baseline_0001.sqlite")
    parser.add_argument("--browser", required=True, type=Path)
    parser.add_argument("--release-sha", default=PINNED_RELEASE_SHA)
    parser.add_argument("--skip-regressions", action="store_true",
                        help="Only for harness debugging; NOT full M3 acceptance")
    args = parser.parse_args()

    assert platform.system() == "Darwin", "This acceptance runner requires macOS"
    binary = args.binary.resolve(strict=True)
    baseline = args.baseline_binary.resolve(strict=True)
    chrome = args.browser.resolve(strict=True)
    fixture = args.baseline_db.resolve(strict=True)
    assert binary != baseline, "Historical and current binaries must differ"
    source_sha = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    product_paths = (
        "cms/admin", "cms/crates", "cms/migrations", "cms/examples", "cms/sdk",
        "cms/Cargo.toml", "cms/Cargo.lock", "themes", "plugins",
    )
    for reference in (args.release_sha, "HEAD"):
        diff = subprocess.check_output(
            ["git", "diff", "--name-only", reference, "--", *product_paths],
            cwd=ROOT, text=True
        ).strip()
        assert not diff, f"Product source differs from pinned release {reference}: {diff}"
    print(f"macOS release source: {args.release_sha}; harness checkout: {source_sha}", flush=True)
    print(f"OS: {platform.platform()} / {platform.machine()}", flush=True)
    print(f"Python {platform.python_version()}, Playwright {version('playwright')}", flush=True)
    print(f"Chrome: {subprocess.check_output([str(chrome), '--version'], text=True).strip()}", flush=True)
    for label, path in (("release", binary), ("pre-0009", baseline)):
        print(f"{label} binary: {path} SHA256={hashlib.sha256(path.read_bytes()).hexdigest()}", flush=True)
    print(f"Historical SQLite fixture: {fixture}", flush=True)

    with TemporaryDirectory(prefix="opendash-m3-macos-release-") as directory:
        site = Path(directory)
        verify_integrated_site(binary, chrome, site)
        historical_sqlite_upgrade(binary, baseline, fixture, site)
    if args.skip_regressions:
        print("NOT_RUN: separate regressions (--skip-regressions); release G5 remains pending")
    else:
        run_regressions(binary, baseline, chrome)
        print("PASS M3-S29-T01 macOS full release E2E, security and snapshot rollback", flush=True)


if __name__ == "__main__":
    main()
