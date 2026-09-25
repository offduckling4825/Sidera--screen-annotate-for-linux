// Sidera - local HTTP bridge
// Copyright (C) 2026 Carl_Jin
// SPDX-License-Identifier: GPL-3.0-only

#![forbid(unsafe_code)]

use sidera_core::{AnnotationState, Command, WpsEvent};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 16_666;
const DEFAULT_MAX_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub addin_dir: Option<PathBuf>,
    pub max_request_bytes: usize,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            addin_dir: None,
            max_request_bytes: DEFAULT_MAX_REQUEST_BYTES,
        }
    }
}

#[derive(Clone)]
pub struct Bridge {
    state: Arc<Mutex<AnnotationState>>,
    config: BridgeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: &'static str,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl HttpResponse {
    fn new(status: u16, reason: &'static str, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            reason,
            content_type,
            body,
        }
    }

    fn text(status: u16, reason: &'static str, body: impl Into<String>) -> Self {
        Self::new(
            status,
            reason,
            "text/plain; charset=utf-8",
            body.into().into_bytes(),
        )
    }

    pub fn to_http_bytes(&self) -> Vec<u8> {
        let headers = format!(
            "HTTP/1.1 {} {}\r\nAccess-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
             Access-Control-Allow-Headers: *\r\nContent-Type: {}\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n",
            self.status,
            self.reason,
            self.content_type,
            self.body.len()
        );
        let mut bytes = headers.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

impl Bridge {
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(AnnotationState::new())),
            config,
        }
    }

    pub fn state_snapshot(&self) -> AnnotationState {
        self.state
            .lock()
            .expect("bridge state mutex poisoned")
            .clone()
    }

    pub fn queue_command(&self, command: Command) {
        self.state
            .lock()
            .expect("bridge state mutex poisoned")
            .enqueue(command);
    }

    pub fn resolved_addin_dir(&self) -> Option<PathBuf> {
        if let Some(path) = &self.config.addin_dir {
            return path.is_dir().then(|| path.clone());
        }
        if let Ok(path) = std::env::var("WPS_ADDIN_DIR") {
            let path = PathBuf::from(path);
            if path.is_dir() {
                return Some(path);
            }
        }
        [
            PathBuf::from("wps-addin"),
            PathBuf::from("../wps-addin"),
            PathBuf::from("/usr/share/sidera/wps-addin"),
        ]
        .into_iter()
        .find(|candidate| candidate.is_dir())
    }

    pub fn handle_request(&self, raw_request: &str) -> HttpResponse {
        let Some(request_line) = raw_request.lines().next() else {
            return HttpResponse::text(400, "Bad Request", "bad request\n");
        };
        let mut fields = request_line.split_whitespace();
        let Some(method) = fields.next() else {
            return HttpResponse::text(400, "Bad Request", "bad request\n");
        };
        let Some(target) = fields.next() else {
            return HttpResponse::text(400, "Bad Request", "bad request\n");
        };
        let Some((path, query)) = split_target(target) else {
            return HttpResponse::text(400, "Bad Request", "bad request\n");
        };

        if method == "OPTIONS" {
            return HttpResponse::text(200, "OK", "");
        }
        if method != "GET" && method != "POST" {
            return HttpResponse::text(405, "Method Not Allowed", "method not allowed\n");
        }

        let mut state = self.state.lock().expect("bridge state mutex poisoned");
        state.mark_bridge_seen();
        match path {
            "/hello" => HttpResponse::text(200, "OK", "OK sidera"),
            "/push" => {
                let Some(message) = query_param(query, "m") else {
                    return HttpResponse::text(400, "Bad Request", "missing m\n");
                };
                if let Some(event) = WpsEvent::parse(&message) {
                    state.apply_wps_event(&event);
                }
                HttpResponse::text(200, "OK", "OK")
            }
            "/poll" => {
                let body = state
                    .dequeue()
                    .map_or_else(String::new, |command| command.as_str().to_owned());
                HttpResponse::text(200, "OK", body)
            }
            _ => {
                drop(state);
                self.serve_static(path)
            }
        }
    }

    pub fn serve(&self, listener: TcpListener) -> io::Result<()> {
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => {
                    let bridge = self.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = bridge.serve_stream(stream) {
                            eprintln!("[sidera-bridge] connection error: {error}");
                        }
                    });
                }
                Err(error) => eprintln!("[sidera-bridge] accept error: {error}"),
            }
        }
        Ok(())
    }

    fn serve_stream(&self, mut stream: TcpStream) -> io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut request = Vec::with_capacity(2048);
        let mut buffer = [0_u8; 2048];
        while request.len() < self.config.max_request_bytes {
            let remaining = self.config.max_request_bytes - request.len();
            let chunk_size = buffer.len().min(remaining);
            let read = stream.read(&mut buffer[..chunk_size])?;
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let response = if !request.windows(4).any(|window| window == b"\r\n\r\n") {
            HttpResponse::text(400, "Bad Request", "request headers too large\n")
        } else {
            match std::str::from_utf8(&request) {
                Ok(request) => self.handle_request(request),
                Err(_) => HttpResponse::text(400, "Bad Request", "request is not UTF-8\n"),
            }
        };
        stream.write_all(&response.to_http_bytes())
    }

    fn serve_static(&self, request_path: &str) -> HttpResponse {
        let Some(directory) = self.resolved_addin_dir() else {
            return HttpResponse::text(404, "Not Found", "WPS add-in directory not found\n");
        };
        let Some(relative) = safe_relative_path(request_path) else {
            return HttpResponse::text(400, "Bad Request", "invalid path\n");
        };
        let Ok(directory) = fs::canonicalize(directory) else {
            return HttpResponse::text(404, "Not Found", "WPS add-in directory not found\n");
        };
        let file_path = directory.join(relative);
        let Ok(file_path) = fs::canonicalize(file_path) else {
            return HttpResponse::text(404, "Not Found", "file not found\n");
        };
        if !file_path.starts_with(&directory) {
            return HttpResponse::text(400, "Bad Request", "invalid path\n");
        }
        let Ok(metadata) = fs::metadata(&file_path) else {
            return HttpResponse::text(404, "Not Found", "file not found\n");
        };
        if !metadata.is_file() {
            return HttpResponse::text(404, "Not Found", "file not found\n");
        }
        let Ok(body) = fs::read(&file_path) else {
            return HttpResponse::text(500, "Internal Server Error", "cannot read file\n");
        };
        HttpResponse::new(200, "OK", content_type(&file_path), body)
    }
}

fn split_target(target: &str) -> Option<(&str, &str)> {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if !path.starts_with('/') {
        return None;
    }
    Some((path, query))
}

fn query_param(query: &str, wanted: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (decode_component(key).ok()? == wanted).then(|| decode_component(value).ok())?
    })
}

fn decode_component(value: &str) -> Result<String, ()> {
    let mut bytes = Vec::with_capacity(value.len());
    let source = value.as_bytes();
    let mut index = 0;
    while index < source.len() {
        match source[index] {
            b'+' => bytes.push(b' '),
            b'%' if index + 2 < source.len() => {
                let high = hex(source[index + 1])?;
                let low = hex(source[index + 2])?;
                bytes.push((high << 4) | low);
                index += 2;
            }
            b'%' => return Err(()),
            byte => bytes.push(byte),
        }
        index += 1;
    }
    String::from_utf8(bytes).map_err(|_| ())
}

fn hex(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(()),
    }
}

fn safe_relative_path(request_path: &str) -> Option<PathBuf> {
    let decoded = decode_component(request_path).ok()?;
    let relative = decoded.trim_start_matches('/');
    let relative = if relative.is_empty() {
        "manifest.xml"
    } else {
        relative
    };
    let path = Path::new(relative);
    if path.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    }) {
        return None;
    }
    Some(path.to_path_buf())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("xml") => "application/xml; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_addin_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sidera-bridge-{suffix}"));
        fs::create_dir_all(path.join("js")).unwrap();
        fs::write(path.join("manifest.xml"), b"<manifest>sidera</manifest>").unwrap();
        fs::write(path.join("js/bridge.js"), b"bridge").unwrap();
        path
    }

    #[test]
    fn hello_and_cors_are_compatible_with_addin() {
        let bridge = Bridge::new(BridgeConfig::default());
        let response = bridge.handle_request("GET /hello?m=sidera-bridge HTTP/1.1\r\n\r\n");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"OK sidera");
        let wire = String::from_utf8(response.to_http_bytes()).unwrap();
        assert!(wire.contains("Access-Control-Allow-Origin: *"));
    }

    #[test]
    fn push_and_poll_update_core_state() {
        let bridge = Bridge::new(BridgeConfig::default());
        bridge.queue_command(Command::Next);
        let poll = bridge.handle_request("GET /poll HTTP/1.1\r\n\r\n");
        assert_eq!(poll.body, b"NEXT");
        let push = bridge.handle_request(
            "GET /push?m=EVENT%20SlideShowBegin%20pos%3D3%20click%3D0 HTTP/1.1\r\n\r\n",
        );
        assert_eq!(push.status, 200);
        let state = bridge.state_snapshot();
        assert_eq!(state.current_page, 3);
        assert!(state.wps_connected);
    }

    #[test]
    fn static_files_are_served_and_traversal_is_rejected() {
        let directory = test_addin_dir();
        let bridge = Bridge::new(BridgeConfig {
            addin_dir: Some(directory.clone()),
            ..BridgeConfig::default()
        });
        let response = bridge.handle_request("GET /js/bridge.js HTTP/1.1\r\n\r\n");
        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, "text/javascript; charset=utf-8");
        assert_eq!(response.body, b"bridge");

        let traversal = bridge.handle_request("GET /../Cargo.toml HTTP/1.1\r\n\r\n");
        assert_eq!(traversal.status, 400);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn static_file_symlinks_cannot_escape_addin_directory() {
        use std::os::unix::fs::symlink;

        let directory = test_addin_dir();
        let outside = directory.parent().unwrap().join(format!(
            "sidera-bridge-outside-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"outside").unwrap();
        symlink(&outside, directory.join("escape")).unwrap();

        let bridge = Bridge::new(BridgeConfig {
            addin_dir: Some(directory.clone()),
            ..BridgeConfig::default()
        });
        let response =
            bridge.handle_request("GET /escape/secret.txt HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert_eq!(response.status, 400);

        fs::remove_dir_all(directory).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn malformed_query_is_not_silently_accepted() {
        let bridge = Bridge::new(BridgeConfig::default());
        let response = bridge.handle_request("GET /push?m=%ZZ HTTP/1.1\r\n\r\n");
        assert_eq!(response.status, 400);
    }
}
