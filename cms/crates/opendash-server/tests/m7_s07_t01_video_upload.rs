use std::{
    fs,
    io::Cursor,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageFormat};
use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

struct TestServer {
    child: Child,
    site_root: PathBuf,
    base_url: String,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.site_root);
    }
}

async fn spawn_server() -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s07-t01-{nanos}"));
    fs::create_dir_all(&site_root).unwrap();

    let bin = env!("CARGO_BIN_EXE_opendash");
    let child = Command::new(bin)
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash server");

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    for _ in 0..80 {
        if client.get(format!("{base_url}/api/health")).send().await.is_ok() {
            return TestServer {
                child,
                site_root,
                base_url,
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server failed to become ready");
}

async fn setup_and_login(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .header(header::ORIGIN, base_url)
        .json(&json!({
            "username": "admin",
            "password": "correct-horse-battery-staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = client
        .post(format!("{base_url}/auth/login"))
        .header(header::ORIGIN, base_url)
        .json(&json!({
            "username": "admin",
            "password": "correct-horse-battery-staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|s| s.split(';').next())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_string())
}

fn box_wrap(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let len = (8 + payload.len()) as u32;
    let mut out = Vec::with_capacity(len as usize);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    out
}

fn full_box(kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(4 + payload.len());
    inner.push(version);
    inner.extend_from_slice(&flags.to_be_bytes()[1..4]);
    inner.extend_from_slice(payload);
    box_wrap(kind, &inner)
}

fn sps_320x180() -> Vec<u8> {
    let mut bits: Vec<u8> = Vec::new();
    fn push_bits(bits: &mut Vec<u8>, val: u32, n: usize) {
        for i in (0..n).rev() {
            bits.push(((val >> i) & 1) as u8);
        }
    }
    fn push_ue(bits: &mut Vec<u8>, val: u32) {
        let code = val + 1;
        let len = 32 - code.leading_zeros() as usize;
        for _ in 0..(len - 1) {
            bits.push(0);
        }
        for i in (0..len).rev() {
            bits.push(((code >> i) & 1) as u8);
        }
    }
    push_bits(&mut bits, 0x67, 8);
    push_bits(&mut bits, 66, 8);
    push_bits(&mut bits, 0xc0, 8);
    push_bits(&mut bits, 30, 8);
    push_ue(&mut bits, 0);
    push_ue(&mut bits, 0);
    push_ue(&mut bits, 0);
    push_ue(&mut bits, 0);
    push_ue(&mut bits, 1);
    push_bits(&mut bits, 0, 1);
    push_ue(&mut bits, 19);
    push_ue(&mut bits, 11);
    push_bits(&mut bits, 1, 1);
    push_bits(&mut bits, 1, 1);
    push_bits(&mut bits, 1, 1);
    push_ue(&mut bits, 0);
    push_ue(&mut bits, 0);
    push_ue(&mut bits, 0);
    push_ue(&mut bits, 6);
    push_bits(&mut bits, 0, 1);
    push_bits(&mut bits, 1, 1);
    while !bits.len().is_multiple_of(8) {
        bits.push(0);
    }
    bits.chunks_exact(8)
        .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | b))
        .collect()
}

fn build_valid_mp4(with_audio: bool) -> (Vec<u8>, String) {
    let mut ftyp_payload = Vec::new();
    ftyp_payload.extend_from_slice(b"isom");
    ftyp_payload.extend_from_slice(&0x00000200u32.to_be_bytes());
    ftyp_payload.extend_from_slice(b"isommp42");
    let ftyp = box_wrap(b"ftyp", &ftyp_payload);

    let video_sample = vec![0x00, 0x00, 0x00, 0x04, 0x65, 0x88, 0x84, 0x21];
    let audio_sample = vec![0x21, 0x10, 0x04, 0x60];

    let mut mdat_payload = Vec::new();
    mdat_payload.extend_from_slice(&video_sample);
    if with_audio {
        mdat_payload.extend_from_slice(&audio_sample);
    }
    let mdat = box_wrap(b"mdat", &mdat_payload);

    let build_moov = |mdat_payload_offset: u32| -> Vec<u8> {
        let mut mvhd_p = vec![0u8; 96];
        mvhd_p[8..12].copy_from_slice(&1000u32.to_be_bytes());
        mvhd_p[12..16].copy_from_slice(&1500u32.to_be_bytes());
        mvhd_p[92..96].copy_from_slice(&2u32.to_be_bytes());
        let mvhd = full_box(b"mvhd", 0, 0, &mvhd_p);

        let mut tkhd_v = vec![0u8; 84];
        tkhd_v[8..12].copy_from_slice(&1u32.to_be_bytes());
        tkhd_v[16..20].copy_from_slice(&1500u32.to_be_bytes());
        tkhd_v[72..76].copy_from_slice(&(320u32 << 16).to_be_bytes());
        tkhd_v[76..80].copy_from_slice(&(180u32 << 16).to_be_bytes());
        let tkhd_v = full_box(b"tkhd", 0, 3, &tkhd_v);

        let mut mdhd_v = vec![0u8; 20];
        mdhd_v[8..12].copy_from_slice(&1000u32.to_be_bytes());
        mdhd_v[12..16].copy_from_slice(&1500u32.to_be_bytes());
        let mdhd_v = full_box(b"mdhd", 0, 0, &mdhd_v);

        let mut hdlr_v = vec![0u8; 20];
        hdlr_v[4..8].copy_from_slice(b"vide");
        let hdlr_v = full_box(b"hdlr", 0, 0, &hdlr_v);

        let url_self = full_box(b"url ", 0, 1, &[]);
        let mut dref_p = Vec::new();
        dref_p.extend_from_slice(&1u32.to_be_bytes());
        dref_p.extend_from_slice(&url_self);
        let dinf = box_wrap(b"dinf", &full_box(b"dref", 0, 0, &dref_p));

        let sps = sps_320x180();
        let pps = vec![0x68u8, 0xce, 0x3c, 0x80];
        let mut avcc_p = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
        avcc_p.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        avcc_p.extend_from_slice(&sps);
        avcc_p.push(1);
        avcc_p.extend_from_slice(&(pps.len() as u16).to_be_bytes());
        avcc_p.extend_from_slice(&pps);
        let avcc = box_wrap(b"avcC", &avcc_p);

        let mut avc1_p = vec![0u8; 78];
        avc1_p[6..8].copy_from_slice(&1u16.to_be_bytes());
        avc1_p[24..26].copy_from_slice(&320u16.to_be_bytes());
        avc1_p[26..28].copy_from_slice(&180u16.to_be_bytes());
        avc1_p.extend_from_slice(&avcc);
        let avc1 = box_wrap(b"avc1", &avc1_p);

        let mut stsd_v_p = Vec::new();
        stsd_v_p.extend_from_slice(&1u32.to_be_bytes());
        stsd_v_p.extend_from_slice(&avc1);
        let stsd_v = full_box(b"stsd", 0, 0, &stsd_v_p);

        let mut stts_v_p = Vec::new();
        stts_v_p.extend_from_slice(&1u32.to_be_bytes());
        stts_v_p.extend_from_slice(&1u32.to_be_bytes());
        stts_v_p.extend_from_slice(&1500u32.to_be_bytes());
        let stts_v = full_box(b"stts", 0, 0, &stts_v_p);

        let mut stsc_p = Vec::new();
        stsc_p.extend_from_slice(&1u32.to_be_bytes());
        stsc_p.extend_from_slice(&1u32.to_be_bytes());
        stsc_p.extend_from_slice(&1u32.to_be_bytes());
        stsc_p.extend_from_slice(&1u32.to_be_bytes());
        let stsc_v = full_box(b"stsc", 0, 0, &stsc_p);

        let mut stsz_v_p = Vec::new();
        stsz_v_p.extend_from_slice(&(video_sample.len() as u32).to_be_bytes());
        stsz_v_p.extend_from_slice(&1u32.to_be_bytes());
        let stsz_v = full_box(b"stsz", 0, 0, &stsz_v_p);

        let mut stco_v_p = Vec::new();
        stco_v_p.extend_from_slice(&1u32.to_be_bytes());
        stco_v_p.extend_from_slice(&mdat_payload_offset.to_be_bytes());
        let stco_v = full_box(b"stco", 0, 0, &stco_v_p);

        let mut stbl_v_p = Vec::new();
        stbl_v_p.extend_from_slice(&stsd_v);
        stbl_v_p.extend_from_slice(&stts_v);
        stbl_v_p.extend_from_slice(&stsc_v);
        stbl_v_p.extend_from_slice(&stsz_v);
        stbl_v_p.extend_from_slice(&stco_v);
        let stbl_v = box_wrap(b"stbl", &stbl_v_p);

        let mut minf_v_p = Vec::new();
        minf_v_p.extend_from_slice(&dinf);
        minf_v_p.extend_from_slice(&stbl_v);
        let minf_v = box_wrap(b"minf", &minf_v_p);

        let mut mdia_v_p = Vec::new();
        mdia_v_p.extend_from_slice(&mdhd_v);
        mdia_v_p.extend_from_slice(&hdlr_v);
        mdia_v_p.extend_from_slice(&minf_v);
        let mdia_v = box_wrap(b"mdia", &mdia_v_p);

        let mut trak_v_p = Vec::new();
        trak_v_p.extend_from_slice(&tkhd_v);
        trak_v_p.extend_from_slice(&mdia_v);
        let trak_v = box_wrap(b"trak", &trak_v_p);

        let mut moov_p = Vec::new();
        moov_p.extend_from_slice(&mvhd);
        moov_p.extend_from_slice(&trak_v);

        if with_audio {
            let mut tkhd_a = vec![0u8; 84];
            tkhd_a[8..12].copy_from_slice(&2u32.to_be_bytes());
            tkhd_a[16..20].copy_from_slice(&1500u32.to_be_bytes());
            let tkhd_a = full_box(b"tkhd", 0, 3, &tkhd_a);

            let mut hdlr_a = vec![0u8; 20];
            hdlr_a[4..8].copy_from_slice(b"soun");
            let hdlr_a = full_box(b"hdlr", 0, 0, &hdlr_a);

            let esds_p = vec![
                0x03, 22, 0x00, 0x02, 0x00,
                0x04, 17, 0x40, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0x05, 2, 0x12, 0x10,
            ];
            let esds = full_box(b"esds", 0, 0, &esds_p);
            let mut mp4a_p = vec![0u8; 28];
            mp4a_p[6..8].copy_from_slice(&1u16.to_be_bytes());
            mp4a_p.extend_from_slice(&esds);
            let mp4a = box_wrap(b"mp4a", &mp4a_p);

            let mut stsd_a_p = Vec::new();
            stsd_a_p.extend_from_slice(&1u32.to_be_bytes());
            stsd_a_p.extend_from_slice(&mp4a);
            let stsd_a = full_box(b"stsd", 0, 0, &stsd_a_p);

            let mut stsz_a_p = Vec::new();
            stsz_a_p.extend_from_slice(&(audio_sample.len() as u32).to_be_bytes());
            stsz_a_p.extend_from_slice(&1u32.to_be_bytes());
            let stsz_a = full_box(b"stsz", 0, 0, &stsz_a_p);

            let audio_off = mdat_payload_offset + video_sample.len() as u32;
            let mut stco_a_p = Vec::new();
            stco_a_p.extend_from_slice(&1u32.to_be_bytes());
            stco_a_p.extend_from_slice(&audio_off.to_be_bytes());
            let stco_a = full_box(b"stco", 0, 0, &stco_a_p);

            let mut stbl_a_p = Vec::new();
            stbl_a_p.extend_from_slice(&stsd_a);
            stbl_a_p.extend_from_slice(&stts_v);
            stbl_a_p.extend_from_slice(&stsc_v);
            stbl_a_p.extend_from_slice(&stsz_a);
            stbl_a_p.extend_from_slice(&stco_a);
            let stbl_a = box_wrap(b"stbl", &stbl_a_p);

            let mut minf_a_p = Vec::new();
            minf_a_p.extend_from_slice(&dinf);
            minf_a_p.extend_from_slice(&stbl_a);
            let minf_a = box_wrap(b"minf", &minf_a_p);

            let mut mdia_a_p = Vec::new();
            mdia_a_p.extend_from_slice(&mdhd_v);
            mdia_a_p.extend_from_slice(&hdlr_a);
            mdia_a_p.extend_from_slice(&minf_a);
            let mdia_a = box_wrap(b"mdia", &mdia_a_p);

            let mut trak_a_p = Vec::new();
            trak_a_p.extend_from_slice(&tkhd_a);
            trak_a_p.extend_from_slice(&mdia_a);
            moov_p.extend_from_slice(&box_wrap(b"trak", &trak_a_p));
        }

        box_wrap(b"moov", &moov_p)
    };

    let dummy_moov = build_moov(0);
    let mdat_payload_offset = (ftyp.len() + dummy_moov.len() + 8) as u32;
    let moov = build_moov(mdat_payload_offset);
    let mut bytes = Vec::with_capacity(ftyp.len() + moov.len() + mdat.len());
    bytes.extend_from_slice(&ftyp);
    bytes.extend_from_slice(&moov);
    bytes.extend_from_slice(&mdat);
    let sha = format!("{:x}", Sha256::digest(&bytes));
    (bytes, sha)
}

#[tokio::test]
async fn video_upload_validation_dedup_and_typed_image_guards() {
    let server = spawn_server().await;
    let client = Client::builder()
        .redirect(Policy::none())
        .build()
        .unwrap();
    let (cookie, csrf) = setup_and_login(&client, &server.base_url).await;

    // 1. Verify SQLite integrity & foreign keys after 0030_media_video.sql
    let db_path = server.site_root.join("database").join("opendash.sqlite");
    let pool = SqlitePool::connect(&format!("sqlite://{}", db_path.display()))
        .await
        .unwrap();
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    let fk_errors: Vec<(String, i64, String, i64)> =
        sqlx::query_as("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(fk_errors.is_empty(), "FK errors: {fk_errors:?}");
    pool.close().await;

    // 2. Upload a valid 1x1 PNG image and confirm unchanged image metadata behavior
    let img = DynamicImage::new_rgb8(2, 2);
    let mut png_bytes = Cursor::new(Vec::new());
    img.write_to(&mut png_bytes, ImageFormat::Png).unwrap();
    let img_res = client
        .post(format!("{}/api/admin/media", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("x-file-name", "sample.png")
        .body(png_bytes.into_inner())
        .send()
        .await
        .unwrap();
    assert_eq!(img_res.status(), StatusCode::CREATED);
    let img_json: Value = img_res.json().await.unwrap();
    assert_eq!(img_json["kind"], "image");
    assert_eq!(img_json["thumb_width"], 2);
    assert!(img_json["duration_ms"].is_null());

    // 3. Upload valid AVC+AAC MP4
    let (mp4_bytes, mp4_sha) = build_valid_mp4(true);
    let vid_res = client
        .post(format!("{}/api/admin/media/video", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header("x-file-name", "hero%20clip.mp4")
        .body(mp4_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(vid_res.status(), StatusCode::CREATED);
    let vid_json: Value = vid_res.json().await.unwrap();
    assert_eq!(vid_json["id"], mp4_sha);
    assert_eq!(vid_json["kind"], "video");
    assert_eq!(vid_json["mime_type"], "video/mp4");
    assert_eq!(vid_json["extension"], "mp4");
    assert_eq!(vid_json["width"], 320);
    assert_eq!(vid_json["height"], 180);
    assert_eq!(vid_json["duration_ms"], 1500);
    assert!(vid_json["thumb_width"].is_null());
    assert!(vid_json["thumb_height"].is_null());
    assert!(vid_json["video_codec"].as_str().unwrap().starts_with("avc1."));
    assert_eq!(vid_json["audio_codec"], "mp4a.40.2");

    // 4. Dedup preserves original metadata & filename
    let dedup_res = client
        .post(format!("{}/api/admin/media/video", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .header("x-file-name", "different-name.mp4")
        .body(mp4_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(dedup_res.status(), StatusCode::CREATED);
    let dedup_json: Value = dedup_res.json().await.unwrap();
    assert_eq!(dedup_json["id"], mp4_sha);
    assert_eq!(dedup_json["filename"], "hero clip.mp4");

    // 5. Media list filtering & 404 thumbnail for video
    let list_vid: Vec<Value> = client
        .get(format!("{}/api/admin/media?kind=video", server.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list_vid.len(), 1);
    assert_eq!(list_vid[0]["id"], mp4_sha);

    let thumb_res = client
        .get(format!(
            "{}/api/admin/media/{}/thumbnail",
            server.base_url, mp4_sha
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(thumb_res.status(), StatusCode::NOT_FOUND);

    // 6. Adversarial rejections + staging cleanup
    let bad_mime = client
        .post(format!("{}/api/admin/media/video", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "video/webm")
        .body(mp4_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(bad_mime.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let mut lying_dims = mp4_bytes.clone();
    let tkhd_pos = lying_dims.windows(4).position(|w| w == b"tkhd").unwrap();
    let w_off = tkhd_pos + 4 + 4 + 72;
    lying_dims[w_off..w_off + 4].copy_from_slice(&(640u32 << 16).to_be_bytes());
    let lying_res = client
        .post(format!("{}/api/admin/media/video", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(lying_dims)
        .send()
        .await
        .unwrap();
    assert_eq!(lying_res.status(), StatusCode::BAD_REQUEST);

    let mut fragmented = mp4_bytes.clone();
    fragmented.extend_from_slice(&8u32.to_be_bytes());
    fragmented.extend_from_slice(b"moof");
    let frag_res = client
        .post(format!("{}/api/admin/media/video", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(fragmented)
        .send()
        .await
        .unwrap();
    assert_eq!(frag_res.status(), StatusCode::BAD_REQUEST);

    // Staging directory must be clean after rejections
    let staging_dir = server.site_root.join("uploads").join("media").join(".staging");
    if staging_dir.exists() {
        let count = fs::read_dir(&staging_dir).unwrap().count();
        assert_eq!(count, 0, "staging directory must not retain rejected files");
    }

    // 7. Typed image-ref validators reject video IDs (SEO og_image_id & Builder core/image)
    let seo_get = client
        .get(format!("{}/api/admin/seo/home", server.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(seo_get.status(), StatusCode::OK);
    let etag = seo_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let seo_res = client
        .put(format!("{}/api/admin/seo/home", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &etag)
        .json(&json!({
            "title": "Home",
            "og_image_id": mp4_sha,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(seo_res.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let builder_res = client
        .post(format!("{}/api/admin/builder/homepage/preview", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "version": 1,
            "document": {
                "schema_version": 1,
                "root": {
                    "id": "root",
                    "kind": "core/section",
                    "props": {},
                    "children": [
                        {
                            "id": "img1",
                            "kind": "core/image",
                            "props": {
                                "media_id": mp4_sha,
                                "alt": "Not an image"
                            },
                            "children": []
                        }
                    ]
                }
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(builder_res.status(), StatusCode::BAD_REQUEST);
}
