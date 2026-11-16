#!/usr/bin/env bash
set -euo pipefail

# Explicit operator installer/upgrader for OpenDash on Linux.
# Usage:
#   sudo ./install-linux.sh --bundle-dir <extracted_bundle> --version <semver_or_sha> [--upgrade] [--external-db-dir <dir>]

BUNDLE_DIR=""
VERSION=""
ALLOW_UPGRADE="false"
EXTERNAL_DB_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle-dir) BUNDLE_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --upgrade) ALLOW_UPGRADE="true"; shift ;;
    --external-db-dir) EXTERNAL_DB_DIR="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$BUNDLE_DIR" || -z "$VERSION" ]]; then
  echo "Usage: $0 --bundle-dir <dir> --version <version> [--upgrade] [--external-db-dir <dir>]" >&2
  exit 2
fi

if [[ ! -x "$BUNDLE_DIR/opendash" ]]; then
  echo "Missing executable $BUNDLE_DIR/opendash" >&2
  exit 1
fi

if [[ $EUID -ne 0 ]]; then
  echo "Run as root (sudo) to create system user and install into /usr/local/lib/opendash." >&2
  exit 1
fi

TARGET_DIR="/usr/local/lib/opendash/$VERSION"
if [[ -e "$TARGET_DIR" && -n "$(ls -A "$TARGET_DIR" 2>/dev/null || true)" ]]; then
  if [[ "$ALLOW_UPGRADE" != "true" ]]; then
    echo "Refusing to overwrite non-empty $TARGET_DIR without --upgrade." >&2
    exit 1
  fi
fi

if ! id -u opendash >/dev/null 2>&1; then
  useradd --system --home-dir /var/lib/opendash --shell /usr/sbin/nologin opendash
fi

install -d -m 0755 /usr/local/lib/opendash "$TARGET_DIR"
install -m 0755 "$BUNDLE_DIR/opendash" "$TARGET_DIR/opendash"
if [[ -f "$BUNDLE_DIR/release-manifest.json" ]]; then
  install -m 0644 "$BUNDLE_DIR/release-manifest.json" "$TARGET_DIR/release-manifest.json"
fi

install -d -m 0700 -o opendash -g opendash /var/lib/opendash /var/lib/opendash/config /var/lib/opendash/database
if [[ -n "$EXTERNAL_DB_DIR" ]]; then
  install -d -m 0700 -o opendash -g opendash "$EXTERNAL_DB_DIR"
fi

install -d -m 0750 -o root -g opendash /etc/opendash
if [[ ! -f /etc/opendash/opendash.env ]]; then
  cat > /etc/opendash/opendash.env <<'EOF'
OPENDASH_SITE_ROOT=/var/lib/opendash
OPENDASH_PORT=18766
OPENDASH_LOG_LEVEL=info
EOF
  chown root:opendash /etc/opendash/opendash.env
  chmod 0640 /etc/opendash/opendash.env
fi

# Validate config before switching current symlink
su -s /bin/sh opendash -c "OPENDASH_SITE_ROOT=/var/lib/opendash $TARGET_DIR/opendash --check-config"

ln -sfn "$TARGET_DIR" /usr/local/lib/opendash/current
echo "Installed OpenDash $VERSION -> /usr/local/lib/opendash/current"
