//! Match and terminate user-selected processes after Sidera starts.
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: i32,
    pub name: String,
    pub exe: String,
    pub cmdline: String,
    pub state: char,
    pub start_time: u64,
}

pub fn rules_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("sidera/blocked-processes"))
}

pub fn load_rules() -> Vec<String> {
    let Some(path) = rules_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut rules: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.chars().count() > 256 {
            continue;
        }
        if !rules.iter().any(|rule| rule == line) {
            rules.push(line.to_string());
        }
        if rules.len() >= 64 {
            break;
        }
    }
    rules
}

pub fn save_rules(rules: &[String]) -> std::io::Result<()> {
    let Some(path) = rules_path() else {
        return Err(std::io::Error::other("无法确定配置目录"));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = String::from("# Sidera 进程屏蔽规则（一行一个进程名或可执行文件路径）\n");
    for rule in rules.iter().take(64) {
        let rule = rule.trim();
        if !rule.is_empty() {
            text.push_str(rule);
            text.push('\n');
        }
    }
    crate::persistence::atomic_write(&path, text.as_bytes())
}

#[cfg(unix)]
pub fn scan() -> Vec<ProcInfo> {
    let uid = unsafe { libc::geteuid() };
    let self_pid = std::process::id() as i32;
    let parent = unsafe { libc::getppid() } as i32;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Ok(pid) = name.parse::<i32>() else {
            continue;
        };
        if pid <= 1 || pid == self_pid || pid == parent {
            continue;
        }
        let dir = entry.path();
        let Ok(meta) = std::fs::metadata(&dir) else {
            continue;
        };
        if meta.uid() != uid {
            continue;
        }
        let comm = std::fs::read_to_string(dir.join("comm")).unwrap_or_default();
        let name = comm.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let exe = std::fs::read_link(dir.join("exe"))
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let cmdline = std::fs::read(dir.join("cmdline"))
            .ok()
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .replace('\0', " ")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        let (state, start_time) = read_process_stat(pid).unwrap_or(('?', 0));
        out.push(ProcInfo {
            pid,
            name,
            exe,
            cmdline,
            state,
            start_time,
        });
    }
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.pid.cmp(&b.pid))
    });
    out
}

#[cfg(windows)]
mod windows_proc {
    use super::ProcInfo;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SUSPEND_RESUME,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn image_name(pid: u32) -> Option<String> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() { return None; }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(process);
            (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    pub(super) fn creation_time(pid: u32) -> Option<u64> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() { return None; }
            let mut created = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let mut exited = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let mut kernel = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let mut user = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let ok = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user);
            CloseHandle(process);
            (ok != 0).then(|| ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
        }
    }

    pub(super) fn scan() -> Vec<ProcInfo> {
        let mut out = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE { return out; }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let self_pid = std::process::id();
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    let pid = entry.th32ProcessID;
                    if pid > 4 && pid != self_pid {
                        let file = String::from_utf16_lossy(
                            &entry.szExeFile.iter().take_while(|&&ch| ch != 0).copied().collect::<Vec<_>>()
                        );
                        let name = file.strip_suffix(".exe")
                            .or_else(|| file.strip_suffix(".EXE"))
                            .unwrap_or(&file).to_string();
                        let exe = image_name(pid).unwrap_or_default();
                        let start_time = creation_time(pid).unwrap_or(0);
                        out.push(ProcInfo {
                            pid: pid as i32, name, exe, cmdline: String::new(),
                            state: 'R', start_time,
                        });
                    }
                    if Process32NextW(snapshot, &mut entry) == 0 { break; }
                }
            }
            CloseHandle(snapshot);
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.pid.cmp(&b.pid)));
        out
    }

    fn nt_call(pid: u32, suspend: bool) -> bool {
        type NtProcess = unsafe extern "system" fn(HANDLE) -> i32;
        unsafe {
            let module = GetModuleHandleW(wide("ntdll.dll").as_ptr());
            if module.is_null() { return false; }
            let name: &[u8] = if suspend { b"NtSuspendProcess\0" } else { b"NtResumeProcess\0" };
            let Some(address) = GetProcAddress(module, name.as_ptr()) else { return false; };
            let function: NtProcess = std::mem::transmute(address);
            let process = OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid);
            if process.is_null() { return false; }
            let status = function(process);
            CloseHandle(process);
            status >= 0
        }
    }

    pub(super) fn suspend(pid: i32) -> bool { nt_call(pid as u32, true) }
    pub(super) fn resume(pid: i32) -> bool { nt_call(pid as u32, false) }
}

#[cfg(windows)]
pub fn scan() -> Vec<ProcInfo> {
    windows_proc::scan()
}

#[cfg(windows)]
fn suspend_process(proc: &ProcInfo) -> bool {
    windows_proc::suspend(proc.pid)
}

#[cfg(windows)]
fn resume_process(pid: i32, start_time: u64) -> bool {
    if start_time != 0 && windows_proc::creation_time(pid as u32) != Some(start_time) {
        return false;
    }
    windows_proc::resume(pid)
}

#[cfg(unix)]
fn read_process_stat(pid: i32) -> Option<(char, u64)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = text.rfind(')')?;
    let rest = text.get(close + 2..)?;
    let mut fields = rest.split_whitespace();
    let state = fields.next()?.chars().next()?;
    let start_time = fields.nth(18)?.parse().ok()?;
    Some((state, start_time))
}

#[cfg(unix)]
fn send_signal(pid: i32, signal: i32) -> bool {
    unsafe { libc::kill(pid, signal) == 0 }
}

#[cfg(unix)]
fn suspend_process(proc: &ProcInfo) -> bool {
    send_signal(proc.pid, libc::SIGSTOP)
}

#[cfg(unix)]
fn resume_process(pid: i32, start_time: u64) -> bool {
    match read_process_stat(pid) {
        Some((_, current)) if current == start_time => send_signal(pid, libc::SIGCONT),
        _ => false,
    }
}

fn exe_name(proc: &ProcInfo) -> &str {
    proc.exe.rsplit(['/', '\\']).next().unwrap_or("")
}

pub fn matches(rule: &str, proc: &ProcInfo) -> bool {
    let rule = rule.trim();
    if rule.is_empty() {
        return false;
    }
    if rule.eq_ignore_ascii_case(&proc.name) || rule.eq_ignore_ascii_case(exe_name(proc)) {
        return true;
    }
    if (rule.contains('/') || rule.contains('\\')) && rule.eq_ignore_ascii_case(&proc.exe) {
        return true;
    }
    proc.cmdline.split_whitespace().next().is_some_and(|first| {
        first
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(rule))
    })
}

fn blocker_state_path() -> Option<PathBuf> {
    dirs::state_dir().or_else(dirs::config_dir).map(|dir| dir.join("sidera/blocker-state"))
}

fn read_blocker_state() -> Vec<(i32, u64)> {
    let Some(path) = blocker_state_path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let start = fields.next()?.parse().ok()?;
            Some((pid, start))
        })
        .collect()
}

fn write_blocker_state(entries: &[(i32, u64)]) {
    let Some(path) = blocker_state_path() else { return };
    let mut text = String::new();
    for (pid, start) in entries {
        text.push_str(&format!("{pid} {start}\n"));
    }
    let _ = crate::persistence::atomic_write(&path, text.as_bytes());
}

#[cfg(unix)]
static BLOCKER_STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(unix)]
static BLOCKER_RELOAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(unix)]
static BLOCKER_CHILD: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

#[cfg(unix)]
extern "C" fn blocker_stop(_: libc::c_int) {
    BLOCKER_STOP.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(unix)]
extern "C" fn blocker_reload(_: libc::c_int) {
    BLOCKER_RELOAD.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Suspend matching user processes while Sidera is running. Started by the
/// systemd user unit `sidera-blocker.service`, or as a child when unavailable.
#[cfg(unix)]
pub fn run_blocker() -> i32 {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .try_init();
    unsafe {
        libc::signal(libc::SIGTERM, blocker_stop as *const () as usize);
        libc::signal(libc::SIGINT, blocker_stop as *const () as usize);
        libc::signal(libc::SIGHUP, blocker_reload as *const () as usize);
    }
    BLOCKER_STOP.store(false, Ordering::Relaxed);
    BLOCKER_RELOAD.store(false, Ordering::Relaxed);

    // A previous crash may have left processes suspended. Resume them before
    // re-evaluating the current rules.
    let stale = read_blocker_state();
    for &(pid, start) in &stale {
        if resume_process(pid, start) {
            crate::diag_log::info(&format!("进程屏蔽：恢复上次暂停的进程 pid {pid}"));
        }
    }
    write_blocker_state(&[]);
    crate::diag_log::info("进程屏蔽守护进程已启动（暂停模式）");

    let mut suspended: Vec<(i32, u64)> = Vec::new();
    let started = Instant::now();
    let mut last_watch = Instant::now();
    let mut missing = 0u32;
    loop {
        if BLOCKER_STOP.load(Ordering::Relaxed) {
            break;
        }
        if BLOCKER_RELOAD.swap(false, Ordering::Relaxed) {
            log::info!("进程屏蔽规则已重新加载");
        }
        let rules = load_rules();
        let processes = scan();
        let mut changed = false;
        let mut index = 0;
        while index < suspended.len() {
            let (pid, start) = suspended[index];
            let Some(proc) = processes.iter().find(|p| p.pid == pid && p.start_time == start) else {
                suspended.remove(index);
                changed = true;
                continue;
            };
            if !rules.iter().any(|rule| matches(rule, proc)) {
                if resume_process(pid, start) {
                    crate::diag_log::info(&format!("进程屏蔽：恢复 {} (pid {pid})", proc.name));
                }
                suspended.remove(index);
                changed = true;
                continue;
            }
            if proc.state != 'T' && suspend_process(proc) {
                crate::diag_log::info(&format!("进程屏蔽：重新暂停 {} (pid {pid})", proc.name));
            }
            index += 1;
        }
        for proc in &processes {
            if proc.name.eq_ignore_ascii_case("sidera") {
                continue;
            }
            if !rules.iter().any(|rule| matches(rule, proc)) {
                continue;
            }
            if suspended.iter().any(|(pid, _)| *pid == proc.pid) {
                continue;
            }
            if proc.state == 'T' {
                continue;
            }
            if suspend_process(proc) {
                suspended.push((proc.pid, proc.start_time));
                changed = true;
                crate::diag_log::info(&format!("进程屏蔽：已暂停 {} (pid {})", proc.name, proc.pid));
            } else {
                crate::diag_log::warn(&format!("进程屏蔽：无法暂停 {} (pid {})", proc.name, proc.pid));
            }
        }
        if changed {
            write_blocker_state(&suspended);
        }
        if started.elapsed() >= Duration::from_secs(3) && last_watch.elapsed() >= Duration::from_secs(2) {
            last_watch = Instant::now();
            if crate::instance::running() {
                missing = 0;
            } else {
                missing += 1;
                if missing >= 2 {
                    log::info!("Sidera 已退出，进程屏蔽守护进程结束");
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    for &(pid, start) in &suspended {
        resume_process(pid, start);
    }
    write_blocker_state(&[]);
    crate::diag_log::info("进程屏蔽守护进程已停止，已恢复暂停的进程");
    0
}

#[cfg(unix)]
pub fn set_daemon_enabled(enabled: bool) {
    use std::process::{Command, Stdio};
    if enabled {
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match Command::new("systemctl")
            .args(["--user", "start", "sidera-blocker.service"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
        {
            Ok(output) if output.status.success() => {
                crate::diag_log::info("进程屏蔽守护进程已启动（systemd 用户服务）");
                return;
            }
            Ok(output) => log::warn!(
                "systemd 用户服务启动失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            Err(error) => log::warn!("无法调用 systemctl --user：{error}"),
        }
        if let Ok(mut slot) = BLOCKER_CHILD.lock() {
            if slot.is_none() {
                if let Ok(exe) = std::env::current_exe() {
                    match Command::new(exe)
                        .arg("--blocker")
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                    {
                        Ok(child) => {
                            *slot = Some(child);
                            crate::diag_log::info("进程屏蔽守护进程已启动（子进程回退）");
                        }
                        Err(error) => crate::diag_log::warn(&format!(
                            "无法启动进程屏蔽守护进程：{error}"
                        )),
                    }
                }
            }
        }
    } else {
        let _ = Command::new("systemctl")
            .args(["--user", "stop", "sidera-blocker.service"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Ok(mut slot) = BLOCKER_CHILD.lock() {
            if let Some(mut child) = slot.take() {
                unsafe { libc::kill(child.id() as i32, libc::SIGTERM); }
                let _ = child.wait();
                crate::diag_log::info("进程屏蔽守护进程已停止");
            }
        }
    }
}

#[cfg(windows)]
static WIN_BLOCKER_STOP: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
static WIN_BLOCKER: std::sync::Mutex<Option<std::thread::JoinHandle<()>>> =
    std::sync::Mutex::new(None);

#[cfg(windows)]
fn win_blocker_loop() {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    // A previous crash may have left processes suspended.
    for (pid, start) in read_blocker_state() {
        if resume_process(pid, start) {
            crate::diag_log::info(&format!("进程屏蔽：恢复上次暂停的进程 pid {pid}"));
        }
    }
    write_blocker_state(&[]);

    let mut suspended: Vec<(i32, u64)> = Vec::new();
    loop {
        if WIN_BLOCKER_STOP.load(Ordering::Relaxed) {
            break;
        }
        let rules = load_rules();
        let processes = scan();
        let mut changed = false;
        let mut index = 0;
        while index < suspended.len() {
            let (pid, start) = suspended[index];
            let current = processes.iter().find(|proc|
                proc.pid == pid && (start == 0 || proc.start_time == start));
            let keep = current.is_some_and(|proc| rules.iter().any(|rule| matches(rule, proc)));
            if keep {
                index += 1;
                continue;
            }
            if resume_process(pid, start) {
                crate::diag_log::info(&format!("进程屏蔽：恢复 pid {pid}"));
            }
            suspended.remove(index);
            changed = true;
        }
        for proc in &processes {
            if proc.name.eq_ignore_ascii_case("sidera")
                || proc.pid as u32 == std::process::id()
                || suspended.iter().any(|(pid, _)| *pid == proc.pid)
                || !rules.iter().any(|rule| matches(rule, proc))
            {
                continue;
            }
            if suspend_process(proc) {
                suspended.push((proc.pid, proc.start_time));
                changed = true;
                crate::diag_log::info(&format!(
                    "进程屏蔽：已暂停 {} (pid {})", proc.name, proc.pid));
            } else {
                crate::diag_log::warn(&format!(
                    "进程屏蔽：无法暂停 {} (pid {})", proc.name, proc.pid));
            }
        }
        if changed {
            write_blocker_state(&suspended);
        }
        for _ in 0..10 {
            if WIN_BLOCKER_STOP.load(Ordering::Relaxed) { break; }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    for (pid, start) in suspended {
        resume_process(pid, start);
    }
    write_blocker_state(&[]);
    crate::diag_log::info("进程屏蔽守护线程已停止，已恢复暂停的进程");
}

#[cfg(windows)]
pub fn set_daemon_enabled(enabled: bool) {
    use std::sync::atomic::Ordering;
    if enabled {
        let Ok(mut handle) = WIN_BLOCKER.lock() else { return };
        if handle.is_some() {
            return;
        }
        WIN_BLOCKER_STOP.store(false, Ordering::Relaxed);
        *handle = Some(std::thread::spawn(win_blocker_loop));
    } else {
        WIN_BLOCKER_STOP.store(true, Ordering::Relaxed);
        let handle = WIN_BLOCKER.lock().ok().and_then(|mut guard| guard.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

pub fn stop_daemon() {
    set_daemon_enabled(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(name: &str, exe: &str, cmdline: &str) -> ProcInfo {
        ProcInfo {
            pid: 123,
            name: name.into(),
            exe: exe.into(),
            cmdline: cmdline.into(),
            state: 'S',
            start_time: 1,
        }
    }

    #[cfg(unix)]
    #[test]
    fn process_stat_and_suspend_resume_round_trip() {
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let pid = child.id() as i32;
        let (_, start) = read_process_stat(pid).unwrap();
        let info = ProcInfo {
            pid,
            name: "sleep".into(),
            exe: "/usr/bin/sleep".into(),
            cmdline: "sleep 30".into(),
            state: 'S',
            start_time: start,
        };
        assert!(suspend_process(&info));
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(read_process_stat(pid).unwrap().0, 'T');
        assert!(resume_process(pid, start));
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_ne!(read_process_stat(pid).unwrap().0, 'T');
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn windows_style_paths_match_names_and_full_executables() {
        let item = proc(
            "notepad",
            "C:\\Windows\\notepad.exe",
            "C:\\Windows\\notepad.exe",
        );
        assert!(matches("notepad", &item));
        assert!(matches("notepad.exe", &item));
        assert!(matches("C:\\Windows\\notepad.exe", &item));
        assert!(!matches("note", &item));
    }

    #[cfg(windows)]
    #[test]
    fn windows_scan_does_not_panic() {
        let _ = scan();
    }

    #[test]
    fn rules_match_names_executables_and_command_arguments_without_substrings() {
        let item = proc(
            "wps",
            "/opt/kingsoft/wps/wps",
            "/opt/kingsoft/wps/wps --foo",
        );
        assert!(matches("wps", &item));
        assert!(matches("/opt/kingsoft/wps/wps", &item));
        assert!(!matches("wp", &item));
        assert!(!matches("", &item));
    }
}
