use super::*;
#[test]
fn accepts_and_reencodes_three_raster_formats() {
    let image = DynamicImage::new_rgb8(3, 2);
    for (format, mime) in [
        (ImageFormat::Png, "image/png"),
        (ImageFormat::Jpeg, "image/jpeg"),
        (ImageFormat::WebP, "image/webp"),
    ] {
        let mut source = Cursor::new(Vec::new());
        image.write_to(&mut source, format).expect("encode fixture");
        let parsed = decode_image(source.into_inner(), mime).expect("media accepted");
        assert_eq!(parsed.mime_type, mime);
        assert_eq!((parsed.width, parsed.height), (3, 2));
        assert_eq!((parsed.thumb_width, parsed.thumb_height), (3, 2));
        assert_eq!(image::guess_format(&parsed.original).unwrap(), format);
        assert_eq!(
            image::guess_format(&parsed.thumbnail).unwrap(),
            ImageFormat::Png
        );
    }
}
#[test]
fn rejects_html_and_fake_png() {
    assert!(
        decode_image(
            b"<svg><script>alert(1)</script></svg>".to_vec(),
            "image/svg+xml"
        )
        .is_err()
    );
    assert!(decode_image(b"not really an image".to_vec(), "image/png").is_err());
    assert!(decode_image(Vec::new(), "image/png").is_err());
}
#[test]
fn canonical_path_never_accepts_traversal() {
    let store = MediaStore::new(std::env::temp_dir());
    assert!(store.path("../x", "png", false).is_err());
    assert!(store.path(&"a".repeat(64), "../png", false).is_err());
    assert!(
        store
            .path(&"a".repeat(64), "png", true)
            .unwrap()
            .ends_with(format!("{}.thumb.png", "a".repeat(64)))
    );
}
