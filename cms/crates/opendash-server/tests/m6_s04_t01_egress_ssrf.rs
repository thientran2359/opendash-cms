#[path = "../src/plugin/egress.rs"]
mod egress;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egress::{
    execute_scoped_http_request, is_forbidden_ip, read_bounded_chunks, redact_headers_for_log,
    validate_outbound_headers, verify_resolved_addrs, MAX_HTTP_BODY_BYTES,
};
use opendash_core::plugin::{NetworkScopeGrant, PluginManifest, PluginPermissions};

fn build_scoped_manifest(port: u16) -> PluginManifest {
    PluginManifest {
        id: "egress-test".into(),
        version: "1.0.0".into(),
        api_version: 1,
        manifest_schema: 2,
        min_host_version: None,
        name: "Egress Test".into(),
        description: "Egress SSRF test plugin".into(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec![],
            http: vec![],
            network: vec![NetworkScopeGrant {
                scheme: "http".into(),
                host: "127.0.0.1".into(),
                port: Some(port),
                methods: vec!["GET".into(), "POST".into()],
                path_prefix: Some("/v1/items".into()),
            }],
            content_read: vec![],
            content_write: vec![],
        },
        hooks: vec![],
        hook_priority: 0,
        actions: vec![],
        declarations: Default::default(),
    }
}

fn spawn_mock_http_server() -> (u16, Arc<AtomicBool>, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock http");
    listener.set_nonblocking(true).expect("set_nonblocking");
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let hits = Arc::new(AtomicUsize::new(0));
    let stop_clone = stop.clone();
    let hits_clone = hits.clone();

    let handle = std::thread::spawn(move || {
        while !stop_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    hits_clone.fetch_add(1, Ordering::Relaxed);
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req.lines().next().unwrap_or("");
                    if first_line.contains("/v1/items/redirect") {
                        let resp = "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes());
                    } else if first_line.contains("/v1/items/oversized") {
                        let header = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
                        let _ = stream.write_all(header.as_bytes());
                        let chunk_data = vec![b'A'; 64 * 1024];
                        for _ in 0..20 {
                            let chunk_hdr = format!("{:x}\r\n", chunk_data.len());
                            if stream.write_all(chunk_hdr.as_bytes()).is_err()
                                || stream.write_all(&chunk_data).is_err()
                                || stream.write_all(b"\r\n").is_err()
                            {
                                break;
                            }
                        }
                        let _ = stream.write_all(b"0\r\n\r\n");
                    } else {
                        let body = r#"{"ok":true}"#;
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: sid=secret123\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(resp.as_bytes());
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });
    (port, stop, hits, handle)
}

#[test]
fn m6_s04_t01_egress_scope_ssrf_pinning_chunked_quota_and_redaction() {
    // 1. Forbidden IP matrix: IPv4 private/loopback/link-local/CGNAT/multicast, IPv6 ULA/link-local/mapped/NAT64
    for ip in [
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1)),
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
        IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
        IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)),
        IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1)),
        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        IpAddr::V6("fc00::1".parse().unwrap()),
        IpAddr::V6("fe80::1".parse().unwrap()),
        IpAddr::V6("::ffff:127.0.0.1".parse().unwrap()),
        IpAddr::V6("::ffff:169.254.169.254".parse().unwrap()),
        IpAddr::V6("64:ff9b::10.0.0.1".parse().unwrap()),
        IpAddr::V6("64:ff9b:1::127.0.0.1".parse().unwrap()),
    ] {
        assert!(is_forbidden_ip(ip), "expected forbidden IP: {ip}");
    }
    assert!(!is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))));

    // Mixed DNS (one public IP + one private IP) must reject the entire set
    let mixed = vec![
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), 443),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 443),
    ];
    assert!(verify_resolved_addrs(&mixed, false).is_err());

    // 2. Outbound header validation + secret redaction
    let mut bad_host_hdr = BTreeMap::new();
    bad_host_hdr.insert("Host".into(), "evil.internal".into());
    assert!(validate_outbound_headers(&bad_host_hdr).is_err());

    let mut bad_hop_hdr = BTreeMap::new();
    bad_hop_hdr.insert("Connection".into(), "keep-alive".into());
    assert!(validate_outbound_headers(&bad_hop_hdr).is_err());

    let mut bad_crlf_hdr = BTreeMap::new();
    bad_crlf_hdr.insert("X-Test".into(), "ok\r\nInjected: 1".into());
    assert!(validate_outbound_headers(&bad_crlf_hdr).is_err());

    let mut secret_hdrs = BTreeMap::new();
    secret_hdrs.insert("Authorization".into(), "Bearer super-secret".into());
    secret_hdrs.insert("Cookie".into(), "session=abc".into());
    secret_hdrs.insert("Set-Cookie".into(), "sid=xyz".into());
    secret_hdrs.insert("Content-Type".into(), "application/json".into());
    let redacted = redact_headers_for_log(&secret_hdrs);
    assert_eq!(redacted.get("Authorization").unwrap(), "[REDACTED]");
    assert_eq!(redacted.get("Cookie").unwrap(), "[REDACTED]");
    assert_eq!(redacted.get("Set-Cookie").unwrap(), "[REDACTED]");
    assert_eq!(redacted.get("Content-Type").unwrap(), "application/json");

    // 3. Streaming chunked cap > 1 MiB unit check
    let oversized = vec![b'x'; MAX_HTTP_BODY_BYTES + 128];
    assert!(read_bounded_chunks(&oversized[..], MAX_HTTP_BODY_BYTES).is_err());

    // 4. Live mock loopback server tests
    let (port, stop, _hits, handle) = spawn_mock_http_server();
    let manifest = build_scoped_manifest(port);

    // Scope matching checks
    assert!(manifest.is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{port}/v1/items")));
    assert!(manifest.is_http_allowed_with_method("POST", &format!("http://127.0.0.1:{port}/v1/items/42")));
    // Sibling prefix, wrong method, wrong port, encoded bypass, credentials, fragment
    assert!(!manifest.is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{port}/v1/items-evil")));
    assert!(!manifest.is_http_allowed_with_method("DELETE", &format!("http://127.0.0.1:{port}/v1/items/42")));
    assert!(!manifest.is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{}/v1/items", port + 1)));
    assert!(!manifest.is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{port}/v1/items/%2e%2e/admin")));
    assert!(!manifest.is_http_allowed_with_method("GET", &format!("http://user:pass@127.0.0.1:{port}/v1/items")));
    assert!(!manifest.is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{port}/v1/items#frag")));

    let deadline = Instant::now() + Duration::from_secs(10);

    // Allowed request (even if proxy env vars point to a blackhole, .no_proxy() bypasses them)
    unsafe {
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:9");
    }
    let ok_res = execute_scoped_http_request(
        &manifest,
        "GET",
        &format!("http://127.0.0.1:{port}/v1/items/1"),
        BTreeMap::new(),
        None,
        true,
        deadline,
    )
    .expect("expected allowed request to succeed");
    assert_eq!(ok_res.status, 200);

    // Redirect blocked (Policy::none() returns 302 without following to 169.254.169.254)
    let redir_res = execute_scoped_http_request(
        &manifest,
        "GET",
        &format!("http://127.0.0.1:{port}/v1/items/redirect"),
        BTreeMap::new(),
        None,
        true,
        deadline,
    )
    .expect("expected 302 returned without following redirect");
    assert_eq!(redir_res.status, 302);

    // Chunked > 1 MiB blocked
    let big_res = execute_scoped_http_request(
        &manifest,
        "GET",
        &format!("http://127.0.0.1:{port}/v1/items/oversized"),
        BTreeMap::new(),
        None,
        true,
        deadline,
    );
    assert!(big_res.is_err(), "expected oversized chunked response to be rejected");

    stop.store(true, Ordering::Relaxed);
    let _ = handle.join();
}
