# Linux Production Deployment (`cms/deploy/`)

## Overview

`cms/deploy/` provides the operator installation script, systemd unit, and Nginx TLS reverse-proxy template packaged into Linux release bundles by [`build-release-linux.sh`](../scripts/build-release-linux.sh). Together they run a single statically linked `x86_64-unknown-linux-musl` `opendash` executable on loopback behind Nginx, aligned with [`M8_OPERATIONS_RELEASE.md`](../../docs/contracts/M8_OPERATIONS_RELEASE.md).

## Interface

### Artifacts

- [`install-linux.sh`](install-linux.sh) — Operator installer/upgrader:
  ```bash
  sudo ./install-linux.sh --bundle-dir <extracted_bundle> --version <semver_or_sha> [--upgrade] [--external-db-dir <dir>]
  ```
- [`opendash.service`](opendash.service) — systemd unit running `/usr/local/lib/opendash/current/opendash` as `opendash:opendash` (`WorkingDirectory=/var/lib/opendash`, `EnvironmentFile=/etc/opendash/opendash.env`, `KillSignal=SIGTERM`, `TimeoutStopSec=35s`, `Restart=on-failure`, `RestartSec=5s`).
- [`nginx.conf.example`](nginx.conf.example) — Nginx TLS reverse proxy (`:80` → `:443` redirect, `proxy_pass http://127.0.0.1:18766`, `client_max_body_size 4100m`, `proxy_request_buffering off`, `proxy_set_header Host $host`, 300s read/send timeouts).

### Runtime Configuration & CLI Hooks

Handled in [`main.rs:main`](../crates/opendash-server/src/main.rs), [`config.rs:RuntimeConfigV1::load`](../crates/opendash-server/src/config.rs), and [`paths.rs:SitePaths::from_env`](../crates/opendash-server/src/paths.rs):

- `--check-config` — Validates `SitePaths` and `RuntimeConfigV1` (including `config/site.json` and env overrides) and prints redacted JSON summary before exiting (`ExecStartPre` in `opendash.service` and final gate in `install-linux.sh`).
- Environment variables (seeded in `/etc/opendash/opendash.env` by `install-linux.sh`): `OPENDASH_SITE_ROOT=/var/lib/opendash`, `OPENDASH_PORT=18766`, `OPENDASH_LOG_LEVEL=info` (also supports `OPENDASH_PUBLIC_ORIGIN` and `OPENDASH_DB_PATH`).

## Invariants

- **Loopback-only listener:** [`backup.rs:PortableRuntimeConfigV1::validate`](../crates/opendash-core/src/backup.rs) restricts `listen_ip` to `127.0.0.1` or `::1`; public TLS traffic must terminate at Nginx.
- **No forwarded-header trust:** OpenDash does not trust `Forwarded` or `X-Forwarded-*` headers for auth or origin checks; set `trusted_public_origin` in `/var/lib/opendash/config/site.json` (or `OPENDASH_PUBLIC_ORIGIN`), validated by [`backup.rs:canonicalize_trusted_origin`](../crates/opendash-core/src/backup.rs) as `http`/`https` without credentials, path, query, or fragment.
- **Strict config precedence:** [`config.rs:RuntimeConfigV1::load`](../crates/opendash-server/src/config.rs) applies `defaults < config/site.json < explicit non-empty env overrides` (`OPENDASH_PORT`, `OPENDASH_PUBLIC_ORIGIN`, `OPENDASH_LOG_LEVEL`).
- **Validated atomic switch & upgrade guard:** [`install-linux.sh`](install-linux.sh) requires root (`$EUID -eq 0`), refuses to overwrite a non-empty `/usr/local/lib/opendash/$VERSION` unless `--upgrade` is passed, and runs `--check-config` as user `opendash` before updating `/usr/local/lib/opendash/current`.
- **Systemd filesystem & capability sandboxing:** [`opendash.service`](opendash.service) enforces `NoNewPrivileges=yes`, `PrivateTmp=yes`, `ProtectSystem=strict`, `ProtectHome=yes`, `ReadWritePaths=/var/lib/opendash`, empty `CapabilityBoundingSet`/`AmbientCapabilities`, `LockPersonality=yes`, `RestrictRealtime=yes`, and `RestrictSUIDSGID=yes`. (If `--external-db-dir` is outside `/var/lib/opendash`, add it to `ReadWritePaths` in a systemd drop-in.)

## Side Effects

- Running [`install-linux.sh`](install-linux.sh) creates the `opendash` system user/group (`/usr/sbin/nologin`, home `/var/lib/opendash`), installs `opendash` (`0755`) and optional `release-manifest.json` (`0644`) under `/usr/local/lib/opendash/<version>/`, updates `/usr/local/lib/opendash/current`, creates `0700` state directories `/var/lib/opendash/{config,database}` (and optional `--external-db-dir`), and writes `/etc/opendash/opendash.env` (`0640`, `root:opendash`) if not already present.

## Verification

Verify the release bundle before installation, then verify configuration and health on the target host:

```bash
python3 cms/scripts/verify-release-artifact.py <bundle_dir>
sudo -u opendash OPENDASH_SITE_ROOT=/var/lib/opendash /usr/local/lib/opendash/current/opendash --check-config
sudo nginx -t
sudo systemctl status opendash --no-pager
curl -sf http://127.0.0.1:18766/api/health
```
