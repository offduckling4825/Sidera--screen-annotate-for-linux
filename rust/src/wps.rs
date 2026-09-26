// ============================================================
// Sidera - WPS 联动桥（本地 HTTP 127.0.0.1:16666 + 加载项注册 + 心跳）
// 平台无关；在独立线程运行 TCP 服务，主线程轮询共享状态
// ============================================================
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::now_ms;

const MAX_REQUEST_LINE: usize = 8 * 1024;
const MAX_HEADER_LINE: usize = 8 * 1024;
const MAX_HEADERS: usize = 64;
const MAX_QUERY_VALUE: usize = 16 * 1024;

fn lock_shared(shared: &Arc<Mutex<WpsShared>>) -> std::sync::MutexGuard<'_, WpsShared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone, Debug)]
pub enum WpsEvent {
    SlideshowBegin(i32),
    SlideshowEnd,
    RealPos(i32),
}

pub struct WpsShared {
    pub connected: bool,
    pub last_seen: i64,
    pub real_pos: i32,
    pub whiteboard: bool,
    pub queue: VecDeque<String>,
    pub events: Vec<WpsEvent>,
}

impl Default for WpsShared {
    fn default() -> Self {
        WpsShared {
            connected: false,
            last_seen: 0,
            real_pos: -1,
            whiteboard: false,
            queue: VecDeque::new(),
            events: Vec::new(),
        }
    }
}

pub struct WpsBridge {
    pub shared: Arc<Mutex<WpsShared>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for WpsBridge {
    fn drop(&mut self) {
        // 通知服务线程退出并等待其结束，确保 16666 端口被释放，
        // 否则关闭 WPS 调试后再打开会因端口被旧线程占用而无法重新绑定。
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

pub fn log_file() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    if std::fs::metadata(&home)
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false)
    {
        home.join("wps-api-debug.log")
    } else {
        std::path::PathBuf::from("/tmp/wps-api-debug.log")
    }
}

pub fn wps_log(msg: &str) {
    log::info!("[WPSAPI] {}", msg);
    let path = log_file();
    if std::fs::metadata(&path)
        .map(|m| m.len() > 1024 * 1024)
        .unwrap_or(false)
    {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{} {}", now_ms(), msg);
    }
}

fn addin_dir() -> Option<std::path::PathBuf> {
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(env) = std::env::var("WPS_ADDIN_DIR") {
        cands.push(std::path::PathBuf::from(env));
    }
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd.join("wps-addin"));
        cands.push(cwd.join("../wps-addin"));
        cands.push(cwd.join("../../wps-addin"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            cands.push(dir.join("wps-addin"));
            cands.push(dir.join("../wps-addin"));
            cands.push(dir.join("../../wps-addin"));
            cands.push(dir.join("../../../wps-addin"));
            cands.push(dir.join("../../../../wps-addin"));
            cands.push(dir.join("../../../../../wps-addin"));
        }
    }
    cands.push(std::path::PathBuf::from("/usr/share/sidera/wps-addin"));
    cands.into_iter().find(|p| p.join("manifest.xml").exists())
}

pub fn ensure_addin_registered() {
    let Some(home) = dirs::home_dir() else { return };
    let path = home.join(".local/share/Kingsoft/wps/jsaddons/publish.xml");
    let entry = "  <jspluginonline name=\"sidera-bridge\" type=\"wpp\" url=\"http://127.0.0.1:16666/\" debug=\"\" enable=\"enable\" install=\"null\"/>\n";
    let content = std::fs::read_to_string(&path).unwrap_or_default();

    // 去掉旧的 screen-annotate-bridge 条目（它常是 enable_dev，WPS 正常模式不加载）
    let mut working = content.clone();
    if working.contains("screen-annotate-bridge") {
        working = working
            .lines()
            .filter(|l| !l.contains("screen-annotate-bridge"))
            .collect::<Vec<_>>()
            .join("\n");
        if !working.ends_with('\n') {
            working.push('\n');
        }
    }

    let has_sidera = working.contains("sidera-bridge");
    let new = if has_sidera {
        working
    } else if working.contains("<jsplugins>") && working.contains("</jsplugins>") {
        working.replace("</jsplugins>", &format!("{}</jsplugins>", entry))
    } else {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<jsplugins>\n{}</jsplugins>\n",
            entry
        )
    };

    if new == content {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&path, new).is_ok() {
        wps_log(&format!("已自动登记/规范化加载项 → {}", path.display()));
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = (b[i + 1] as char).to_digit(16);
            let lo = (b[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        if b[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn query_m(query: &str) -> Option<String> {
    for kv in query.split('&') {
        if let Some(v) = kv.strip_prefix("m=") {
            if v.len() > MAX_QUERY_VALUE {
                return None;
            }
            return Some(percent_decode(v));
        }
    }
    None
}

fn reply(stream: &mut TcpStream, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn reply_file(stream: &mut TcpStream, path: &str) {
    let Some(dir) = addin_dir() else {
        reply(stream, b"OK");
        return;
    };
    let mut rel = path.trim_start_matches('/').to_string();
    if rel.is_empty() {
        rel = "manifest.xml".to_string();
    }
    if rel.contains("..") || rel.contains("//") {
        reply(stream, b"OK");
        return;
    }
    let fp = dir.join(&rel);
    let Ok(body) = std::fs::read(&fp) else {
        reply(stream, b"OK");
        return;
    };
    let ct = if rel.ends_with(".xml") {
        "application/xml"
    } else if rel.ends_with(".js") {
        "text/javascript"
    } else if rel.ends_with(".html") || rel.ends_with(".htm") {
        "text/html"
    } else if rel.ends_with(".svg") {
        "image/svg+xml"
    } else if rel.ends_with(".json") {
        "application/json"
    } else {
        "text/plain"
    };
    let header = format!(
        "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store, no-cache, must-revalidate\r\nPragma: no-cache\r\nExpires: 0\r\nContent-Type: {}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        ct,
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&body);
    let _ = stream.flush();
}

fn handle_line(shared: &Arc<Mutex<WpsShared>>, raw: &str) {
    // 白板模式：与外界完全隔离，忽略 WPS 事件（与 C++ 版一致）
    if lock_shared(shared).whiteboard {
        return;
    }
    let line = raw.trim();
    if line.is_empty() {
        return;
    }
    wps_log(&format!("收 << {}", line));
    if !line.starts_with("EVENT ") {
        return;
    }
    let pos = extract_num(line, "pos=");
    let click = extract_num(line, "click=");
    let name = line.split_whitespace().nth(1).unwrap_or("");
    wps_log(&format!("事件 {} pos={} click={}", name, pos, click));
    let mut s = lock_shared(shared);
    if name == "SlideShowBegin" {
        let p = if pos > 0 { pos } else { 1 };
        s.real_pos = p;
        s.events.push(WpsEvent::SlideshowBegin(p));
        return;
    }
    if name == "SlideShowEnd" {
        s.real_pos = -1;
        s.events.push(WpsEvent::SlideshowEnd);
        return;
    }
    if pos > 0 && pos != s.real_pos {
        s.real_pos = pos;
        s.events.push(WpsEvent::RealPos(pos));
    }
}

fn extract_num(s: &str, key: &str) -> i32 {
    if let Some(i) = s.find(key) {
        let rest = &s[i + key.len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        return digits.parse().unwrap_or(-1);
    }
    -1
}

fn handle_conn(stream: &mut TcpStream, shared: &Arc<Mutex<WpsShared>>) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err()
        || line.len() > MAX_REQUEST_LINE
        || !line.ends_with("\r\n")
    {
        return;
    }
    let mut header_count = 0;
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) => return,
            Ok(n) if n > MAX_HEADER_LINE => return,
            Ok(_) if header == "\r\n" => break,
            Ok(_) => {
                if !header.ends_with("\r\n") || !header.contains(':') {
                    return;
                }
                header_count += 1;
                if header_count > MAX_HEADERS {
                    return;
                }
            }
            Err(_) => return,
        }
    }

    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("");
    if parts.next().is_some() || version != "HTTP/1.1" || target.is_empty() {
        reply(stream, b"Bad Request");
        return;
    }
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let m = query_m(query);

    {
        let mut s = lock_shared(shared);
        s.last_seen = now_ms();
        let newly = !s.connected;
        if newly {
            s.connected = true;
        }
        drop(s);
        if newly {
            wps_log("客户端接入");
        }
    }

    if method == "OPTIONS" {
        reply(stream, b"");
        return;
    }
    match path {
        "/hello" => {
            wps_log(&format!("加载项问候: {}", m.unwrap_or_default()));
            reply(stream, b"OK sidera");
        }
        "/push" => {
            if let Some(m) = m {
                handle_line(shared, &m);
            }
            reply(stream, b"OK");
        }
        "/poll" => {
            let cmd = {
                let mut s = lock_shared(shared);
                s.queue.pop_front()
            };
            if let Some(c) = &cmd {
                wps_log(&format!("加载项取走 {}", c));
            }
            reply(stream, cmd.unwrap_or_default().as_bytes());
        }
        _ => reply_file(stream, path),
    }
}

impl WpsBridge {
    pub fn start() -> Option<WpsBridge> {
        let listener = TcpListener::bind(("127.0.0.1", 16666)).ok()?;
        let _ = listener.set_nonblocking(true);
        let shared = Arc::new(Mutex::new(WpsShared::default()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let s2 = shared.clone();
        let sd = shutdown.clone();
        let handle = std::thread::spawn(move || {
            while !sd.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut st, _)) => {
                        let _ = st.set_read_timeout(Some(Duration::from_secs(1)));
                        handle_conn(&mut st, &s2);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
            // listener 随线程结束被 drop，端口在此释放
        });
        wps_log("WPS 桥已启动，监听 127.0.0.1:16666");
        match addin_dir() {
            Some(d) => wps_log(&format!("加载项目录: {}", d.display())),
            None => {
                wps_log("警告: 未找到 wps-addin 目录，静态分发将返回空（请设置 WPS_ADDIN_DIR）")
            }
        }
        ensure_addin_registered();
        Some(WpsBridge {
            shared,
            shutdown,
            handle: Some(handle),
        })
    }

    pub fn connected(&self) -> bool {
        lock_shared(&self.shared).connected
    }

    pub fn enqueue(&self, cmd: &str) {
        lock_shared(&self.shared).queue.push_back(cmd.to_string());
        wps_log(&format!("入队指令 {}", cmd));
    }

    pub fn set_whiteboard(&self, on: bool) {
        lock_shared(&self.shared).whiteboard = on;
    }

    pub fn take_events(&self) -> Vec<WpsEvent> {
        let mut s = lock_shared(&self.shared);
        std::mem::take(&mut s.events)
    }

    /// 心跳检查：3 秒无请求判离线
    pub fn tick(&self) -> bool {
        let mut s = lock_shared(&self.shared);
        if s.connected && now_ms() - s.last_seen > 3000 {
            s.connected = false;
            // 不再清 real_pos（避免重连把同页误判为换页）
            drop(s);
            wps_log("客户端离线（3s 无请求）");
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn request(port: u16, req: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        s.write_all(req.as_bytes()).unwrap();
        s.flush().unwrap();
        let mut buf = String::new();
        let _ = s.read_to_string(&mut buf);
        buf
    }

    #[test]
    fn bridge_endpoints() {
        // 用测试端口避免与真实 16666 冲突：直接调用内部逻辑需要端口参数，
        // 这里复用 start()（16666）；若已被占用则跳过。
        let Some(bridge) = WpsBridge::start() else {
            eprintln!("16666 已被占用，跳过");
            return;
        };
        // 轮询直到服务就绪
        let mut ready = false;
        for _ in 0..50 {
            if TcpStream::connect(("127.0.0.1", 16666)).is_ok() {
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(ready, "服务未就绪");

        let hello = request(
            16666,
            "GET /hello?m=sidera-bridge HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        );
        assert!(hello.contains("OK sidera"), "hello 响应异常: {hello:?}");

        let push = request(
            16666,
            "GET /push?m=EVENT%20SlideShowBegin%20pos=3%20click=0 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        );
        assert!(push.contains("OK"), "push 响应异常: {push:?}");

        // 入队 NEXT 后 /poll 应返回 NEXT
        bridge.enqueue("NEXT");
        let poll = request(
            16666,
            "GET /poll HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        );
        assert!(poll.contains("NEXT"), "poll 响应异常: {poll:?}");

        // 事件应被记录
        let events = bridge.take_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, WpsEvent::SlideshowBegin(3))),
            "未收到 SlideshowBegin 事件: {events:?}"
        );

        // 静态加载项分发（能找到 wps-addin 目录时）
        if addin_dir().is_some() {
            let m = request(
                16666,
                "GET /manifest.xml HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
            );
            assert!(
                m.contains("JsPlugin") || m.to_lowercase().contains("manifest"),
                "静态 manifest 分发异常: {m:?}"
            );
        }

        // 回归：关闭后必须能重新绑定 16666（Drop 需停止线程并释放端口）
        drop(bridge);
        let restarted = WpsBridge::start();
        assert!(
            restarted.is_some(),
            "关闭 WPS 桥后无法重新启动：端口/线程未释放"
        );
    }
}
