# M7 Deterministic Test Fixtures

All files in this directory are deterministic, synthetic, test-only fixtures for M7 E2E and adversarial verification (`M7-S08-T01`). None of them contain real credentials, personal data, or third-party proprietary media.

## Manifest & Checksums

| File | Size (bytes) | SHA256 | Provenance & Purpose | License / Scope |
| --- | ---: | --- | --- | --- |
| `video-avc-aac.mp4` | 1027 | `82130ff87ab339520f0fcf2ad42183fbcd68d2419f882e134ff036e44de8c534` | Synthesized ISO BMFF MP4 (`isom`, `avc1`/`avcC` 320x180 1.5s + `mp4a`/`esds` 44.1kHz + `mdat`) | Project-owned synthetic test fixture |
| `smtp-test-cert.pem` | — | `4e7dc48ff6a6e5cf7d15b3d0ef0eca692718238b86d013e2edefcef5f730b0e9` | Self-signed X.509 RSA-2048 localhost certificate (`CN=localhost`, SAN `DNS:localhost,IP:127.0.0.1`, `basicConstraints=CA:TRUE`) generated via OpenSSL | Test-only localhost TLS fixture (`#[cfg(test)]` trust injection only; never production trust) |
| `smtp-test-key.pem` | — | `5235cbe73e4acf61ab9f59a05c6cc6f05ff204f0d25575b0cd9364a9c8b732e6` | Unencrypted RSA-2048 private key paired with `smtp-test-cert.pem` | Test-only ephemeral localhost key; never use outside local loopback tests |
