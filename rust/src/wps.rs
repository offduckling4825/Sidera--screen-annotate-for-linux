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
    pub page_index: i32,
    pub page_total: i32,
    pub can_jump: bool,
    pub deck_epoch: i32,
    pub can_start: bool,
    pub start_pending_until: i64,
    pub whiteboard: bool,
    pub queue: VecDeque<(i64, String)>,
    pub command_failed_until: i64,
    pub events: Vec<WpsEvent>,
}

impl Default for WpsShared {
    fn default() -> Self {
        WpsShared {
            connected: false,
            last_seen: 0,
            real_pos: -1,
            page_index: -1,
            page_total: -1,
            can_jump: false,
            deck_epoch: -1,
            can_start: false,
            start_pending_until: 0,
            whiteboard: false,
            queue: VecDeque::new(),
            command_failed_until: 0,
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
    if std::fs::metadata(&home).map(|m| !m.permissions().readonly()).unwrap_or(false) {
        home.join("wps-api-debug.log")
    } else {
        std::path::PathBuf::from("/tmp/wps-api-debug.log")
    }
}

pub fn wps_log(msg: &str) {
    log::info!("[WPSAPI] {}", msg);
    let path = log_file();
    if std::fs::metadata(&path).map(|m| m.len() > 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
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
    cands
        .into_iter()
        .find(|p| p.join("manifest.xml").exists())
}

pub fn ensure_addin_registered() {
    // 与手动安装复用同一条安全合并路径：修复 enable_dev，保留其他加载项，
    // 备份后原子替换；遇到损坏的 XML 不覆盖原文件。
    let Some(dir) = addin_dir() else { return };
    match std::process::Command::new("bash").arg(dir.join("install.sh")).output() {
        Ok(output) if output.status.success() => {
            wps_log(String::from_utf8_lossy(&output.stdout).trim());
        }
        Ok(output) => wps_log(&format!("加载项登记失败：{}", String::from_utf8_lossy(&output.stderr).trim())),
        Err(error) => wps_log(&format!("无法运行加载项登记：{}", error)),
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
    if shared.lock().unwrap().whiteboard {
        return;
    }
    let line = raw.trim();
    if line.is_empty() {
        return;
    }
    if line.starts_with("COMMAND_FAILED ") {
        let mut s = shared.lock().unwrap();
        s.command_failed_until = now_ms() + 3000;
        s.start_pending_until = 0;
        s.queue.clear();
        wps_log(line);
        return;
    }
    if line.starts_with("COMMAND_OK ") {
        wps_log(line);
        return;
    }
    if !line.starts_with("EVENT ") {
        return;
    }
    let pos = extract_num(line, "pos=");
    let name = line.split_whitespace().nth(1).unwrap_or("");
    let mut s = shared.lock().unwrap();
    if name == "SlideShowState" {
        // Old add-ins do not send ready=; they must never advertise Start.
        s.can_start = pos < 1 && extract_num(line, "ready=") == 1;
        // Missing fields (including older add-ins) clear stale document data.
        let page = extract_num(line, "page=");
        let total = extract_num(line, "total=");
        s.page_total = if total > 0 { total } else { -1 };
        s.page_index = if page > 0 && (total < 1 || page <= total) { page } else { -1 };
        s.deck_epoch = extract_num(line, "deck=");
        s.can_jump = extract_num(line, "jump=") == 1 && s.deck_epoch > 0 && page > 0 && total >= page;
        let (can_jump, epoch) = (s.can_jump, s.deck_epoch);
        s.queue.retain(|(_, cmd)| !cmd.starts_with("GOTO ") || (can_jump && jump_command(cmd).is_some_and(|(_, id)| id == epoch)));
    }
    if name == "SlideShowEnd" || (name == "SlideShowState" && pos < 1) {
        if s.real_pos > 0 {
            s.events.push(WpsEvent::SlideshowEnd);
            wps_log("放映结束");
        }
        s.real_pos = -1;
        if name == "SlideShowEnd" { s.can_start = false; s.start_pending_until = 0; s.can_jump = false; }
        let (can_start, can_jump) = (s.can_start, s.can_jump);
        s.queue.retain(|(_, cmd)| (can_start && cmd == "START") || (can_jump && cmd.starts_with("GOTO ")));
        return;
    }
    if pos > 0 && pos != s.real_pos {
        s.can_start = false;
        s.start_pending_until = 0;
        s.queue.retain(|(_, cmd)| cmd != "START");
        if s.real_pos < 1 {
            s.events.push(WpsEvent::SlideshowBegin(pos));
        } else {
            s.events.push(WpsEvent::RealPos(pos));
        }
        s.real_pos = pos;
        wps_log(&format!("事件 {} pos={}", name, pos));
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

fn jump_command(cmd: &str) -> Option<(i32, i32)> {
    let mut words = cmd.split_whitespace();
    if words.next()? != "GOTO" { return None; }
    let page: i32 = words.next()?.parse().ok()?;
    let epoch: i32 = words.next()?.parse().ok()?;
    (page > 0 && epoch > 0 && words.next().is_none()).then_some((page, epoch))
}

fn handle_conn(stream: &mut TcpStream, shared: &Arc<Mutex<WpsShared>>) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    // 丢弃请求头（读一小段，防止连接被半包阻塞）
    let _ = reader.fill_buf();

    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let m = query_m(query);

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
            // 只有带实际放映状态的轮询才算联动就绪。加载静态文件、
            // 启动探针或浏览器预检不能把按钮错误地显示为可用。
            let Some(state) = m.filter(|m| m.starts_with("EVENT SlideShowState pos=")) else {
                reply(stream, b"");
                return;
            };
            handle_line(shared, &state);
            let cmd = {
                let mut s = shared.lock().unwrap();
                if !s.connected { wps_log("WPS 加载项已就绪"); }
                s.connected = true;
                s.last_seen = now_ms();
                s.queue.retain(|(created, _)| now_ms() - created <= 1500);
                let playing = s.real_pos > 0;
                let can_start = s.can_start;
                let (can_jump, epoch, total) = (s.can_jump, s.deck_epoch, s.page_total);
                if s.whiteboard { s.queue.clear(); }
                else { s.queue.retain(|(_, cmd)| {
                    if let Some((page, id)) = jump_command(cmd) { can_jump && epoch == id && page <= total }
                    else if cmd == "START" { can_start && !playing } else { playing }
                }); }
                s.queue.pop_front().map(|(_, cmd)| cmd)
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
        let bridge = Self::from_listener(listener);
        wps_log("WPS 桥已启动，监听 127.0.0.1:16666");
        match addin_dir() {
            Some(d) => wps_log(&format!("加载项目录: {}", d.display())),
            None => wps_log("警告: 未找到 wps-addin 目录（请设置 WPS_ADDIN_DIR）"),
        }
        ensure_addin_registered();
        Some(bridge)
    }

    pub(crate) fn from_listener(listener: TcpListener) -> WpsBridge {
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
        WpsBridge {
            shared,
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn connected(&self) -> bool {
        let s = self.shared.lock().unwrap();
        s.connected && now_ms() - s.last_seen <= 3000
    }

    pub fn ready(&self) -> bool {
        let s = self.shared.lock().unwrap();
        s.connected && now_ms() - s.last_seen <= 3000 && s.real_pos > 0
            && !s.whiteboard && now_ms() >= s.command_failed_until
    }

    pub fn showing(&self) -> bool {
        self.position().is_some()
    }

    pub fn position(&self) -> Option<i32> {
        let s = self.shared.lock().unwrap();
        (s.connected && now_ms() - s.last_seen <= 3000 && s.real_pos > 0 && !s.whiteboard).then_some(s.real_pos)
    }

    pub fn start_available(&self) -> bool {
        let s = self.shared.lock().unwrap();
        s.connected && now_ms() - s.last_seen <= 3000 && s.can_start && s.real_pos < 1 && !s.whiteboard
    }

    pub fn page_progress(&self) -> (i32, i32) {
        let s = self.shared.lock().unwrap();
        if s.connected && now_ms() - s.last_seen <= 3000 && !s.whiteboard {
            (s.page_index, s.page_total)
        } else {
            (-1, -1)
        }
    }

    pub fn enqueue(&self, cmd: &str) -> bool {
        let mut s = self.shared.lock().unwrap();
        let start = cmd == "START";
        if !matches!(cmd, "START" | "NEXT" | "PREV" | "EXIT") || !s.connected
            || now_ms() - s.last_seen > 3000 || s.whiteboard
            || (start && (!s.can_start || s.real_pos > 0 || now_ms() < s.start_pending_until))
            || (!start && s.real_pos < 1)
            || now_ms() < s.command_failed_until {
            return false;
        }
        s.queue.retain(|(created, _)| now_ms() - created <= 1500);
        if s.queue.len() >= 8 { return false; }
        s.queue.push_back((now_ms(), cmd.to_string()));
        if start { s.start_pending_until = now_ms() + 3000; }
        wps_log(&format!("入队指令 {}", cmd));
        true
    }

    pub fn jump_epoch(&self) -> Option<i32> {
        let s = self.shared.lock().unwrap();
        (s.connected && now_ms() - s.last_seen <= 3000 && !s.whiteboard && s.can_jump
            && now_ms() >= s.command_failed_until).then_some(s.deck_epoch)
    }

    pub fn enqueue_jump(&self, page: i32, epoch: i32) -> bool {
        let mut s = self.shared.lock().unwrap();
        if !s.connected || now_ms() - s.last_seen > 3000 || s.whiteboard || !s.can_jump
            || epoch != s.deck_epoch || page < 1 || page > s.page_total || now_ms() < s.command_failed_until {
            return false;
        }
        // Keep only the newest absolute destination; do not replay a trail of
        // intermediate selections after a slow or temporarily blocked poll.
        s.queue.retain(|(created, cmd)| now_ms() - created <= 1500 && !cmd.starts_with("GOTO "));
        if s.queue.len() >= 8 { return false; }
        s.queue.push_back((now_ms(), format!("GOTO {page} {epoch}")));
        true
    }

    pub fn set_whiteboard(&self, on: bool) {
        let mut s = self.shared.lock().unwrap();
        if s.whiteboard != on {
            s.whiteboard = on;
            s.page_index = -1;
            s.page_total = -1;
            s.can_start = false;
            s.start_pending_until = 0;
            s.queue.clear();
            s.events.clear();
            s.can_jump = false;
            s.deck_epoch = -1;
        }
    }

    pub fn take_events(&self) -> Vec<WpsEvent> {
        let mut s = self.shared.lock().unwrap();
        std::mem::take(&mut s.events)
    }

    /// 心跳检查：3 秒无请求判离线
    pub fn tick(&self) -> bool {
        let mut s = self.shared.lock().unwrap();
        if s.connected && now_ms() - s.last_seen > 3000 {
            s.connected = false;
            s.can_jump = false;
            s.can_start = false;
            s.start_pending_until = 0;
            s.queue.clear();
            // 保留已知页号，短暂断线后重连同一页不清除已有墨迹。
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

    #[test]
    fn absolute_jump_requires_fresh_capability_and_replaces_stale_destinations() {
        let bridge = WpsBridge::from_listener(TcpListener::bind(("127.0.0.1", 0)).unwrap());
        {
            let mut s = bridge.shared.lock().unwrap();
            s.connected = true; s.last_seen = now_ms();
        }
        handle_line(&bridge.shared, "EVENT SlideShowState pos=-1 page=3 total=20 jump=1 deck=90");
        assert!(bridge.enqueue_jump(8, 90));
        assert!(bridge.enqueue_jump(12, 90));
        assert!(!bridge.enqueue_jump(21, 90));
        assert!(!bridge.enqueue_jump(5, 89));
        assert_eq!(bridge.shared.lock().unwrap().queue.len(), 1);
        assert_eq!(bridge.shared.lock().unwrap().queue[0].1, "GOTO 12 90");
        handle_line(&bridge.shared, "EVENT SlideShowState pos=-1 page=3 total=20 jump=1 deck=91");
        assert!(bridge.shared.lock().unwrap().queue.is_empty());
        assert!(!bridge.enqueue_jump(5, 90));
        handle_line(&bridge.shared, "EVENT SlideShowState pos=3 page=3 total=20");
        assert_eq!(bridge.jump_epoch(), None, "older add-ins cannot advertise an unsupported command");
        assert!(!bridge.enqueue_jump(5, 91));
    }
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
        // 临时端口、不运行注册器：测试不能占用生产桥或修改用户 WPS 配置。
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let bridge = WpsBridge::from_listener(listener);
        let hello = request(port, "GET /hello?m=sidera-bridge HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        assert!(hello.contains("OK sidera"), "hello 响应异常: {hello:?}");
        assert!(!bridge.connected());
        assert!(!bridge.enqueue("NEXT"));

        let push = request(
            port,
            "GET /push?m=EVENT%20SlideShowBegin%20pos=3%20click=0 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        );
        assert!(push.contains("OK"), "push 响应异常: {push:?}");

        let poll_request = "GET /poll?m=EVENT%20SlideShowState%20pos=3%20click=0 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        request(port, poll_request);
        assert!(bridge.ready());
        assert!(bridge.enqueue("NEXT"));
        let poll = request(port, poll_request);
        assert!(poll.contains("NEXT"), "poll 响应异常: {poll:?}");

        // 事件应被记录
        let events = bridge.take_events();
        assert!(
            events.iter().any(|e| matches!(e, WpsEvent::SlideshowBegin(3))),
            "未收到 SlideshowBegin 事件: {events:?}"
        );

        // 静态加载项分发（能找到 wps-addin 目录时）
        if addin_dir().is_some() {
            let m = request(
                port,
                "GET /manifest.xml HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
            );
            assert!(
                m.contains("JsPlugin") || m.to_lowercase().contains("manifest"),
                "静态 manifest 分发异常: {m:?}"
            );
        }

        assert!(bridge.enqueue("PREV"));
        bridge.shared.lock().unwrap().queue.front_mut().unwrap().0 -= 1600;
        assert!(!request(port, poll_request).contains("PREV"), "过期指令不得补执行");
        assert!(bridge.enqueue("EXIT"));
        request(port, "GET /poll?m=EVENT%20SlideShowState%20pos=-1%20click=-1 HTTP/1.1\r\nHost: x\r\n\r\n");
        assert!(!bridge.ready());
        assert!(bridge.shared.lock().unwrap().queue.is_empty());
        assert!(bridge.take_events().iter().any(|e| matches!(e, WpsEvent::SlideshowEnd)));

        // 回归：Drop 停止线程并释放端口。
        drop(bridge);
        TcpListener::bind(("127.0.0.1", port)).expect("关闭桥后端口未释放");
    }

    #[test]
    fn readiness_disconnect_whiteboard_and_command_failure() {
        let bridge = WpsBridge::from_listener(TcpListener::bind(("127.0.0.1", 0)).unwrap());
        {
            let mut s = bridge.shared.lock().unwrap();
            s.connected = true; s.last_seen = now_ms(); s.real_pos = 2;
        }
        assert!(bridge.enqueue("NEXT"));
        bridge.shared.lock().unwrap().last_seen -= 4000;
        assert!(!bridge.ready());
        assert!(!bridge.enqueue("NEXT")); // 不依赖 UI 心跳定时器及时运行。
        assert!(bridge.tick());
        assert!(bridge.shared.lock().unwrap().queue.is_empty());
        {
            let mut s = bridge.shared.lock().unwrap();
            s.connected = true; s.last_seen = now_ms();
        }
        assert!(bridge.enqueue("PREV"));
        bridge.set_whiteboard(true);
        assert!(!bridge.ready());
        assert!(bridge.shared.lock().unwrap().queue.is_empty());
        bridge.set_whiteboard(false);
        assert!(bridge.ready());
        handle_line(&bridge.shared, "COMMAND_FAILED NEXT");
        assert!(!bridge.ready());
        assert!(!bridge.enqueue("NEXT"));
    }

    #[test]
    fn repeated_state_and_animation_do_not_switch_pages() {
        let shared = Arc::new(Mutex::new(WpsShared::default()));
        handle_line(&shared, "EVENT SlideShowState pos=1 click=0");
        handle_line(&shared, "EVENT SlideShowBegin pos=1 click=0");
        handle_line(&shared, "EVENT SlideShowState pos=1 click=1");
        assert_eq!(shared.lock().unwrap().events.len(), 1);
        handle_line(&shared, "EVENT SlideShowState pos=2 click=0");
        assert!(matches!(shared.lock().unwrap().events.last(), Some(WpsEvent::RealPos(2))));
        handle_line(&shared, "EVENT SlideShowEnd pos=-1 click=-1");
        handle_line(&shared, "EVENT SlideShowState pos=-1 click=-1");
        assert_eq!(shared.lock().unwrap().events.len(), 3);
        handle_line(&shared, "EVENT SlideShowState pos=2 click=0");
        assert!(matches!(shared.lock().unwrap().events.last(), Some(WpsEvent::SlideshowBegin(2))));
    }

    #[test]
    fn page_counters_are_separate_from_show_order_and_expire_with_the_link() {
        let bridge = WpsBridge::from_listener(TcpListener::bind(("127.0.0.1", 0)).unwrap());
        {
            let mut s = bridge.shared.lock().unwrap();
            s.connected = true; s.last_seen = now_ms();
        }
        handle_line(&bridge.shared, "EVENT SlideShowState pos=2 click=0 page=9 total=30");
        assert_eq!(bridge.page_progress(), (9, 30));
        assert_eq!(bridge.position(), Some(2));
        assert!(matches!(bridge.take_events().as_slice(), [WpsEvent::SlideshowBegin(2)]));
        handle_line(&bridge.shared, "EVENT SlideShowState pos=2 click=1 page=9 total=31");
        assert_eq!(bridge.page_progress(), (9, 31));
        assert!(bridge.take_events().is_empty(), "counter changes must not clear annotation pages");
        handle_line(&bridge.shared, "EVENT SlideShowState pos=-1 click=-1 ready=1 page=6 total=31");
        assert_eq!(bridge.page_progress(), (6, 31), "editing still has a real current slide");
        assert_eq!(bridge.position(), None);
        bridge.shared.lock().unwrap().last_seen -= 4000;
        assert_eq!(bridge.page_progress(), (-1, -1), "do not display stale metadata while offline");
        bridge.shared.lock().unwrap().last_seen = now_ms();
        bridge.set_whiteboard(true);
        handle_line(&bridge.shared, "EVENT SlideShowState pos=3 click=0 page=10 total=31");
        bridge.set_whiteboard(false);
        assert_eq!(bridge.page_progress(), (-1, -1), "wait for fresh data after leaving whiteboard");
        handle_line(&bridge.shared, "EVENT SlideShowState pos=3 click=0 page=10 total=31");
        assert_eq!(bridge.page_progress(), (10, 31));
        handle_line(&bridge.shared, "EVENT SlideShowState pos=3 click=0");
        assert_eq!(bridge.page_progress(), (-1, -1), "older add-ins clear previous counters");
        handle_line(&bridge.shared, "EVENT SlideShowState pos=3 click=0 page=32 total=31");
        assert_eq!(bridge.page_progress(), (-1, 31));
    }

    #[test]
    fn registration_files_and_preflight_do_not_establish_a_connection() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let bridge = WpsBridge::from_listener(listener);
        for req in ["GET /ribbon.xml", "OPTIONS /poll", "GET /poll", "GET /hello"] {
            request(port, &format!("{req} HTTP/1.1\r\nHost: x\r\n\r\n"));
            assert!(!bridge.connected());
            assert!(!bridge.ready());
        }
    }

    #[test]
    fn start_requires_a_fresh_ready_deck_and_survives_idle_poll() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let bridge = WpsBridge::from_listener(listener);
        let ready = "GET /poll?m=EVENT%20SlideShowState%20pos=-1%20click=-1%20ready=1 HTTP/1.1\r\nHost: x\r\n\r\n";
        assert!(!bridge.start_available());
        assert!(!bridge.enqueue("START"));
        request(port, ready);
        assert!(bridge.start_available());
        assert!(!bridge.enqueue("NEXT"));
        assert!(bridge.enqueue("START"));
        assert!(!bridge.enqueue("START"));
        assert!(request(port, ready).ends_with("START"));
        assert!(!bridge.enqueue("START"), "repeated taps must wait for the show or timeout");
        handle_line(&bridge.shared, "EVENT SlideShowState pos=1 click=0 ready=0");
        assert!(!bridge.start_available());
        assert!(bridge.ready());
        assert!(!bridge.enqueue("START"));
        handle_line(&bridge.shared, "EVENT SlideShowState pos=-1 click=-1 ready=1");
        assert!(bridge.enqueue("START"));
        request(port, "GET /poll?m=EVENT%20SlideShowState%20pos=-1%20click=-1 HTTP/1.1\r\nHost: x\r\n\r\n");
        assert!(!bridge.start_available(), "old add-ins never advertise START");
        assert!(bridge.shared.lock().unwrap().queue.is_empty());
        request(port, ready);
        bridge.shared.lock().unwrap().last_seen -= 4000;
        assert!(!bridge.start_available());
        assert!(!bridge.enqueue("START"));
    }
}
