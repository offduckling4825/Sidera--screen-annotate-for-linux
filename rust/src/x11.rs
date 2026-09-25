// ============================================================
// Sidera - X11 后端封装（x11rb，纯 Rust，不需要 libX11/libXtst）
// 覆盖：ARGB 覆盖层窗口、XShape 输入穿透、XTest 假键/虚拟右键、
//       全局热键、WPS 全屏检测、截图、合成器检测
// ============================================================
use x11rb::connection::Connection;
use x11rb::protocol::shape::{self, SK, SO};
use x11rb::protocol::xproto::{
    self, Atom, AtomEnum, Colormap, ColormapAlloc, ConfigureWindowAux, CreateGCAux,
    CreateWindowAux, Drawable, EventMask, Gcontext, ImageFormat, Keycode, ModMask, Rectangle,
    VisualClass, Window, WindowClass,
};
use x11rb::protocol::xinput::{self, DeviceClassData, Fp3232};
use x11rb::protocol::xtest;
use x11rb::rust_connection::RustConnection;
use std::cell::RefCell;
use std::collections::HashMap;

pub type XResult<T> = Result<T, Box<dyn std::error::Error>>;

pub const XK_ESCAPE: u32 = 0xff1b;
pub const XK_UP: u32 = 0xff52;
pub const XK_DOWN: u32 = 0xff54;
pub const XK_D: u32 = 0x0064;

pub struct X11 {
    pub conn: RustConnection,
    #[allow(dead_code)]
    pub screen_num: usize,
    pub root: Window,
    pub width: i32,
    pub height: i32,
    pub argb_visual: u32,
    pub argb_depth: u8,
    pub cmap: Colormap,
    pub net_wm_state: Atom,
    pub net_wm_fullscreen: Atom,
    pub net_wm_cm: Atom,
    pub key_escape: Keycode,
    pub key_up: Keycode,
    pub key_down: Keycode,
    pub key_d: Keycode,
    pub atom_touch_major: Atom,
    pub atom_touch_minor: Atom,
    pub atom_pos_x: Atom,
}

impl X11 {
    pub fn connect() -> XResult<Self> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;

        // 根窗口的完整几何（可能跨多显示器）
        let geo = xproto::get_geometry(&conn, root)?.reply()?;
        let width = geo.width as i32;
        let height = geo.height as i32;

        // 找 32 位 TrueColor 视觉（ARGB）
        let mut argb_visual = screen.root_visual;
        let mut argb_depth = screen.root_depth;
        for d in &screen.allowed_depths {
            if d.depth == 32 {
                for v in &d.visuals {
                    if v.class == VisualClass::TRUE_COLOR {
                        argb_visual = v.visual_id;
                        argb_depth = 32;
                    }
                }
            }
        }

        let cmap = conn.generate_id()?;
        xproto::create_colormap(&conn, ColormapAlloc::NONE, cmap, root, argb_visual)?;

        let net_wm_state = xproto::intern_atom(&conn, false, b"_NET_WM_STATE")?.reply()?.atom;
        let net_wm_fullscreen =
            xproto::intern_atom(&conn, false, b"_NET_WM_STATE_FULLSCREEN")?.reply()?.atom;
        let net_wm_cm = xproto::intern_atom(&conn, false, b"_NET_WM_CM_S0")?.reply()?.atom;
        let atom_touch_major =
            xproto::intern_atom(&conn, false, b"Abs MT Touch Major")?.reply()?.atom;
        let atom_touch_minor =
            xproto::intern_atom(&conn, false, b"Abs MT Touch Minor")?.reply()?.atom;
        let atom_pos_x = xproto::intern_atom(&conn, false, b"Abs MT Position X")?.reply()?.atom;

        let mut x = X11 {
            conn,
            screen_num,
            root,
            width,
            height,
            argb_visual,
            argb_depth,
            cmap,
            net_wm_state,
            net_wm_fullscreen,
            net_wm_cm,
            key_escape: 0,
            key_up: 0,
            key_down: 0,
            key_d: 0,
            atom_touch_major,
            atom_touch_minor,
            atom_pos_x,
        };
        x.key_escape = x.keysym_to_keycode(XK_ESCAPE);
        x.key_up = x.keysym_to_keycode(XK_UP);
        x.key_down = x.keysym_to_keycode(XK_DOWN);
        x.key_d = x.keysym_to_keycode(XK_D);
        x.conn.flush()?;
        Ok(x)
    }

    fn keysym_to_keycode(&self, ks: u32) -> Keycode {
        let setup = self.conn.setup();
        let min = setup.min_keycode;
        let max = setup.max_keycode;
        let count = max - min + 1;
        if let Ok(cookie) = xproto::get_keyboard_mapping(&self.conn, min, count) {
            if let Ok(reply) = cookie.reply() {
                let per = reply.keysyms_per_keycode as usize;
                if per == 0 {
                    return 0;
                }
                for (i, chunk) in reply.keysyms.chunks(per).enumerate() {
                    if chunk.iter().any(|k| *k == ks) {
                        return min + i as u8;
                    }
                }
            }
        }
        0
    }

    /// 创建覆盖层窗口，返回 (window, gc)
    pub fn create_overlay(&self) -> XResult<(Window, Gcontext)> {
        self.create_window(0, 0, self.width, self.height)
    }

    pub fn create_window(&self, x: i32, y: i32, w: i32, h: i32) -> XResult<(Window, Gcontext)> {
        let win = self.conn.generate_id()?;
        let event_mask = EventMask::EXPOSURE
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
            | EventMask::BUTTON1_MOTION
            | EventMask::BUTTON3_MOTION
            | EventMask::KEY_PRESS
            | EventMask::LEAVE_WINDOW
            | EventMask::STRUCTURE_NOTIFY;
        let aux = CreateWindowAux::new()
            .background_pixel(0u32)
            .border_pixel(0u32)
            .colormap(self.cmap)
            .override_redirect(1u32)
            .event_mask(event_mask);
        xproto::create_window(
            &self.conn,
            self.argb_depth,
            win,
            self.root,
            x as i16,
            y as i16,
            w as u16,
            h as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            self.argb_visual,
            &aux,
        )?;
        let gc = self.conn.generate_id()?;
        xproto::create_gc(&self.conn, gc, win, &CreateGCAux::new().graphics_exposures(0u32))?;
        xproto::map_window(&self.conn, win)?;
        xproto::configure_window(
            &self.conn,
            win,
            &ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE),
        )?;
        self.conn.flush()?;
        Ok((win, gc))
    }

    /// 把整张 pixmap（预乘 RGBA）转成 X11 的 BGRA 并上传到 (x,y)
    pub fn put_pixmap(
        &self,
        win: Window,
        gc: Gcontext,
        pm: &tiny_skia::Pixmap,
        x: i32,
        y: i32,
    ) -> XResult<()> {
        let src = pm.data();
        let mut buf = Vec::with_capacity(src.len());
        for px in src.chunks_exact(4) {
            buf.push(px[2]);
            buf.push(px[1]);
            buf.push(px[0]);
            buf.push(px[3]);
        }
        xproto::put_image(
            &self.conn,
            ImageFormat::Z_PIXMAP,
            win as Drawable,
            gc,
            pm.width() as u16,
            pm.height() as u16,
            x as i16,
            y as i16,
            0,
            self.argb_depth,
            &buf,
        )?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn destroy_window(&self, win: Window) {
        let _ = xproto::destroy_window(&self.conn, win);
        let _ = self.conn.flush();
    }

    /// 设置输入区域：白=可点击。rects 为空且 full=true 表示恢复整窗可输入。
    pub fn set_input_shape(&self, win: Window, rects: &[Rectangle], full: bool) -> XResult<()> {
        if full {
            shape::mask(&self.conn, SO::SET, SK::INPUT, win, 0, 0, x11rb::NONE)?;
            self.conn.flush()?;
            return Ok(());
        }
        let mask = self.conn.generate_id()?;
        xproto::create_pixmap(
            &self.conn,
            1,
            mask,
            win,
            self.width as u16,
            self.height as u16,
        )?;
        let gc = self.conn.generate_id()?;
        xproto::create_gc(&self.conn, gc, mask, &CreateGCAux::new().foreground(0u32))?;
        let all = [Rectangle {
            x: 0,
            y: 0,
            width: self.width as u16,
            height: self.height as u16,
        }];
        xproto::poly_fill_rectangle(&self.conn, mask, gc, &all)?;
        if !rects.is_empty() {
            xproto::change_gc(&self.conn, gc, &xproto::ChangeGCAux::new().foreground(1u32))?;
            xproto::poly_fill_rectangle(&self.conn, mask, gc, rects)?;
        }
        shape::mask(&self.conn, SO::SET, SK::INPUT, win, 0, 0, mask)?;
        xproto::free_gc(&self.conn, gc)?;
        xproto::free_pixmap(&self.conn, mask)?;
        self.conn.flush()?;
        Ok(())
    }

    pub fn fake_key(&self, keycode: Keycode) {
        if keycode == 0 {
            return;
        }
        let _ = xtest::fake_input(
            &self.conn,
            xproto::KEY_PRESS_EVENT,
            keycode,
            0,
            self.root,
            0,
            0,
            0,
        );
        let _ = xtest::fake_input(
            &self.conn,
            xproto::KEY_RELEASE_EVENT,
            keycode,
            0,
            self.root,
            0,
            0,
            0,
        );
        let _ = self.conn.flush();
    }

    pub fn virtual_right_click(&self, x: i32, y: i32) {
        let _ = xtest::fake_input(
            &self.conn,
            xproto::MOTION_NOTIFY_EVENT,
            0,
            0,
            self.root,
            x as i16,
            y as i16,
            0,
        );
        let _ = xtest::fake_input(
            &self.conn,
            xproto::BUTTON_PRESS_EVENT,
            3,
            0,
            self.root,
            x as i16,
            y as i16,
            0,
        );
        let _ = xtest::fake_input(
            &self.conn,
            xproto::BUTTON_RELEASE_EVENT,
            3,
            0,
            self.root,
            x as i16,
            y as i16,
            0,
        );
        let _ = self.conn.flush();
    }

    /// 抓取 Ctrl+Shift+D 全局热键（含 NumLock/CapsLock 组合）
    pub fn grab_hotkey(&self) -> bool {
        if self.key_d == 0 {
            return false;
        }
        let mods = ModMask::CONTROL | ModMask::SHIFT;
        let combos = [
            mods,
            mods | ModMask::from(1u16 << 1),  // Mod2 (NumLock)
            mods | ModMask::from(1u16),       // Lock (CapsLock)
            mods | ModMask::from(1u16 << 1) | ModMask::from(1u16),
        ];
        for m in combos {
            let _ = xproto::grab_key(
                &self.conn,
                false,
                self.root,
                m,
                self.key_d,
                xproto::GrabMode::ASYNC,
                xproto::GrabMode::ASYNC,
            );
        }
        let _ = self.conn.flush();
        true
    }

    pub fn has_compositor(&self) -> bool {
        if let Ok(cookie) = xproto::get_selection_owner(&self.conn, self.net_wm_cm) {
            if let Ok(reply) = cookie.reply() {
                return reply.owner != x11rb::NONE;
            }
        }
        false
    }

    /// WPS/OnlyOffice/LibreOffice 全屏放映检测（递归窗口树）
    pub fn is_presentation_fullscreen(&self, self_win: Window) -> bool {
        let mut stack: Vec<Window> = vec![self.root];
        let mut count = 0;
        while let Some(w) = stack.pop() {
            if count > 6000 {
                break;
            }
            count += 1;
            if w == self_win {
                continue;
            }
            if self.is_office_window(w) {
                let attr = xproto::get_window_attributes(&self.conn, w)
                    .ok()
                    .and_then(|c| c.reply().ok());
                if let Some(attr) = attr {
                    if attr.map_state == xproto::MapState::VIEWABLE {
                        let mut fullscreen = false;
                        let prop = xproto::get_property(
                            &self.conn,
                            false,
                            w,
                            self.net_wm_state,
                            AtomEnum::ATOM,
                            0,
                            1024,
                        )
                        .ok()
                        .and_then(|c| c.reply().ok());
                        if let Some(prop) = prop {
                            for chunk in prop.value.chunks_exact(4) {
                                let a = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                                if a == self.net_wm_fullscreen {
                                    fullscreen = true;
                                    break;
                                }
                            }
                        }
                        if !fullscreen {
                            let geo = xproto::get_geometry(&self.conn, w)
                                .ok()
                                .and_then(|c| c.reply().ok());
                            if let Some(geo) = geo {
                                if geo.width as i32 >= self.width - 8
                                    && geo.height as i32 >= self.height - 8
                                {
                                    fullscreen = true;
                                }
                            }
                        }
                        if fullscreen {
                            return true;
                        }
                    }
                }
            }
            let tree = xproto::query_tree(&self.conn, w)
                .ok()
                .and_then(|c| c.reply().ok());
            if let Some(tree) = tree {
                for ch in tree.children {
                    stack.push(ch);
                }
            }
        }
        false
    }

    fn is_office_window(&self, w: Window) -> bool {
        let prop = xproto::get_property(
            &self.conn,
            false,
            w,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            0,
            1024,
        )
        .ok()
        .and_then(|c| c.reply().ok());
        let Some(prop) = prop else {
            return false;
        };
        let name = String::from_utf8_lossy(&prop.value).to_lowercase();
        const KEYS: &[&str] = &[
            "wps",
            "wpp",
            "et",
            "wpspdf",
            "onlyoffice",
            "desktopeditors",
            "soffice",
            "impress",
            "libreoffice",
        ];
        KEYS.iter().any(|k| name.contains(k))
    }

    /// 抓取整个根窗口，返回 RGBA 像素
    pub fn screenshot(&self) -> Option<(Vec<u8>, u32, u32)> {
        let w = self.width as u16;
        let h = self.height as u16;
        let reply = xproto::get_image(
            &self.conn,
            ImageFormat::Z_PIXMAP,
            self.root,
            0,
            0,
            w,
            h,
            u32::MAX,
        )
        .ok()?
        .reply()
        .ok()?;
        let mut rgba = vec![0u8; w as usize * h as usize * 4];
        for i in 0..(w as usize * h as usize) {
            let b = reply.data.get(i * 4).copied().unwrap_or(0);
            let g = reply.data.get(i * 4 + 1).copied().unwrap_or(0);
            let r = reply.data.get(i * 4 + 2).copied().unwrap_or(0);
            rgba[i * 4] = r;
            rgba[i * 4 + 1] = g;
            rgba[i * 4 + 2] = b;
            rgba[i * 4 + 3] = 255;
        }
        Some((rgba, w as u32, h as u32))
    }

    /// XShape 扩展是否可用
    pub fn shape_available(&self) -> bool {
        shape::query_version(&self.conn)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some()
    }
}

// ============================================================
// X11 后端实现（Backend trait）
// ============================================================
pub struct X11Plat {
    pub x11: X11,
    pub win: Window,
    pub gc: Gcontext,
    touch_axis: RefCell<HashMap<u8, TouchAxis>>,
}

#[derive(Clone, Copy, Default)]
struct TouchAxis {
    major: Option<u16>,
    posx: Option<u16>,
    posx_min: f64,
    posx_max: f64,
}

impl X11Plat {
    pub fn new(x11: X11, win: Window, gc: Gcontext) -> Self {
        X11Plat {
            x11,
            win,
            gc,
            touch_axis: RefCell::new(HashMap::new()),
        }
    }

    /// 由 XI2 触摸事件的 valuator 计算触点直径（像素）
    pub fn touch_diameter(&self, devid: u8, mask: &[u32], values: &[Fp3232]) -> f64 {
        if !self.touch_axis.borrow().contains_key(&devid) {
            let info = self.load_touch_axis(devid);
            self.touch_axis.borrow_mut().insert(devid, info);
        }
        let axis = *self.touch_axis.borrow().get(&devid).unwrap();
        let Some(mi) = axis.major else {
            return 0.0;
        };
        let Some(mv) = fp_value(mask, values, mi) else {
            return 0.0;
        };
        let scale = if axis.posx_max > axis.posx_min {
            (self.x11.width as f64) / (axis.posx_max - axis.posx_min)
        } else {
            1.0
        };
        (mv * scale).clamp(0.0, 4096.0)
    }

    fn load_touch_axis(&self, devid: u8) -> TouchAxis {
        let mut a = TouchAxis::default();
        if let Some(reply) = xinput::xi_query_device(&self.x11.conn, devid)
            .ok()
            .and_then(|c| c.reply().ok())
        {
            for info in &reply.infos {
                if info.deviceid as u8 != devid {
                    continue;
                }
                for class in &info.classes {
                    if let DeviceClassData::Valuator(v) = &class.data {
                        if v.label == self.x11.atom_touch_major
                            || v.label == self.x11.atom_touch_minor
                        {
                            if a.major.is_none() {
                                a.major = Some(v.number);
                            }
                        } else if v.label == self.x11.atom_pos_x {
                            a.posx = Some(v.number);
                            a.posx_min = fp3232_to_f64(v.min);
                            a.posx_max = fp3232_to_f64(v.max);
                        }
                    }
                }
            }
        }
        log::info!(
            "[TOUCH] dev {} major_idx={:?} posx={:?} range=({:.0},{:.0})",
            devid,
            a.major,
            a.posx,
            a.posx_min,
            a.posx_max
        );
        a
    }
}

fn fp3232_to_f64(v: Fp3232) -> f64 {
    v.integral as f64 + (v.frac as f64) / 4294967296.0
}

/// 从 XI2 valuator_mask/axisvalues 中取指定 valuator 的值
fn fp_value(mask: &[u32], values: &[Fp3232], idx: u16) -> Option<f64> {
    let mut k = 0usize;
    for (i, m) in mask.iter().enumerate() {
        let mut bits = *m;
        while bits != 0 {
            let b = bits.trailing_zeros();
            let number = (i as u32) * 32 + b;
            if number as u16 == idx {
                return values.get(k).map(|v| fp3232_to_f64(*v));
            }
            k += 1;
            bits &= bits - 1;
        }
    }
    None
}

impl crate::backend::Backend for X11Plat {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn screen_size(&self) -> (i32, i32) {
        (self.x11.width, self.x11.height)
    }
    fn present(&self, pm: &tiny_skia::Pixmap, x: i32, y: i32) {
        let _ = self.x11.put_pixmap(self.win, self.gc, pm, x, y);
    }
    fn set_input_region(&self, rects: &[crate::backend::IRect], full: bool) {
        let xr: Vec<Rectangle> = rects
            .iter()
            .map(|r| Rectangle {
                x: r.x as i16,
                y: r.y as i16,
                width: r.w.max(0) as u16,
                height: r.h.max(0) as u16,
            })
            .collect();
        let _ = self.x11.set_input_shape(self.win, &xr, full);
    }
    fn fake_key(&self, key: crate::backend::Key) {
        use crate::backend::Key;
        let kc = match key {
            Key::Up => self.x11.key_up,
            Key::Down => self.x11.key_down,
            Key::Escape => self.x11.key_escape,
        };
        self.x11.fake_key(kc);
    }
    fn virtual_right_click(&self, x: i32, y: i32) {
        self.x11.virtual_right_click(x, y);
    }
    fn screenshot(&self) -> Option<(Vec<u8>, u32, u32)> {
        self.x11.screenshot()
    }
    fn is_presentation_fullscreen(&self) -> bool {
        self.x11.is_presentation_fullscreen(self.win)
    }
    fn has_compositor(&self) -> bool {
        self.x11.has_compositor()
    }
    fn shape_available(&self) -> bool {
        self.x11.shape_available()
    }
    fn grab_hotkey(&self) -> bool {
        self.x11.grab_hotkey()
    }
    fn show_splash(&self, fonts: &crate::text::Fonts, icon: Option<&tiny_skia::Pixmap>, dur_ms: u64) {
        crate::splash::show(&self.x11, fonts, icon, dur_ms);
    }
}
