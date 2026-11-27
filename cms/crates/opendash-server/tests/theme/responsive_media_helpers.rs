use std::{
    fs,
    io::Cursor,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use image::{DynamicImage, ImageFormat, RgbImage};
use reqwest::{Client, Method, StatusCode, header};
use serde_json::{Value, json};

static NEXT_SITE_SEQ: AtomicU64 = AtomicU64::new(1);

pub(super) struct TestServer {
    child: Child,
    site_root: PathBuf,
    pub(super) base_url: String,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.site_root);
    }
}

pub(super) async fn spawn_server() -> TestServer {
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
            "opendash-s03-render-{}-{nanos}-{}",
            std::process::id(),
            NEXT_SITE_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&site_root).unwrap();
    let stderr = fs::File::create(site_root.join("startup.log")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_opendash"))
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
            .is_ok_and(|r| r.status() == StatusCode::OK)
        {
            return TestServer {
                child,
                site_root,
                base_url,
            };
        }
        assert!(
            Instant::now() < deadline,
            "server startup timeout at {base_url}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub(super) async fn setup_admin_and_login(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .header(header::ORIGIN, base_url)
        .json(&json!({ "username": "admin", "password": "correct-horse-battery-staple" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let res = client
        .post(format!("{base_url}/auth/login"))
        .header(header::ORIGIN, base_url)
        .json(&json!({ "username": "admin", "password": "correct-horse-battery-staple" }))
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

#[allow(clippy::too_many_arguments)]
pub(super) async fn admin_json(
    client: &Client,
    method: Method,
    url: String,
    base: &str,
    cookie: &str,
    csrf: &str,
    if_match: Option<&str>,
    body: Option<Value>,
) -> Value {
    let mut req = client
        .request(method, url)
        .header(header::ORIGIN, base)
        .header(header::COOKIE, cookie)
        .header("x-csrf-token", csrf);
    if let Some(etag) = if_match {
        req = req.header(header::IF_MATCH, etag);
    }
    if let Some(payload) = body {
        req = req.json(&payload);
    }
    let res = req.send().await.unwrap();
    let status = res.status();
    let txt = res.text().await.unwrap();
    assert!(status.is_success(), "request failed {status}: {txt}");
    serde_json::from_str(&txt).unwrap_or(Value::Null)
}

pub(super) fn encode_wide_jpeg(w: u32, h: u32, seed: u64) -> Vec<u8> {
    let mut rgb = RgbImage::new(w, h);
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    for y in 0..h {
        for x in 0..w {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let r =
                (((x * 255) / w.max(1)) as i32 + ((state & 0x1f) as i32 - 16)).clamp(0, 255) as u8;
            let g = (((y * 255) / h.max(1)) as i32 + (((state >> 8) & 0x1f) as i32 - 16))
                .clamp(0, 255) as u8;
            let b = ((((x + y) * 127) / (w + h).max(1)) as i32
                + (((state >> 16) & 0x1f) as i32 - 16))
                .clamp(0, 255) as u8;
            rgb.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(rgb)
        .write_to(&mut out, ImageFormat::Jpeg)
        .unwrap();
    out.into_inner()
}

pub(super) fn build_valid_mp4() -> Vec<u8> {
    fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let len = (8 + payload.len()) as u32;
        let mut out = Vec::with_capacity(len as usize);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }
    fn fbx(kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut inner = Vec::with_capacity(4 + payload.len());
        inner.push(version);
        inner.extend_from_slice(&flags.to_be_bytes()[1..4]);
        inner.extend_from_slice(payload);
        bx(kind, &inner)
    }
    let ftyp = bx(b"ftyp", b"isom\x00\x00\x02\x00isommp42");
    let sample = [0x00u8, 0x00, 0x00, 0x04, 0x65, 0x88, 0x84, 0x21];
    let mdat = bx(b"mdat", &sample);
    let build_moov = |mdat_off: u32| -> Vec<u8> {
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
        let dinf = bx(
            b"dinf",
            &fbx(
                b"dref",
                0,
                0,
                &[0, 0, 0, 1, 0, 0, 0, 12, b'u', b'r', b'l', b' ', 0, 0, 0, 1],
            ),
        );
        let mut sps_bits = Vec::new();
        let push_bits = |bits: &mut Vec<u8>, val: u32, n: usize| {
            for i in (0..n).rev() {
                bits.push(((val >> i) & 1) as u8);
            }
        };
        let push_ue = |bits: &mut Vec<u8>, val: u32| {
            let code = val + 1;
            let len = 32 - code.leading_zeros() as usize;
            for _ in 0..(len - 1) {
                bits.push(0);
            }
            for i in (0..len).rev() {
                bits.push(((code >> i) & 1) as u8);
            }
        };
        push_bits(&mut sps_bits, 0x67, 8);
        push_bits(&mut sps_bits, 66, 8);
        push_bits(&mut sps_bits, 0xc0, 8);
        push_bits(&mut sps_bits, 30, 8);
        push_ue(&mut sps_bits, 0);
        push_ue(&mut sps_bits, 0);
        push_ue(&mut sps_bits, 0);
        push_ue(&mut sps_bits, 0);
        push_ue(&mut sps_bits, 1);
        push_bits(&mut sps_bits, 0, 1);
        push_ue(&mut sps_bits, 19);
        push_ue(&mut sps_bits, 11);
        push_bits(&mut sps_bits, 1, 1);
        push_bits(&mut sps_bits, 1, 1);
        push_bits(&mut sps_bits, 1, 1);
        push_ue(&mut sps_bits, 0);
        push_ue(&mut sps_bits, 0);
        push_ue(&mut sps_bits, 0);
        push_ue(&mut sps_bits, 6);
        push_bits(&mut sps_bits, 0, 1);
        push_bits(&mut sps_bits, 1, 1);
        while !sps_bits.len().is_multiple_of(8) {
            sps_bits.push(0);
        }
        let sps: Vec<u8> = sps_bits
            .chunks_exact(8)
            .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | b))
            .collect();
        let mut avcc = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
        avcc.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        avcc.extend_from_slice(&sps);
        avcc.extend_from_slice(&[1, 0, 4, 0x68, 0xce, 0x3c, 0x80]);
        let mut avc1_p = vec![0u8; 78];
        avc1_p[6..8].copy_from_slice(&1u16.to_be_bytes());
        avc1_p[24..26].copy_from_slice(&320u16.to_be_bytes());
        avc1_p[26..28].copy_from_slice(&180u16.to_be_bytes());
        avc1_p.extend_from_slice(&bx(b"avcC", &avcc));
        let mut stsd_p = vec![0, 0, 0, 1];
        stsd_p.extend_from_slice(&bx(b"avc1", &avc1_p));
        let stbl = bx(
            b"stbl",
            &[
                fbx(b"stsd", 0, 0, &stsd_p),
                fbx(b"stts", 0, 0, &[0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 5, 0xdc]),
                fbx(
                    b"stsc",
                    0,
                    0,
                    &[0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1],
                ),
                fbx(
                    b"stsz",
                    0,
                    0,
                    &[0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, sample.len() as u8],
                ),
                fbx(
                    b"stco",
                    0,
                    0,
                    &[0, 0, 0, 1]
                        .into_iter()
                        .chain(mdat_off.to_be_bytes())
                        .collect::<Vec<_>>(),
                ),
            ]
            .concat(),
        );
        let minf = bx(b"minf", &[dinf, stbl].concat());
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
    let mdat_off = (ftyp.len() + build_moov(0).len() + 8) as u32;
    [ftyp, build_moov(mdat_off), mdat].concat()
}

pub(super) async fn wait_for_variants_version(
    client: &Client,
    base: &str,
    cookie: &str,
    media_id: &str,
    expected_version: i64,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let res = client
            .get(format!("{base}/api/admin/media/{media_id}"))
            .header(header::COOKIE, cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body: Value = res.json().await.unwrap();
        if body["version"].as_i64() == Some(expected_version)
            && body["variants_ready_version"].as_i64() == Some(expected_version)
        {
            return body;
        }
        assert!(
            Instant::now() < deadline,
            "variants v={expected_version} timeout: {body}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
