//! Bounded HTTP subset for the loopback WPS add-in (one GET per connection).
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub struct Request { pub method: String, pub target: String }
pub fn read(stream: &mut TcpStream) -> Result<Request, u16> {
    stream.set_read_timeout(Some(Duration::from_millis(100))).map_err(|_| 400u16)?;
    stream.set_write_timeout(Some(Duration::from_millis(200))).map_err(|_| 400u16)?;
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut bytes = Vec::new();
    loop {
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") { break; }
        if bytes.len() >= 8192 { return Err(431); }
        if Instant::now() >= deadline { return Err(408); }
        let mut buffer = [0; 1024];
        match stream.read(&mut buffer) {
            Ok(0) => return Err(400),
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => (),
            Err(_) => return Err(400),
        }
    }
    if bytes.len() > 8192 { return Err(431); }
    let port = stream.local_addr().map_err(|_| 400u16)?.port();
    parse(&bytes, port)
}
fn parse(bytes: &[u8], port: u16) -> Result<Request, u16> {
    let text = std::str::from_utf8(bytes).map_err(|_| 400u16)?;
    let mut lines = text.split("\r\n");
    let line = lines.next().ok_or(400u16)?;
    if line.len() > 4096 { return Err(414); }
    let words: Vec<_> = line.split(' ').collect();
    if words.len() != 3 || !matches!(words[2], "HTTP/1.1" | "HTTP/1.0")
        || !words[1].starts_with('/') || words[1].starts_with("//")
        || words[1].chars().any(|c| c.is_control()) { return Err(400); }
    if !matches!(words[0], "GET" | "OPTIONS") { return Err(405); }
    let mut host = None;
    let mut origin = None;
    for line in lines.take_while(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or(400u16)?;
        let value = value.trim();
        if value.chars().any(|c| c.is_control()) { return Err(400); }
        match key.to_ascii_lowercase().as_str() {
            "host" => { if host.replace(value).is_some() { return Err(400); } }
            "origin" => { if origin.replace(value).is_some() { return Err(400); } }
            "content-length" if value != "0" => return Err(400),
            "transfer-encoding" => return Err(400),
            "sec-fetch-site" if value == "cross-site" => return Err(403),
            _ => (),
        }
    }
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !hosts.iter().any(|h| Some(h.as_str()) == host) { return Err(403); }
    if let Some(origin) = origin {
        if !hosts.iter().any(|h| origin == format!("http://{h}")) { return Err(403); }
    }
    Ok(Request { method: words[0].into(), target: words[1].into() })
}
pub fn reply(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let reason = match status { 200 => "OK", 400 => "Bad Request", 403 => "Forbidden", 404 => "Not Found",
        405 => "Method Not Allowed", 408 => "Request Timeout", 414 => "URI Too Long", 431 => "Request Header Fields Too Large", _ => "Error" };
    let port = stream.local_addr().map(|address| address.port()).unwrap_or(16666);
    let header = format!("HTTP/1.1 {status} {reason}\r\nAccess-Control-Allow-Origin: http://127.0.0.1:{port}\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nVary: Origin\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}
pub fn static_path(path: &str) -> Option<&'static str> {
    match path {
        "/" | "/manifest.xml" => Some("manifest.xml"),
        "/index.html" => Some("index.html"), "/main.js" => Some("main.js"),
        "/js/bridge.js" => Some("js/bridge.js"), "/ribbon.xml" => Some("ribbon.xml"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_cross_site_wrong_host_ambiguous_headers_and_non_get() {
        for (request, status) in [
            ("GET /poll HTTP/1.1\r\nHost: evil.invalid\r\n\r\n", 403),
            ("GET /poll HTTP/1.1\r\nHost: 127.0.0.1:16666\r\nOrigin: https://evil.invalid\r\n\r\n", 403),
            ("GET /poll HTTP/1.1\r\nHost: 127.0.0.1:16666\r\nHost: localhost:16666\r\n\r\n", 400),
            ("POST /push HTTP/1.1\r\nHost: 127.0.0.1:16666\r\n\r\n", 405),
            ("GET /poll HTTP/1.1\r\nHost: 127.0.0.1:16666\r\nTransfer-Encoding: chunked\r\n\r\n", 400),
        ] { assert_eq!(parse(request.as_bytes(), 16666).err(), Some(status)); }
        assert!(parse(b"GET /poll HTTP/1.1\r\nHost: 127.0.0.1:16666\r\nOrigin: http://127.0.0.1:16666\r\n\r\n", 16666).is_ok());
        for path in ["/install.sh", "/../LICENSE", "/js/../../config", "/publish.xml", "/tests/test_install.py"] {
            assert!(static_path(path).is_none());
        }
    }
}
