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
    assert_eq!(store.staging_dir(), std::env::temp_dir().join(".staging"));
}

fn noisy_rgb_image(width: u32, height: u32) -> DynamicImage {
    let mut buf = image::RgbImage::new(width, height);
    for (x, y, px) in buf.enumerate_pixels_mut() {
        let r = ((x * 13 + y * 7) % 256) as u8;
        let g = ((x * 5 + y * 19) % 256) as u8;
        let b = ((x * 23 + y * 3) % 256) as u8;
        *px = image::Rgb([r, g, b]);
    }
    DynamicImage::ImageRgb8(buf)
}

#[test]
fn generates_non_upscaled_size_gated_variants_without_duplicate_thumb() {
    let img = noisy_rgb_image(1600, 900);
    let mut jpg = Cursor::new(Vec::new());
    img.write_to(&mut jpg, ImageFormat::Jpeg).unwrap();
    let decoded = decode_image(jpg.into_inner(), "image/jpeg").expect("decode 1600x900 jpeg");

    assert_eq!((decoded.width, decoded.height), (1600, 900));
    assert!(
        !decoded.variants.iter().any(|v| {
            v.spec == MediaVariantSpec::Thumbnail && v.format != MediaVariantFormat::Webp
        }),
        "must never create thumbnail.jpg or thumbnail.png in variants"
    );

    let med_jpg = decoded
        .variants
        .iter()
        .find(|v| v.spec == MediaVariantSpec::Medium && v.format == MediaVariantFormat::Jpeg)
        .expect("medium.jpg generated for 1600w");
    assert_eq!((med_jpg.width, med_jpg.height), (768, 432));

    let large_jpg = decoded
        .variants
        .iter()
        .find(|v| v.spec == MediaVariantSpec::Large && v.format == MediaVariantFormat::Jpeg)
        .expect("large.jpg generated for 1600w");
    assert_eq!((large_jpg.width, large_jpg.height), (1536, 864));

    for v in &decoded.variants {
        if v.format == MediaVariantFormat::Webp {
            let limit = match v.spec {
                MediaVariantSpec::Thumbnail => decoded.thumbnail.len(),
                MediaVariantSpec::Full => decoded.original.len(),
                MediaVariantSpec::Medium => med_jpg.bytes.len(),
                MediaVariantSpec::Large => large_jpg.bytes.len(),
            };
            assert!(
                v.bytes.len() < limit,
                "WebP variant {:?} must satisfy size gate ({} < {})",
                v.spec,
                v.bytes.len(),
                limit
            );
        }
    }

    // Small 500x300 JPEG must not produce medium/large variants.
    let small = noisy_rgb_image(500, 300);
    let mut small_jpg = Cursor::new(Vec::new());
    small.write_to(&mut small_jpg, ImageFormat::Jpeg).unwrap();
    let dec_small = decode_image(small_jpg.into_inner(), "image/jpeg").unwrap();
    assert!(
        !dec_small
            .variants
            .iter()
            .any(|v| matches!(v.spec, MediaVariantSpec::Medium | MediaVariantSpec::Large))
    );
}

#[test]
fn opaque_png_picks_single_smallest_fallback_and_alpha_uses_lossless_or_lossy() {
    // Opaque PNG 800x400: only 1 non-WebP row for `medium`.
    let opaque = noisy_rgb_image(800, 400);
    let mut raw = Cursor::new(Vec::new());
    opaque.write_to(&mut raw, ImageFormat::Png).unwrap();
    let dec = decode_image(raw.into_inner(), "image/png").unwrap();
    let non_webp_medium: Vec<_> = dec
        .variants
        .iter()
        .filter(|v| v.spec == MediaVariantSpec::Medium && v.format != MediaVariantFormat::Webp)
        .collect();
    assert_eq!(non_webp_medium.len(), 1);

    // Alpha PNG <= 1 MP: falls back cleanly without panicking and preserves alpha semantics.
    let mut rgba = image::RgbaImage::new(800, 400);
    for (x, y, px) in rgba.enumerate_pixels_mut() {
        *px = image::Rgba([
            (x % 256) as u8,
            (y % 256) as u8,
            40,
            if x % 2 == 0 { 128 } else { 255 },
        ]);
    }
    let mut alpha_raw = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut alpha_raw, ImageFormat::Png)
        .unwrap();
    let dec_alpha = decode_image(alpha_raw.into_inner(), "image/png").unwrap();
    assert!(
        dec_alpha
            .variants
            .iter()
            .any(|v| { v.spec == MediaVariantSpec::Medium && v.format == MediaVariantFormat::Png })
    );
}

#[tokio::test]
async fn read_gate_wakes_readers_when_replace_finishes_and_rejects_deleting() {
    let store = Arc::new(MediaStore::new(std::env::temp_dir()));
    let id = "a".repeat(64);
    assert!(
        store
            .wait_for_no_active_readers_before_swap(&id, Duration::from_millis(50))
            .await
    );

    let replace_guard = store
        .try_begin_replace_swap(&id)
        .expect("start replace swap");
    let store_reader = Arc::clone(&store);
    let id_reader = id.clone();
    let waiter = tokio::spawn(async move { store_reader.acquire_read_lease(&id_reader).await });

    tokio::time::sleep(Duration::from_millis(30)).await;
    drop(replace_guard);

    let lease = waiter.await.expect("join").expect("reader wakes cleanly");
    assert!(store.has_active_reader(&id));
    assert!(
        !store
            .wait_for_no_active_readers_before_swap(&id, Duration::from_millis(20))
            .await
    );
    drop(lease);

    let del_guard = store
        .begin_delete_drain(&id, Duration::from_millis(50))
        .await
        .expect("drain succeeds");
    let err = store
        .acquire_read_lease(&id)
        .await
        .err()
        .expect("deleting returns 404");
    assert_eq!(err.0, axum::http::StatusCode::NOT_FOUND);
    drop(del_guard);
}

#[tokio::test]
async fn tx2_failure_and_startup_recovery_roll_back_extension_change_cleanly() {
    use crate::{
        operations::SiteCoordinator,
        paths::SitePaths,
        storage::{
            init_db,
            media::{self as media_storage, MediaReplacePhase, NewReplaceJobInput},
        },
    };

    let make_png = |width: u32, height: u32, seed: u8| -> Vec<u8> {
        let mut rgb = image::RgbImage::new(width, height);
        for (x, y, px) in rgb.enumerate_pixels_mut() {
            *px = image::Rgb([
                ((x * 13 + u32::from(seed)) % 256) as u8,
                ((y * 19 + u32::from(seed)) % 256) as u8,
                ((x + y + u32::from(seed)) % 256) as u8,
            ]);
        }
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(rgb)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    };

    let make_webp = |width: u32, height: u32| -> Vec<u8> {
        let mut rgb = image::RgbImage::new(width, height);
        for (x, y, px) in rgb.enumerate_pixels_mut() {
            *px = image::Rgb([
                ((x * 23 + y * 5) % 256) as u8,
                ((x * 7 + y * 11) % 256) as u8,
                ((x * 17 + y * 3) % 256) as u8,
            ]);
        }
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(rgb)
            .write_to(&mut out, ImageFormat::WebP)
            .unwrap();
        out.into_inner()
    };

    let root = std::env::temp_dir().join(format!(
        "opendash-replace-unit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root_canon = if let Ok(p) = std::fs::canonicalize(std::env::temp_dir()) {
        p.join(root.file_name().unwrap())
    } else {
        root
    };
    std::fs::create_dir_all(&root_canon).unwrap();

    let paths = SitePaths::from_parts(
        root_canon.clone(),
        root_canon.join("database").join("opendash.sqlite"),
    )
    .canonicalize_for_runtime()
    .unwrap();
    let coordinator = SiteCoordinator::acquire(paths.clone()).unwrap();
    let db = init_db(paths.db_path()).await.unwrap();
    let store = MediaStore::new(paths.uploads_dir());

    let old_decoded = decode_image(make_png(900, 500, 7), "image/png").unwrap();
    let stage_up = store.create_upload_staging_dir().await.unwrap();
    store
        .stage_image_bundle(&stage_up, &old_decoded)
        .await
        .unwrap();
    let saved = media_storage::allocate_or_save_with_variants(
        &db,
        &store,
        &old_decoded.metadata("orig.png".to_string()),
        &stage_up,
        Some(&old_decoded),
        &old_decoded.variants,
    )
    .await
    .unwrap();

    // 1) Simulated Tx 2 failure replacing PNG -> WebP rolls back cleanly
    let job_id_1 = crate::auth::generate_random_token();
    let new_decoded = decode_image(make_webp(900, 500), "image/webp").unwrap();
    let r_stage_1 = store.create_replace_staging_dir(&job_id_1).await.unwrap();
    let queued_1 = store
        .stage_replace_image_bundle(&r_stage_1, &saved.id, saved.version + 1, &new_decoded)
        .await
        .unwrap();
    let err = super::replace::execute_replace_swap_and_commit_inner(
        &db,
        &store,
        &coordinator,
        &saved,
        StagedReplacePayload {
            job_id: job_id_1.clone(),
            stage_guard: ReplaceStagingGuard::new(r_stage_1),
            content: StagedReplaceContent::Image {
                decoded: new_decoded,
                filename: "new.webp".to_string(),
                variants: queued_1,
            },
        },
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, axum::http::StatusCode::INTERNAL_SERVER_ERROR);

    let shard = &saved.id[..2];
    assert!(store.canonical_path(&saved.id, "png").unwrap().is_file());
    assert!(store.thumbnail_path(&saved.id, "png").unwrap().is_file());
    assert!(!store.canonical_path(&saved.id, "webp").unwrap().exists());
    assert!(
        !store
            .uploads_root()
            .join(format!("variants/{shard}/{}/v2", saved.id))
            .exists()
    );
    assert!(
        store
            .uploads_root()
            .join(format!("variants/{shard}/{}/v1", saved.id))
            .is_dir()
    );
    assert_eq!(
        media_storage::get_replace_job(&db, &job_id_1)
            .await
            .unwrap()
            .unwrap()
            .phase,
        MediaReplacePhase::RolledBack
    );
    assert!(store.try_begin_replace_swap(&saved.id).is_some());

    // 2) Simulate power loss in 'promoting_new' (old PNG in quarantine, partial live .webp & v2/)
    let job_id_2 = crate::auth::generate_random_token();
    let crash_decoded = decode_image(make_webp(900, 500), "image/webp").unwrap();
    let crash_stage = store.create_replace_staging_dir(&job_id_2).await.unwrap();
    let crash_variants = store
        .stage_replace_image_bundle(&crash_stage, &saved.id, 2, &crash_decoded)
        .await
        .unwrap();
    let _job_2 = media_storage::begin_replace_job_tx1(
        &db,
        &NewReplaceJobInput {
            job_id: job_id_2.clone(),
            media_id: saved.id.clone(),
            expected_generation: 1,
            expected_version: 1,
            new_content_sha256: crash_decoded.id.clone(),
            new_kind: "image".to_string(),
            new_filename: "crash.webp".to_string(),
            new_mime_type: "image/webp".to_string(),
            new_extension: "webp".to_string(),
            new_byte_size: crash_decoded.original.len() as i64,
            new_width: 900,
            new_height: 500,
            new_thumb_width: Some(crash_decoded.thumb_width as i64),
            new_thumb_height: Some(crash_decoded.thumb_height as i64),
            new_duration_ms: None,
            new_video_codec: None,
            new_audio_codec: None,
            new_variants_json: serde_json::to_string(&crash_variants).unwrap(),
            staging_rel_dir: format!(".staging/replace-{job_id_2}"),
            quarantine_rel_dir: format!(".quarantine/{job_id_2}"),
        },
    )
    .await
    .unwrap();
    let q2 = store.quarantine_job_dir(&job_id_2);
    std::fs::create_dir_all(&q2).unwrap();
    std::fs::rename(
        store.canonical_path(&saved.id, "png").unwrap(),
        q2.join(format!("{}.png", saved.id)),
    )
    .unwrap();
    std::fs::rename(
        store.thumbnail_path(&saved.id, "png").unwrap(),
        q2.join(format!("{}.thumb.png", saved.id)),
    )
    .unwrap();
    std::fs::rename(
        crash_stage.join("canonical"),
        store.canonical_path(&saved.id, "webp").unwrap(),
    )
    .unwrap();
    if crash_stage.join("v2").is_dir() {
        std::fs::rename(
            crash_stage.join("v2"),
            store
                .uploads_root()
                .join(format!("variants/{shard}/{}/v2", saved.id)),
        )
        .unwrap();
    }

    let recovered = recover_pending_replace_jobs_on_startup(&db, &store)
        .await
        .unwrap();
    assert_eq!(recovered, 1);
    store.cleanup_staging().await;
    assert!(store.canonical_path(&saved.id, "png").unwrap().is_file());
    assert!(store.thumbnail_path(&saved.id, "png").unwrap().is_file());
    assert!(!store.canonical_path(&saved.id, "webp").unwrap().exists());
    assert!(
        !store
            .uploads_root()
            .join(format!("variants/{shard}/{}/v2", saved.id))
            .exists()
    );
    assert_eq!(
        media_storage::get_replace_job(&db, &job_id_2)
            .await
            .unwrap()
            .unwrap()
            .phase,
        MediaReplacePhase::RolledBack
    );

    db.close().await;
    drop(coordinator);
    let _ = std::fs::remove_dir_all(&root_canon);
}
