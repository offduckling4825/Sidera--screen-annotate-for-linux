// ============================================================
// Sidera - Wayland 后端（wlr-layer-shell，面向 niri/sway/hyprland 等独立合成器）
// 软件渲染 wl_shm；输入用 wl_pointer / wl_touch；点击穿透用 wl_surface 输入区域
// ============================================================
use std::cell::{Cell, RefCell};
use std::os::unix::net::UnixListener;
use std::time::Duration;

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::touch::TouchHandler;
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::raw::RawPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_touch,
};
use tiny_skia::Pixmap;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_buffer, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface, wl_touch};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{self, WpFractionalScaleV1};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;
use wayland_client::protocol::wl_keyboard::{KeyState, KeymapFormat};
use wayland_client::{Connection, QueueHandle};

use crate::app::App;
use crate::backend::{Backend, IRect, Key};
use crate::gesture::TouchKind;
use crate::text::Fonts;
use crate::{Ip, Rt, Timers};

// ---- 虚拟键盘协议绑定（从仓库 XML 生成）----
mod vk {
    use wayland_client;
    use wayland_client::protocol::*;
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("../protocols/virtual-keyboard-unstable-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("../protocols/virtual-keyboard-unstable-v1.xml");
}
use vk::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use vk::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;

/// 内嵌的 xkb keymap（构建期用 xkbcli 生成，避免运行时依赖 libxkbcommon）
const KEYMAP_XKB: &str = include_str!("keymap.xkb");

impl wayland_client::Dispatch<ZwpVirtualKeyboardManagerV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl wayland_client::Dispatch<ZwpVirtualKeyboardV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl wayland_client::Dispatch<WpFractionalScaleManagerV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl wayland_client::Dispatch<WpFractionalScaleV1, ()> for WlState {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            let sf = (scale as f64) / 120.0;
            state.app.set_scale(sf);
            state.rt.backend.set_scale(sf);
            state.rt.redraw_full(&state.app);
        }
    }
}

impl wayland_client::Dispatch<WpViewporter, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &WpViewporter,
        _: <WpViewporter as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl wayland_client::Dispatch<WpViewport, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: <WpViewport as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

// ---------------- 平台对象（Backend 实现） ----------------
pub struct WlPlat {
    compositor: CompositorState,
    layer: LayerSurface,
    conn: Connection,
    qh: QueueHandle<WlState>,
    raw: RefCell<RawPool>,
    bufs: RefCell<[Option<wl_buffer::WlBuffer>; 2]>,
    busy: Cell<[bool; 2]>,
    next: Cell<usize>,
    logical: Cell<(i32, i32)>,
    dims: Cell<(i32, i32)>, // 物理缓冲区尺寸
    scale: Cell<f64>,
    viewport: RefCell<Option<WpViewport>>,
    uinput: RefCell<Option<crate::uinput::KeyInjector>>,
    back: RefCell<Pixmap>,
    vk: RefCell<Option<ZwpVirtualKeyboardV1>>,
    dirty: Cell<Option<(i32, i32, i32, i32)>>, // 脏区域包围盒（物理）x0,y0,x1,y1
}

impl WlPlat {
    /// 依据逻辑尺寸与 scale 重建物理缓冲区与 viewport 目标
    fn reconfigure(&self) {
        let (lw, lh) = self.logical.get();
        if lw <= 0 || lh <= 0 {
            return;
        }
        let sc = self.scale.get();
        let pw = ((lw as f64) * sc).round().max(1.0) as i32;
        let ph = ((lh as f64) * sc).round().max(1.0) as i32;
        if self.dims.get() == (pw, ph) && self.bufs.borrow()[0].is_some() {
            if let Some(vp) = self.viewport.borrow().as_ref() {
                vp.set_destination(lw, lh);
            }
            return;
        }
        self.dims.set((pw, ph));
        let frame_bytes = (pw as usize) * (ph as usize) * 4;
        if self.raw.borrow_mut().resize(2 * frame_bytes).is_err() {
            return;
        }
        for i in 0..2 {
            if let Some(b) = self.bufs.borrow_mut()[i].take() {
                b.destroy();
            }
        }
        let stride = pw * 4;
        let qh = self.qh.clone();
        {
            let mut raw = self.raw.borrow_mut();
            let b0 = raw.create_buffer(0, pw, ph, stride, wl_shm::Format::Argb8888, (), &qh);
            let b1 = raw.create_buffer(
                frame_bytes as i32,
                pw,
                ph,
                stride,
                wl_shm::Format::Argb8888,
                (),
                &qh,
            );
            let mut bufs = self.bufs.borrow_mut();
            bufs[0] = Some(b0);
            bufs[1] = Some(b1);
            raw.mmap().fill(0);
        }
        if let Some(p) = Pixmap::new(pw as u32, ph as u32) {
            *self.back.borrow_mut() = p;
        }
        if let Some(vp) = self.viewport.borrow().as_ref() {
            vp.set_destination(lw, lh);
        }
        self.busy.set([false, false]);
        self.next.set(0);
        self.dirty.set(Some((0, 0, pw, ph)));
        self.flush();
    }

    fn union_dirty(&self, x: i32, y: i32, w: i32, h: i32) {
        let (sw, sh) = self.dims.get();
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(sw);
        let y1 = (y + h).min(sh);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let r = match self.dirty.get() {
            None => (x0, y0, x1, y1),
            Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
        };
        self.dirty.set(Some(r));
    }
}

impl Backend for WlPlat {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn screen_size(&self) -> (i32, i32) {
        self.logical.get()
    }
    fn present(&self, pm: &Pixmap, x: i32, y: i32) {
        let (w, h) = self.dims.get();
        if w <= 0 || h <= 0 {
            return;
        }
        let sc = self.scale.get();
        let x = ((x as f64) * sc).round() as i32;
        let y = ((y as f64) * sc).round() as i32;
        {
            let mut back = self.back.borrow_mut();
            if back.width() != w as u32 || back.height() != h as u32 {
                match Pixmap::new(w as u32, h as u32) {
                    Some(p) => *back = p,
                    None => return,
                }
            }
            let bw = back.width() as i32;
            let data = back.data_mut();
            let src = pm.data();
            let pw = pm.width() as i32;
            let ph = pm.height() as i32;
            for row in 0..ph {
                let dy = y + row;
                if dy < 0 || dy >= h {
                    continue;
                }
                for col in 0..pw {
                    let dx = x + col;
                    if dx < 0 || dx >= w {
                        continue;
                    }
                    let si = ((row * pw + col) * 4) as usize;
                    let di = ((dy * bw + dx) * 4) as usize;
                    data[di..di + 4].copy_from_slice(&src[si..si + 4]);
                }
            }
        }
        self.union_dirty(x, y, pm.width() as i32, pm.height() as i32);
    }
    fn flush(&self) {
        let Some((x0, y0, x1, y1)) = self.dirty.get() else {
            return;
        };
        // 找一个空闲 buffer（双缓冲，避免单 buffer 死锁）
        let busy = self.busy.get();
        let idx = if !busy[0] {
            0
        } else if !busy[1] {
            1
        } else {
            return; // 都忙：保留脏区域，等 release
        };
        self.dirty.set(None);
        let (w, h) = self.dims.get();
        if w <= 0 || h <= 0 {
            return;
        }
        let frame_bytes = (w as usize) * (h as usize) * 4;
        // 脏区域同时写入两份 buffer，保证两份内容一致
        {
            let back = self.back.borrow();
            let mut raw = self.raw.borrow_mut();
            let mmap = raw.mmap();
            let src = back.data();
            let bw = back.width() as i32;
            for bi in 0..2usize {
                let base = bi * frame_bytes;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let si = ((yy * bw + xx) * 4) as usize;
                        let mi = base + ((yy * w + xx) * 4) as usize;
                        mmap[mi] = src[si + 2];
                        mmap[mi + 1] = src[si + 1];
                        mmap[mi + 2] = src[si];
                        mmap[mi + 3] = src[si + 3];
                    }
                }
            }
        }
        let surface = self.layer.wl_surface();
        surface.damage_buffer(x0, y0, x1 - x0, y1 - y0);
        if let Some(buf) = self.bufs.borrow()[idx].as_ref() {
            surface.attach(Some(buf), 0, 0);
        }
        self.layer.commit();
        let mut b = self.busy.get();
        b[idx] = true;
        self.busy.set(b);
        self.next.set((idx + 1) % 2);
    }
    fn set_input_region(&self, rects: &[IRect], full: bool) {
        if full {
            self.layer.set_input_region(None);
        } else {
            match Region::new(&self.compositor) {
                Ok(region) => {
                    for r in rects {
                        region.add(r.x, r.y, r.w.max(1), r.h.max(1));
                    }
                    self.layer.set_input_region(Some(region.wl_region()));
                }
                Err(_) => {
                    self.layer.set_input_region(None);
                }
            }
        }
        self.layer.commit();
    }
    fn fake_key(&self, key: Key) {
        let code = match key {
            Key::Up => 103u16,
            Key::Down => 108u16,
            Key::Escape => 1u16,
        };
        // 优先合成器虚拟键盘（wlr 系）
        if let Some(kb) = self.vk.borrow().clone() {
            let t = (crate::app::now_ms() & 0xffff_ffff) as u32;
            kb.key(t, code as u32, KeyState::Pressed);
            kb.key(t.wrapping_add(1), code as u32, KeyState::Released);
            let _ = self.conn.flush();
            return;
        }
        // 否则用 uinput 内核虚拟键盘（KWin 等）
        if let Some(u) = self.uinput.borrow_mut().as_mut() {
            u.key(code);
            return;
        }
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log::warn!("[WARN] 无可用的按键注入（虚拟键盘/uinput 均不可用）");
        }
    }
    fn virtual_right_click(&self, _x: i32, _y: i32) {}
    fn screenshot(&self) -> Option<(Vec<u8>, u32, u32)> {
        None
    }
    fn is_presentation_fullscreen(&self) -> bool {
        false
    }
    fn has_compositor(&self) -> bool {
        true
    }
    fn on_configure(&self, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        self.logical.set((w, h));
        self.reconfigure();
    }
    fn set_scale(&self, s: f64) {
        self.scale.set(s.clamp(1.0, 4.0));
        self.reconfigure();
    }
    fn show_splash(&self, fonts: &Fonts, icon: Option<&Pixmap>, dur_ms: u64) {
        let (w, h) = self.dims.get();
        if w <= 0 || h <= 0 {
            return;
        }
        let pm = crate::splash::draw(fonts, icon, 460, 210, 100);
        {
            let mut back = self.back.borrow_mut();
            if back.width() != w as u32 || back.height() != h as u32 {
                return;
            }
            let x = (w - 460) / 2;
            let y = (h - 210) / 2;
            let pw = pm.width() as i32;
            let ph = pm.height() as i32;
            let data = back.data_mut();
            let src = pm.data();
            for row in 0..ph {
                let dy = y + row;
                if dy < 0 || dy >= h {
                    continue;
                }
                for col in 0..pw {
                    let dx = x + col;
                    if dx < 0 || dx >= w {
                        continue;
                    }
                    let si = ((row * pw + col) * 4) as usize;
                    let di = ((dy * w + dx) * 4) as usize;
                    data[di..di + 4].copy_from_slice(&src[si..si + 4]);
                }
            }
        }
        self.dirty.set(Some((0, 0, w, h)));
        self.flush();
        let _ = self.conn.flush();
        std::thread::sleep(std::time::Duration::from_millis(dur_ms));
    }
}

impl wayland_client::Dispatch<wl_buffer::WlBuffer, ()> for WlState {
    fn event(
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            if let Some(p) = state.rt.backend.as_any().downcast_ref::<WlPlat>() {
                let bufs = p.bufs.borrow();
                let mut busy = p.busy.get();
                for i in 0..2 {
                    if bufs[i].as_ref() == Some(buffer) {
                        busy[i] = false;
                    }
                }
                p.busy.set(busy);
            }
        }
    }
}

// ---------------- 事件状态 ----------------
struct WlState {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    conn: Connection,
    vk_manager: Option<ZwpVirtualKeyboardManagerV1>,
    #[allow(dead_code)]
    fractional_scale: Option<WpFractionalScaleV1>,
    pointer: Option<wl_pointer::WlPointer>,
    touch: Option<wl_touch::WlTouch>,
    surface: wl_surface::WlSurface,
    configured: bool,
    splash_shown: bool,

    rt: Rt,
    app: App,
    ip: Ip,
    wps: Option<crate::wps::WpsBridge>,
    timers: Timers,
    single: Option<UnixListener>,
}

/// 发送 xkb keymap（memfd + 内嵌文本）
fn send_keymap(kb: &ZwpVirtualKeyboardV1, conn: &Connection) {
    use std::os::fd::{AsFd, FromRawFd};
    let mut data = KEYMAP_XKB.as_bytes().to_vec();
    data.push(0);
    let size = data.len() as u32;
    let name = std::ffi::CString::new("sidera-keymap").unwrap();
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return;
    }
    let owned = unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) };
    unsafe {
        libc::ftruncate(fd, size as i64);
        let m = libc::mmap(
            std::ptr::null_mut(),
            size as usize,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        );
        if m != libc::MAP_FAILED {
            std::ptr::copy_nonoverlapping(data.as_ptr(), m as *mut u8, data.len());
            libc::munmap(m, size as usize);
        }
    }
    kb.keymap(KeymapFormat::XkbV1, owned.as_fd(), size);
    let _ = conn.flush();
    // owned 在此 drop 关闭 fd（wayland-client 已 dup）
}

impl CompositorHandler for WlState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }
    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for WlState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for WlState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        std::process::exit(0);
    }
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (cw, ch) = configure.new_size;
        let (w, h) = if cw == 0 || ch == 0 {
            // 合成器没给尺寸时用已有/默认
            let (sw, sh) = self.rt.backend.screen_size();
            if sw > 0 && sh > 0 {
                (sw, sh)
            } else {
                (1920, 1080)
            }
        } else {
            (cw as i32, ch as i32)
        };
        self.rt.backend.on_configure(w, h);
        self.app.resize_screen(w, h);
        self.configured = true;
        // 先闪屏，结束后再显示软件本体
        if !self.splash_shown {
            self.splash_shown = true;
            if std::env::var("SIDERA_NO_SPLASH").is_err() {
                self.rt.backend.show_splash(&self.rt.fonts, self.rt.icon_big.as_ref(), 1400);
            }
        }
        crate::apply_input_shape(&self.rt, &self.app);
        self.rt.redraw_full(&self.app);
    }
}

impl SeatHandler for WlState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
        if capability == Capability::Touch && self.touch.is_none() {
            self.touch = self.seat_state.get_touch(qh, &seat).ok();
        }
        // 首次拿到 seat 能力时创建虚拟键盘（若合成器支持）
        let already = self
            .rt
            .backend
            .as_any()
            .downcast_ref::<WlPlat>()
            .map(|p| p.vk.borrow().is_some())
            .unwrap_or(true);
        if !already {
            if let Some(vkm) = self.vk_manager.clone() {
                let kb = vkm.create_virtual_keyboard(&seat, qh, ());
                send_keymap(&kb, &self.conn);
                if let Some(p) = self.rt.backend.as_any().downcast_ref::<WlPlat>() {
                    *p.vk.borrow_mut() = Some(kb);
                }
                log::info!("[INFO] 虚拟键盘已就绪（翻页/退出）");
            }
        }
    }
    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
        if capability == Capability::Touch {
            if let Some(t) = self.touch.take() {
                t.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for WlState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for ev in events {
            if &ev.surface != &self.surface {
                continue;
            }
            let x = ev.position.0 as i32;
            let y = ev.position.1 as i32;
            match ev.kind {
                PointerEventKind::Enter { .. } => {}
                PointerEventKind::Leave { .. } => {
                    if self.app.hover.is_some() {
                        self.app.hover = None;
                        self.rt.redraw_full(&self.app);
                    }
                }
                PointerEventKind::Motion { .. } => {
                    crate::on_motion(&self.rt, &mut self.app, &mut self.ip, x, y);
                }
                PointerEventKind::Press { button, .. } => {
                    let b = match button {
                        0x110 => 1u8,
                        0x111 => 3u8,
                        _ => 0u8,
                    };
                    if b != 0 {
                        crate::on_press(
                            &self.rt,
                            &mut self.app,
                            &mut self.ip,
                            self.wps.as_ref(),
                            x,
                            y,
                            b,
                        );
                    }
                }
                PointerEventKind::Release { .. } => {
                    crate::on_release(&self.rt, &mut self.app, &mut self.ip, x, y);
                }
                PointerEventKind::Axis { .. } => {}
            }
        }
    }
}

impl TouchHandler for WlState {
    fn down(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _serial: u32,
        _time: u32,
        surface: wl_surface::WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        if surface != self.surface {
            return;
        }
        let d = self.ip.touch_shape.get(&id).copied().unwrap_or(0.0);
        crate::handle_touch(
            &self.rt,
            &mut self.app,
            &mut self.ip,
            self.wps.as_ref(),
            id,
            position.0 as i32,
            position.1 as i32,
            TouchKind::Begin,
            d,
        );
    }
    fn up(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _serial: u32,
        _time: u32,
        id: i32,
    ) {
        crate::handle_touch(
            &self.rt,
            &mut self.app,
            &mut self.ip,
            self.wps.as_ref(),
            id,
            0,
            0,
            TouchKind::End,
            0.0,
        );
        self.ip.touch_shape.remove(&id);
    }
    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _time: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let d = self.ip.touch_shape.get(&id).copied().unwrap_or(0.0);
        crate::handle_touch(
            &self.rt,
            &mut self.app,
            &mut self.ip,
            self.wps.as_ref(),
            id,
            position.0 as i32,
            position.1 as i32,
            TouchKind::Update,
            d,
        );
    }
    fn shape(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        id: i32,
        major: f64,
        minor: f64,
    ) {
        // wl_touch.shape 给的是 surface 逻辑坐标下的接触椭圆长短轴
        self.ip.touch_shape.insert(id, major.max(minor));
    }
    fn orientation(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _id: i32,
        _orientation: f64,
    ) {
    }
    fn cancel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
    ) {
        self.app.reset_palm_gesture();
    }
}

impl ShmHandler for WlState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(WlState);
delegate_output!(WlState);
delegate_shm!(WlState);
delegate_seat!(WlState);
delegate_pointer!(WlState);
delegate_touch!(WlState);
delegate_layer!(WlState);
delegate_registry!(WlState);

impl ProvidesRegistryState for WlState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers![OutputState, SeatState];
}

fn periodic(state: &mut WlState) {
    crate::tick(&state.rt, &mut state.app, &mut state.wps, &mut state.timers);
    state.rt.backend.flush();
    if let Some(l) = state.single.as_ref() {
        while let Ok((mut c, _)) = l.accept() {
            use std::io::Read;
            let mut buf = [0u8; 16];
            let _ = c.read(&mut buf);
            state.rt.redraw_full(&state.app);
        }
    }
}

pub fn run(
    fonts: Fonts,
    icon: Option<Pixmap>,
    icon_big: Option<Pixmap>,
    single: Option<UnixListener>,
) {
    if let Err(e) = run_inner(fonts, icon, icon_big, single) {
        log::error!("[ERROR] Wayland 后端启动失败: {}", e);
    }
}

fn run_inner(
    fonts: Fonts,
    icon: Option<Pixmap>,
    icon_big: Option<Pixmap>,
    single: Option<UnixListener>,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init::<WlState>(&conn)?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let shm = Shm::bind(&globals, &qh)?;

    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface.clone(),
        Layer::Overlay,
        Some("sidera"),
        None,
    );
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(0, 0);
    layer.commit();

    let raw = RawPool::new(1920 * 1080 * 4, &shm)?;
    let registry_state = RegistryState::new(&globals);
    let vk_manager = registry_state
        .bind_one::<ZwpVirtualKeyboardManagerV1, WlState, ()>(&qh, 1..=1, ())
        .ok();
    if vk_manager.is_none() {
        log::warn!("[WARN] 合成器不支持虚拟键盘协议，Wayland 下无法注入翻页/退出按键");
    }
    let fs_manager = registry_state
        .bind_one::<WpFractionalScaleManagerV1, WlState, ()>(&qh, 1..=1, ())
        .ok();
    let viewporter = registry_state
        .bind_one::<WpViewporter, WlState, ()>(&qh, 1..=1, ())
        .ok();
    let fractional_scale = fs_manager.map(|m| m.get_fractional_scale(&surface, &qh, ()));
    let viewport = viewporter.map(|v| v.get_viewport(&surface, &qh, ()));

    let wlplat = WlPlat {
        compositor: compositor.clone(),
        layer: layer.clone(),
        conn: conn.clone(),
        qh: qh.clone(),
        raw: RefCell::new(raw),
        bufs: RefCell::new([None, None]),
        busy: Cell::new([false, false]),
        next: Cell::new(0),
        logical: Cell::new((0, 0)),
        dims: Cell::new((0, 0)),
        scale: Cell::new(1.0),
        viewport: RefCell::new(viewport),
        uinput: RefCell::new(crate::uinput::KeyInjector::new()),
        back: RefCell::new(Pixmap::new(1, 1).unwrap()),
        vk: RefCell::new(None),
        dirty: Cell::new(None),
    };

    let rt = Rt {
        backend: Box::new(wlplat),
        fonts,
        icon,
        icon_big,
    };

    let mut app = App::new(Pixmap::new(1, 1).unwrap(), 1920, 1080);
    crate::app::load_settings(&mut app);
    if let Ok(v) = std::env::var("WPS_API_DEBUG") {
        app.wps_debug = v == "1";
    }

    let mut state = WlState {
        registry_state,
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        conn: conn.clone(),
        vk_manager,
        fractional_scale,
        pointer: None,
        touch: None,
        surface,
        configured: false,
        splash_shown: false,
        rt,
        app,
        ip: Ip::default(),
        wps: None,
        timers: Timers::new(),
        single,
    };

    let mut event_loop: EventLoop<WlState> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue).insert(handle.clone())?;
    let timer = Timer::from_duration(Duration::from_millis(16));
    handle.insert_source(timer, |_instant, _meta, st: &mut WlState| {
        periodic(st);
        TimeoutAction::ToDuration(Duration::from_millis(16))
    })?;

    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}
