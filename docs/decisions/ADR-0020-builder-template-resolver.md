# ADR-0020 — Builder template hierarchy và nguồn template

**Owner:** Atlas · **Date:** 2026-11-19 · **Status:** Final Decision G2,
ACCEPTED khi PR merge; local decision record, chưa G3/IMPLEMENTED/VERIFIED.
Chủ dự án yêu cầu chốt bảng resolver sau khi xác nhận hai phạm vi Builder và
bỏ chuyển homepage legacy thành Page. Áp dụng ADR-0010.

## Quyết định

Chọn [contract resolver](../contracts/BUILDER_TEMPLATE_RESOLVER.md): xét từng
candidate từ cụ thể đến tổng quát; tại cùng candidate, DB override trước
active theme revision. Custom assignment và selector stable ID có thứ tự rõ;
template parts giải quyết theo key, content thuộc bản ghi độc lập. Khác biệt
với thứ tự cũ “mọi DB override trước theme” là chủ đích: theme có template
riêng cho CPT phải thắng DB single chung.

Site-owned overrides tiếp tục hiệu lực theo target xuyên theme, base revision
là provenance. Không tự ghi đè layout khi cập nhật theme. Reset chỉ bỏ đúng
override, xác nhận nguồn thay thế thực tế và kiểm cả active selection/CAS.
Thiếu block giữ AST theo chính sách hiện hành; template hỏng dùng safe fallback.

Chủ dự án chốt `home` cho trang chủ; trang Blog dùng `archive-post` nếu có,
nếu không dùng `archive` mặc định. Không có role `blog` hoặc `front-page`. Post/CPT dùng single/archive theo type; bổ sung 20/11 chốt Page mặc định
dùng page và cho chọn Page layout, thay quyết định Page chỉ dùng single-page. Có template không tự bật route public.
Admin chọn hai Page khác nhau trong Settings:
Trang chủ dùng Page content; Trang bài viết đại diện archive post. Không convert
homepage legacy thành Page content, không copy/dual-write. Bỏ quyết định adapter
tự đưa homepage legacy vào resolver đích; bảo toàn dữ liệu cũ không tạo chế độ
legacy trong Settings. Theo bổ sung 20/11 bên dưới, không cần compatibility
upgrade/package cho các bản thử nghiệm.
Section tiếp tục copy, không synced propagation.
Giữ Rust/SQLite, immutable revisions, C-SEC, publication, media ACL và route
authority hiện hành. Không bổ sung PHP, child themes hoặc route chưa được duyệt.

## Phạm vi thay thế và phần tiếp theo

Sau merge, thay thứ tự resolver tại M5_THEME_TEMPLATES mục 3 trong phạm vi đã
định nghĩa; giữ bản cũ làm lịch sử/source evidence. Không sửa hồi tố nghiệm
thu M4/M5 hoặc chuyển trạng thái Issue. API/schema cho capability Builder,
content mode, manifest/part reference và Page-at-root claims vẫn cần delta
G2/G3 hoàn chỉnh trước triển khai. Contract resolver không tự cấp Ready.

Docs validation ghi tại PR/head; code/runtime NOT_RUN. Không thay site demo
hoặc dữ liệu website để giả lập quyết định đã implemented.

## Bổ sung chủ dự án xác nhận — 20/11/2026

Thông qua cấu trúc gói theme tại [đặc tả sản phẩm](../product/BUILDER_TEMPLATE_AND_CONTENT.md):
template chính/header/footer ở root; template-part/, page-layouts/, sections/,
assets/. Section theme liên kết thư viện Admin bằng bản gốc + tùy chỉnh DB,
giữ chỉnh sửa khi cập nhật, không ghi ngược revision files hoặc live propagation.
Page layout gán rõ → page → index; content Page độc lập layout.
Custom block authoring cho Builder được để lại thảo luận sau, không chốt
schema/SDK hoặc yêu cầu thực thi ví dụ custom-title. Package/API G2 và G3
còn phải hoàn thiện; không triển khai hay thay dữ liệu site trong lượt docs này.

## Chỉ đạo chủ dự án — không tương thích ngược, 20/11/2026

OpenDash đang phát triển, chưa có bản sản phẩm cũ cần hỗ trợ. Thiết kế mới
không phải giữ format theme/API/schema cũ, adapter legacy, dual-read/dual-write,
converter hay lộ trình upgrade từ các bản thử nghiệm. Chốt một format/API/schema
đích thống nhất; cập nhật source, fixtures và kiểm chứng theo cùng baseline mới.
Những điều khoản tương thích ngược trong tài liệu lịch sử không ràng buộc
phạm vi thiết kế mới này. Không phát sinh task compatibility riêng.

Điều này không bỏ yêu cầu sản phẩm bảo toàn content/layout khi người dùng
đổi/cập nhật theme, hay quyền, publication, backup/restore trong mô hình mới.
Không tự xóa dữ liệu hoặc thư mục hiện có để thực hiện quyết định thiết kế.
