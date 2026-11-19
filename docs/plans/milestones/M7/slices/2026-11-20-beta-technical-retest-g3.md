# M7-S14 — technical retest macOS sau Content Foundation

**Final Decision:** Aster, owner authorization 20/11/2026. Tách technical retest khỏi
human cold walkthrough để Harbor làm trước, giữ nguyên human/Beta/release gates.
Đây là extension T05 của slice M7-S14, không là milestone hoặc primary acceptance mới.
[PRD](../../../../product/PRODUCT_REQUIREMENTS.md),
[ADR-0017](../../../../decisions/ADR-0017-usable-beta-staging.md),
[ADR-0019](../../../../decisions/ADR-0019-content-foundation.md),
[shared B01–B10](2026-11-16-usable-beta-g3.md#mandatory-case-matrix) và
[Content Foundation C1–C4](../../../../contracts/CONTENT_FOUNDATION_2026_11.md)
là authority. Chỉ đạo macOS/Aster/Harbor mới thay phần Windows/Forge cũ.

## Candidate cố định và impact disposition

- Candidate source/base: `5ad37389404728bd1344fb56b8c8d4817eda7f60`.
- Product subtree `cms/`: `baabe37cee4c250b3f241232ee91493281e72638`.
- Latest product merge C4: `65e27e027885020f60230f33f7c24dbcf24c9d91`,
  G5 evidence PR #408 merge nằm trong candidate. Candidate CI cần xác minh live.
- #318–#326 macOS remediation và #373–#378 Content Foundation đã Closed;
  coordinator đã đối chiếu G5 reports, đặc biệt #377/#378 Done. Closure không thay
  fresh candidate integrated evidence. Windows F08/F09 receipts giữ nguyên;
  common RC, native Windows/Linux và human dogfood không được suy PASS.
- Impact: C1 publication/settings ảnh hưởng B02/B03/B07; C2 taxonomy/Builder B02/B05;
  C3 canonical routes/redirect/claims B03/B05/B10; C4 lifecycle/Wasmi/default overrides
  B04/B06/B07; migration0041 và inherited/legacy/site custom cần B08/B09 revalidation.
  B01 và toàn bộ unseeded journey chạy lại, không ghép receipt cũ thành cùng candidate.
- Agent branch `agent/harbor`, không tạo task branch hoặc linked worktree mặc định.
  Sync main an toàn trước pickup; docs-only drift được phép khi `cms/` tree bằng
  hash trên, ghi actual checkout SHA riêng với candidate và binary. Nếu bất kỳ
  product delta nào xuất hiện, STOP để Aster repin/impact-review trước runtime.
  Không build stale ignored Admin dist: build Admin từ đúng tree trước binary.

## Quan hệ với #302/#303/#304

T05 tạo technical report trước và không cần human tester. #302 giữ human cold
walkthrough, kết hợp technical report cùng candidate; vẫn Backlog khi chưa có tester
và các gate còn thiếu. #303 readiness chỉ sau report/human/fix gates; #304 không Done
vì T05 merge. Technical result FAIL/NOT_RUN dẫn BETA_BLOCKED, không hạ chuẩn.
Findings mới phải được Aster triage/G3/ownership riêng trước sửa; không vừa sửa vừa
đo rồi gọi là cùng candidate. Báo cáo T05 có thể hoàn tất với verdict BLOCKED khi
đã attempt mọi case reachable và ghi owner/resolution cho các khoảng trống.

### M7-S14-T05 — Technical retest Beta macOS trên candidate Content Foundation

- **Thuộc:** V1 internal Beta integration, M7-S14
- **Module:** FND-05
- **Capability:** FND-05.5
- **Primary Acceptance Slice của capability:** M7-S13; chỉ regression integration
- **Track:** Fast-Track
- **Lý do Fast-Track:** Verification/report only theo contract hiện hữu, không đổi product/schema/API
- **G2:** AGREED
- **G3:** READY
- **Trạng thái:** PLANNED; hiệu lực sau merge spec, khác Project Ready và runtime PASS
- **Người triển khai / review:** Harbor / self-review; Aster coordinator
- **Nguồn quyết định:** Owner request 20/11/2026, candidate/impact và gates trong toàn bộ tài liệu này

#### Tổng quan và giá trị

Xác minh các capability đã G5 riêng hoạt động cùng nhau trên một candidate macOS
hiện hành, trước khi người thật dùng thử. Phân biệt agent UI walkthrough với human.

#### Mục tiêu và hành vi quan sát được

Có ma trận technical B01–B10, Content Foundation impact, negative/security/data
regressions và manifest source/binary/Admin/ZIP/receipt tái lập được. Không case nào
được suy từ compilation, CI hoặc kết quả của candidate khác.

#### Phạm vi công việc

Rebuild native macOS Intel release với embedded Admin từ candidate. Primary path
site trống, chỉ Admin UI tạo nội dung; separate seeded/harness fixture paths được
label rõ. Dùng Vietnamese studio brief: homepage/about, hai Post gồm một draft,
Page, site-owned CPT draft_publish và custom taxonomy/terms, image, menu binding,
Builder Page/homepage/private Section, theme/plugin ZIP, permalink override.
Hai browser sessions Admin/Editor, desktop1440x900, public768/390; responsive viewport
không là chứng nhận physical mobile authoring. Chạy matrix sau trên binary đã pin.

| Case | Required fresh technical evidence |
| --- | --- |
| B01 | Bootstrap/login/logout/deep link/back/navigation, role menus và empty/error states |
| B02 | CPT/fields/panels, custom taxonomy/terms/associations, Editor entries/image, menu binding; restart giữ IDs/values |
| B03 | Draft/private/Section không public/archive/sitemap; authenticated preview, Save/Publish boundary, unsaved public unchanged, Builder Save public update |
| B04 | Theme ZIP activation; sample import riêng trên nonempty site giữ content/menu; cancel/duplicate không overwrite |
| B05 | Homepage/header/footer/single/archive và exact Page Builder; private Section/template copy, saved reopen/render, missing-block safe fallback |
| B06 | Theme switch/update, plugin enable/disable/re-enable/restart giữ layout/data; real Wasmi hooks, trap/timeout và Admin recovery |
| B07 | Dirty navigation,401 same-account re-auth/cross-account isolation,412 stale save/role downgrade, Editor Admin-only denial, CSRF/Origin |
| B08 | Encrypted backup/download/restore root mới, safe-before-activation/restart, sessions cleared/SMTP off, explicit activation/re-edit/public; source unchanged |
| B09 | Wrong passphrase/tamper/missing asset/secret/newer schema/disk-full và denied ZIP/permissions fail safely; source/root publication unchanged |
| B10 | SEO/contact/video/public ACL regressions, canonical claims/CAS/301 and inherited/legacy/custom UI; current full Rust release/security tests |

#### Ngoài phạm vi

Không product/harness/SDK/fixture source edits, schema migration mới, dependency upgrade,
owner site hoặc port18964, human certification, Beta PASS, common RC/V1, deployment.
Không gọi agent walkthrough là human, không waive Windows security/data follow-ups.

#### Phụ thuộc và điều kiện chặn

Spec này merged tại immutable source SHA; #378 G4 #406 và native G5 #408 merged,
Project Done; predecessor #377 và required remediation G5 có evidence. Coordinator
pin candidate/impact ở đầu. Issue có Harbor owner, live Project Ready, clean/free
agent/harbor và không conflicting active CoS turn. Không đòi human availability cho
T05. Nếu có S0/S1 hiện hành, báo BLOCKED và attempt safe reachable checks, không cấp
product PASS hoặc thử hành vi gây hại. Candidate drift cần Aster quyết định lại.

#### Kỹ thuật và file ownership

- `docs/verification/2026-11-20-beta-technical-retest-macos.md`

Harbor là writer duy nhất report này; Aster giữ plan/contract/index. Manifest,
screenshots, logs, DB copies, ZIPs, backups và receipts ở isolated private artifacts
ngoài Git. Không được sửa harness để làm PASS. Nếu harness thiếu case/không chạy,
ghi NOT_RUN/FAIL với command/diagnostic, propose bounded follow-up riêng trên Issue.
Dùng existing scripts sau khi đọc --help/source, không mặc định mọi runner cùng flags:
admin_nav_single_edit_e2e.py, m7_browser_e2e.py, m8_browser_e2e.py,
content_foundation_m7_s17_t02_e2e.py, content_foundation_m6_s11_t01_e2e.py và
content_foundation_m6_s11_t01_backup_e2e.py trong cms/scripts. Chỉ dùng synthetic roots
và bounded timeout, giữ failed receipts; cleanup đúng process do mình tạo.

#### Cách triển khai theo contract

Fetch/sync bằng fast-forward, verify candidate cms tree trước build. Chạy
`npm run typecheck --prefix cms/admin`, `npm run build --prefix cms/admin`,
`cargo build --manifest-path cms/Cargo.toml --locked --release`.
Ghi actual commands/exits, rust/node/browser/OS/arch, source và artifact hashes;
kiểm byte-equal embedded Admin assets. Chạy full
`cargo test --manifest-path cms/Cargo.toml --locked --release --workspace -- --test-threads=1`
hai lượt trên cùng source và isolated canonical 0700 TMPDIR; thêm isolated targeted
SSRF/restore/ACL negatives theo source thực và giữ original FAIL nếu có.
Full suite không thay B01–B10 Chrome journey; API/SQL assertions hỗ trợ negative,
không seed primary UI path. Backup/restore re-edit cả CPT/custom taxonomy/permalink
provider default + site override, không mất draft/private/layout. Thực hiện read-only
provenance đối chiếu F08/F09 macOS/Windows receipts, không xóa historical hold.

#### Nghiệm thu

- [ ] Report đủ B01–B10 và impact C1–C4 trên candidate/source/binary pin, actual observed PASS/FAIL/NOT_RUN/BLOCKED, artifacts/hash/steps; primary unseeded UI tách harness evidence.
- [ ] Required auth/401/409/412/private/provider/security/restore negatives có fresh applicable evidence hoặc explicit gap + resolution owner; không lọc test để đổi verdict.
- [ ] Hai full release runs và isolated negatives ghi lệnh/exits, failed receipts giữ nguyên; restart/backup→restore→re-edit bảo toàn canonical content/layout và source data.
- [ ] Verdict TECHNICAL_PASS hoặc TECHNICAL_BLOCKED có severity S0–S3, reproduction/owner và missing cases; Human NOT_RUN, Beta NOT_GRANTED, #302/#303/#304 giữ gates riêng.
- [ ] Report self-review exact head và required CI PASS, merged evidence-only PR; Issue Done chỉ nói technical audit report hoàn tất, không là release authorization.

#### Kiểm thử, rủi ro và rollback

Kiểm pin trước/sau từng run, không đụng live data. No SQL/API/file workaround trong
primary path; nếu blocked ghi lỗi thật và chuyển separate fixture path đã label.
Không down-migrate: snapshot restore bằng matching binary và retained copies.
Rollback task là dừng owned process, giữ artifacts/root gốc. Report không chứa
credentials, PII, tokens, backup bytes hoặc private host IDs. Docs validation:
`python3 cms/scripts/check-plans.py`, self-test, agent artifact checker, diff check.

#### Bàn giao và review

PR Agent Harbor, Task M7-S14-T05, candidate khác source docs SHA nếu docs drift,
full matrix, source/tree/binary/Admin/fixture/ZIP hashes, commands/exits và limits.
Không closing keyword #302/#303/#304. Harbor tự review/merge evidence khi đủ CI/gates;
Aster quyết định findings/repin và human readiness. Không bắt buộc peer review.
