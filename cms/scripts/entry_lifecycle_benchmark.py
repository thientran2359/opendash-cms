#!/usr/bin/env python3
"""Canonical S04 performance benchmark driver (fixture version 1).

Measures baseline vs candidate executables on identical isolated deterministic
1k / 10k / 50k entry fixtures across NFR gates P1, P2, and P3 per
`docs/designs/entry-lifecycle-bulk-quick-edit.md` ("Canonical S04 benchmark
workload (fixture version 1)").

Safety guarantees:
- Requires explicit `--baseline-binary`, `--candidate-binary`, and `--artifact-dir`.
- Never touches any live site root or port 18964; uses isolated temporary roots
  and ephemeral loopback ports only.
- Verifies byte-identical canonical semantic manifests between baseline and
  migrated candidate clones before any timing begins.
- Uses independent stopped-seed clones for every bulk repetition (untimed),
  alternates baseline/candidate order across matched cases/repetitions, collects
  both server monotonic storage elapsed time (`entry_lifecycle_diag`) and
  end-to-end client timing, and fails closed if any P1–P3 gate misses.
"""
from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import math
import os
import pathlib
import platform
import re
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.parse
from typing import Any

SCRIPTS_DIR = pathlib.Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from media_smoke import make_png  # noqa: E402

FORBIDDEN_LIVE_PORT = 18964
ADMIN_USER = "admin"
ADMIN_PASS = "Bench-Admin-123!"
EDITOR_USER = "editor"
EDITOR_PASS = "Bench-Editor-123!"
DIAG_RE = re.compile(
    r"entry_lifecycle_diag\s+op=(?P<op>\S+)\s+type_key=(?P<type_key>\S+)\s+"
    r"entry_id=(?P<entry_id>\d+)\s+actor_id=(?P<actor_id>\d+)\s+"
    r"actor_role=(?P<actor_role>\S+)\s+expected_version=(?P<expected_version>\d+)\s+"
    r"version_after=(?P<version_after>\d+)\s+outcome=(?P<outcome>\S+)\s+"
    r"status=(?P<status>\d+)\s+storage_us=(?P<storage_us>\d+)"
)


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        while chunk := fh.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()


def free_port() -> int:
    for _ in range(50):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("127.0.0.1", 0))
            port = int(s.getsockname()[1])
            if port != FORBIDDEN_LIVE_PORT:
                return port
    raise RuntimeError("Could not allocate an ephemeral loopback port")


def clone_site_dir(src: pathlib.Path, dst: pathlib.Path) -> None:
    if dst.exists():
        shutil.rmtree(dst)
    if sys.platform == "darwin":
        res = subprocess.run(["cp", "-cR", str(src), str(dst)], capture_output=True)
        if res.returncode == 0:
            return
    shutil.copytree(src, dst)


def dir_sqlite_sizes(site_root: pathlib.Path) -> dict[str, int]:
    db_dir = site_root / "database"
    db_file = db_dir / "opendash.sqlite"
    wal_file = db_dir / "opendash.sqlite-wal"
    shm_file = db_dir / "opendash.sqlite-shm"
    return {
        "db_bytes": db_file.stat().st_size if db_file.exists() else 0,
        "wal_bytes": wal_file.stat().st_size if wal_file.exists() else 0,
        "shm_bytes": shm_file.stat().st_size if shm_file.exists() else 0,
    }


def process_rss_bytes(pid: int) -> int:
    try:
        out = subprocess.check_output(["ps", "-o", "rss=", "-p", str(pid)], text=True).strip()
        return int(out) * 1024 if out else 0
    except Exception:
        return 0


def nearest_rank_percentile(samples: list[float], pct: float) -> float:
    if not samples:
        return 0.0
    ordered = sorted(samples)
    rank = max(1, min(len(ordered), math.ceil((pct / 100.0) * len(ordered))))
    return ordered[rank - 1]


def summarize_ms(samples: list[float]) -> dict[str, Any]:
    if not samples:
        return {"n": 0, "min_ms": 0.0, "p50_ms": 0.0, "p95_ms": 0.0, "max_ms": 0.0, "mean_ms": 0.0}
    ordered = sorted(samples)
    return {
        "n": len(ordered),
        "min_ms": round(ordered[0], 3),
        "p50_ms": round(nearest_rank_percentile(ordered, 50.0), 3),
        "p95_ms": round(nearest_rank_percentile(ordered, 95.0), 3),
        "max_ms": round(ordered[-1], 3),
        "mean_ms": round(sum(ordered) / len(ordered), 3),
    }


class HttpClient:
    """Persistent HTTP/1.1 loopback client for low-overhead sequential requests."""

    def __init__(self, host: str, port: int) -> None:
        self.host = host
        self.port = port
        self.base = f"http://{host}:{port}"
        self.conn = http.client.HTTPConnection(host, port, timeout=120)

    def close(self) -> None:
        try:
            self.conn.close()
        except Exception:
            pass

    def raw(
        self,
        method: str,
        path: str,
        body: bytes | None = None,
        headers: dict[str, str] | None = None,
    ) -> tuple[int, bytes, dict[str, str]]:
        hdrs = dict(headers or {})
        hdrs.setdefault("Connection", "keep-alive")
        if method in ("POST", "PUT", "PATCH", "DELETE"):
            hdrs.setdefault("Origin", self.base)
        for attempt in range(2):
            try:
                self.conn.request(method, path, body=body, headers=hdrs)
                resp = self.conn.getresponse()
                data = resp.read()
                resp_headers = {k.lower(): v for k, v in resp.getheaders()}
                return resp.status, data, resp_headers
            except (http.client.RemoteDisconnected, BrokenPipeError, ConnectionResetError):
                self.conn.close()
                self.conn = http.client.HTTPConnection(self.host, self.port, timeout=120)
                if attempt == 1:
                    raise
        raise AssertionError("unreachable")

    def json_req(
        self,
        method: str,
        path: str,
        payload: Any = None,
        auth: dict[str, str] | None = None,
        extra_headers: dict[str, str] | None = None,
        expected: int | tuple[int, ...] | None = 200,
    ) -> tuple[int, Any, dict[str, str]]:
        hdrs: dict[str, str] = {}
        if auth:
            hdrs.update(auth)
        if extra_headers:
            hdrs.update(extra_headers)
        body: bytes | None = None
        if payload is not None:
            body = json.dumps(payload).encode("utf-8")
            hdrs["Content-Type"] = "application/json"
        status, data, rh = self.raw(method, path, body, hdrs)
        if expected is not None:
            ok = (status == expected) if isinstance(expected, int) else (status in expected)
            if not ok:
                raise RuntimeError(
                    f"{method} {path} expected {expected}, got {status}: {data[:400]!r}"
                )
        parsed = json.loads(data.decode("utf-8")) if data else None
        return status, parsed, rh

    def login(self, username: str, password: str) -> dict[str, str]:
        self.conn.request(
            "POST",
            "/auth/login",
            body=json.dumps({"username": username, "password": password}).encode("utf-8"),
            headers={"Content-Type": "application/json", "Origin": self.base},
        )
        resp = self.conn.getresponse()
        raw_body = resp.read()
        if resp.status != 200:
            raise RuntimeError(f"Login failed for {username}: {resp.status} {raw_body!r}")
        cookies = [
            v.split(";", 1)[0]
            for k, v in resp.getheaders()
            if k.lower() == "set-cookie"
        ]
        csrf = json.loads(raw_body.decode("utf-8"))["csrf_token"]
        return {"Cookie": "; ".join(cookies), "X-CSRF-Token": csrf}


class ServerHandle:
    def __init__(self, binary: pathlib.Path, site_root: pathlib.Path, stderr_path: pathlib.Path) -> None:
        self.binary = binary
        self.site_root = site_root
        self.stderr_path = stderr_path
        self.port = free_port()
        self.stderr_fh = stderr_path.open("wb")
        env = dict(os.environ)
        env["OPENDASH_SITE_ROOT"] = str(site_root)
        env["OPENDASH_HOST"] = "127.0.0.1"
        env["OPENDASH_PORT"] = str(self.port)
        self.proc = subprocess.Popen(
            [str(binary)],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=self.stderr_fh,
        )
        self.client = HttpClient("127.0.0.1", self.port)
        self.peak_rss_bytes = 0
        self.wal_high_water_bytes = 0
        self._wait_healthy()

    def _wait_healthy(self) -> None:
        deadline = time.monotonic() + 60.0
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                self.stderr_fh.flush()
                err_txt = self.stderr_path.read_text(encoding="utf-8", errors="replace")[-1000:]
                raise RuntimeError(
                    f"Server {self.binary} exited early with code {self.proc.returncode}: {err_txt}"
                )
            try:
                status, _, _ = self.client.raw("GET", "/api/health")
                if status == 200:
                    self.sample_resources()
                    return
            except Exception:
                time.sleep(0.03)
        raise RuntimeError(f"Server {self.binary} did not become healthy on port {self.port}")

    def sample_resources(self) -> None:
        rss = process_rss_bytes(self.proc.pid)
        if rss > self.peak_rss_bytes:
            self.peak_rss_bytes = rss
        sizes = dir_sqlite_sizes(self.site_root)
        if sizes["wal_bytes"] > self.wal_high_water_bytes:
            self.wal_high_water_bytes = sizes["wal_bytes"]

    def diag_offset(self) -> int:
        self.stderr_fh.flush()
        return self.stderr_path.stat().st_size if self.stderr_path.exists() else 0

    def parse_diags_since(self, byte_offset: int) -> list[dict[str, Any]]:
        self.stderr_fh.flush()
        with self.stderr_path.open("rb") as fh:
            fh.seek(byte_offset)
            text = fh.read().decode("utf-8", errors="replace")
        out: list[dict[str, Any]] = []
        for m in DIAG_RE.finditer(text):
            d = m.groupdict()
            out.append(
                {
                    "op": d["op"],
                    "type_key": d["type_key"],
                    "entry_id": int(d["entry_id"]),
                    "actor_id": int(d["actor_id"]),
                    "actor_role": d["actor_role"],
                    "expected_version": int(d["expected_version"]),
                    "version_after": int(d["version_after"]),
                    "outcome": d["outcome"],
                    "status": int(d["status"]),
                    "storage_us": int(d["storage_us"]),
                }
            )
        return out

    def stop(self) -> None:
        self.sample_resources()
        self.client.close()
        if self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=10)
        self.stderr_fh.close()


def checkpoint_and_fk_check(db_path: pathlib.Path) -> None:
    conn = sqlite3.connect(str(db_path))
    try:
        conn.execute("PRAGMA foreign_keys = ON")
        fk = conn.execute("PRAGMA foreign_key_check").fetchall()
        assert not fk, f"foreign_key_check failed on {db_path}: {fk[:10]}"
        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    finally:
        conn.close()


def compute_semantic_manifest(db_path: pathlib.Path) -> tuple[str, int]:
    """Computes canonical sorted-key compact UTF-8 JSON semantic manifest + SHA-256
    for fixture posts (spec line 498), excluding generated DB row IDs and bootstrap timestamps."""
    conn = sqlite3.connect(str(db_path))
    try:
        terms_by_id = {
            int(r[0]): (str(r[1]), str(r[2]))
            for r in conn.execute("SELECT id, kind, slug FROM taxonomy_terms").fetchall()
        }
        entry_terms: dict[int, list[str]] = {}
        for eid, tid in conn.execute("SELECT entry_id, term_id FROM entry_terms"):
            kind, slug = terms_by_id[int(tid)]
            entry_terms.setdefault(int(eid), []).append(f"{kind}:{slug}")

        claims_by_entry: dict[int, list[str]] = {}
        for path, tid, loc in conn.execute(
            "SELECT path, target_id, locale FROM permalink_route_claims WHERE target_kind = 'entry'"
        ):
            claims_by_entry.setdefault(int(tid), []).append(f"{loc}:{path}")

        rows = conn.execute(
            "SELECT id, content_type_key, locale, slug, title, translation_group_id, "
            "       publication_status, has_been_published, published_at, created_at, "
            "       updated_at, version, content_mode, values_json, primary_category_term_id "
            "  FROM content_entries "
            " WHERE content_type_key = 'post' "
            " ORDER BY slug ASC"
        ).fetchall()

        h = hashlib.sha256()
        h.update(b"[")
        for idx, row in enumerate(rows):
            (
                eid,
                tk,
                loc,
                slug,
                title,
                grp,
                status,
                hbp,
                pub_at,
                created_at,
                updated_at,
                ver,
                cmode,
                vals_json,
                primary_cat_id,
            ) = row
            assert slug.startswith("bench-"), f"unexpected non-fixture post slug {slug!r}"
            fixture_idx = int(slug.split("-", 1)[1])
            vals = json.loads(vals_json)
            media_sha = vals.get("featured_image")
            primary_slug = terms_by_id[int(primary_cat_id)][1] if primary_cat_id is not None else None
            rec = {
                "content_mode": cmode,
                "created_at": created_at,
                "group": grp,
                "has_been_published": int(hbp),
                "index": fixture_idx,
                "locale": loc,
                "media_sha256": media_sha,
                "primary_category": primary_slug,
                "published_at": pub_at,
                "route_claims": sorted(claims_by_entry.get(int(eid), [])),
                "slug": slug,
                "status": status,
                "terms": sorted(entry_terms.get(int(eid), [])),
                "title": title,
                "type": tk,
                "updated_at": updated_at,
                "values": vals,
                "version": int(ver),
            }
            if idx > 0:
                h.update(b",")
            h.update(json.dumps(rec, sort_keys=True, separators=(",", ":")).encode("utf-8"))
        h.update(b"]\n")
        return h.hexdigest(), len(rows)
    finally:
        conn.close()


def build_canonical_seed(
    baseline_binary: pathlib.Path,
    seed_root: pathlib.Path,
    n_posts: int,
    log_dir: pathlib.Path,
) -> dict[str, Any]:
    """Builds the canonical deterministic fixture version 1 using the baseline binary."""
    seed_root.mkdir(parents=True, exist_ok=True)
    srv = ServerHandle(baseline_binary, seed_root, log_dir / f"seed-boot1-{n_posts}.stderr.log")
    cat_ids: dict[str, dict[int, int]] = {"vi": {}, "en": {}}
    tag_ids: dict[str, dict[str, int]] = {"vi": {}, "en": {}}
    media_ids: dict[int, str] = {}
    try:
        srv.client.json_req(
            "POST", "/auth/setup", {"username": ADMIN_USER, "password": ADMIN_PASS}, expected=201
        )
        admin = srv.client.login(ADMIN_USER, ADMIN_PASS)
        srv.client.json_req(
            "POST",
            "/api/admin/users",
            {"username": EDITOR_USER, "password": EDITOR_PASS, "role": "editor"},
            admin,
            expected=201,
        )

        # Enable multilingual & add `en`.
        _, _, loc_h = srv.client.json_req("GET", "/api/admin/locales", auth=admin)
        _, _, ml_h = srv.client.json_req(
            "PUT",
            "/api/admin/locales/multilingual",
            {"enabled": True},
            admin,
            {"If-Match": loc_h["etag"]},
        )
        srv.client.json_req(
            "POST",
            "/api/admin/locales",
            {"code": "en", "label": "English"},
            admin,
            {"If-Match": ml_h["etag"]},
            expected=201,
        )

        # Provision 5 flat categories c0..c4 and tags common, t0..t3, bulk-added in EACH locale.
        for loc in ("vi", "en"):
            for k in range(5):
                _, term, _ = srv.client.json_req(
                    "POST",
                    "/api/admin/taxonomy/terms",
                    {
                        "kind": "category",
                        "name": f"Bench Cat {k} {loc}",
                        "slug": f"bench-{loc}-c{k}",
                        "locale": loc,
                    },
                    admin,
                    expected=201,
                )
                cat_ids[loc][k] = int(term["id"])
            for tname in ("common", "t0", "t1", "t2", "t3", "bulk-added"):
                _, term, _ = srv.client.json_req(
                    "POST",
                    "/api/admin/taxonomy/terms",
                    {
                        "kind": "tag",
                        "name": f"Bench Tag {tname} {loc}",
                        "slug": f"bench-{loc}-{tname}",
                        "locale": loc,
                    },
                    admin,
                    expected=201,
                )
                tag_ids[loc][tname] = int(term["id"])

        # Upload 20 deterministic PNGs via media_smoke.make_png(seed=1..20, 2x2).
        for seed in range(1, 21):
            png = make_png(seed=seed, width=2, height=2)
            expected_sha = hashlib.sha256(png).hexdigest()
            st, data, _ = srv.client.raw(
                "POST",
                "/api/admin/media",
                png,
                {
                    **admin,
                    "Content-Type": "image/png",
                    "X-File-Name": f"bench-media-{seed:02d}.png",
                },
            )
            assert st == 201, (st, data)
            asset = json.loads(data.decode("utf-8"))
            assert asset["id"] == expected_sha, (asset["id"], expected_sha)
            media_ids[seed] = asset["id"]

        # Assert zero unrelated ordinary posts exist before fixture seed.
        _, table0, _ = srv.client.json_req("GET", "/api/admin/content-types/post/entry-table?limit=1", auth=admin)
        assert table0["total"] == 0, f"expected 0 posts before fixture seed, got {table0['total']}"
    finally:
        srv.stop()

    db_path = seed_root / "database" / "opendash.sqlite"
    checkpoint_and_fk_check(db_path)

    # Fixed fixture transaction while stopped (baseline schema, foreign_keys ON).
    body_html = "<p>" + ("x" * 1024) + "</p>"
    conn = sqlite3.connect(str(db_path))
    try:
        conn.execute("PRAGMA foreign_keys = ON")
        conn.execute("BEGIN IMMEDIATE")
        entry_rows = []
        for i in range(n_posts):
            j = i // 2
            loc = "vi" if i % 2 == 0 else "en"
            slug = f"bench-{i:06d}"
            title = f"Bench Alpha {i:06d}" if (j % 4 == 0) else f"Bench Beta {i:06d}"
            grp = f"bench-pair-{j:06d}" if (j % 2 == 0) else f"bench-solo-{i:06d}"
            published = j % 2 == 0
            status = "published" if published else "draft"
            hbp = 1 if published else 0
            pub_at = "2024-01-01 00:00:00" if published else None
            vals: dict[str, Any] = {"body": body_html}
            if j % 5 == 0:
                vals["featured_image"] = media_ids[1 + (j % 20)]
            primary_cat = cat_ids[loc][j % 5]
            entry_rows.append(
                (
                    "post",
                    loc,
                    grp,
                    slug,
                    title,
                    json.dumps(vals, separators=(",", ":")),
                    1,
                    "2024-01-01 00:00:00",
                    "2024-01-01 00:00:00",
                    status,
                    None,
                    None,
                    "editor",
                    primary_cat,
                    hbp,
                    pub_at,
                )
            )
        conn.executemany(
            "INSERT INTO content_entries ("
            "  content_type_key, locale, translation_group_id, slug, title, values_json,"
            "  version, created_at, updated_at, publication_status, page_layout_key,"
            "  custom_template_key, content_mode, primary_category_term_id,"
            "  has_been_published, published_at"
            ") VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            entry_rows,
        )

        # Map actual row IDs back to i via slug (never assuming allocator values).
        slug_to_id = {
            str(r[0]): int(r[1])
            for r in conn.execute(
                "SELECT slug, id FROM content_entries WHERE content_type_key = 'post'"
            ).fetchall()
        }
        assert len(slug_to_id) == n_posts
        term_links = []
        for i in range(n_posts):
            j = i // 2
            loc = "vi" if i % 2 == 0 else "en"
            eid = slug_to_id[f"bench-{i:06d}"]
            term_links.append((eid, cat_ids[loc][j % 5]))
            term_links.append((eid, tag_ids[loc]["common"]))
            term_links.append((eid, tag_ids[loc][f"t{j % 4}"]))
        conn.executemany(
            "INSERT INTO entry_terms (entry_id, term_id) VALUES (?, ?)",
            term_links,
        )
        conn.commit()
    finally:
        conn.close()
    checkpoint_and_fk_check(db_path)

    # Reopen baseline and GET/PUT permalink settings with identical config + If-Match
    # to rebuild route claims through storage/permalinks.rs::put_settings.
    srv2 = ServerHandle(baseline_binary, seed_root, log_dir / f"seed-boot2-{n_posts}.stderr.log")
    try:
        admin = srv2.client.login(ADMIN_USER, ADMIN_PASS)
        _, pcfg, ph = srv2.client.json_req("GET", "/api/admin/settings/permalinks", auth=admin)
        etag = ph.get("etag") or f'"v{pcfg["version"]}"'
        srv2.client.json_req(
            "PUT",
            "/api/admin/settings/permalinks",
            {
                "post_structure": pcfg["post_structure"],
                "content_type_bases": pcfg["content_type_bases"],
                "taxonomy_bases": pcfg["taxonomy_bases"],
            },
            admin,
            {"If-Match": etag},
        )
    finally:
        srv2.stop()

    checkpoint_and_fk_check(db_path)

    # Verify exact fixture counts, status/locale/media/route claim distributions.
    conn = sqlite3.connect(str(db_path))
    try:
        total = conn.execute("SELECT COUNT(*) FROM content_entries WHERE content_type_key='post'").fetchone()[0]
        vi_cnt = conn.execute("SELECT COUNT(*) FROM content_entries WHERE content_type_key='post' AND locale='vi'").fetchone()[0]
        en_cnt = conn.execute("SELECT COUNT(*) FROM content_entries WHERE content_type_key='post' AND locale='en'").fetchone()[0]
        pub_cnt = conn.execute("SELECT COUNT(*) FROM content_entries WHERE content_type_key='post' AND publication_status='published'").fetchone()[0]
        draft_cnt = conn.execute("SELECT COUNT(*) FROM content_entries WHERE content_type_key='post' AND publication_status='draft'").fetchone()[0]
        claim_cnt = conn.execute("SELECT COUNT(*) FROM permalink_route_claims WHERE target_kind='entry'").fetchone()[0]
        et_cnt = conn.execute("SELECT COUNT(*) FROM entry_terms").fetchone()[0]
        assert total == n_posts, (total, n_posts)
        assert vi_cnt == n_posts // 2 and en_cnt == n_posts // 2
        assert pub_cnt + draft_cnt == n_posts
        assert claim_cnt == pub_cnt, f"expected {pub_cnt} route claims, got {claim_cnt}"
        assert et_cnt == n_posts * 3, f"expected {n_posts * 3} entry_terms, got {et_cnt}"
    finally:
        conn.close()

    manifest_sha, manifest_rows = compute_semantic_manifest(db_path)
    assert manifest_rows == n_posts
    return {
        "n_posts": n_posts,
        "cat_ids": cat_ids,
        "tag_ids": tag_ids,
        "media_ids": media_ids,
        "manifest_sha256": manifest_sha,
    }


def prepare_clean_clones(
    baseline_binary: pathlib.Path,
    candidate_binary: pathlib.Path,
    seed_root: pathlib.Path,
    baseline_clean: pathlib.Path,
    candidate_clean: pathlib.Path,
    expected_manifest_sha: str,
    n_posts: int,
    log_dir: pathlib.Path,
) -> dict[str, Any]:
    """Clones stopped seed into baseline_clean and candidate_clean, runs migration on
    candidate_clean only, verifies all rows active and byte-identical semantic manifest."""
    clone_site_dir(seed_root, baseline_clean)
    clone_site_dir(seed_root, candidate_clean)

    # Boot candidate once to apply migration 0054, verify all rows active, then stop & checkpoint.
    cand = ServerHandle(candidate_binary, candidate_clean, log_dir / f"migrate-cand-{n_posts}.stderr.log")
    try:
        admin = cand.client.login(ADMIN_USER, ADMIN_PASS)
        _, snap, _ = cand.client.json_req("GET", "/api/admin/content-types/post/entry-table?limit=1", auth=admin)
        assert snap["total"] == n_posts and snap["status_counts"]["trash"] == 0, snap["status_counts"]
    finally:
        cand.stop()

    cand_db = candidate_clean / "database" / "opendash.sqlite"
    base_db = baseline_clean / "database" / "opendash.sqlite"
    checkpoint_and_fk_check(cand_db)
    checkpoint_and_fk_check(base_db)

    base_sha, base_rows = compute_semantic_manifest(base_db)
    cand_sha, cand_rows = compute_semantic_manifest(cand_db)
    assert base_sha == expected_manifest_sha and base_rows == n_posts
    assert cand_sha == expected_manifest_sha and cand_rows == n_posts, (
        f"Semantic manifest mismatch after candidate migration! baseline={base_sha} candidate={cand_sha}"
    )

    conn = sqlite3.connect(str(cand_db))
    try:
        trashed = conn.execute("SELECT COUNT(*) FROM content_entries WHERE trashed_at IS NOT NULL").fetchone()[0]
        assert trashed == 0, f"expected 0 trashed rows after migration, got {trashed}"
    finally:
        conn.close()

    return {
        "baseline_manifest_sha256": base_sha,
        "candidate_manifest_sha256": cand_sha,
        "baseline_db_bytes": base_db.stat().st_size,
        "candidate_db_bytes": cand_db.stat().st_size,
    }


def run_p1_list_benchmark(
    baseline_binary: pathlib.Path,
    candidate_binary: pathlib.Path,
    baseline_clean: pathlib.Path,
    candidate_clean: pathlib.Path,
    seed_meta: dict[str, Any],
    work_root: pathlib.Path,
    log_dir: pathlib.Path,
    warmups: int,
    reps: int,
) -> dict[str, Any]:
    """Runs P1: 6 list queries x 2 roles (Admin, Editor), alternating baseline/candidate order,
    5 warmups + 30 measured requests, strict result equivalence, plus candidate-only 10%-trash diagnostic."""
    n_posts = seed_meta["n_posts"]
    vi_c0 = seed_meta["cat_ids"]["vi"][0]
    cases = [
        ("all_p20_off0", "status=all&sort=id&order=asc&limit=20&offset=0"),
        ("all_p100_off100", "status=all&sort=id&order=asc&limit=100&offset=100"),
        ("draft_vi_p20", "status=draft&locale=vi&sort=id&order=asc&limit=20&offset=0"),
        ("pub_en_title_desc", "status=published&locale=en&sort=title&order=desc&limit=20&offset=0"),
        ("search_alpha_vi", "q=Alpha&locale=vi&sort=id&order=asc&limit=20&offset=0"),
        ("tax_cat_c0_vi", f"taxonomy_key=category&term_id={vi_c0}&locale=vi&sort=id&order=asc&limit=20&offset=0"),
    ]

    base_run = work_root / f"p1-base-{n_posts}"
    cand_run = work_root / f"p1-cand-{n_posts}"
    clone_site_dir(baseline_clean, base_run)
    clone_site_dir(candidate_clean, cand_run)

    b_srv = ServerHandle(baseline_binary, base_run, log_dir / f"p1-base-{n_posts}.stderr.log")
    c_srv = ServerHandle(candidate_binary, cand_run, log_dir / f"p1-cand-{n_posts}.stderr.log")
    case_reports: list[dict[str, Any]] = []
    gates_passed = True
    try:
        auths = {
            "baseline": {
                "admin": b_srv.client.login(ADMIN_USER, ADMIN_PASS),
                "editor": b_srv.client.login(EDITOR_USER, EDITOR_PASS),
            },
            "candidate": {
                "admin": c_srv.client.login(ADMIN_USER, ADMIN_PASS),
                "editor": c_srv.client.login(EDITOR_USER, EDITOR_PASS),
            },
        }
        servers = {"baseline": b_srv, "candidate": c_srv}

        case_idx = 0
        for role in ("admin", "editor"):
            for case_name, qs in cases:
                path = f"/api/admin/content-types/post/entry-table?{qs}"
                order = ("baseline", "candidate") if (case_idx % 2 == 0) else ("candidate", "baseline")
                case_idx += 1
                samples: dict[str, list[float]] = {"baseline": [], "candidate": []}
                fingerprints: dict[str, Any] = {}
                for target in order:
                    srv = servers[target]
                    auth = auths[target][role]
                    for _ in range(warmups):
                        srv.client.json_req("GET", path, auth=auth)
                    for _ in range(reps):
                        t0 = time.perf_counter()
                        _, body, _ = srv.client.json_req("GET", path, auth=auth)
                        dt_ms = (time.perf_counter() - t0) * 1000.0
                        samples[target].append(dt_ms)
                        if target not in fingerprints:
                            fingerprints[target] = {
                                "total": body["total"],
                                "counts": {
                                    "all": body["status_counts"]["all"],
                                    "draft": body["status_counts"]["draft"],
                                    "published": body["status_counts"]["published"],
                                },
                                "rows": [(r["slug"], r["title"], r["publication_status"], r["locale"]) for r in body["items"]],
                            }
                    srv.sample_resources()

                assert fingerprints["baseline"] == fingerprints["candidate"], (
                    f"P1 semantic mismatch on {role}/{case_name}: "
                    f"base={fingerprints['baseline']} cand={fingerprints['candidate']}"
                )
                b_stat = summarize_ms(samples["baseline"])
                c_stat = summarize_ms(samples["candidate"])
                threshold = max(1.25 * b_stat["p95_ms"], b_stat["p95_ms"] + 50.0)
                passed = c_stat["p95_ms"] <= threshold
                if not passed:
                    gates_passed = False
                case_reports.append(
                    {
                        "role": role,
                        "case": case_name,
                        "query": qs,
                        "order": list(order),
                        "total": fingerprints["candidate"]["total"],
                        "baseline": b_stat,
                        "candidate": c_stat,
                        "threshold_p95_ms": round(threshold, 3),
                        "pass": passed,
                        "baseline_samples_ms": [round(x, 3) for x in samples["baseline"]],
                        "candidate_samples_ms": [round(x, 3) for x in samples["candidate"]],
                    }
                )

        # Candidate-only diagnostic after trashing first floor(N/10) posts by ascending index (both locales).
        trash_n = n_posts // 10
        cand_admin = auths["candidate"]["admin"]
        trashed_done = 0
        while trashed_done < trash_n:
            batch_lim = min(100, trash_n - trashed_done)
            _, tbl, _ = c_srv.client.json_req(
                "GET",
                f"/api/admin/content-types/post/entry-table?status=all&sort=id&order=asc&limit={batch_lim}&offset=0",
                auth=cand_admin,
            )
            for row in tbl["items"]:
                c_srv.client.json_req(
                    "POST",
                    f"/api/admin/content-types/post/entries/{row['id']}/trash",
                    {},
                    cand_admin,
                    {"If-Match": f'"v{row["version"]}"'},
                )
                trashed_done += 1

        # Optional encrypted backup size measurement (candidate post-10%-trash).
        _, trash_diag_snap, _ = c_srv.client.json_req(
            "GET",
            "/api/admin/content-types/post/entry-table?status=all&sort=id&order=asc&limit=1",
            auth=cand_admin,
        )
        assert trash_diag_snap["status_counts"]["trash"] == trash_n
        assert trash_diag_snap["status_counts"]["all"] == n_posts - trash_n

        post_trash_diag: list[dict[str, Any]] = []
        for role in ("admin", "editor"):
            auth = auths["candidate"][role]
            for case_name, qs in cases:
                path = f"/api/admin/content-types/post/entry-table?{qs}"
                for _ in range(warmups):
                    c_srv.client.json_req("GET", path, auth=auth)
                s_ms = []
                for _ in range(reps):
                    t0 = time.perf_counter()
                    c_srv.client.json_req("GET", path, auth=auth)
                    s_ms.append((time.perf_counter() - t0) * 1000.0)
                post_trash_diag.append({"role": role, "case": case_name, "candidate_10pct_trash": summarize_ms(s_ms)})
        c_srv.sample_resources()

        resources = {
            "baseline_peak_rss_bytes": b_srv.peak_rss_bytes,
            "baseline_wal_high_water_bytes": b_srv.wal_high_water_bytes,
            "candidate_peak_rss_bytes": c_srv.peak_rss_bytes,
            "candidate_wal_high_water_bytes": c_srv.wal_high_water_bytes,
        }
    finally:
        b_srv.stop()
        c_srv.stop()
        shutil.rmtree(base_run, ignore_errors=True)
        shutil.rmtree(cand_run, ignore_errors=True)

    return {
        "gate_pass": gates_passed,
        "cases": case_reports,
        "candidate_post_10pct_trash_diagnostic": post_trash_diag,
        "resources": resources,
    }


def run_p2_publish_draft_benchmark(
    baseline_binary: pathlib.Path,
    candidate_binary: pathlib.Path,
    baseline_clean: pathlib.Path,
    candidate_clean: pathlib.Path,
    n_posts: int,
    work_root: pathlib.Path,
    log_dir: pathlib.Path,
    reps: int,
) -> dict[str, Any]:
    """Runs P2: Admin & Editor, cohorts B=20 and B=100, publish first B draft vi posts
    and move-to-draft first B published vi posts, independent stopped-seed clone per repetition."""
    cases_out: list[dict[str, Any]] = []
    gates_passed = True

    for role in ("admin", "editor"):
        user, pw = (ADMIN_USER, ADMIN_PASS) if role == "admin" else (EDITOR_USER, EDITOR_PASS)
        for cohort in (20, 100):
            for op, init_status, target_status in (
                ("publish", "draft", "published"),
                ("draft", "published", "draft"),
            ):
                base_batch_ms: list[float] = []
                cand_batch_ms: list[float] = []
                base_row_ms: list[float] = []
                cand_row_ms: list[float] = []
                peak_rss = {"baseline": 0, "candidate": 0}
                wal_hwm = {"baseline": 0, "candidate": 0}

                for rep in range(reps):
                    order = ("baseline", "candidate") if (rep % 2 == 0) else ("candidate", "baseline")
                    for target in order:
                        binary = baseline_binary if target == "baseline" else candidate_binary
                        clean_src = baseline_clean if target == "baseline" else candidate_clean
                        run_dir = work_root / f"p2-{n_posts}-{role}-{cohort}-{op}-{target}-r{rep}"
                        clone_site_dir(clean_src, run_dir)
                        srv = ServerHandle(binary, run_dir, log_dir / f"p2-{n_posts}-{target}.stderr.log")
                        try:
                            auth = srv.client.login(user, pw)
                            _, tbl, _ = srv.client.json_req(
                                "GET",
                                f"/api/admin/content-types/post/entry-table?status={init_status}&locale=vi&sort=id&order=asc&limit={cohort}&offset=0",
                                auth=auth,
                            )
                            items = tbl["items"]
                            assert len(items) == cohort, (len(items), cohort)
                            row_samples = []
                            t_batch0 = time.perf_counter()
                            for row in items:
                                t_row0 = time.perf_counter()
                                _, res, _ = srv.client.json_req(
                                    "POST",
                                    f"/api/admin/content-types/post/entries/{row['id']}/publication",
                                    {"status": target_status},
                                    auth,
                                    {"If-Match": f'"v{row["version"]}"'},
                                )
                                row_samples.append((time.perf_counter() - t_row0) * 1000.0)
                                assert res["publication_status"] == target_status
                                assert res["version"] == row["version"] + 1
                            batch_ms = (time.perf_counter() - t_batch0) * 1000.0
                            srv.sample_resources()
                            if rep == 0:
                                # Post-run invariant verification on first repetition.
                                _, check_tbl, _ = srv.client.json_req(
                                    "GET",
                                    "/api/admin/content-types/post/entry-table?locale=vi&limit=1",
                                    auth=auth,
                                )
                                assert check_tbl["status_counts"]["all"] == n_posts // 2
                            if target == "baseline":
                                base_batch_ms.append(batch_ms)
                                base_row_ms.extend(row_samples)
                            else:
                                cand_batch_ms.append(batch_ms)
                                cand_row_ms.extend(row_samples)
                            peak_rss[target] = max(peak_rss[target], srv.peak_rss_bytes)
                            wal_hwm[target] = max(wal_hwm[target], srv.wal_high_water_bytes)
                        finally:
                            srv.stop()
                            shutil.rmtree(run_dir, ignore_errors=True)

                b_batch = summarize_ms(base_batch_ms)
                c_batch = summarize_ms(cand_batch_ms)
                threshold = max(1.25 * b_batch["p95_ms"], b_batch["p95_ms"] + 1000.0)
                passed = c_batch["p95_ms"] <= threshold
                if not passed:
                    gates_passed = False
                cases_out.append(
                    {
                        "role": role,
                        "cohort": cohort,
                        "operation": op,
                        "baseline_batch": b_batch,
                        "candidate_batch": c_batch,
                        "baseline_row": summarize_ms(base_row_ms),
                        "candidate_row": summarize_ms(cand_row_ms),
                        "threshold_batch_p95_ms": round(threshold, 3),
                        "pass": passed,
                        "peak_rss_bytes": peak_rss,
                        "wal_high_water_bytes": wal_hwm,
                        "baseline_batch_samples_ms": [round(x, 3) for x in base_batch_ms],
                        "candidate_batch_samples_ms": [round(x, 3) for x in cand_batch_ms],
                    }
                )
    return {"gate_pass": gates_passed, "cases": cases_out}


def run_p3_lifecycle_and_taxonomy_benchmark(
    candidate_binary: pathlib.Path,
    candidate_clean: pathlib.Path,
    seed_meta: dict[str, Any],
    work_root: pathlib.Path,
    log_dir: pathlib.Path,
    reps: int,
) -> dict[str, Any]:
    """Runs P3 (candidate only): Trash, Restore, Taxonomy add/remove/replace (Admin & Editor),
    and Purge (Admin only, reported without threshold) across cohorts B=20 and B=100.
    1 untimed validation run + `reps` measured independent resets per case.
    Reports BOTH server monotonic storage time (parsed from `entry_lifecycle_diag`) and
    end-to-end client row/batch timing."""
    n_posts = seed_meta["n_posts"]
    vi_tag_bulk = seed_meta["tag_ids"]["vi"]["bulk-added"]
    vi_tag_common = seed_meta["tag_ids"]["vi"]["common"]
    p3_limit_ms = 500.0 if n_posts <= 10000 else 2000.0

    ops_spec = [
        ("trash", ("admin", "editor"), True),
        ("restore", ("admin", "editor"), True),
        ("taxonomy_add", ("admin", "editor"), True),
        ("taxonomy_remove", ("admin", "editor"), True),
        ("taxonomy_replace", ("admin", "editor"), True),
        ("purge", ("admin",), False),
    ]

    cases_out: list[dict[str, Any]] = []
    gates_passed = True

    for op, roles, has_gate in ops_spec:
        for role in roles:
            user, pw = (ADMIN_USER, ADMIN_PASS) if role == "admin" else (EDITOR_USER, EDITOR_PASS)
            for cohort in (20, 100):
                server_row_ms: list[float] = []
                client_row_ms: list[float] = []
                client_batch_ms: list[float] = []
                peak_rss = 0
                wal_hwm = 0
                hook_enqueued_total = 0
                hook_dropped_total = 0

                # rep == -1 is the untimed validation run; 0..reps-1 are measured repetitions.
                for rep in range(-1, reps):
                    is_validation = rep == -1
                    run_dir = work_root / f"p3-{n_posts}-{op}-{role}-{cohort}-r{rep}"
                    clone_site_dir(candidate_clean, run_dir)
                    stderr_file = log_dir / f"p3-{n_posts}-{op}-{role}-{cohort}.stderr.log"
                    srv = ServerHandle(candidate_binary, run_dir, stderr_file)
                    try:
                        auth = srv.client.login(user, pw)
                        _, tbl, _ = srv.client.json_req(
                            "GET",
                            f"/api/admin/content-types/post/entry-table?status=published&locale=vi&sort=id&order=asc&limit={cohort}&offset=0",
                            auth=auth,
                        )
                        items = tbl["items"]
                        schema_token = tbl["schema_token"]
                        assert len(items) == cohort, (len(items), cohort)

                        # Pre-trash same cohort outside timing for Restore and Purge.
                        if op in ("restore", "purge"):
                            for row in items:
                                srv.client.json_req(
                                    "POST",
                                    f"/api/admin/content-types/post/entries/{row['id']}/trash",
                                    {},
                                    auth,
                                    {"If-Match": f'"v{row["version"]}"'},
                                )
                            _, trash_tbl, _ = srv.client.json_req(
                                "GET",
                                f"/api/admin/content-types/post/entry-table?status=trash&locale=vi&sort=id&order=asc&limit={cohort}&offset=0",
                                auth=auth,
                            )
                            items = trash_tbl["items"]
                            assert len(items) == cohort, (len(items), cohort)

                        diag_start = srv.diag_offset()
                        rep_client_rows: list[float] = []
                        t_batch0 = time.perf_counter()
                        for row in items:
                            t_r0 = time.perf_counter()
                            if op == "trash":
                                _, body, _ = srv.client.json_req(
                                    "POST",
                                    f"/api/admin/content-types/post/entries/{row['id']}/trash",
                                    {},
                                    auth,
                                    {"If-Match": f'"v{row["version"]}"'},
                                )
                                assert body["version"] == row["version"] + 1
                                sec = body.get("secondary") or {}
                                hook_enqueued_total += int(sec.get("hook_enqueued", 0))
                                hook_dropped_total += int(sec.get("hook_dropped", 0))
                            elif op == "restore":
                                _, body, _ = srv.client.json_req(
                                    "POST",
                                    f"/api/admin/content-types/post/entries/{row['id']}/restore",
                                    {},
                                    auth,
                                    {"If-Match": f'"v{row["version"]}"'},
                                )
                                assert body["publication_status"] == "draft"
                                assert body["version"] == row["version"] + 1
                                sec = body.get("secondary") or {}
                                hook_enqueued_total += int(sec.get("hook_enqueued", 0))
                                hook_dropped_total += int(sec.get("hook_dropped", 0))
                            elif op == "purge":
                                srv.client.json_req(
                                    "DELETE",
                                    f"/api/admin/content-types/post/entries/{row['id']}/purge",
                                    None,
                                    auth,
                                    {"If-Match": f'"v{row["version"]}"', "X-Confirm-Purge": "true"},
                                    expected=204,
                                )
                            else:
                                tax_op = op.split("_", 1)[1]
                                term_list = [vi_tag_bulk] if tax_op in ("add", "replace") else [vi_tag_common]
                                _, body, _ = srv.client.json_req(
                                    "PATCH",
                                    f"/api/admin/content-types/post/entries/{row['id']}/metadata",
                                    {
                                        "expected_schema_token": schema_token,
                                        "taxonomy": {
                                            "key": "tag",
                                            "operation": tax_op,
                                            "term_ids": term_list,
                                        },
                                    },
                                    auth,
                                    {"If-Match": f'"v{row["version"]}"'},
                                )
                                assert body["version"] == row["version"] + 1
                                sec = body.get("secondary") or {}
                                hook_enqueued_total += int(sec.get("hook_enqueued", 0))
                                hook_dropped_total += int(sec.get("hook_dropped", 0))
                            rep_client_rows.append((time.perf_counter() - t_r0) * 1000.0)

                        rep_batch_ms = (time.perf_counter() - t_batch0) * 1000.0
                        srv.sample_resources()
                        diags = srv.parse_diags_since(diag_start)

                        # Extract per-entry server monotonic storage elapsed time (ms).
                        if op in ("trash", "restore", "purge"):
                            matching = [d for d in diags if d["op"] == op and d["outcome"] == "ok"]
                            assert len(matching) == cohort, (op, len(matching), cohort)
                            rep_server_rows = [d["storage_us"] / 1000.0 for d in matching]
                        else:
                            # Metadata patch has both preflight (metadata_prepare) and writer tx (metadata_commit).
                            prep_by_id = {d["entry_id"]: d["storage_us"] for d in diags if d["op"] == "metadata_prepare" and d["outcome"] == "ok"}
                            comm_by_id = {d["entry_id"]: d["storage_us"] for d in diags if d["op"] == "metadata_commit" and d["outcome"] == "ok"}
                            assert len(comm_by_id) == cohort, (op, len(comm_by_id), cohort)
                            rep_server_rows = [
                                (prep_by_id.get(row["id"], 0) + comm_by_id[row["id"]]) / 1000.0
                                for row in items
                            ]

                        if is_validation:
                            # Thorough post-run invariant checks on untimed validation run.
                            _, after_tbl, _ = srv.client.json_req(
                                "GET",
                                "/api/admin/content-types/post/entry-table?locale=vi&limit=1",
                                auth=auth,
                            )
                            if op == "trash":
                                assert after_tbl["status_counts"]["trash"] == cohort
                                assert after_tbl["status_counts"]["all"] == (n_posts // 2) - cohort
                            elif op == "restore":
                                assert after_tbl["status_counts"]["trash"] == 0
                                assert after_tbl["status_counts"]["all"] == n_posts // 2
                            elif op == "purge":
                                assert after_tbl["status_counts"]["trash"] == 0
                                assert after_tbl["status_counts"]["all"] == (n_posts // 2) - cohort
                            else:
                                assert after_tbl["status_counts"]["all"] == n_posts // 2
                                _, sample_row, _ = srv.client.json_req(
                                    "GET",
                                    f"/api/admin/content-types/post/entries/{items[0]['id']}",
                                    auth=auth,
                                )
                                terms_set = set(sample_row["term_ids"])
                                if op == "taxonomy_add":
                                    assert vi_tag_bulk in terms_set and vi_tag_common in terms_set
                                elif op == "taxonomy_remove":
                                    assert vi_tag_common not in terms_set
                                elif op == "taxonomy_replace":
                                    assert vi_tag_bulk in terms_set and vi_tag_common not in terms_set
                                # Category + primary category must be preserved!
                                assert sample_row["primary_category_term_id"] in terms_set
                        else:
                            server_row_ms.extend(rep_server_rows)
                            client_row_ms.extend(rep_client_rows)
                            client_batch_ms.append(rep_batch_ms)
                            peak_rss = max(peak_rss, srv.peak_rss_bytes)
                            wal_hwm = max(wal_hwm, srv.wal_high_water_bytes)
                    finally:
                        srv.stop()
                        shutil.rmtree(run_dir, ignore_errors=True)

                srv_stat = summarize_ms(server_row_ms)
                cli_row_stat = summarize_ms(client_row_ms)
                cli_batch_stat = summarize_ms(client_batch_ms)
                passed = (not has_gate) or (srv_stat["p95_ms"] <= p3_limit_ms)
                if not passed:
                    gates_passed = False
                cases_out.append(
                    {
                        "operation": op,
                        "role": role,
                        "cohort": cohort,
                        "has_threshold": has_gate,
                        "threshold_server_row_p95_ms": p3_limit_ms if has_gate else None,
                        "pass": passed,
                        "server_row": srv_stat,
                        "client_row": cli_row_stat,
                        "client_batch": cli_batch_stat,
                        "peak_rss_bytes": peak_rss,
                        "wal_high_water_bytes": wal_hwm,
                        "hook_enqueued_total": hook_enqueued_total,
                        "hook_dropped_total": hook_dropped_total,
                        "client_batch_samples_ms": [round(x, 3) for x in client_batch_ms],
                    }
                )
    return {"gate_pass": gates_passed, "cases": cases_out}


def measure_backup_growth(
    candidate_binary: pathlib.Path,
    candidate_clean: pathlib.Path,
    n_posts: int,
    work_root: pathlib.Path,
    log_dir: pathlib.Path,
) -> dict[str, Any]:
    """Measures candidate encrypted backup size and media-reconcile pin safety before & after 10% Trash."""
    run_dir = work_root / f"backup-probe-{n_posts}"
    clone_site_dir(candidate_clean, run_dir)
    srv = ServerHandle(candidate_binary, run_dir, log_dir / f"backup-{n_posts}.stderr.log")
    try:
        admin = srv.client.login(ADMIN_USER, ADMIN_PASS)
        _, b_before, _ = srv.client.json_req(
            "POST",
            "/api/admin/operations/backups",
            {"passphrase": "Bench-Backup-Passphrase-123!"},
            admin,
            expected=(200, 201),
        )
        _, rec_before, _ = srv.client.json_req("GET", "/api/admin/operations/media/reconcile", auth=admin)
        # Trash first floor(N/10) posts (or max 200 for fast backup probe when N is huge? No, use exact floor(N/10)).
        trash_n = n_posts // 10
        trashed = 0
        while trashed < trash_n:
            lim = min(100, trash_n - trashed)
            _, tbl, _ = srv.client.json_req(
                "GET",
                f"/api/admin/content-types/post/entry-table?status=all&sort=id&order=asc&limit={lim}&offset=0",
                auth=admin,
            )
            for row in tbl["items"]:
                srv.client.json_req(
                    "POST",
                    f"/api/admin/content-types/post/entries/{row['id']}/trash",
                    {},
                    admin,
                    {"If-Match": f'"v{row["version"]}"'},
                )
                trashed += 1
        _, b_after, _ = srv.client.json_req(
            "POST",
            "/api/admin/operations/backups",
            {"passphrase": "Bench-Backup-Passphrase-123!"},
            admin,
            expected=(200, 201),
        )
        _, rec_after, _ = srv.client.json_req("GET", "/api/admin/operations/media/reconcile", auth=admin)
        assert rec_after.get("orphan_count", 0) == 0, f"Trashing entries must never orphan retained media pins: {rec_after}"
        return {
            "backup_before_trash_bytes": b_before.get("size_bytes", 0),
            "backup_after_10pct_trash_bytes": b_after.get("size_bytes", 0),
            "reconcile_before_orphans": rec_before.get("orphan_count", 0),
            "reconcile_after_10pct_trash_orphans": rec_after.get("orphan_count", 0),
        }
    finally:
        srv.stop()
        shutil.rmtree(run_dir, ignore_errors=True)


def run_benchmark(args: argparse.Namespace) -> int:
    baseline_binary = args.baseline_binary.resolve()
    candidate_binary = args.candidate_binary.resolve()
    artifact_dir = args.artifact_dir.resolve()
    assert baseline_binary.is_file(), f"Missing baseline binary: {baseline_binary}"
    assert candidate_binary.is_file(), f"Missing candidate binary: {candidate_binary}"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    log_dir = artifact_dir / "logs"
    log_dir.mkdir(parents=True, exist_ok=True)

    sizes = [int(x.strip()) for x in args.sizes.split(",") if x.strip()]
    report: dict[str, Any] = {
        "fixture_version": 1,
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "host": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "cpu_count": os.cpu_count(),
            "loadavg_start": list(os.getloadavg()) if hasattr(os, "getloadavg") else None,
        },
        "binaries": {
            "baseline_path": str(baseline_binary),
            "baseline_sha256": sha256_file(baseline_binary),
            "candidate_path": str(candidate_binary),
            "candidate_sha256": sha256_file(candidate_binary),
        },
        "config": {
            "sizes": sizes,
            "list_warmups": args.list_warmups,
            "list_reps": args.list_reps,
            "bulk_reps": args.bulk_reps,
        },
        "results": {},
        "all_gates_pass": True,
    }

    with tempfile.TemporaryDirectory(prefix="opendash-s04-bench-", dir=artifact_dir.parent) as td:
        work_root = pathlib.Path(td).resolve()
        for n_posts in sizes:
            t_n0 = time.monotonic()
            print(f"[bench] === Building canonical fixture N={n_posts} ===", flush=True)
            seed_root = work_root / f"seed-{n_posts}"
            seed_meta = build_canonical_seed(baseline_binary, seed_root, n_posts, log_dir)
            base_clean = work_root / f"clean-base-{n_posts}"
            cand_clean = work_root / f"clean-cand-{n_posts}"
            clone_info = prepare_clean_clones(
                baseline_binary,
                candidate_binary,
                seed_root,
                base_clean,
                cand_clean,
                seed_meta["manifest_sha256"],
                n_posts,
                log_dir,
            )
            shutil.rmtree(seed_root, ignore_errors=True)

            print(f"[bench] N={n_posts}: Running P1 list/count suite...", flush=True)
            p1 = run_p1_list_benchmark(
                baseline_binary,
                candidate_binary,
                base_clean,
                cand_clean,
                seed_meta,
                work_root,
                log_dir,
                args.list_warmups,
                args.list_reps,
            )
            print(f"[bench] N={n_posts}: P1 gate_pass={p1['gate_pass']}", flush=True)

            print(f"[bench] N={n_posts}: Running P2 publish/draft suite...", flush=True)
            p2 = run_p2_publish_draft_benchmark(
                baseline_binary,
                candidate_binary,
                base_clean,
                cand_clean,
                n_posts,
                work_root,
                log_dir,
                args.bulk_reps,
            )
            print(f"[bench] N={n_posts}: P2 gate_pass={p2['gate_pass']}", flush=True)

            print(f"[bench] N={n_posts}: Running P3 lifecycle/taxonomy/purge suite...", flush=True)
            p3 = run_p3_lifecycle_and_taxonomy_benchmark(
                candidate_binary,
                cand_clean,
                seed_meta,
                work_root,
                log_dir,
                args.bulk_reps,
            )
            print(f"[bench] N={n_posts}: P3 gate_pass={p3['gate_pass']}", flush=True)

            backup_info = None
            if not args.skip_backup:
                print(f"[bench] N={n_posts}: Measuring backup & media pin safety...", flush=True)
                backup_info = measure_backup_growth(
                    candidate_binary, cand_clean, n_posts, work_root, log_dir
                )

            shutil.rmtree(base_clean, ignore_errors=True)
            shutil.rmtree(cand_clean, ignore_errors=True)

            size_pass = bool(p1["gate_pass"] and p2["gate_pass"] and p3["gate_pass"])
            if not size_pass:
                report["all_gates_pass"] = False
            report["results"][str(n_posts)] = {
                "n_posts": n_posts,
                "elapsed_sec": round(time.monotonic() - t_n0, 2),
                "manifest_sha256": seed_meta["manifest_sha256"],
                "clones": clone_info,
                "p1": p1,
                "p2": p2,
                "p3": p3,
                "backup_and_pins": backup_info,
                "all_gates_pass": size_pass,
            }
            # Persist incremental progress after each N.
            (artifact_dir / "benchmark-report.json").write_text(
                json.dumps(report, indent=2) + "\n", encoding="utf-8"
            )

    report["host"]["loadavg_end"] = list(os.getloadavg()) if hasattr(os, "getloadavg") else None
    (artifact_dir / "benchmark-report.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )

    # Compact human-readable summary for quick inspection.
    summary_lines = [
        f"S04 Canonical Benchmark Summary (all_gates_pass={report['all_gates_pass']})",
        f"Baseline : {report['binaries']['baseline_sha256'][:12]} ({baseline_binary})",
        f"Candidate: {report['binaries']['candidate_sha256'][:12]} ({candidate_binary})",
    ]
    for sz in sizes:
        r = report["results"][str(sz)]
        summary_lines.append(
            f"  N={sz}: manifest={r['manifest_sha256'][:12]} P1={r['p1']['gate_pass']} "
            f"P2={r['p2']['gate_pass']} P3={r['p3']['gate_pass']} ({r['elapsed_sec']}s)"
        )
    summary_txt = "\n".join(summary_lines) + "\n"
    (artifact_dir / "benchmark-summary.txt").write_text(summary_txt, encoding="utf-8")
    print(summary_txt, flush=True)
    return 0 if report["all_gates_pass"] else 2


def main() -> None:
    parser = argparse.ArgumentParser(description="Canonical S04 entry-lifecycle performance benchmark.")
    parser.add_argument("--baseline-binary", required=True, type=pathlib.Path)
    parser.add_argument("--candidate-binary", required=True, type=pathlib.Path)
    parser.add_argument("--artifact-dir", required=True, type=pathlib.Path)
    parser.add_argument("--sizes", default="1000,10000,50000")
    parser.add_argument("--list-warmups", type=int, default=5)
    parser.add_argument("--list-reps", type=int, default=30)
    parser.add_argument("--bulk-reps", type=int, default=30)
    parser.add_argument("--skip-backup", action="store_true")
    args = parser.parse_args()
    raise SystemExit(run_benchmark(args))


if __name__ == "__main__":
    main()
