use super::*;

#[test]
fn finalize_document_blocks_js_respects_textarea_and_flag() {
    let html = "<!doctype html><html><body><textarea></body></textarea><main>Hi</main></body></html>";
    let unchanged = finalize_document_blocks_js(html.to_owned(), false).unwrap();
    assert!(!unchanged.contains("/assets/opendash-blocks.js"));
    assert_eq!(unchanged, html);

    let injected = finalize_document_blocks_js(html.to_owned(), true).unwrap();
    let script_idx = injected
        .find("<script src=\"/assets/opendash-blocks.js?v=")
        .expect("blocks js script tag injected");
    let textarea_close = injected.find("</textarea>").unwrap();
    let real_body_close = injected.rfind("</body>").unwrap();
    assert!(
        script_idx > textarea_close && script_idx < real_body_close,
        "script must appear after </textarea> and before final </body>: {injected}"
    );
    assert!(injected[script_idx..].contains("defer></script></body>"));
}

#[test]
fn core_l10n_has_interactive_and_popup_translations() {
    let en = core_l10n("en");
    let vi = core_l10n("vi");
    assert_eq!(en.share_email_label.as_ref(), "Email");
    assert_eq!(vi.share_email_label.as_ref(), "Email");
    for (name, ev, vv) in [
        ("tabs_default_label", en.tabs_default_label.as_ref(), vi.tabs_default_label.as_ref()),
        ("slider_prev", en.slider_prev.as_ref(), vi.slider_prev.as_ref()),
        ("gallery_close", en.gallery_close.as_ref(), vi.gallery_close.as_ref()),
        ("copied", en.copied.as_ref(), vi.copied.as_ref()),
        ("popup_default_label", en.popup_default_label.as_ref(), vi.popup_default_label.as_ref()),
        ("popup_close", en.popup_close.as_ref(), vi.popup_close.as_ref()),
        (
            "popup_invalid_id_preview",
            en.popup_invalid_id_preview.as_ref(),
            vi.popup_invalid_id_preview.as_ref(),
        ),
    ] {
        assert!(!ev.trim().is_empty(), "empty EN translation for {name}");
        assert!(!vv.trim().is_empty(), "empty VI translation for {name}");
    }
    assert_ne!(en.popup_close.as_ref(), vi.popup_close.as_ref());
}
