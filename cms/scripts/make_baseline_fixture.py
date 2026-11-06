"""Generate or verify the pre-M1.1 baseline database fixture (fixtures/baseline_0001.sqlite).

Baseline origin:
  Git commit: 1fba9ac (pre-M1.1 prototype slice)
  Rebuild command:
    git checkout 1fba9ac
    cargo build --locked
    python scripts/make_baseline_fixture.py --binary target/debug/opendash.exe
"""

import argparse
import io
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
from tempfile import TemporaryDirectory
from urllib.request import Request, urlopen
from zipfile import ZIP_DEFLATED, ZipFile


def create_baseline_fixture(destination: Path, binary: Path | None = None):
    destination = destination.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)

    if binary is not None:
        if not binary.exists():
            print(f"Error: Specified binary does not exist at {binary}", file=sys.stderr)
            sys.exit(1)
        # Generate using pre-M1.1 binary over HTTP
        with TemporaryDirectory(prefix="opendash-basegen-") as folder:
            temp_db = Path(folder) / "cms.sqlite"
            env = dict(os.environ, OPENDASH_PORT="0", OPENDASH_SITE_ROOT=str(temp_db.parent), OPENDASH_DB_PATH=str(temp_db))
            with subprocess.Popen(
                [str(binary)], env=env, stdout=subprocess.PIPE, text=True, encoding="utf-8"
            ) as proc:
                line = proc.stdout.readline().strip()
                base = line.removeprefix("OpenDash: ").removesuffix("/builder")

                archive = io.BytesIO()
                with ZipFile(archive, "w", ZIP_DEFLATED) as theme:
                    theme.writestr(
                        "block.json",
                        json.dumps({
                            "id": "sample/banner",
                            "fields": {"title": "Tiêu đề", "note": "Mô tả"},
                        }),
                    )
                    theme.writestr(
                        "block.html",
                        "<article><h2>{{ values.title }}</h2><p>{{ values.note }}</p></article>",
                    )
                theme_bytes = archive.getvalue()

                req = Request(base + "/theme", data=theme_bytes, headers={"Content-Type": "application/zip"})
                urlopen(req).read()

                body = json.dumps({
                    "type": "sample/banner",
                    "values": {"title": "Baseline Title", "note": "Baseline Note"},
                }).encode()
                req = Request(base + "/save", data=body, headers={"Content-Type": "application/json"})
                urlopen(req).read()

                proc.terminate()
                proc.wait()

            src = sqlite3.connect(temp_db)
            if destination.exists():
                destination.unlink()
            dst = sqlite3.connect(destination)
            src.backup(dst)
            dst.close()
            src.close()
    else:
        # Verify existing fixture
        if not destination.exists():
            print(f"Error: Fixture file not found at {destination}", file=sys.stderr)
            sys.exit(1)
        conn = sqlite3.connect(destination)
        tables = [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()]
        assert "theme_blocks" in tables and "home_page" in tables, f"Missing tables: {tables}"
        row = conn.execute("SELECT block_id, values_json FROM home_page WHERE id = 1").fetchone()
        assert row is not None and "Baseline Title" in row[1], f"Unexpected home_page: {row}"
        conn.close()

    print(f"PASS: Baseline fixture ready at {destination} ({destination.stat().st_size} bytes)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, default=Path("fixtures/baseline_0001.sqlite"))
    parser.add_argument("--binary", type=Path, help="Path to pre-M1.1 opendash binary to regenerate fixture")
    args = parser.parse_args()
    create_baseline_fixture(args.destination, args.binary)
