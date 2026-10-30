//! Match and terminate user-selected processes after Sidera starts.
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: i32,
    pub name: String,
    pub exe: String,
    pub cmdline: String,
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
        out.push(ProcInfo {
            pid,
            name,
            exe,
            cmdline,
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

fn exe_name(proc: &ProcInfo) -> &str {
    proc.exe.rsplit('/').next().unwrap_or("")
}

pub fn matches(rule: &str, proc: &ProcInfo) -> bool {
    let rule = rule.trim();
    if rule.is_empty() {
        return false;
    }
    if rule.eq_ignore_ascii_case(&proc.name) || rule.eq_ignore_ascii_case(exe_name(proc)) {
        return true;
    }
    if rule.contains('/') && rule.eq_ignore_ascii_case(&proc.exe) {
        return true;
    }
    proc.cmdline.split_whitespace().next().is_some_and(|first| {
        first
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(rule))
    })
}

pub fn kill_rules(rules: &[String]) -> Vec<(i32, String, bool)> {
    let self_pid = std::process::id() as i32;
    let mut results = Vec::new();
    for proc in scan() {
        if proc.pid == self_pid {
            continue;
        }
        if !rules.iter().any(|rule| matches(rule, &proc)) {
            continue;
        }
        let ok = unsafe { libc::kill(proc.pid, libc::SIGTERM) == 0 };
        results.push((proc.pid, proc.name, ok));
    }
    results
}

pub fn run_configured() {
    let rules = load_rules();
    if rules.is_empty() {
        return;
    }
    let results = kill_rules(&rules);
    for (pid, name, ok) in results {
        if ok {
            crate::diag_log::info(&format!("进程屏蔽：已结束 {name} (pid {pid})"));
        } else {
            crate::diag_log::warn(&format!("进程屏蔽：无法结束 {name} (pid {pid})"));
        }
    }
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
        }
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
