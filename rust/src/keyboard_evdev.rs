// ============================================================
// Sidera - 直接读内核 evdev 的键盘快捷键（X11 / XWayland / Wayland 通用）
//   只观察，不拦截、不注入；用于识别物理 Ctrl+P。
//   权限与触摸一致：DEB 安装 uaccess 规则；源码运行需在 input 组。
// ============================================================
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use evdev::{Device, EventType, KeyCode};

static STARTED: AtomicBool = AtomicBool::new(false);
static LIVE_KEYBOARDS: AtomicUsize = AtomicUsize::new(0);
static PEN_PENDING: AtomicBool = AtomicBool::new(false);

/// 当前进程是否已成功打开至少一个键盘设备。
pub fn active() -> bool {
    LIVE_KEYBOARDS.load(Ordering::Relaxed) > 0
}

/// 取出一次 Ctrl+P（消费后清零）。
pub fn take_pen_shortcut() -> bool {
    PEN_PENDING.swap(false, Ordering::Relaxed)
}

/// 启动键盘观察线程（幂等）。即使暂时没有权限也会持续热插拔扫描。
pub fn start() {
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(|| {
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut warned = false;
        loop {
            let mut found = false;
            for (path, dev) in evdev::enumerate() {
                if seen.contains(&path) { continue; }
                let name = dev.name().unwrap_or_default().to_ascii_lowercase();
                // 忽略本程序自己创建的虚拟键盘，避免回环。
                if name.contains("sidera") { continue; }
                let Some(keys) = dev.supported_keys() else { continue; };
                // 只接受真正的键盘；触摸屏/遥控器上的个别按键不参与。
                if !(keys.contains(KeyCode::KEY_A) || keys.contains(KeyCode::KEY_P)) {
                    continue;
                }
                seen.insert(path);
                found = true;
                if LIVE_KEYBOARDS.fetch_add(1, Ordering::Relaxed) == 0 {
                    log::info!("[KEYS] evdev 键盘已启用");
                    crate::diag_log::info("WPS 画笔快捷键：evdev 键盘监听已启用");
                }
                std::thread::spawn(move || {
                    read_keyboard(dev);
                    LIVE_KEYBOARDS.fetch_sub(1, Ordering::Relaxed);
                });
            }
            if !found && !warned && LIVE_KEYBOARDS.load(Ordering::Relaxed) == 0 {
                warned = true;
                log::warn!("[KEYS] 未找到可读键盘设备，Ctrl+P 将使用可用回退");
                crate::diag_log::warn("WPS 画笔快捷键：未找到可读键盘设备（检查 /dev/input 权限）");
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn read_keyboard(mut dev: Device) {
    let mut state = CtrlPState::new();
    loop {
        let events = match dev.fetch_events() {
            Ok(events) => events,
            Err(_) => break,
        };
        for ev in events {
            if ev.event_type() != EventType::KEY { continue; }
            if state.feed(KeyCode(ev.code()), ev.value()) {
                PEN_PENDING.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// 纯状态机，便于单元测试：返回本次事件是否构成 Ctrl+P 按下。
pub(crate) struct CtrlPState {
    left: bool,
    right: bool,
}

impl CtrlPState {
    pub(crate) fn new() -> Self { Self { left: false, right: false } }
    pub(crate) fn feed(&mut self, code: KeyCode, value: i32) -> bool {
        let pressed = value != 0;
        if code == KeyCode::KEY_LEFTCTRL {
            self.left = pressed; false
        } else if code == KeyCode::KEY_RIGHTCTRL {
            self.right = pressed; false
        } else {
            code == KeyCode::KEY_P && value == 1 && (self.left || self.right)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_p_fires_once_per_press_and_ignores_repeat() {
        let mut state = CtrlPState::new();
        assert!(!state.feed(KeyCode::KEY_LEFTCTRL, 1));
        assert!(state.feed(KeyCode::KEY_P, 1));
        assert!(!state.feed(KeyCode::KEY_P, 2), "auto-repeat must not fire");
        assert!(!state.feed(KeyCode::KEY_LEFTCTRL, 0));
        assert!(!state.feed(KeyCode::KEY_P, 1), "P without Ctrl must be ignored");
        assert!(!state.feed(KeyCode::KEY_RIGHTCTRL, 1));
        assert!(state.feed(KeyCode::KEY_P, 1));
    }
}
