#!/usr/bin/env bash
set -euo pipefail

# Build reproducible-input Linux x86_64-unknown-linux-musl release bundle.
# Usage:
#   bash cms/scripts/build-release-linux.sh [--out-dir <dir>] [--skip-npm-ci]

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$ROOT_DIR/cms/target/release-bundle"
SKIP_NPM_CI="false"
TARGET="x86_64-unknown-linux-musl"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --skip-npm-ci) SKIP_NPM_CI="true"; shift ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/deploy"

echo "[1/4] Building embedded Admin assets..."
pushd "$ROOT_DIR/cms/admin" >/dev/null
if [[ "$SKIP_NPM_CI" != "true" ]]; then
  npm ci
fi
npm run build
popd >/dev/null

echo "[2/4] Building opendash for $TARGET..."
rustup target add "$TARGET"
cargo build --release --locked --target "$TARGET" --manifest-path "$ROOT_DIR/cms/Cargo.toml"

BIN_SRC="$ROOT_DIR/cms/target/$TARGET/release/opendash"
if [[ ! -f "$BIN_SRC" ]]; then
  echo "Expected binary not found at $BIN_SRC" >&2
  exit 1
fi

cp "$BIN_SRC" "$OUT_DIR/opendash"
chmod 0755 "$OUT_DIR/opendash"
cp "$ROOT_DIR/README.md" "$OUT_DIR/README.md"
cp "$ROOT_DIR/cms/deploy/opendash.service" "$OUT_DIR/deploy/opendash.service"
cp "$ROOT_DIR/cms/deploy/nginx.conf.example" "$OUT_DIR/deploy/nginx.conf.example"
cp "$ROOT_DIR/cms/deploy/install-linux.sh" "$OUT_DIR/deploy/install-linux.sh"
chmod 0755 "$OUT_DIR/deploy/install-linux.sh"

echo "[3/4] Generating release-manifest.json and checksums.txt..."
python3 - "$ROOT_DIR" "$OUT_DIR" "$TARGET" <<'PY'
import hashlib
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
out_dir = pathlib.Path(sys.argv[2])
target = sys.argv[3]

def sha256_file(p: pathlib.Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()

def dir_sha256(d: pathlib.Path) -> str:
    h = hashlib.sha256()
    for p in sorted(x for x in d.rglob("*") if x.is_file()):
        rel = p.relative_to(d).as_posix()
        h.update(f"{rel}:{sha256_file(p)}\n".encode("utf-8"))
    return h.hexdigest()

source_sha = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
rust_ver = subprocess.check_output(["rustc", "--version"], text=True).strip()
node_ver = subprocess.check_output(["node", "--version"], text=True).strip()

migrations = {}
for m in sorted((root / "cms" / "migrations").glob("*.sql")):
    migrations[m.name] = sha256_file(m)

manifest = {
    "format_version": 1,
    "target": target,
    "source_sha": source_sha,
    "rust_version": rust_ver,
    "node_version": node_ver,
    "cargo_lock_sha256": sha256_file(root / "cms" / "Cargo.lock"),
    "admin_dist_sha256": dir_sha256(root / "cms" / "admin" / "dist"),
    "binary_sha256": sha256_file(out_dir / "opendash"),
    "migration_checksums": migrations,
}
(out_dir / "release-manifest.json").write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)

checksum_lines = []
for p in sorted(x for x in out_dir.rglob("*") if x.is_file() and x.name != "checksums.txt"):
    rel = p.relative_to(out_dir).as_posix()
    checksum_lines.append(f"{sha256_file(p)}  {rel}")
(out_dir / "checksums.txt").write_text("\n".join(checksum_lines) + "\n", encoding="utf-8")
PY

echo "[4/4] Bundle ready at $OUT_DIR"
