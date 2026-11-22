#[path = "../src/plugin/egress.rs"]
mod egress;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use egress::{
    MAX_HTTP_BODY_BYTES, execute_scoped_http_request, is_forbidden_ip, read_bounded_chunks,
    redact_headers_for_log, validate_outbound_headers, verify_resolved_addrs,
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
            ..Default::default()
        },
        hooks: vec![],
        hook_priority: 0,
        actions: vec![],
        declarations: Default::default(),
        editor_panels: vec![],
    }
}

struct MockHttpServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    redirect_destination_hits: Arc<AtomicUsize>,
    oversized_requests: Arc<AtomicUsize>,
}

impl Drop for MockHttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().expect("mock HTTP server thread exit");
        }
    }
}

struct RestoreEnvVar {
    name: &'static str,
    previous: Option<OsString>,
}

impl RestoreEnvVar {
    fn override_for_test(name: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(name);
        unsafe { std::env::set_var(name, value) };
        Self { name, previous }
    }
}

impl Drop for RestoreEnvVar {
    fn drop(&mut self) {
        unsafe {
            if let Some(previous) = self.previous.take() {
                std::env::set_var(self.name, previous);
            } else {
                std::env::remove_var(self.name);
            }
        }
    }
}

// A fresh 10s *invocation* budget per request; the production HTTP client
// keeps its shorter MAX_HTTP_TIMEOUT_SECS cap. Never share an Instant across
// independent plugin HTTP invocations under a busy unfiltered suite.
fn invocation_deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn spawn_mock_http_server() -> MockHttpServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock http");
    listener.set_nonblocking(true).expect("set_nonblocking");
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let redirect_destination_hits = Arc::new(AtomicUsize::new(0));
    let redirect_hits_clone = redirect_destination_hits.clone();
    let oversized_requests = Arc::new(AtomicUsize::new(0));
    let oversized_requests_clone = oversized_requests.clone();

    let handle = std::thread::spawn(move || {
        while !stop_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // Accepted streams inherit listener non-blocking mode on Windows Winsock.
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req.lines().next().unwrap_or("");
                    if first_line.contains("/v1/items/redirect-destination") {
                        redirect_hits_clone.fetch_add(1, Ordering::Relaxed);
                        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes());
                    } else if first_line.contains("/v1/items/redirect-local") {
                        // Independent observable destination: would be hit if
                        // an accidental client redirect follow were enabled.
                        let resp = format!(
                            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/v1/items/redirect-destination\r\nContent-Length: 0\r\n\r\n"
                        );
                        let _ = stream.write_all(resp.as_bytes());
                    } else if first_line.contains("/v1/items/redirect") {
                        let resp = "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest\r\nContent-Length: 0\r\n\r\n";
                        let _ = stream.write_all(resp.as_bytes());
                    } else if first_line.contains("/v1/items/oversized") {
                        oversized_requests_clone.fetch_add(1, Ordering::Relaxed);
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
    MockHttpServer {
        port,
        stop,
        handle: Some(handle),
        redirect_destination_hits,
        oversized_requests,
    }
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
    assert!(!is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(
        93, 184, 216, 34
    ))));

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
    // Apply the test proxy override before launching the mock server thread.
    // The server drops/joins first; then the original environment is restored.
    let _proxy_restore = RestoreEnvVar::override_for_test("HTTP_PROXY", "http://127.0.0.1:9");
    let server = spawn_mock_http_server();
    let port = server.port;
    let manifest = build_scoped_manifest(port);

    // Scope matching checks
    assert!(
        manifest.is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{port}/v1/items"))
    );
    assert!(
        manifest
            .is_http_allowed_with_method("POST", &format!("http://127.0.0.1:{port}/v1/items/42"))
    );
    // Sibling prefix, wrong method, wrong port, encoded bypass, credentials, fragment
    assert!(
        !manifest
            .is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{port}/v1/items-evil"))
    );
    assert!(
        !manifest
            .is_http_allowed_with_method("DELETE", &format!("http://127.0.0.1:{port}/v1/items/42"))
    );
    assert!(
        !manifest
            .is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{}/v1/items", port + 1))
    );
    assert!(!manifest.is_http_allowed_with_method(
        "GET",
        &format!("http://127.0.0.1:{port}/v1/items/%2e%2e/admin")
    ));
    assert!(!manifest.is_http_allowed_with_method(
        "GET",
        &format!("http://user:pass@127.0.0.1:{port}/v1/items")
    ));
    assert!(
        !manifest
            .is_http_allowed_with_method("GET", &format!("http://127.0.0.1:{port}/v1/items#frag"))
    );

    // Allowed request (even if proxy env vars point to a blackhole, .no_proxy() bypasses them)
    let ok_res = execute_scoped_http_request(
        &manifest,
        "GET",
        &format!("http://127.0.0.1:{port}/v1/items/1"),
        BTreeMap::new(),
        None,
        true,
        invocation_deadline(),
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
        invocation_deadline(),
    )
    .expect("expected 302 returned without following redirect");
    assert_eq!(redir_res.status, 302);
    assert_eq!(
        server.redirect_destination_hits.load(Ordering::Relaxed),
        0,
        "link-local redirect must not be followed"
    );

    // A loopback destination is observable by this test server. Even an
    // otherwise allowed redirect target must not be followed automatically.
    let local_redir_res = execute_scoped_http_request(
        &manifest,
        "GET",
        &format!("http://127.0.0.1:{port}/v1/items/redirect-local"),
        BTreeMap::new(),
        None,
        true,
        invocation_deadline(),
    )
    .expect("expected local 302 returned without following redirect");
    assert_eq!(local_redir_res.status, 302);
    assert_eq!(
        server.redirect_destination_hits.load(Ordering::Relaxed),
        0,
        "HTTP client followed the redirect destination"
    );

    // Chunked > 1 MiB blocked
    let big_res = execute_scoped_http_request(
        &manifest,
        "GET",
        &format!("http://127.0.0.1:{port}/v1/items/oversized"),
        BTreeMap::new(),
        None,
        true,
        invocation_deadline(),
    );
    // The original network assertion is error/fail-closed: a streaming
    // transport abort may surface as a read error instead of the quota text.
    // The independent bounded-reader unit case above asserts the 1 MiB cap.
    assert!(
        big_res.is_err(),
        "expected oversized chunked response rejection"
    );
    assert_eq!(
        server.oversized_requests.load(Ordering::Relaxed),
        1,
        "oversized chunked fixture was not exercised"
    );
    assert_eq!(
        server.redirect_destination_hits.load(Ordering::Relaxed),
        0,
        "redirect destination must remain unreachable after oversized response"
    );
}
