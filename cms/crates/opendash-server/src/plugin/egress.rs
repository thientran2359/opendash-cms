use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::collections::BTreeMap;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use opendash_core::plugin::PluginManifest;

pub const MAX_HTTP_CALLS_PER_INVOCATION: usize = 10;
pub const MAX_HTTP_TIMEOUT_SECS: u64 = 2;
pub const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024; // 1 MiB
const STREAM_CHUNK_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone)]
pub struct EgressResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.octets()[0] == 0
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
                || v4.octets()[0] >= 224
        }
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            // NAT64: 64:ff9b::/96 and local-use 64:ff9b:1::/48
            let is_nat64 = segs[0] == 0x0064 && segs[1] == 0xff9b;
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_nat64
                || if let Some(v4_mapped) = v6.to_ipv4_mapped() {
                    is_forbidden_ip(IpAddr::V4(v4_mapped))
                } else {
                    false
                }
                || (segs[0] & 0xfe00) == 0xfc00
                || (segs[0] & 0xffc0) == 0xfe80
                || (segs[0] & 0xff00) == 0xff00
        }
    }
}

pub fn is_forbidden_header_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "proxy-connection"
            | "content-length"
    )
}

pub fn redact_headers_for_log(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(k, v)| {
            let lower = k.to_ascii_lowercase();
            let val = if matches!(
                lower.as_str(),
                "authorization" | "cookie" | "set-cookie" | "proxy-authorization" | "x-api-key"
            ) {
                "[REDACTED]".to_string()
            } else {
                v.clone()
            };
            (k.clone(), val)
        })
        .collect()
}

pub fn validate_outbound_headers(headers: &BTreeMap<String, String>) -> Result<(), String> {
    if headers.len() > 50 {
        return Err("Số lượng header HTTP vượt quá 50.".into());
    }
    for (k, v) in headers {
        if k.is_empty() || k.len() > 128 || v.len() > 4096 {
            return Err("Header key hoặc value vượt quá giới hạn độ dài.".into());
        }
        if k.contains('\r')
            || k.contains('\n')
            || k.contains('\0')
            || v.contains('\r')
            || v.contains('\n')
            || v.contains('\0')
            || k.chars().any(|c| c.is_control())
        {
            return Err("permission_denied: Header chứa ký tự điều khiển hoặc CRLF không hợp lệ.".into());
        }
        if is_forbidden_header_name(k) {
            return Err(format!(
                "permission_denied: Không được phép ghi đè header '{}'.",
                k
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct PinnedDnsResolver {
    addrs: Vec<SocketAddr>,
}

impl PinnedDnsResolver {
    pub fn new(addrs: Vec<SocketAddr>) -> Self {
        Self { addrs }
    }
}

impl Resolve for PinnedDnsResolver {
    fn resolve(&self, _name: Name) -> Resolving {
        let addrs = self.addrs.clone();
        Box::pin(async move {
            let iter: Addrs = Box::new(addrs.into_iter());
            Ok(iter)
        })
    }
}

pub fn verify_resolved_addrs(
    addrs: &[SocketAddr],
    allow_loopback: bool,
) -> Result<Vec<SocketAddr>, String> {
    if addrs.is_empty() {
        return Err("Lỗi DNS: không tìm thấy địa chỉ IP.".into());
    }
    for addr in addrs {
        let ip = addr.ip();
        if is_forbidden_ip(ip) && !(allow_loopback && ip.is_loopback()) {
            return Err(
                "permission_denied: Tên miền phân giải về địa chỉ loopback hoặc private network bị cấm."
                    .into(),
            );
        }
    }
    Ok(addrs.to_vec())
}

pub fn resolve_and_pin_addrs(
    clean_host: &str,
    port: u16,
    allow_loopback: bool,
    deadline: Instant,
) -> Result<Vec<SocketAddr>, String> {
    if let Ok(ip) = clean_host.parse::<IpAddr>() {
        if is_forbidden_ip(ip) && !(allow_loopback && ip.is_loopback()) {
            return Err(
                "permission_denied: Không được phép truy cập mạng loopback hoặc private network."
                    .into(),
            );
        }
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    if clean_host.eq_ignore_ascii_case("localhost") {
        if !allow_loopback {
            return Err(
                "permission_denied: Không được phép truy cập mạng loopback hoặc private network."
                    .into(),
            );
        }
        return Ok(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)]);
    }

    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("Quá thời gian thực thi (deadline exceeded).".into());
    }
    let dns_timeout = std::cmp::min(Duration::from_secs(1), remaining);
    let lookup = format!("{}:{}", clean_host, port);
    let lookup_res = if let Ok(h) = tokio::runtime::Handle::try_current() {
        h.block_on(async { tokio::time::timeout(dns_timeout, tokio::net::lookup_host(&lookup)).await })
    } else if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        rt.block_on(async { tokio::time::timeout(dns_timeout, tokio::net::lookup_host(&lookup)).await })
    } else {
        return Err("Lỗi DNS: không khởi tạo được runtime.".into());
    };

    let addrs_iter = match lookup_res {
        Ok(Ok(addrs)) => addrs,
        Ok(Err(_)) => return Err("Lỗi DNS: không thể phân giải tên miền.".into()),
        Err(_) => return Err("Lỗi DNS: quá thời gian phân giải tên miền.".into()),
    };
    let addrs_vec: Vec<SocketAddr> = addrs_iter.collect();
    verify_resolved_addrs(&addrs_vec, allow_loopback)
}

pub fn read_bounded_chunks<R: Read>(
    mut reader: R,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut chunk = [0u8; STREAM_CHUNK_BYTES];
    loop {
        let n = reader
            .read(&mut chunk)
            .map_err(|_| "Lỗi khi đọc response HTTP.".to_string())?;
        if n == 0 {
            break;
        }
        if out.len().saturating_add(n) > max_bytes {
            return Err("Response HTTP vượt quá giới hạn 1 MB.".into());
        }
        out.extend_from_slice(&chunk[..n]);
    }
    Ok(out)
}

pub fn execute_scoped_http_request(
    manifest: &PluginManifest,
    method: &str,
    target_url: &str,
    headers: BTreeMap<String, String>,
    body: Option<String>,
    allow_loopback: bool,
    deadline: Instant,
) -> Result<EgressResponse, String> {
    let method_upper = method.to_ascii_uppercase();
    if !matches!(
        method_upper.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD"
    ) {
        return Err("Method HTTP không được hỗ trợ.".into());
    }

    let parsed_url = url::Url::parse(target_url).map_err(|_| "URL không hợp lệ.".to_string())?;
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err("Chỉ hỗ trợ scheme http hoặc https.".into());
    }
    if !parsed_url.username().is_empty()
        || parsed_url.password().is_some()
        || parsed_url.fragment().is_some()
    {
        return Err(
            "permission_denied: URL không được chứa credentials hoặc fragment.".into(),
        );
    }

    if !manifest.is_http_allowed_with_method(&method_upper, target_url) {
        return Err(
            "permission_denied: URL không nằm trong allowlist HTTP/network của manifest.".into(),
        );
    }

    let clean_host = parsed_url
        .host_str()
        .ok_or_else(|| "URL không chứa host hợp lệ.".to_string())?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let port = parsed_url
        .port_or_known_default()
        .ok_or_else(|| "URL không có port hợp lệ.".to_string())?;

    // Block internal CMS control plane even when loopback is enabled for tests.
    let path_lower = parsed_url.path().to_ascii_lowercase();
    if path_lower.starts_with("/api/admin")
        || path_lower == "/save"
        || path_lower.starts_with("/save/")
        || path_lower == "/preview"
        || path_lower.starts_with("/preview/")
        || path_lower.starts_with("/theme")
    {
        return Err(
            "permission_denied: Không được phép gọi control plane nội bộ qua egress.".into(),
        );
    }

    validate_outbound_headers(&headers)?;

    if let Some(ref b) = body {
        if b.len() > MAX_HTTP_BODY_BYTES {
            return Err("Body request HTTP vượt quá giới hạn 1 MB.".into());
        }
    }

    let pinned_addrs = resolve_and_pin_addrs(clean_host, port, allow_loopback, deadline)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("Quá thời gian thực thi (deadline exceeded).".into());
    }
    let http_timeout = std::cmp::min(Duration::from_secs(MAX_HTTP_TIMEOUT_SECS), remaining);

    let resolver = Arc::new(PinnedDnsResolver::new(pinned_addrs));
    let client = reqwest::blocking::Client::builder()
        .timeout(http_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .dns_resolver(resolver)
        .build()
        .map_err(|_| "Không thể khởi tạo HTTP client.".to_string())?;

    let req_method = reqwest::Method::from_bytes(method_upper.as_bytes())
        .map_err(|_| "Method HTTP không hợp lệ.".to_string())?;
    let mut req_builder = client.request(req_method, parsed_url);

    let _redacted_for_log = redact_headers_for_log(&headers);
    for (k, v) in headers {
        req_builder = req_builder.header(k, v);
    }
    if let Some(b) = body {
        req_builder = req_builder.body(b);
    }

    match req_builder.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mut resp_headers = BTreeMap::new();
            for (k, v) in resp.headers() {
                if let Ok(val_str) = v.to_str() {
                    resp_headers.insert(k.as_str().to_string(), val_str.to_string());
                }
            }
            let _redacted_resp_for_log = redact_headers_for_log(&resp_headers);
            let body_buf = read_bounded_chunks(resp, MAX_HTTP_BODY_BYTES)?;
            let resp_text = String::from_utf8(body_buf)
                .map_err(|_| "Response HTTP không phải UTF-8 hợp lệ.".to_string())?;
            Ok(EgressResponse {
                status,
                headers: resp_headers,
                body: resp_text,
            })
        }
        Err(e) => {
            if e.is_timeout() {
                Err("Lỗi kết nối HTTP: Quá thời gian chờ (timeout).".into())
            } else if e.is_redirect() {
                Err("Lỗi kết nối HTTP: Bị chặn chuyển hướng (redirect policy).".into())
            } else {
                Err("Lỗi kết nối HTTP: Không thể hoàn thành yêu cầu.".into())
            }
        }
    }
}
