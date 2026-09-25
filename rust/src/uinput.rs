// ============================================================
// Sidera - uinput 内核级虚拟键盘（不依赖合成器协议，X11/Wayland 通用）
// 用于 KWin 等没有 zwp_virtual_keyboard 的合成器注入翻页/退出按键
// ============================================================
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};

pub struct KeyInjector {
    dev: VirtualDevice,
}

impl KeyInjector {
    pub fn new() -> Option<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        keys.insert(KeyCode::KEY_UP);
        keys.insert(KeyCode::KEY_DOWN);
        keys.insert(KeyCode::KEY_ESC);
        let dev = VirtualDevice::builder()
            .ok()?
            .name("Sidera Virtual Keyboard")
            .with_keys(&keys)
            .ok()?
            .build()
            .ok()?;
        log::info!("[INFO] uinput 虚拟键盘已就绪（翻页/退出）");
        Some(KeyInjector { dev })
    }

    /// code 为 evdev 扫描码（Up=103, Down=108, Esc=1）
    pub fn key(&mut self, code: u16) {
        let _ = self.dev.emit(&[
            InputEvent::new(EventType::KEY.0, code, 1),
            InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0),
            InputEvent::new(EventType::KEY.0, code, 0),
            InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0),
        ]);
    }
}
