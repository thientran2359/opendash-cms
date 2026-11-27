use super::*;

#[test]
fn plural_rules_cover_en_vi_fr_ru_ar() {
    let en = PluralRule::parse("nplurals=2; plural=(n != 1);").unwrap();
    assert_eq!(en.evaluate(0, 2), 1);
    assert_eq!(en.evaluate(1, 2), 0);
    assert_eq!(en.evaluate(2, 2), 1);
    assert_eq!(en.evaluate(-1, 2), 0);

    let vi = PluralRule::parse("nplurals=1; plural=0;").unwrap();
    assert_eq!(vi.evaluate(0, 1), 0);
    assert_eq!(vi.evaluate(5, 1), 0);

    let fr = PluralRule::parse("nplurals=2; plural=(n > 1);").unwrap();
    assert_eq!(fr.evaluate(0, 2), 0);
    assert_eq!(fr.evaluate(1, 2), 0);
    assert_eq!(fr.evaluate(2, 2), 1);

    let ru = PluralRule::parse(
        "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<12 || n%100>14) ? 1 : 2);",
    )
    .unwrap();
    assert_eq!(ru.evaluate(1, 3), 0);
    assert_eq!(ru.evaluate(2, 3), 1);
    assert_eq!(ru.evaluate(5, 3), 2);
    assert_eq!(ru.evaluate(11, 3), 2);
    assert_eq!(ru.evaluate(21, 3), 0);

    let ar = PluralRule::parse(
        "nplurals=6; plural=(n==0 ? 0 : n==1 ? 1 : n==2 ? 2 : n%100>=3 && n%100<=10 ? 3 : n%100>=11 ? 4 : 5);",
    )
    .unwrap();
    assert_eq!(ar.evaluate(0, 6), 0);
    assert_eq!(ar.evaluate(1, 6), 1);
    assert_eq!(ar.evaluate(2, 6), 2);
    assert_eq!(ar.evaluate(5, 6), 3);
    assert_eq!(ar.evaluate(15, 6), 4);
    assert_eq!(ar.evaluate(101, 6), 5);
}

#[test]
fn plural_rule_rejects_div_zero_and_excess_depth() {
    assert!(PluralRule::parse("nplurals=2; plural=(n % 0);").is_err());
    assert!(PluralRule::parse("nplurals=7; plural=0;").is_err());
    let deep = format!("{}n{}", "(".repeat(20), ")".repeat(20));
    assert!(PluralRule::parse(&format!("nplurals=2; plural={deep};")).is_err());
}

#[test]
fn po_compile_and_mo_roundtrip_with_context_and_plural() {
    let po = r#"
msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"
"Plural-Forms: nplurals=2; plural=(n != 1);\n"

msgctxt "nav"
msgid "Posts"
msgstr "Bài viết"

msgid "Save"
msgstr "Lưu"

msgid "{n} item"
msgid_plural "{n} items"
msgstr[0] "{n} mục"
msgstr[1] "{n} các mục"

#, fuzzy
msgid "Draft"
msgstr "Bản nháp cũ"
"#;
    let mo = compile_po_to_mo(po).unwrap();
    let cat = parse_mo(&mo).unwrap();
    assert_eq!(cat.get("Save"), "Lưu");
    assert_eq!(cat.get_context("nav", "Posts"), "Bài viết");
    assert_eq!(cat.get_plural("{n} item", "{n} items", 1), "{n} mục");
    assert_eq!(cat.get_plural("{n} item", "{n} items", 3), "{n} các mục");
    // Fuzzy entries must be skipped when compiling .mo
    assert_eq!(cat.get("Draft"), "Draft");
}

#[test]
fn mo_rejects_overlapping_oom_amplification_and_sysdep() {
    // Build a rev 0.1 .mo claiming 1000 entries pointing to the same 16 KiB slice (16 MiB > 8 MiB limit)
    let nstrings: u32 = 1000;
    let orig_table_off = 36u32;
    let trans_table_off = orig_table_off + nstrings * 8;
    let data_off = trans_table_off + nstrings * 8;
    let slice_len = 16 * 1024u32;
    let mut buf = Vec::with_capacity(data_off as usize + slice_len as usize);
    buf.extend_from_slice(&0x9504_12deu32.to_le_bytes());
    buf.extend_from_slice(&1u32.to_le_bytes()); // minor = 1
    buf.extend_from_slice(&nstrings.to_le_bytes());
    buf.extend_from_slice(&orig_table_off.to_le_bytes());
    buf.extend_from_slice(&trans_table_off.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // hash_size
    buf.extend_from_slice(&0u32.to_le_bytes()); // hash_off
    buf.extend_from_slice(&0u32.to_le_bytes()); // n_sysdep_strings = 0
    buf.extend_from_slice(&0u32.to_le_bytes()); // padding to 36
    for _ in 0..nstrings * 2 {
        buf.extend_from_slice(&slice_len.to_le_bytes());
        buf.extend_from_slice(&data_off.to_le_bytes());
    }
    buf.resize(data_off as usize + slice_len as usize, b'a');
    assert!(parse_mo(&buf).is_err());

    // sysdep != 0 must be rejected
    let mut sysdep = vec![0u8; 40];
    sysdep[0..4].copy_from_slice(&0x9504_12deu32.to_le_bytes());
    sysdep[4..8].copy_from_slice(&1u32.to_le_bytes());
    sysdep[32..36].copy_from_slice(&1u32.to_le_bytes());
    assert!(parse_mo(&sysdep).is_err());
}

#[test]
fn pot_coverage_and_placeholder_validation() {
    let pot = PotDocument::parse(
        r#"
msgid "Save"
msgstr ""

msgid "Hello {name}"
msgstr ""
"#,
    )
    .unwrap();
    let po = r#"
msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"
"Plural-Forms: nplurals=1; plural=0;\n"

msgid "Save"
msgstr "Lưu"
"#;
    let cat = parse_po(po).unwrap();
    assert_eq!(cat.coverage_against_pot(&pot), 50);
    assert!(validate_placeholders("Hello {{name}}", "Xin chào {{name}}").is_ok());
    assert!(validate_placeholders("Hello {{name}}", "Xin chào").is_err());
    assert_eq!(canonicalize_locale_code("vi_VN").unwrap(), "vi-vn");
    assert!(validate_extension_text_domain("opendash").is_err());
    assert!(validate_extension_text_domain("opendash-starter").is_ok());
}
