# ADR-0017 — Usable Beta trước common RC và V1 public

**Decision owner:** Atlas · **Date:** 2026-11-16.
**Status:** ACCEPTED / G2 AGREED effective on merge. Product verification NOT_RUN.
**Decision:** [Issue #292 — Atlas Turn 2](https://github.com/thientran2359/opendash-cms/issues/292#issuecomment-6917897791).
**Baseline inspected:** main `d1338d631e1421db075db6f024afabee988de99a`.

## Context and evidence

[PRD](../product/PRODUCT_REQUIREMENTS.md) yêu cầu CMS dễ dùng từ browser.
[Windows Admin evidence](../verification/2026-11-16-m7-s13-t01-admin-nav-single-edit-windows.md)
ghi capability tests trên binary riêng; dev.py Mộc Studio là demo được seed.
Cả hai không chứng minh người dùng dựng site từ đầu không cần trợ giúp kỹ thuật.
Project snapshot 16/11: #157/#188 In progress; #214/#215/#242/#245/#246/#247/#248
Backlog; #284 Done. Snapshot không thay Project live hoặc thu hồi evidence.
Harbor đã báo native Mac exact-SHA tests ở #188/#246; toàn bộ các task đó vẫn mở.

## Final Decision

Chọn A: checkpoint **Usable Beta nội bộ Windows** → common RC → V1 public.
Giữ Rust/SQLite/single executable, Linux VPS mục tiêu, Builder và plugin server logic,
mọi capability V1, native Windows/Linux/macOS và security/data gates.
Không chọn B (V1 Windows-only): cần quyết định G0 mới của owner.
Không chọn C (đóng release rồi mới UX): không xử lý ưu tiên dễ dùng hiện tại.
Trade-off: thêm lượt tích hợp/retest và điều chỉnh thứ tự, không giảm chuẩn phát hành.

Beta dùng site cô lập/dữ liệu thử, public routes localhost không là deploy Internet.
Biên tập thường ngày chỉ qua Admin; operator được cấu hình/chạy binary và kích hoạt
restored root theo [ADR-0015](ADR-0015-m8-operations-release.md). Không hot-swap.
Post/page draft và published, Builder Save, CPT khác giữ đúng semantics PRD.
Preview chưa lưu không đổi public. Đổi theme/tắt plugin bảo toàn dữ liệu/layout.

## Acceptance and work ownership

[G3 checkpoint](../plans/milestones/M7/slices/2026-11-16-usable-beta-g3.md)
quy định đủ bốn task: Forge audit Windows; Atlas triage/G3 fixes; Forge QA + human
dogfood; Atlas Beta readiness. Nova là responsible mặc định cho UX fixes được giao
G3 riêng. Harbor giữ #188 và phần macOS; Forge giữ #242/#245. Một task active mỗi
agent branch; không hai writer trên shared file. Forge review docs PR trước merge.

Audit có thể hoàn thành bằng báo cáo lỗi đầy đủ; không yêu cầu product PASS để ghi
nhận audit hoàn thành. Beta readiness chỉ PASS khi mọi hành trình bắt buộc đạt trên
candidate cố định, không còn blocker bảo mật/dữ liệu/phục hồi/hành trình.
Phải có ít nhất một người thử không viết code; agent/browser automation không thay
human usability. Atlas điều phối người thử, không mặc định owner là tester.
Thiếu người thử chặn dogfood/readiness, không chặn audit hoặc sửa lỗi.

## Contract impact and release gates

Đây là staging/integration overlay, không thêm module hay nghiệm thu chính trùng lặp.
Không đổi API/ABI/migration, ADR-0015, 18 task M8 hoặc historical G5.
M8 contract/G3 chỉ thêm cross-reference; final common RC, native, notices, license
và owner release authorization giữ nguyên. Beta PASS không đóng #248, #215 hoặc
native tasks. Source đổi phải đánh giá impact, repin và rerun affected platforms.
Docs merged + Issue/G3/ownership/dependencies + Project Ready mới mở pickup.
Không tự dispatch từ tài liệu. Owner chỉ cần quyết định khi đổi scope G0, license
hoặc phát hành công khai.

