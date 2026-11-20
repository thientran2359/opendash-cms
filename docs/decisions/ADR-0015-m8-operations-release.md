# ADR-0015 — Consistent recovery and verifiable V1 release

**Decision owner:** Atlas · **Date:** 2026-11-14 · **Status:** ACCEPTED / G2 AGREED effective on merge with contract/G3 under ADR-0010. Implementation NOT_STARTED, verification NOT_RUN.

## Context

Owner requested full M8 G1→G2→G3 this chat while Nova owns M7. [Source inventory](../../.agents/research/2026-11-14-m8-operations-release-g1.md) uses main `8c5b90072c231c1f0813eb593d7be4447fb0a11d`, migration0031. [PRD](../product/PRODUCT_REQUIREMENTS.md) and [M8 candidates](../archive/plans/milestones/M8/TASK_PLAN.md) require media lifecycle, manual backup/restore, Linux and V1 acceptance. M7 Windows acceptance exists; macOS/full closure remains deferred by ADR-0014.

## Final Decision

| ID | Selected technical decision | Consequence |
| --- | --- | --- |
| D01 | Lifetime site-root+DB process leases and one cooperative mutation admission barrier, including recovery/workers/SMTP | Consistency spans SQLite, packages, media and secrets; public reads stay available while writes pause. No claim about external sqlite tools bypassing leases. |
| D02 | All-saved-data reference inventory, tombstone→quarantine→finalize delete journal, manual reconcile/purge | Draft/inactive/hidden data pins assets; public ACL alone cannot authorize deletion. Crash recovery idempotent, no scheduled GC. |
| D03 | VACUUM INTO snapshot under drained writer barrier plus canonical file manifest/hashes | External OPENDASH_DB_PATH is copied as logical db/main.sqlite; never raw WAL/main-file copy. |
| D04 | Flat-entry ZIP9.3.0 profile plus passphrase age0.11.2 streaming encryption, private job staging | SMTP secrets included safely; no plaintext backup download or custom cryptography. |
| D05 | Restore only to a generated separate site root, immutable receipt and explicit operator activation | No overwrite of running DB/files, no cross-volume fake atomicity. Admin creates restored copy; operator starts/switches it through documented deployment steps. |
| D06 | Restored site rebases DB/config, clears sessions, disables outgoing SMTP and pending actions until explicit re-enable | Prevent stale sessions, duplicate mail and unintended egress. Content/theme/plugin declarations retained; no plugin execution during validation. |
| D07 | Linux x86_64-musl executable with embedded Admin, configured loopback/proxy origins and graceful shutdown | No Node/SQLite/OpenSSL/age executable on server; operator-managed TLS/proxy. No silent forwarded-header trust. |
| D08 | Exact toolchain/dependency inputs, checksum/build manifest and finite Linux security/performance corpus | Packaging and benchmarks need real binaries/Linux, not governance CI success alone. |
| D09 | All8 candidates mapped to18 full G3 tasks with explicit seam handoff | Windows development can follow M7 Windows acceptance; final multi-OS/V1 still waits all deferred macOS evidence and Linux. |
| D10 | Public release is a separate gated owner action, license terms owner-controlled | Technical preparation may finish while license decision/environment provision remains external; no inferred publication/deployment authority. |

## Alternatives and consequences

Live file copies or DB-only backup can lose references. Hot replacement across active pools/external DB/files cannot provide atomic V1 restore with one rename. A new-root verified copy provides a safe rollback target and matches the candidate acceptance; activating it is an operator action documented and rehearsed before release. Backup encryption adds passphrase responsibility and a pinned new dependency; lost passphrase has no recovery backdoor. No claim that a matching checksum proves artifact authorship, or that advisory locks stop hostile local root access.

## Contract and authority

Normative [M8 contract](../contracts/M8_OPERATIONS_RELEASE.md) and [full G3](../archive/plans/milestones/M8/slices/2026-11-14-m8-operations-release-g3.md). Atlas records this Final Decision under [ADR-0010](ADR-0010-agent-project-and-merge-authority.md); source/standards evidence and exact reviewed head/checks belong the PR. No Nova branch/file reassignment, active Issue pickup, scheduler, runtime changes or product release in this planning delivery.

M8 implementation pickup waits merged immutable G3, actual M7 Windows acceptance (#213), ownership and Project Ready, not a fabricated full M7 closure. Final M8/V1 acceptance waits M5#157/M6#188/M7#214/#215 full required native evidence. Linux absence/performance failure/secret recovery/security findings block the corresponding acceptance. Owner chooses project license before public release; agents do not fill that product decision with a guess.
