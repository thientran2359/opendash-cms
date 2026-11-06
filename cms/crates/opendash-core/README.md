# OpenDash Core

Rust domain crate, không phụ thuộc HTTP, SQLite hay Wasmi. Các file `content.rs`, `theme.rs` và `plugin.rs` hiện chỉ có validation mẫu, không phải toàn bộ schema CMS V1.

Cây Builder `Section → Row → Column → Element` và field binding sẽ đặt tại `src/builder.rs` khi thiết kế AST được triển khai và kiểm chứng theo [kiến trúc](../../../docs/architecture/CMS_ARCHITECTURE.md).
