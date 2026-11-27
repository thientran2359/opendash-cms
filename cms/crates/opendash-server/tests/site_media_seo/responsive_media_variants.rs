use std::{
    fs,
    io::Cursor,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

static NEXT_SITE_SEQ: AtomicU64 = AtomicU64::new(1);

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
    let site_root = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "opendash-s01-media-{}-{nanos}-{}",
            std::process::id(),
            NEXT_SITE_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&site_root).unwrap();

    let stderr = fs::File::create(site_root.join("startup.log")).unwrap();
    let bin = env!("CARGO_BIN_EXE_opendash");
    let mut child = Command::new(bin)
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn opendash server");

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            panic!(
                "test server exited {status}: {}",
                fs::read_to_string(site_root.join("startup.log")).unwrap_or_default()
            );
        }
        if client
            .get(format!("{base_url}/api/health"))
            .send()
            .await
            .is_ok_and(|res| res.status() == StatusCode::OK)
        {
            return TestServer {
                child,
                site_root,
                base_url,
            };
        }
        if Instant::now() >= deadline {
            panic!(
                "server failed to become ready at {base_url}: {}",
                fs::read_to_string(site_root.join("startup.log")).unwrap_or_default()
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn setup_admin_and_editor(
    client: &Client,
    base_url: &str,
) -> ((String, String), (String, String)) {
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

    let login = |username: &str, password: &str| {
        let client = client.clone();
        let url = format!("{base_url}/auth/login");
        let origin = base_url.to_string();
        let u = username.to_string();
        let p = password.to_string();
        async move {
            let res = client
                .post(url)
                .header(header::ORIGIN, origin)
                .json(&json!({ "username": u, "password": p }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let cookie = res
                .headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .filter_map(|s| s.split(';').next())
                .collect::<Vec<_>>()
                .join("; ");
            let body: Value = res.json().await.unwrap();
            (cookie, body["csrf_token"].as_str().unwrap().to_string())
        }
    };

    let (admin_cookie, admin_csrf) = login("admin", "correct-horse-battery-staple").await;
    let create_ed = client
        .post(format!("{base_url}/api/admin/users"))
        .header(header::ORIGIN, base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "editor-password-1234",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_ed.status(), StatusCode::CREATED);
    let editor_creds = login("editor1", "editor-password-1234").await;
    ((admin_cookie, admin_csrf), editor_creds)
}

async fn publish_home_with_images(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    image_ids: &[&str],
) {
    let created = client
        .post(format!("{base}/api/admin/content-types/page/entries"))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, base)
        .header("x-csrf-token", csrf)
        .json(&json!({"title":"Media Home","slug":"media-home","values":{"body":"Home"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let entry: Value = created.json().await.unwrap();
    let page_id = entry["id"].as_i64().unwrap();
    let ver = entry["version"].as_i64().unwrap();

    let pub_res = client
        .post(format!(
            "{base}/api/admin/content-types/page/entries/{page_id}/publication"
        ))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, base)
        .header("x-csrf-token", csrf)
        .header("if-match", format!("\"v{ver}\""))
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_res.status(), StatusCode::OK);

    let reading = client
        .get(format!("{base}/api/admin/settings/reading"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    let r_etag = reading.headers()["etag"].to_str().unwrap().to_owned();
    let sel = client
        .put(format!("{base}/api/admin/settings/reading"))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, base)
        .header("x-csrf-token", csrf)
        .header("if-match", r_etag)
        .json(&json!({"homepage_page_id": page_id, "posts_page_id": null}))
        .send()
        .await
        .unwrap();
    assert_eq!(sel.status(), StatusCode::OK);

    let get_doc = client
        .get(format!("{base}/api/admin/entries/page/{page_id}/builder"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get_doc.status(), StatusCode::OK);
    let b_etag = get_doc.headers()[header::ETAG].to_str().unwrap().to_owned();

    let children: Vec<Value> = image_ids
        .iter()
        .enumerate()
        .map(|(i, id)| json!({"id": format!("img{i}"), "type": "core/image", "props": {"media_id": id, "alt": "photo"}}))
        .collect();
    let save_doc = client
        .put(format!("{base}/api/admin/entries/page/{page_id}/builder"))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, base)
        .header("x-csrf-token", csrf)
        .header("if-match", b_etag)
        .json(&json!({
            "schema_version": 1,
            "sections": [{
                "id": "sec1",
                "children": [{
                    "id": "row1",
                    "children": [{
                        "id": "col1",
                        "children": children
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(save_doc.status(), StatusCode::OK);
}

fn encode_noisy_raster(w: u32, h: u32, fmt: ImageFormat) -> Vec<u8> {
    let mut rgb = RgbImage::new(w, h);
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    for y in 0..h {
        for x in 0..w {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let base_r = ((x * 255) / w.max(1)) as i32;
            let base_g = ((y * 255) / h.max(1)) as i32;
            let base_b = (((x + y) * 127) / (w + h).max(1)) as i32;
            let r = (base_r + ((state & 0x1f) as i32 - 16)).clamp(0, 255) as u8;
            let g = (base_g + (((state >> 8) & 0x1f) as i32 - 16)).clamp(0, 255) as u8;
            let b = (base_b + (((state >> 16) & 0x1f) as i32 - 16)).clamp(0, 255) as u8;
            rgb.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(rgb)
        .write_to(&mut out, fmt)
        .unwrap();
    out.into_inner()
}

fn encode_alpha_png(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = RgbaImage::new(w, h);
    for (x, y, px) in rgba.enumerate_pixels_mut() {
        *px = image::Rgba([
            ((x * 9 + y) % 256) as u8,
            ((y * 13) % 256) as u8,
            180,
            if x % 2 == 0 { 120 } else { 255 },
        ]);
    }
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

async fn upload_img(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> Value {
    let res = client
        .post(format!("{base}/api/admin/media"))
        .header(header::ORIGIN, base)
        .header(header::COOKIE, cookie)
        .header("x-csrf-token", csrf)
        .header(header::CONTENT_TYPE, mime)
        .body(bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    res.json().await.unwrap()
}

fn minimal_mp4() -> (Vec<u8>, String) {
    fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let len = (8 + payload.len()) as u32;
        let mut out = Vec::with_capacity(len as usize);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }
    fn fbx(kind: &[u8; 4], ver: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut inner = Vec::with_capacity(4 + payload.len());
        inner.push(ver);
        inner.extend_from_slice(&flags.to_be_bytes()[1..4]);
        inner.extend_from_slice(payload);
        bx(kind, &inner)
    }
    fn sps_320x180() -> Vec<u8> {
        let mut bits = Vec::new();
        let pb = |b: &mut Vec<u8>, val: u32, n: usize| {
            for i in (0..n).rev() {
                b.push(((val >> i) & 1) as u8);
            }
        };
        let pu = |b: &mut Vec<u8>, val: u32| {
            let code = val + 1;
            let len = 32 - code.leading_zeros() as usize;
            for _ in 0..(len - 1) {
                b.push(0);
            }
            for i in (0..len).rev() {
                b.push(((code >> i) & 1) as u8);
            }
        };
        pb(&mut bits, 0x67, 8);
        pb(&mut bits, 66, 8);
        pb(&mut bits, 0xc0, 8);
        pb(&mut bits, 30, 8);
        for v in [0, 0, 0, 0, 1] {
            pu(&mut bits, v);
        }
        pb(&mut bits, 0, 1);
        pu(&mut bits, 19);
        pu(&mut bits, 11);
        pb(&mut bits, 1, 1);
        pb(&mut bits, 1, 1);
        pb(&mut bits, 1, 1);
        for v in [0, 0, 0, 6] {
            pu(&mut bits, v);
        }
        pb(&mut bits, 0, 1);
        pb(&mut bits, 1, 1);
        while !bits.len().is_multiple_of(8) {
            bits.push(0);
        }
        bits.chunks(8)
            .map(|c| {
                c.iter()
                    .enumerate()
                    .fold(0u8, |acc, (i, &b)| acc | (b << (7 - i)))
            })
            .collect()
    }
    let mut fp = b"isom".to_vec();
    fp.extend_from_slice(&0x200u32.to_be_bytes());
    fp.extend_from_slice(b"isommp42");
    let ftyp = bx(b"ftyp", &fp);
    let vs = [0x00u8, 0x00, 0x00, 0x04, 0x65, 0x88, 0x84, 0x21];
    let mdat = bx(b"mdat", &vs);
    let build_moov = |off: u32| {
        let mut mvhd_p = vec![0u8; 96];
        mvhd_p[8..12].copy_from_slice(&1000u32.to_be_bytes());
        mvhd_p[12..16].copy_from_slice(&1500u32.to_be_bytes());
        mvhd_p[92..96].copy_from_slice(&2u32.to_be_bytes());
        let mut tkhd_p = vec![0u8; 84];
        tkhd_p[8..12].copy_from_slice(&1u32.to_be_bytes());
        tkhd_p[16..20].copy_from_slice(&1500u32.to_be_bytes());
        tkhd_p[72..76].copy_from_slice(&(320u32 << 16).to_be_bytes());
        tkhd_p[76..80].copy_from_slice(&(180u32 << 16).to_be_bytes());
        let mut mdhd_p = vec![0u8; 20];
        mdhd_p[8..12].copy_from_slice(&1000u32.to_be_bytes());
        mdhd_p[12..16].copy_from_slice(&1500u32.to_be_bytes());
        let mut hdlr_p = vec![0u8; 20];
        hdlr_p[4..8].copy_from_slice(b"vide");
        let sps = sps_320x180();
        let pps = [0x68u8, 0xce, 0x3c, 0x80];
        let mut avcc = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
        avcc.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        avcc.extend_from_slice(&sps);
        avcc.push(1);
        avcc.extend_from_slice(&(pps.len() as u16).to_be_bytes());
        avcc.extend_from_slice(&pps);
        let mut avc1 = vec![0u8; 78];
        avc1[6..8].copy_from_slice(&1u16.to_be_bytes());
        avc1[24..26].copy_from_slice(&320u16.to_be_bytes());
        avc1[26..28].copy_from_slice(&180u16.to_be_bytes());
        avc1.extend_from_slice(&bx(b"avcC", &avcc));
        let mut stsd_p = 1u32.to_be_bytes().to_vec();
        stsd_p.extend_from_slice(&bx(b"avc1", &avc1));
        let stts = fbx(b"stts", 0, 0, &[0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 5, 0xdc]);
        let stsc = fbx(
            b"stsc",
            0,
            0,
            &[0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1],
        );
        let mut stsz_p = (vs.len() as u32).to_be_bytes().to_vec();
        stsz_p.extend_from_slice(&1u32.to_be_bytes());
        let mut stco_p = 1u32.to_be_bytes().to_vec();
        stco_p.extend_from_slice(&off.to_be_bytes());
        let stbl = bx(
            b"stbl",
            &[
                fbx(b"stsd", 0, 0, &stsd_p),
                stts,
                stsc,
                fbx(b"stsz", 0, 0, &stsz_p),
                fbx(b"stco", 0, 0, &stco_p),
            ]
            .concat(),
        );
        let url_self = fbx(b"url ", 0, 1, &[]);
        let mut dref_p = 1u32.to_be_bytes().to_vec();
        dref_p.extend_from_slice(&url_self);
        let dref = fbx(b"dref", 0, 0, &dref_p);
        let minf = bx(b"minf", &[bx(b"dinf", &dref), stbl].concat());
        let mdia = bx(
            b"mdia",
            &[
                fbx(b"mdhd", 0, 0, &mdhd_p),
                fbx(b"hdlr", 0, 0, &hdlr_p),
                minf,
            ]
            .concat(),
        );
        let trak = bx(b"trak", &[fbx(b"tkhd", 0, 3, &tkhd_p), mdia].concat());
        bx(b"moov", &[fbx(b"mvhd", 0, 0, &mvhd_p), trak].concat())
    };
    let offset = (ftyp.len() + build_moov(0).len() + 8) as u32;
    let mut bytes = [ftyp, build_moov(offset), mdat].concat();
    let sha = format!("{:x}", Sha256::digest(&bytes));
    bytes.shrink_to_fit();
    (bytes, sha)
}

#[tokio::test]
async fn responsive_variants_srcset_head_304_and_acl() {
    let srv = spawn_server().await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let ((admin_cookie, admin_csrf), (ed_cookie, _)) =
        setup_admin_and_editor(&client, &srv.base_url).await;

    let small_jpg = upload_img(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "image/jpeg",
        encode_noisy_raster(500, 300, ImageFormat::Jpeg),
    )
    .await;
    let med_png = upload_img(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "image/png",
        encode_noisy_raster(769, 400, ImageFormat::Png),
    )
    .await;
    let large_jpg = upload_img(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "image/jpeg",
        encode_noisy_raster(1600, 900, ImageFormat::Jpeg),
    )
    .await;
    let large_webp = upload_img(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "image/webp",
        encode_noisy_raster(1600, 900, ImageFormat::WebP),
    )
    .await;
    let alpha_png = upload_img(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "image/png",
        encode_alpha_png(800, 400),
    )
    .await;

    // 500w JPEG: no medium/large, fallback_srcset is null
    let small_id = small_jpg["id"].as_str().unwrap();
    let small_details: Value = client
        .get(format!("{}/api/admin/media/{small_id}", srv.base_url))
        .header(header::COOKIE, &ed_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(small_details["fallback_srcset"].is_null());
    assert_eq!(small_details["etag"], "\"gv1-v1\"");
    assert_eq!(small_details["variants_ready_version"], 1);

    // Opaque 769w PNG: strictly 1 non-WebP medium variant and zero large
    let med_id = med_png["id"].as_str().unwrap();
    let med_details: Value = client
        .get(format!("{}/api/admin/media/{med_id}", srv.base_url))
        .header(header::COOKIE, &ed_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let med_non_webp: Vec<&Value> = med_details["variants"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["spec"] == "medium" && v["format"] != "webp")
        .collect();
    assert_eq!(med_non_webp.len(), 1);
    assert!(
        !med_details["variants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["spec"] == "large")
    );

    // 1600w JPEG: medium + large + no thumbnail.{jpg,png} on disk
    let lg_id = large_jpg["id"].as_str().unwrap();
    let var_dir = srv
        .site_root
        .join("uploads/variants")
        .join(&lg_id[..2])
        .join(lg_id)
        .join("v1");
    assert!(!var_dir.join("thumbnail.jpg").exists());
    assert!(!var_dir.join("thumbnail.png").exists());
    assert!(var_dir.join("medium.jpg").is_file());
    assert!(var_dir.join("large.jpg").is_file());

    // 1600w WebP source: medium.webp + large.webp, fallback_srcset is null, no full.webp
    let wp_id = large_webp["id"].as_str().unwrap();
    let wp_details: Value = client
        .get(format!("{}/api/admin/media/{wp_id}", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(wp_details["fallback_srcset"].is_null());
    assert!(
        wp_details["webp_srcset"]
            .as_str()
            .unwrap()
            .contains(&format!("/file?v=1 1600w"))
    );
    assert!(
        !wp_details["variants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["spec"] == "full")
    );

    // Alpha PNG preserves PNG fallback
    let alpha_id = alpha_png["id"].as_str().unwrap();
    let alpha_details: Value = client
        .get(format!("{}/api/admin/media/{alpha_id}", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        alpha_details["variants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["spec"] == "medium" && v["format"] == "png")
    );

    // Admin variant errors (400 malformed) + HEAD + 304
    let bad_admin = client
        .get(format!(
            "{}/api/admin/media/{lg_id}/variant/bad.jpeg",
            srv.base_url
        ))
        .header(header::COOKIE, &ed_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(bad_admin.status(), StatusCode::BAD_REQUEST);

    // Public unreferenced must be 404; publish home to expose lg_id and wp_id
    assert_eq!(
        client
            .get(format!("{}/media/{lg_id}/variant/medium.jpg", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    publish_home_with_images(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        &[lg_id, wp_id],
    )
    .await;

    let pub_get = client
        .get(format!("{}/media/{lg_id}/variant/medium.jpg", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_get.status(), StatusCode::OK);
    assert_eq!(pub_get.headers()[header::CACHE_CONTROL], "public, no-cache");
    let v_etag = pub_get.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_string();
    let v_bytes = pub_get.bytes().await.unwrap();

    let pub_head = client
        .head(format!("{}/media/{lg_id}/variant/medium.jpg", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_head.status(), StatusCode::OK);
    assert_eq!(
        pub_head.headers()[header::CONTENT_LENGTH].to_str().unwrap(),
        v_bytes.len().to_string()
    );
    assert!(pub_head.bytes().await.unwrap().is_empty());

    let pub_304 = client
        .get(format!("{}/media/{lg_id}/variant/medium.jpg", srv.base_url))
        .header(header::IF_NONE_MATCH, &v_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(pub_304.status(), StatusCode::NOT_MODIFIED);

    assert_eq!(
        client
            .get(format!(
                "{}/media/{lg_id}/variant/medium.jpeg",
                srv.base_url
            ))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn post_replace_content_sha256_dedup_allocates_new_id_for_image_and_video() {
    let srv = spawn_server().await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let ((admin_cookie, admin_csrf), _) = setup_admin_and_editor(&client, &srv.base_url).await;

    let jpg_bytes = encode_noisy_raster(600, 400, ImageFormat::Jpeg);
    let img1 = upload_img(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "image/jpeg",
        jpg_bytes.clone(),
    )
    .await;
    let img1_id = img1["id"].as_str().unwrap().to_string();

    let (mp4_bytes, vid_sha) = minimal_mp4();
    let vid1_res = client
        .post(format!("{}/api/admin/media/video", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(mp4_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(vid1_res.status(), StatusCode::CREATED);
    let vid1: Value = vid1_res.json().await.unwrap();
    assert_eq!(vid1["id"], vid_sha);

    // Simulate Asset 1 having been replaced in-place so its `content_sha256` moved on while PK `id` stays.
    let db_path = srv.site_root.join("database/opendash.sqlite");
    let pool =
        SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&db_path))
            .await
            .unwrap();
    let replaced_img_sha = "1".repeat(64);
    let replaced_vid_sha = "2".repeat(64);
    sqlx::query(
        "UPDATE media_assets SET content_sha256 = ?, version = 2, generation = 2 WHERE id = ?",
    )
    .bind(&replaced_img_sha)
    .bind(&img1_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE media_assets SET content_sha256 = ?, version = 2, generation = 2 WHERE id = ?",
    )
    .bind(&replaced_vid_sha)
    .bind(&vid_sha)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let img2 = upload_img(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "image/jpeg",
        jpg_bytes,
    )
    .await;
    let img2_id = img2["id"].as_str().unwrap();
    assert_ne!(img2_id, img1_id);
    assert_eq!(img2["content_sha256"], img1_id);

    let vid2_res = client
        .post(format!("{}/api/admin/media/video", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::COOKIE, &admin_cookie)
        .header("x-csrf-token", &admin_csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(mp4_bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(vid2_res.status(), StatusCode::CREATED);
    let vid2: Value = vid2_res.json().await.unwrap();
    let vid2_id = vid2["id"].as_str().unwrap();
    assert_ne!(vid2_id, vid_sha);
    assert_eq!(vid2["content_sha256"], vid_sha);

    assert_eq!(
        client
            .get(format!("{}/api/admin/media/{img2_id}/file", srv.base_url))
            .header(header::COOKIE, &admin_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{}/api/admin/media/{vid2_id}/file", srv.base_url))
            .header(header::COOKIE, &admin_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}
