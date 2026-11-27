use std::{
    fs::{self, File},
    io::{Cursor, Read},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use age::secrecy::SecretString;
use image::{DynamicImage, ImageFormat, RgbImage};
use opendash_core::backup::BackupManifestV1;
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use zip::ZipArchive;

static NEXT_SITE_SEQ: AtomicU64 = AtomicU64::new(1);

struct TempSite {
    root: PathBuf,
    child: Option<Child>,
    base_url: String,
}

impl TempSite {
    async fn start() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = NEXT_SITE_SEQ.fetch_add(1, Ordering::Relaxed);
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-s02-ops-{}-{nanos}-{seq}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        let mut s = Self {
            root,
            child: None,
            base_url: String::new(),
        };
        s.restart().await;
        s
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    async fn restart(&mut self) {
        self.stop();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let stderr = fs::File::create(self.root.join("startup.log")).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &self.root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap();
        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                panic!(
                    "ops server exited {status}: {}",
                    fs::read_to_string(self.root.join("startup.log")).unwrap_or_default()
                );
            }
            if client
                .get(format!("{base_url}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                self.child = Some(child);
                self.base_url = base_url;
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "ops server failed health check at {base_url}: {}",
                    fs::read_to_string(self.root.join("startup.log")).unwrap_or_default()
                );
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Drop for TempSite {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn noisy_image(w: u32, h: u32, fmt: ImageFormat, seed: u64) -> Vec<u8> {
    let mut rgb = RgbImage::new(w, h);
    let mut st = 0x9e37_79b9_7f4a_7c15u64 ^ seed;
    for y in 0..h {
        for x in 0..w {
            st ^= st << 13;
            st ^= st >> 7;
            st ^= st << 17;
            let r = (((x * 255) / w.max(1)) as i32 + ((st & 0x1f) as i32 - 16)).clamp(0, 255) as u8;
            let g = (((y * 255) / h.max(1)) as i32 + (((st >> 8) & 0x1f) as i32 - 16)).clamp(0, 255)
                as u8;
            let b = (((x + y) * 127 / (w + h).max(1)) as i32 + (((st >> 16) & 0x1f) as i32 - 16))
                .clamp(0, 255) as u8;
            rgb.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(rgb)
        .write_to(&mut out, fmt)
        .unwrap();
    out.into_inner()
}

fn build_mp4_bytes(duration_units: u32) -> Vec<u8> {
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
        let pb = |b: &mut Vec<u8>, v: u32, n: usize| {
            for i in (0..n).rev() {
                b.push(((v >> i) & 1) as u8);
            }
        };
        let pu = |b: &mut Vec<u8>, v: u32| {
            let code = v + 1;
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
        mvhd_p[12..16].copy_from_slice(&duration_units.to_be_bytes());
        mvhd_p[92..96].copy_from_slice(&2u32.to_be_bytes());
        let mut tkhd_p = vec![0u8; 84];
        tkhd_p[8..12].copy_from_slice(&1u32.to_be_bytes());
        tkhd_p[16..20].copy_from_slice(&duration_units.to_be_bytes());
        tkhd_p[72..76].copy_from_slice(&(320u32 << 16).to_be_bytes());
        tkhd_p[76..80].copy_from_slice(&(180u32 << 16).to_be_bytes());
        let mut mdhd_p = vec![0u8; 20];
        mdhd_p[8..12].copy_from_slice(&1000u32.to_be_bytes());
        mdhd_p[12..16].copy_from_slice(&duration_units.to_be_bytes());
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
        let mut stts_d = vec![0, 0, 0, 1, 0, 0, 0, 1];
        stts_d.extend_from_slice(&duration_units.to_be_bytes());
        let stts = fbx(b"stts", 0, 0, &stts_d);
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
    [ftyp, build_moov(offset), mdat].concat()
}

async fn login(client: &Client, base: &str, user: &str, pass: &str) -> (String, String) {
    let res = client
        .post(format!("{base}/auth/login"))
        .header(header::ORIGIN, base)
        .json(&json!({ "username": user, "password": pass }))
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
    let v: Value = res.json().await.unwrap();
    (cookie, v["csrf_token"].as_str().unwrap().to_string())
}

async fn setup_users(client: &Client, base: &str) -> ((String, String), (String, String)) {
    let s = client
        .post(format!("{base}/auth/setup"))
        .header(header::ORIGIN, base)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(s.status(), StatusCode::CREATED);
    let admin = login(client, base, "admin", "AdminPassword123!").await;
    let ed = client
        .post(format!("{base}/api/admin/users"))
        .header(header::ORIGIN, base)
        .header(header::COOKIE, &admin.0)
        .header("x-csrf-token", &admin.1)
        .json(&json!({ "username": "ed1", "password": "EditorPassword123!", "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed.status(), StatusCode::CREATED);
    let editor = login(client, base, "ed1", "EditorPassword123!").await;
    (admin, editor)
}

async fn poll_job(client: &Client, base: &str, cookie: &str, job_id: &str) -> Value {
    let mut last_job: Option<Value> = None;
    for _ in 0..300 {
        let resp = client
            .get(format!("{base}/api/admin/operations/jobs/{job_id}"))
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap();
        if resp.status() == StatusCode::OK {
            let j: Value = resp.json().await.unwrap();
            if matches!(
                j["state"].as_str(),
                Some("succeeded" | "failed" | "cancelled" | "interrupted")
            ) {
                return j;
            }
            last_job = Some(j);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("job {job_id} timed out; last={last_job:?}");
}

#[tokio::test]
async fn replace_media_rbac_cas_kind_image_and_video_preserve_id() {
    let srv = TempSite::start().await;
    let client = Client::new();
    let ((a_cookie, a_csrf), (e_cookie, e_csrf)) = setup_users(&client, &srv.base_url).await;

    let png_bytes = noisy_image(900, 500, ImageFormat::Png, 1);
    let up_img = client
        .post(format!("{}/api/admin/media", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .body(png_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(up_img.status(), StatusCode::CREATED);
    let img: Value = up_img.json().await.unwrap();
    let img_id = img["id"].as_str().unwrap().to_string();

    let mp4_1 = build_mp4_bytes(1500);
    let up_vid = client
        .post(format!("{}/api/admin/media/video", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(mp4_1)
        .send()
        .await
        .unwrap();
    assert_eq!(up_vid.status(), StatusCode::CREATED);
    let vid: Value = up_vid.json().await.unwrap();
    let vid_id = vid["id"].as_str().unwrap().to_string();

    // Editor forbidden (small probe avoids TCP RST before unread-body early rejection)
    let small_probe = b"preflight-probe".to_vec();
    let webp_new = noisy_image(950, 520, ImageFormat::WebP, 2);
    let ed_res = client
        .put(format!("{}/api/admin/media/{img_id}/file", srv.base_url))
        .header(header::COOKIE, &e_cookie)
        .header("x-csrf-token", &e_csrf)
        .header(header::IF_MATCH, "\"gv1-v1\"")
        .header(header::CONTENT_TYPE, "image/webp")
        .body(small_probe.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(ed_res.status(), StatusCode::FORBIDDEN);

    // 428 missing If-Match & 412 stale If-Match
    let r_428 = client
        .put(format!("{}/api/admin/media/{img_id}/file", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::CONTENT_TYPE, "image/webp")
        .body(small_probe.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(r_428.status(), StatusCode::PRECONDITION_REQUIRED);

    let r_412 = client
        .put(format!("{}/api/admin/media/{img_id}/file", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::IF_MATCH, "\"gv9-v1\"")
        .header(header::CONTENT_TYPE, "image/webp")
        .body(small_probe.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(r_412.status(), StatusCode::PRECONDITION_FAILED);

    // 422 cross-kind image <-> video
    let mp4_2 = build_mp4_bytes(2000);
    let cross_1 = client
        .put(format!("{}/api/admin/media/{img_id}/file", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::IF_MATCH, "\"gv1-v1\"")
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(small_probe.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(cross_1.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let cross_2 = client
        .put(format!("{}/api/admin/media/{vid_id}/file", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::IF_MATCH, "\"gv1-v1\"")
        .header(header::CONTENT_TYPE, "image/png")
        .body(small_probe)
        .send()
        .await
        .unwrap();
    assert_eq!(cross_2.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Valid PNG -> WebP replace preserves ID, updates version/generation & variants, quarantines flat old files
    let rep_img = client
        .put(format!("{}/api/admin/media/{img_id}/file", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::IF_MATCH, "\"gv1-v1\"")
        .header(header::CONTENT_TYPE, "image/webp")
        .header("x-file-name", "updated-banner.webp")
        .body(webp_new)
        .send()
        .await
        .unwrap();
    assert_eq!(rep_img.status(), StatusCode::OK);
    assert_eq!(rep_img.headers()[header::ETAG], "\"gv2-v2\"");
    let det: Value = rep_img.json().await.unwrap();
    assert_eq!(det["id"], img_id);
    assert_eq!(det["extension"], "webp");
    assert_eq!(det["version"], 2);
    assert_eq!(det["generation"], 2);
    assert_eq!(det["variants_ready_version"], 2);

    let shard = &img_id[..2];
    assert!(
        srv.root
            .join(format!("uploads/media/{shard}/{img_id}.webp"))
            .is_file()
    );
    assert!(
        !srv.root
            .join(format!("uploads/media/{shard}/{img_id}.png"))
            .exists()
    );
    assert!(
        srv.root
            .join(format!("uploads/variants/{shard}/{img_id}/v2"))
            .is_dir()
    );
    assert!(
        !srv.root
            .join(format!("uploads/variants/{shard}/{img_id}/v1"))
            .exists()
    );

    // Valid MP4 -> MP4 replace updates ETag + If-Range
    let rep_vid = client
        .put(format!("{}/api/admin/media/{vid_id}/file", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::IF_MATCH, "\"gv1-v1\"")
        .header(header::CONTENT_TYPE, "video/mp4")
        .body(mp4_2)
        .send()
        .await
        .unwrap();
    assert_eq!(rep_vid.status(), StatusCode::OK);
    let vid_det: Value = rep_vid.json().await.unwrap();
    assert_eq!(vid_det["id"], vid_id);
    assert_eq!(vid_det["etag"], "\"gv2-v2\"");

    let range_stale = client
        .get(format!("{}/api/admin/media/{vid_id}/file", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header(header::RANGE, "bytes=0-15")
        .header(header::IF_RANGE, "\"gv1-v1\"")
        .send()
        .await
        .unwrap();
    assert_eq!(range_stale.status(), StatusCode::OK);

    let range_fresh = client
        .get(format!("{}/api/admin/media/{vid_id}/file", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header(header::RANGE, "bytes=0-15")
        .header(header::IF_RANGE, "\"gv2-v2\"")
        .send()
        .await
        .unwrap();
    assert_eq!(range_fresh.status(), StatusCode::PARTIAL_CONTENT);
}

#[tokio::test]
async fn replace_crash_recovery_reconcile_200_plus_backup_restore_and_regenerate() {
    let mut srv = TempSite::start().await;
    let client = Client::new();
    let ((a_cookie, a_csrf), _) = setup_users(&client, &srv.base_url).await;

    let upload_png = |bytes: Vec<u8>| {
        let c = client.clone();
        let url = format!("{}/api/admin/media", srv.base_url);
        let ck = a_cookie.clone();
        let cs = a_csrf.clone();
        async move {
            let r = c
                .post(url)
                .header(header::COOKIE, ck)
                .header("x-csrf-token", cs)
                .header(header::CONTENT_TYPE, "image/png")
                .body(bytes)
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::CREATED);
            r.json::<Value>().await.unwrap()
        }
    };

    let m_del = upload_png(noisy_image(820, 420, ImageFormat::Png, 10)).await;
    let m_del_id = m_del["id"].as_str().unwrap().to_string();
    let m_crash = upload_png(noisy_image(820, 420, ImageFormat::Png, 11)).await;
    let m_crash_id = m_crash["id"].as_str().unwrap().to_string();
    let m_bak = upload_png(noisy_image(820, 420, ImageFormat::Png, 12)).await;
    let m_bak_id = m_bak["id"].as_str().unwrap().to_string();
    let m_fail = upload_png(noisy_image(820, 420, ImageFormat::Png, 13)).await;
    let m_fail_id = m_fail["id"].as_str().unwrap().to_string();

    // Pin m_bak in Home SEO so it's safely referenced
    let pin = client
        .put(format!("{}/api/admin/seo/home", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({ "title": "Home", "og_image_id": m_bak_id, "noindex": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(pin.status(), StatusCode::OK);

    // 1. Delete removes uploads/variants/{shard}/{id}
    let del_res = client
        .delete(format!("{}/api/admin/media/{m_del_id}", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::IF_MATCH, "\"gv1-v1\"")
        .send()
        .await
        .unwrap();
    assert_eq!(del_res.status(), StatusCode::OK);
    assert!(
        !srv.root
            .join(format!("uploads/variants/{}/{m_del_id}", &m_del_id[..2]))
            .exists()
    );

    // 2. Simulate crash in 'promoting_new', seed >200 quarantine dirs + active replace quarantine + aged variant orphan
    srv.stop();
    let db_path = srv.root.join("database/opendash.sqlite");
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();

    let crash_job = format!("{:064x}", 0x9999u64);
    let q_crash = srv.root.join("uploads/.quarantine").join(&crash_job);
    fs::create_dir_all(&q_crash).unwrap();
    let cs = &m_crash_id[..2];
    fs::rename(
        srv.root
            .join(format!("uploads/media/{cs}/{m_crash_id}.png")),
        q_crash.join(format!("{m_crash_id}.png")),
    )
    .unwrap();
    fs::rename(
        srv.root
            .join(format!("uploads/media/{cs}/{m_crash_id}.thumb.png")),
        q_crash.join(format!("{m_crash_id}.thumb.png")),
    )
    .unwrap();
    fs::write(
        srv.root
            .join(format!("uploads/media/{cs}/{m_crash_id}.webp")),
        noisy_image(820, 420, ImageFormat::WebP, 99),
    )
    .unwrap();
    fs::create_dir_all(
        srv.root
            .join(format!("uploads/variants/{cs}/{m_crash_id}/v2")),
    )
    .unwrap();
    fs::write(
        srv.root
            .join(format!("uploads/variants/{cs}/{m_crash_id}/v2/medium.webp")),
        b"partial",
    )
    .unwrap();
    sqlx::query(
        "INSERT INTO media_replace_jobs (
            job_id, media_id, expected_generation, expected_version, new_version,
            old_content_sha256, new_content_sha256, new_kind, new_filename, new_mime_type,
            new_extension, new_byte_size, new_width, new_height, new_thumb_width, new_thumb_height,
            new_variants_json, old_canonical_rel_path, old_thumb_rel_path,
            staging_rel_dir, quarantine_rel_dir, phase
         ) VALUES (?, ?, 1, 1, 2, ?, ?, 'image', 'c.webp', 'image/webp', 'webp', 100, 820, 420, 320, 164, '[]', ?, ?, ?, ?, 'promoting_new')",
    )
    .bind(&crash_job)
    .bind(&m_crash_id)
    .bind(&m_crash_id)
    .bind(&m_crash_id)
    .bind(format!("media/{cs}/{m_crash_id}.png"))
    .bind(format!("media/{cs}/{m_crash_id}.thumb.png"))
    .bind(format!(".staging/replace-{crash_job}"))
    .bind(format!(".quarantine/{crash_job}"))
    .execute(&pool)
    .await
    .unwrap();

    // Seed 215 completed finalized delete jobs (to exceed LIMIT 200) and aged variant orphan
    for idx in 1..=215u64 {
        let jid = format!("d{:063x}", idx);
        let mid = format!("f{:063x}", idx);
        let qd = srv.root.join("uploads/.quarantine").join(&jid);
        fs::create_dir_all(&qd).unwrap();
        sqlx::query(
            "INSERT INTO media_delete_jobs (
                job_id, media_id, generation, expected_version, phase,
                original_rel_path, quarantine_original_rel_path
             ) VALUES (?, ?, 1, 1, 'finalized', 'media/00/x.png', 'q/x.png')",
        )
        .bind(&jid)
        .bind(&mid)
        .execute(&pool)
        .await
        .unwrap();
    }

    let orphan_var_path = srv.root.join(format!(
        "uploads/variants/{}/{m_bak_id}/v99/medium.jpg",
        &m_bak_id[..2]
    ));
    fs::create_dir_all(orphan_var_path.parent().unwrap()).unwrap();
    fs::write(&orphan_var_path, b"stale-variant-bytes").unwrap();
    File::options()
        .write(true)
        .open(&orphan_var_path)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(26 * 3600))
        .unwrap();

    // Corrupt m_fail disk file so batch regenerate advances cursor past failed_ids
    fs::write(
        srv.root
            .join(format!("uploads/media/{}/{m_fail_id}.png", &m_fail_id[..2])),
        b"corrupted-not-png",
    )
    .unwrap();
    pool.close().await;

    // 3. Restart -> startup recovery rolls back crash_job before serving
    srv.restart().await;
    let (a_cookie, a_csrf) = login(&client, &srv.base_url, "admin", "AdminPassword123!").await;
    assert!(
        srv.root
            .join(format!("uploads/media/{cs}/{m_crash_id}.png"))
            .is_file()
    );
    assert!(
        !srv.root
            .join(format!("uploads/media/{cs}/{m_crash_id}.webp"))
            .exists()
    );
    assert!(
        !srv.root
            .join(format!("uploads/variants/{cs}/{m_crash_id}/v2"))
            .exists()
    );

    // 4. Reconcile plan/apply classifies Variant orphan and quarantines without breaking >200 jobs
    let plan: Value = client
        .post(format!(
            "{}/api/admin/operations/reconcile/plan",
            srv.base_url
        ))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let var_cands: Vec<&Value> = plan["orphan_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "variant")
        .collect();
    assert_eq!(var_cands.len(), 1);
    let apply_res = client
        .post(format!(
            "{}/api/admin/operations/reconcile/apply",
            srv.base_url
        ))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .json(&json!({
            "plan_id": plan["plan_id"],
            "expected_generation": plan["generation"],
            "expected_version": plan["version"],
            "digest": plan["digest"],
            "confirm": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(apply_res.status(), StatusCode::OK);
    assert!(!orphan_var_path.exists());

    // 5. Active replace job in .quarantine/ is NEVER purged even if mtime > 7 days
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();
    let active_q_job = format!("{:064x}", 0x8888u64);
    let active_q_dir = srv.root.join("uploads/.quarantine").join(&active_q_job);
    fs::create_dir_all(&active_q_dir).unwrap();
    let aged_file = active_q_dir.join(format!("{m_crash_id}.png"));
    fs::write(&aged_file, b"old-active").unwrap();
    let old_time = SystemTime::now() - Duration::from_secs(9 * 24 * 3600);
    File::options()
        .write(true)
        .open(&aged_file)
        .unwrap()
        .set_modified(old_time)
        .unwrap();
    sqlx::query(
        "INSERT INTO media_replace_jobs (
            job_id, media_id, expected_generation, expected_version, new_version,
            old_content_sha256, new_content_sha256, new_kind, new_filename, new_mime_type,
            new_extension, new_byte_size, new_width, new_height, new_thumb_width, new_thumb_height,
            new_variants_json, old_canonical_rel_path, staging_rel_dir, quarantine_rel_dir, phase
         ) VALUES (?, ?, 1, 1, 2, ?, ?, 'image', 'a.png', 'image/png', 'png', 100, 820, 420, 320, 164, '[]', 'media/00/a.png', '.staging/a', ?, 'moving_old')",
    )
    .bind(&active_q_job)
    .bind(&m_crash_id)
    .bind(&m_crash_id)
    .bind(&m_crash_id)
    .bind(format!(".quarantine/{active_q_job}"))
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    let purge_res = client
        .post(format!(
            "{}/api/admin/operations/quarantine/purge",
            srv.base_url
        ))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .json(&json!({ "confirm": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(purge_res.status(), StatusCode::OK);
    assert!(
        active_q_dir.is_dir(),
        "active replace quarantine must survive purge"
    );

    // 6. Backup rejects active replace job, then succeeds omitting uploads/variants/, and Restore resets variants cleanly
    let bak_blocked: Value = {
        let r = client
            .post(format!("{}/api/admin/operations/backups", srv.base_url))
            .header(header::COOKIE, &a_cookie)
            .header("x-csrf-token", &a_csrf)
            .json(&json!({ "passphrase": "strong-passphrase-s02", "confirm_passphrase": "strong-passphrase-s02" }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::ACCEPTED);
        let jid = r.json::<Value>().await.unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        poll_job(&client, &srv.base_url, &a_cookie, &jid).await
    };
    assert_eq!(bak_blocked["state"], "failed");

    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .unwrap();
    sqlx::query("DELETE FROM media_replace_jobs WHERE job_id = ?")
        .bind(&active_q_job)
        .execute(&pool)
        .await
        .unwrap();
    // Reset variants_ready_version on m_bak and delete its on-disk variants to exercise status + regenerate
    sqlx::query("UPDATE media_assets SET variants_ready_version = 0 WHERE id = ?")
        .bind(&m_bak_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM media_variants WHERE media_id = ?")
        .bind(&m_bak_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let _ = fs::remove_dir_all(
        srv.root
            .join(format!("uploads/variants/{}/{m_bak_id}", &m_bak_id[..2])),
    );

    // 7. Backup (4A) + Decrypt manifest omits uploads/variants/ + Restore resets media_variants
    let bak_init = client
        .post(format!("{}/api/admin/operations/backups", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .json(&json!({ "passphrase": "strong-passphrase-s02", "confirm_passphrase": "strong-passphrase-s02" }))
        .send()
        .await
        .unwrap();
    assert_eq!(bak_init.status(), StatusCode::ACCEPTED);
    let bak_id = bak_init.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let bak_job = poll_job(&client, &srv.base_url, &a_cookie, &bak_id).await;
    assert_eq!(bak_job["state"], "succeeded");
    let age_bytes = client
        .get(format!(
            "{}/api/admin/operations/backups/{bak_id}/file",
            srv.base_url
        ))
        .header(header::COOKIE, &a_cookie)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec();

    // Verify encrypted archive omits variants
    {
        let dec = age::Decryptor::new(Cursor::new(&age_bytes)).unwrap();
        let identity =
            age::scrypt::Identity::new(SecretString::from("strong-passphrase-s02".to_string()));
        let mut zip_bytes = Vec::new();
        dec.decrypt(std::iter::once(&identity as &dyn age::Identity))
            .unwrap()
            .read_to_end(&mut zip_bytes)
            .unwrap();
        let mut archive = ZipArchive::new(Cursor::new(zip_bytes)).unwrap();
        let mut manifest_raw = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest_raw)
            .unwrap();
        let manifest: BackupManifestV1 = serde_json::from_str(&manifest_raw).unwrap();
        assert!(
            !manifest
                .files
                .iter()
                .any(|f| f.logical_key.starts_with("variants/"))
        );
    }

    // Restore
    let up_bak: Value = client
        .post(format!(
            "{}/api/admin/operations/restore-uploads",
            srv.base_url
        ))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(age_bytes)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let rst_id = client
        .post(format!("{}/api/admin/operations/restores", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .json(&json!({ "upload_id": up_bak["upload_id"], "passphrase": "strong-passphrase-s02" }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rst_job = poll_job(&client, &srv.base_url, &a_cookie, &rst_id).await;
    assert_eq!(rst_job["state"], "succeeded");
    let rst_sum: Value = serde_json::from_str(rst_job["summary_json"].as_str().unwrap()).unwrap();
    let rst_root = PathBuf::from(rst_sum["target_root"].as_str().unwrap());
    assert!(!rst_root.join("uploads/variants").exists());
    let rst_pool = SqlitePool::connect_with(
        SqliteConnectOptions::new().filename(rst_root.join("database/opendash.sqlite")),
    )
    .await
    .unwrap();
    let ready_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_assets WHERE variants_ready_version > 0")
            .fetch_one(&rst_pool)
            .await
            .unwrap();
    let var_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_variants")
        .fetch_one(&rst_pool)
        .await
        .unwrap();
    rst_pool.close().await;
    assert_eq!((ready_count, var_count), (0, 0));

    // 8. Cooperative batch regenerate + corrupt cursor advance + cancel + backup receipt retention
    let st: Value = client
        .get(format!("{}/api/admin/media/variants/status", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(st["pending_images"].as_i64().unwrap() >= 1);

    let r1: Value = client
        .post(format!(
            "{}/api/admin/media/variants/regenerate",
            srv.base_url
        ))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .json(&json!({ "force": true, "batch_size": 1 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let run_id = r1["run_id"].as_str().unwrap().to_string();
    assert!(!r1["done"].as_bool().unwrap());

    // Cancel active run -> subsequent resume must return 409 regenerate_run_not_active
    let can_res = client
        .post(format!(
            "{}/api/admin/operations/jobs/{run_id}/cancel",
            srv.base_url
        ))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(can_res.status(), StatusCode::OK);
    let resume_can = client
        .post(format!("{}/api/admin/media/variants/regenerate", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .header("x-csrf-token", &a_csrf)
        .json(&json!({ "run_id": run_id, "force": true, "batch_size": 1, "after_id": r1["next_after_id"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resume_can.status(), StatusCode::CONFLICT);

    // Full non-forced run skips failed m_fail without infinite loop and regenerates m_bak
    let mut after_cursor: Option<String> = None;
    let mut active_run: Option<String> = None;
    let mut saw_failed_cursor = false;
    loop {
        let mut payload = serde_json::Map::new();
        payload.insert("force".into(), json!(true));
        payload.insert("batch_size".into(), json!(1));
        if let Some(ref a) = after_cursor {
            payload.insert("after_id".into(), json!(a));
        }
        if let Some(ref r) = active_run {
            payload.insert("run_id".into(), json!(r));
        }
        let resp = client
            .post(format!(
                "{}/api/admin/media/variants/regenerate",
                srv.base_url
            ))
            .header(header::COOKIE, &a_cookie)
            .header("x-csrf-token", &a_csrf)
            .json(&Value::Object(payload))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = resp.json().await.unwrap();
        if body["failed_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == &m_fail_id)
        {
            saw_failed_cursor = true;
        }
        if body["done"].as_bool().unwrap() {
            break;
        }
        active_run = body["run_id"].as_str().map(str::to_string);
        after_cursor = body["next_after_id"].as_str().map(str::to_string);
    }
    assert!(
        saw_failed_cursor,
        "corrupted file must land in failed_ids while cursor advances"
    );
    let bak_det: Value = client
        .get(format!("{}/api/admin/media/{m_bak_id}", srv.base_url))
        .header(header::COOKIE, &a_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(bak_det["variants_ready_version"], 1);
    assert!(!bak_det["variants"].as_array().unwrap().is_empty());

    // Backup receipt still retained in operation_jobs!
    let b_check = client
        .get(format!(
            "{}/api/admin/operations/jobs/{bak_id}",
            srv.base_url
        ))
        .header(header::COOKIE, &a_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(b_check.status(), StatusCode::OK);
}
