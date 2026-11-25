# Sample Theme v3 (`sample-theme-v3`)

Theme mẫu v3 tối giản minh họa [Đặc tả Theme Format v3](../../../../docs/designs/theme-format-and-template-resolver.md), các C-SEC helpers trong [Theme Ergonomics](../../../../docs/designs/theme-ergonomics-and-csec-helpers.md), và tích hợp write-time projection `entry.ext["rust-seo-plugin"]` từ [rust-seo-plugin](../../plugins/rust-seo-plugin/README.md).

## Cấu trúc gói Theme v3

- [`theme.json`](theme.json): Khai báo `manifest_version: 3`, template gốc `{ "kind": "single", "file": "single.html" }` và danh sách tài nguyên tường minh `assets: ["assets/app.css"]` (quy tắc D01).
- [`single.html`](single.html): Template chi tiết đặt tại root gói, tuân thủ `UndefinedBehavior::Strict` và C-SEC:
  - Chèn CSS an toàn qua `{{ theme_assets['assets/app.css'] }}`.
  - Hiển thị `breadcrumbs`, `entry.published_at | date_format("%d/%m/%Y")`, `entry.reading_time_minutes | format_number`, `related_entries`, và đúng một `{{ od_content() }}`.
  - Truy cập mở rộng plugin an toàn hai bước dưới `UndefinedBehavior::Strict`:
    ```jinja
    {% set seo_ext = entry.ext["rust-seo-plugin"] | default({}) %}
    {% if seo_ext.badge | default("") %}
      <span class="sample-badge">{{ seo_ext.badge | default("") }}</span>
    {% endif %}
    ```
- [`assets/app.css`](assets/app.css): Stylesheet thuần tĩnh an toàn theo bộ kiểm CSS của C-SEC.

## Kiểm tra offline với `opendash --check-theme`

```bash
./cms/target/release/opendash --check-theme cms/examples/themes/sample-theme-v3
```

Khi đóng gói ZIP để upload trong Admin → Giao diện, chỉ đưa 3 tệp khai báo trong manifest vào gốc archive (`theme.json`, `single.html`, `assets/app.css`; không đóng gói `README.md` vào ZIP runtime vì D01 cấm file chưa khai báo):

```bash
zip -r /tmp/sample-theme-v3.zip theme.json single.html assets/app.css
./cms/target/release/opendash --check-theme /tmp/sample-theme-v3.zip
```
