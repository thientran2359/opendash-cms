# OpenDash Core

Rust domain crate, không phụ thuộc HTTP, SQLite hay Wasmi. Module theme kiểm tra descriptor và dữ liệu block. Module plugin kiểm tra manifest và quyền ABI v1. Module content giữ validator khóa CPT của scaffold.

Cây Builder `Section → Row → Column → Element` và field binding sẽ đặt tại `src/builder.rs` khi thiết kế AST được triển khai và kiểm chứng theo [kiến trúc](../../../docs/architecture/CMS_ARCHITECTURE.md).
