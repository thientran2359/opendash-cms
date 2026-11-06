use minijinja::{Environment, context};

/// A public HTML demo using MiniJinja autoescaping.
/// No theme ZIP, block registry or preview/save API exists in this scaffold.
pub fn render_home() -> Result<String, minijinja::Error> {
    let mut environment = Environment::new();
    environment.add_template(
        "home.html",
        r#"<!doctype html>
<html lang="vi">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{{ name }} — Scaffold</title>
</head>
<body style="font-family:system-ui,sans-serif;max-width:700px;margin:72px auto;padding:0 20px">
  <h1>{{ name }}</h1>
  <p>Khung dự án CMS đang hoạt động. Chức năng quản lý nội dung, theme và plugin chưa được triển khai.</p>
  <p><a href="/admin/">Mở giao diện Admin mẫu</a></p>
</body>
</html>"#,
    )?;
    environment
        .get_template("home.html")?
        .render(context!(name => opendash_core::CMS_NAME))
}
