use super::*;

/// Global Site Options using UI-only `Accordion` primitives so all 12 data option keys
/// live flat at the root (`social_facebook`, `site_logo`, `brand_primary_color`, etc.).
pub fn global_site_option_fields() -> Vec<FieldSpec> {
    let acc_social = {
        let mut s = FieldSpec::new(FieldType::Accordion, "acc_social_links", "Mạng xã hội");
        s.open_by_default = true;
        s.multi_expand = true;
        s
    };
    let acc_brand = {
        let mut s = FieldSpec::new(
            FieldType::Accordion,
            "acc_brand_contact",
            "Thương hiệu & Liên hệ chung",
        );
        s.open_by_default = true;
        s.multi_expand = true;
        s
    };
    let url_spec = |key: &str, label: &str, ph: &str| {
        let mut s = FieldSpec::new(FieldType::Url, key, label);
        s.public = true;
        s.placeholder = Some(ph.to_owned());
        s
    };

    let mut logo = FieldSpec::new(FieldType::Media, "site_logo", "Logo website");
    logo.public = true;
    logo.media_kind = Some(MediaFieldKind::Image);

    let mut brand_color = FieldSpec::new(FieldType::Color, "brand_primary_color", "Màu chủ đạo");
    brand_color.public = true;
    brand_color.default = Some(Value::String("#2563eb".to_owned()));

    let mut phone = FieldSpec::new(FieldType::Text, "contact_phone", "Số điện thoại");
    phone.public = true;
    phone.max_chars = Some(40);
    phone.placeholder = Some("0901 234 567".to_owned());

    let mut addr = FieldSpec::new(FieldType::Textarea, "contact_address", "Địa chỉ");
    addr.public = true;
    addr.max_chars = Some(300);

    let mut footer = FieldSpec::new(FieldType::Text, "footer_note", "Ghi chú chân trang");
    footer.public = true;
    footer.max_chars = Some(240);
    footer.placeholder = Some("© 2026 OpenDash".to_owned());

    vec![
        acc_social,
        url_spec(
            "social_facebook",
            "Facebook",
            "https://facebook.com/yourpage",
        ),
        url_spec(
            "social_youtube",
            "YouTube",
            "https://youtube.com/@yourchannel",
        ),
        url_spec("social_tiktok", "TikTok", "https://tiktok.com/@yourhandle"),
        url_spec(
            "social_instagram",
            "Instagram",
            "https://instagram.com/yourhandle",
        ),
        url_spec("social_x", "X (Twitter)", "https://x.com/yourhandle"),
        url_spec(
            "social_linkedin",
            "LinkedIn",
            "https://linkedin.com/in/yourprofile",
        ),
        url_spec("social_email", "Email liên hệ", "mailto:hello@example.com"),
        acc_brand,
        logo,
        brand_color,
        phone,
        addr,
        footer,
    ]
}
