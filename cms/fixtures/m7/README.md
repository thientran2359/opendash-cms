# M7 Deterministic Test Fixtures (`m7`)

## Overview

Deterministic, synthetic, test-only fixtures used by the M7 E2E and media/SMTP integration suites ([m7_s08_t01_e2e.rs](../../crates/opendash-server/tests/site_media_seo/site_features_e2e.rs)) and [demo-website-runner.py](../../scripts/demo-website-runner.py). None of these files contain real credentials, personal data, or third-party proprietary media.

## Interface

| File | Size (bytes) | SHA256 | Specification & Consumers |
| --- | ---: | --- | --- |
| `video-avc-aac.mp4` | 1027 | `82130ff87ab339520f0fcf2ad42183fbcd68d2419f882e134ff036e44de8c534` | Synthesized ISO BMFF MP4 (`isom`, `avc1`/`avcC` 320x180 1.5s + `mp4a`/`esds` 44.1kHz + `mdat`); consumed by `[cms/crates/opendash-server/tests/site_media_seo/site_features_e2e.rs:m7_s08_t01_deterministic_e2e_and_adversarial_corpus]` and `[cms/scripts/demo-website-runner.py:VIDEO_FIXTURE]` |
| `smtp-test-cert.pem` | 1147 | `4e7dc48ff6a6e5cf7d15b3d0ef0eca692718238b86d013e2edefcef5f730b0e9` | Self-signed X.509 RSA-2048 localhost certificate (`CN=localhost`, SAN `DNS:localhost,IP:127.0.0.1`, `basicConstraints=CA:TRUE`); canonical Git LF bytes (`1166` bytes on CRLF Windows checkouts) |
| `smtp-test-key.pem` | 1704 | `5235cbe73e4acf61ab9f59a05c6cc6f05ff204f0d25575b0cd9364a9c8b732e6` | Unencrypted RSA-2048 private key paired with `smtp-test-cert.pem`; canonical Git LF bytes (`1732` bytes on CRLF Windows checkouts) |

## Invariants

- **Deterministic Checksums:** Byte contents and SHA256 digests are frozen so media upload, `Range 206` streaming, and loopback TLS tests remain reproducible.
- **Test-Only Trust Boundary:** `smtp-test-cert.pem` and `smtp-test-key.pem` are restricted to `#[cfg(test)]` loopback trust injection and local tests—never loaded into production trust stores or non-loopback endpoints.

## Side Effects

- None; static read-only fixture files copied or read into temporary test `site_root` directories at test runtime.

## Verification

Run the M7 E2E and media/SMTP integration tests:

```powershell
cargo test --manifest-path cms/Cargo.toml --locked --test m7_s08_t01_e2e
cargo test --manifest-path cms/Cargo.toml --locked --test m7_s07_t01_video_upload --test m7_s07_t02_video_delivery --test m7_s06_t01_smtp
```
