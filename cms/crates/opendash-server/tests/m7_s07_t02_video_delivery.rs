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
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s07-t02-{nanos}"));
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

fn build_valid_mp4() -> (Vec<u8>, String) {
    let mut ftyp_payload = Vec::new();
    ftyp_payload.extend_from_slice(b"isom");
    ftyp_payload.extend_from_slice(&0x00000200u32.to_be_bytes());
    ftyp_payload.extend_from_slice(b"isommp42");
    let ftyp = box_wrap(b"ftyp", &ftyp_payload);

    let video_sample = vec![0x00, 0x00, 0x00, 0x04, 0x65, 0x88, 0x84, 0x21];
    let mdat = box_wrap(b"mdat", &video_sample);

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
async fn video_range_head_conditional_csp_and_public_acl() {
    let server = spawn_server().await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = setup_and_login(&client, &server.base_url).await;

    // Upload poster PNG + MP4 video
    let img = DynamicImage::new_rgb8(2, 2);
    let mut png_bytes = Cursor::new(Vec::new());
    img.write_to(&mut png_bytes, ImageFormat::Png).unwrap();
    let img_res = client
        .post(format!("{}/api/admin/media", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .body(png_bytes.into_inner())
        .send()
        .await
        .unwrap();
    assert_eq!(img_res.status(), StatusCode::CREATED);
    let poster_id = img_res.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (mp4_bytes, video_id) = build_valid_mp4();
    let total_len = mp4_bytes.len();
    let vid_res = client
        .post(format!("{}/api/admin/media/video", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(mp4_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(vid_res.status(), StatusCode::CREATED);

    let doc_with_video = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec1",
            "children": [{
                "id": "row1",
                "children": [{
                    "id": "col1",
                    "children": [{
                        "id": "vid1",
                        "type": "core/video",
                        "props": {
                            "media_id": video_id,
                            "poster_image_id": poster_id,
                            "controls": true,
                            "muted": false
                        }
                    }]
                }]
            }]
        }]
    });

    // 1. Unreferenced / preview-only must be 404 on public /media/{id}/file (even HEAD / If-None-Match / Range)
    let preview = client
        .post(format!("{}/api/admin/builder/homepage/preview", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&doc_with_video)
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);

    assert_eq!(
        client
            .get(format!("{}/media/{}/file", server.base_url, video_id))
            .header(header::IF_NONE_MATCH, format!("\"{video_id}\""))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .head(format!("{}/media/{}/file", server.base_url, video_id))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // Admin endpoint works with 206 Range + private, no-store
    let admin_range = client
        .get(format!("{}/api/admin/media/{}/file", server.base_url, video_id))
        .header(header::COOKIE, &cookie)
        .header(header::RANGE, "bytes=0-3")
        .send()
        .await
        .unwrap();
    assert_eq!(admin_range.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        admin_range.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store"
    );
    assert_eq!(admin_range.bytes().await.unwrap().as_ref(), &mp4_bytes[..4]);

    // 2. Save homepage grants public access and sets media-src 'self' CSP
    let save_res = client
        .put(format!("{}/api/admin/builder/homepage", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&doc_with_video)
        .send()
        .await
        .unwrap();
    assert_eq!(save_res.status(), StatusCode::OK);

    let home = client.get(format!("{}/", server.base_url)).send().await.unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    let home_csp = home
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(home_csp.contains("media-src 'self'"), "CSP: {home_csp}");
    let home_html = home.text().await.unwrap();
    assert!(home_html.contains(&format!("<video src=\"/media/{video_id}/file\"")));
    assert!(home_html.contains(&format!("poster=\"/media/{poster_id}/file\"")));
    assert!(!home_html.contains("autoplay"));

    // 3. Public Range, HEAD, 416, 304, If-Range
    let r0 = client
        .get(format!("{}/media/{}/file", server.base_url, video_id))
        .header(header::RANGE, "bytes=0-0")
        .send()
        .await
        .unwrap();
    assert_eq!(r0.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        r0.headers().get(header::CONTENT_RANGE).unwrap(),
        &format!("bytes 0-0/{total_len}")
    );
    assert_eq!(r0.bytes().await.unwrap().as_ref(), &mp4_bytes[0..1]);

    let r_open = client
        .get(format!("{}/media/{}/file", server.base_url, video_id))
        .header(header::RANGE, "bytes=10-")
        .send()
        .await
        .unwrap();
    assert_eq!(r_open.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(r_open.bytes().await.unwrap().as_ref(), &mp4_bytes[10..]);

    let r_suffix = client
        .get(format!("{}/media/{}/file", server.base_url, video_id))
        .header(header::RANGE, "bytes=-8")
        .send()
        .await
        .unwrap();
    assert_eq!(r_suffix.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        r_suffix.bytes().await.unwrap().as_ref(),
        &mp4_bytes[total_len - 8..]
    );

    let r_clamp = client
        .get(format!("{}/media/{}/file", server.base_url, video_id))
        .header(header::RANGE, "bytes=4-9999999")
        .send()
        .await
        .unwrap();
    assert_eq!(r_clamp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        r_clamp.headers().get(header::CONTENT_RANGE).unwrap(),
        &format!("bytes 4-{}/{total_len}", total_len - 1)
    );

    for bad in ["bytes=abc", "bytes=0-1,2-3", "bytes=999999-"] {
        let r_bad = client
            .get(format!("{}/media/{}/file", server.base_url, video_id))
            .header(header::RANGE, bad)
            .send()
            .await
            .unwrap();
        assert_eq!(r_bad.status(), StatusCode::RANGE_NOT_SATISFIABLE, "range {bad}");
        assert_eq!(
            r_bad.headers().get(header::CONTENT_RANGE).unwrap(),
            &format!("bytes */{total_len}")
        );
    }

    let r_304 = client
        .get(format!("{}/media/{}/file", server.base_url, video_id))
        .header(header::IF_NONE_MATCH, format!("\"{video_id}\""))
        .send()
        .await
        .unwrap();
    assert_eq!(r_304.status(), StatusCode::NOT_MODIFIED);

    let r_if_range_mismatch = client
        .get(format!("{}/media/{}/file", server.base_url, video_id))
        .header(header::RANGE, "bytes=0-3")
        .header(header::IF_RANGE, "\"stale-etag\"")
        .send()
        .await
        .unwrap();
    assert_eq!(r_if_range_mismatch.status(), StatusCode::OK);
    assert_eq!(r_if_range_mismatch.bytes().await.unwrap().len(), total_len);

    // HEAD ignores Range (even malformed/multiple) and returns full 200 without body
    for head_range in ["bytes=0-3", "bytes=abc", "bytes=0-1,2-3"] {
        let h = client
            .head(format!("{}/media/{}/file", server.base_url, video_id))
            .header(header::RANGE, head_range)
            .send()
            .await
            .unwrap();
        assert_eq!(h.status(), StatusCode::OK);
        assert_eq!(
            h.headers().get(header::CONTENT_LENGTH).unwrap(),
            &total_len.to_string()
        );
        assert!(h.bytes().await.unwrap().is_empty());
    }

    // 4. Re-saving homepage without video revokes ACL immediately
    let doc_without_video = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec1",
            "children": [{
                "id": "row1",
                "children": [{
                    "id": "col1",
                    "children": [{
                        "id": "txt1",
                        "type": "core/text",
                        "props": { "text": "No video" }
                    }]
                }]
            }]
        }]
    });
    let resave = client
        .put(format!("{}/api/admin/builder/homepage", server.base_url))
        .header(header::ORIGIN, &server.base_url)
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&doc_without_video)
        .send()
        .await
        .unwrap();
    assert_eq!(resave.status(), StatusCode::OK);

    assert_eq!(
        client
            .get(format!("{}/media/{}/file", server.base_url, video_id))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{}/media/{}/file", server.base_url, poster_id))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}
