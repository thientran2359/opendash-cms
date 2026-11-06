# OpenDash CMS — Khu vực phát triển

Thư mục `cms/` tập trung mã nguồn, SDK, scripts và công cụ build của OpenDash. Tài liệu kỹ thuật đã được đặt riêng tại [`docs/`](../docs/README.md) ở gốc. [README cấp gốc](../README.md) giải thích cấu trúc website và cách chạy ứng dụng.

| Thành phần | Vị trí |
| --- | --- |
| Rust workspace, lockfile | [Cargo.toml](Cargo.toml), `Cargo.lock` |
| Domain models, validation | [crates/opendash-core](crates/opendash-core/README.md) |
| Axum HTTP + SQLite + MiniJinja | [crates/opendash-server](crates/opendash-server/src/main.rs) |
| React Admin / Builder | [admin](admin/package.json) |
| Plugin SDK | [sdk](sdk/README.md) |
| Theme và plugin examples | [examples](examples/plugins/sample-plugin/README.md) |
| SQLite migrations và fixture nguồn | [migrations](migrations/README.md), [fixtures](fixtures/README.md) |
| Build/health scripts | [scripts](scripts/README.md) |
| Tài liệu, kiến trúc, hợp đồng | [docs](../docs/README.md) |
| Thử nghiệm lịch sử | [spikes](spikes/README.md) |

Từ thư mục gốc website:

```powershell
powershell -ExecutionPolicy Bypass -File cms/scripts/build.ps1
cargo run --manifest-path cms/Cargo.toml --locked -p opendash-server
```

Hoặc chạy các lệnh Cargo từ bên trong `cms/`. Server tự nhận diện thư mục website ở cấp cha và lưu dữ liệu mặc định tại [`../database/`](../database/README.md).

M1/M2 lưu gói đã cài trong SQLite. Xem [lệnh chạy và giới hạn](../README.md).
