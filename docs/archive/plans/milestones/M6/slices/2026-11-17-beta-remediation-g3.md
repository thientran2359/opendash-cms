# M6 — Beta egress test reproducibility G3

## Owner directive — macOS Beta and Harbor, 2026-11-17

Chủ dự án chuyển ưu tiên Usable Beta sang **macOS**. Aster điều phối, chốt
kỹ thuật và tự review; không cần peer review. Aster chỉ tham gia thảo luận
liên-agent khi chủ dự án tạo cuộc thảo luận. Harbor là implementer của toàn bộ
chín task remediation F01–F09 và thực hiện technical retest macOS; một task
active trên `agent/harbor`, không hai writer trên cùng file. Aster chuẩn bị
candidate và phối hợp human dogfood; automation không thay người thử thật.

Chỉ đạo này thay ownership, nền tảng Beta và yêu cầu peer review cũ bên dưới.
Các PR #310–#313 đã merge. G3 vẫn cần immutable source/base, dependencies,
file ownership và Project Ready được xác minh trước pickup. Bằng chứng Windows
cũ giữ nguyên; không suy thành macOS PASS. Beta yêu cầu native macOS B01–B10
và negative regressions trên cùng candidate. Windows/Linux/native common RC và
V1 gates vẫn riêng; không đóng chúng từ Beta PASS. F08 giữ security hold đến
khi có giải thích provenance và kiểm chứng macOS đầy đủ; đối chiếu Windows
vẫn là follow-up bắt buộc trước common RC. F09 cần full release suite macOS
hai lượt và isolated run; receipt Windows thất bại vẫn giữ và phải được đối
chiếu trước common RC, không gọi là Windows defect resolved.

Thứ tự Harbor: M8-S10-T01 → M7-S15-T01 → M5-S11-T01 → M7-S15-T02 →
M7-S15-T03 → M7-S15-T04 → M7-S15-T05 → M6-S09-T01 → M8-S10-T02.
Mọi task sau giữ Backlog đến khi dependencies và slot đủ gate. Aster ghi
self-review/acceptance tại head SHA, checks thực chạy và giới hạn; không waive
security/data, CI, human test hoặc release authorization.


## Shared gate, provenance and verification

Coordinator: Aster; owner-directed #301 handoff, 17/11/2026. Source reviewed:
`84c13959ceeb0537d29451f979eb4fb06ba53c57`. Audit #300/#306 used product
`d1338d631e1421db075db6f024afabee988de99a`; do not relabel that Windows evidence
as tests of this newer tree. #308/#309 add test isolation/macOS evidence, not a
Windows Beta retest. Implementation NOT_STARTED; runtime verification NOT_RUN.

[Audit](../../../../../verification/2026-11-16-usable-beta-windows-audit.md) ·
[Planning](../../../PLANNING_WORKFLOW.md) ·
[Task #301](https://github.com/thientran2359/opendash-cms/issues/301).
G2/G3 metadata records the owner-authorized Aster decision; effectiveness requires
self-review and docs merge, with no mandatory peer review. No Issue publication,
Ready, implementation, release or dispatch from an unmerged specification.
Pin merged source SHA and current code base when publishing, check both open/closed
Issues for exact task IDs, verify Project Ready and agent slot before pickup.
One writer per file; proposed responsible agents are not active claims.

Use canonical Base APIs, ApiError and auth/CSRF/Origin guards. SQL stays in storage.
No dependency upgrades, applied-migration edits, security relaxation, fixture seeding
of the primary Beta journey, secrets or private artifacts in Git. Existing ABI,
publication semantics, rollback and provider/data preservation remain mandatory.
Stop-and-Propose before new API/schema/security policy beyond the explicit delta.
Any product fix requires focused negative regressions and affected B01–B10 rerun;
record actual commands/exits, source/tree/binary and fixture hashes, native OS,
roles and screenshot/log hashes. Build and CI are not runtime or human evidence.
Keep previous failed receipts. No retry-until-PASS, skipped assertions or timeout
inflation to hide defects. Source changes require candidate repin and impact matrix.
All tests use isolated site copies, loopback and disposable roots; retain originals
and matched snapshots/binaries. Git revert does not undo migrated data.

### M6-S09-T01 — Investigate redirect SSRF full-suite failure without weakening egress

- **Thuộc:** V1 internal Beta regression · M6 · M6-S09
- **Module:** PLG-02
- **Capability:** PLG-02.3
- **Primary Acceptance Slice của capability:** M6-S04; regression only, no new capability
- **Track:** Fast-Track
- **G2:** AGREED
- **Lý do Fast-Track:** Test/fixture reliability only; product security policy unchanged.
- **G3:** READY
- **Trạng thái:** PLANNED; owner-authorized self-review and merge, not Project Ready
- **Nguồn quyết định:** #301, audit #300/#306, shared rules and source evidence below
- **Người triển khai / review:** harbor / Aster; coordinator Aster

#### Tổng quan và giá trị

F09 full release suite exit101 vs isolated pass; test m6_s04_t01_egress_ssrf shares an Instant deadline across multiple requests. #308 changed different plugin tests.

#### Mục tiêu và hành vi quan sát được

Unfiltered suite and isolated test consistently exercise allowed request, denied redirect follow and oversized-body denial; no fabricated SSRF vulnerability or fix.

#### Phạm vi công việc

Investigate mock readiness/lifetime, environment cleanup and per-operation budget; constrain any repair to test/fixture orchestration.

#### Ngoài phạm vi

No unrelated product changes, live site mutation, release approval or native multi-OS closure. No contract changes except the explicitly reviewed design below.

#### Phụ thuộc và điều kiện chặn

Free Aster slot after M8-S10-T01 and M7-S15-T03; obtain authorized native macOS operator/Forge validation without claiming macOS as Windows. #214 source and Harbor ownership unchanged.

#### Kỹ thuật và file ownership

- `cms/crates/opendash-server/tests/m6_s04_t01_egress_ssrf.rs`

Only these files may change. Evidence is included in the PR receipt; source-derived test names must be checked at pickup. Shared files are reserved sequentially, not simultaneously.

#### Cách triển khai theo contract

Inspect the actual failure before patching. If a per-invocation deadline is required, use existing production invocation duration independently per modeled invocation; do not increase duration or disable security. Use deterministic server readiness and graceful cleanup; restore HTTP_PROXY environment safely. If product egress/SSRF defect is found, stop and request distinct contract-owned product fix; do not hide in this test task.

#### Nghiệm thu

- [ ] Record original full-suite failing receipt and one bounded diagnostic run; isolated pass alone never closes finding.
- [ ] Native macOS unfiltered release suite twice plus isolated target exercise all assertions with zero ignored/filtered tests for full runs; Windows regression remains separate before common RC.
- [ ] Redirect destination counter stays zero; private/link-local/deny policy and oversized response assertions remain; production egress limits untouched.
- [ ] If not reproducible after evidence review, no-product-change disposition needs actual repeated matrix and reviewer signoff; retain original failure.

#### Kiểm thử, rủi ro và rollback

cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --release --no-fail-fast -- --test-threads=4 (twice, record both); isolated target --test m6_s04_t01_egress_ssrf -- --test-threads=1; native macOS required, Windows separate before common RC. If macOS executor unavailable keep BLOCKED; no retry loop.
Use the shared provenance/rollback rules. Failure leaves the task open; never remove a guard to get a green run.

#### Bàn giao và review

Submit Task ID, exact head/base, bounded diff, commands and exit codes, negative-case matrix, artifact hashes and limits. Reviewer checks current head and required CI. Merge is not G5; update Issue/Project and index only with actual verified evidence. No closing keywords before acceptance.
