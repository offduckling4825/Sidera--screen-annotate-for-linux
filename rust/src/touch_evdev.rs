// ============================================================
// Sidera - 直接读内核 evdev 的触摸输入（绕开 XInput2 Touch 类）
//   适用于 X 服务器未暴露 XITouchClass、但内核有 MT 协议的触摸屏/白板。
//   - 始终排除指点设备(触摸板)，避免误抓
//   - MT protocol B: ABS_MT_SLOT + TRACKING_ID + POSITION_X/Y (+TOUCH_MAJOR)
//   - MT protocol A: 无 slot/tracking，按 SYN 帧成对解析 + 跨帧最近邻配对
//   - 单点绝对设备: ABS_X/Y + BTN_TOUCH
//   读线程按 SYN_REPORT 成帧，通过 channel 发到主线程。
// ============================================================
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

use evdev::{AbsoluteAxisCode, Device, EventType, KeyCode, PropType, RelativeAxisCode};

#[derive(Clone, Copy, PartialEq)]
pub enum Phase {
    Begin,
    Update,
    End,
}

pub struct Contact {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub major: f64,
    pub phase: Phase,
}

struct Map {
    lw: f64,
    lh: f64,
    xcode: AbsoluteAxisCode,
    ycode: AbsoluteAxisCode,
    mcode: AbsoluteAxisCode,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    mmin: f64,
    mmax: f64,
    has_major: bool,
    has_tracking: bool,
    is_mt: bool,
}

impl Map {
    fn mapx(&self, v: i32) -> i32 {
        if self.xmax > self.xmin {
            (((v as f64 - self.xmin) / (self.xmax - self.xmin)) * self.lw).round() as i32
        } else {
            v
        }
    }
    fn mapy(&self, v: i32) -> i32 {
        if self.ymax > self.ymin {
            (((v as f64 - self.ymin) / (self.ymax - self.ymin)) * self.lh).round() as i32
        } else {
            v
        }
    }
    /// 触点直径（逻辑像素）：优先 major 轴自身量程，其次退回 X 轴量程
    fn mapmajor(&self, v: i32) -> f64 {
        if !self.has_major {
            return 0.0;
        }
        if self.mmax > self.mmin {
            ((v as f64 - self.mmin) / (self.mmax - self.mmin)) * self.lw
        } else if self.xmax > self.xmin {
            (v as f64) * self.lw / (self.xmax - self.xmin)
        } else {
            v as f64
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Slot {
    active: bool,
    id: i32,
    x: i32,
    y: i32,
    major: i32,
    reported: bool,
    rx: i32,
    ry: i32,
    rmajor: i32,
}

/// 协议 A 一帧内的一个待配对触点
struct Pending {
    x: Option<i32>,
    y: Option<i32>,
    major: i32,
}

/// 扫描并启动 evdev 触摸读取线程；返回联系人帧的接收端
pub fn spawn(logical_w: i32, logical_h: i32) -> Option<Receiver<Vec<Contact>>> {
    let (tx, rx) = channel::<Vec<Contact>>();

    let mut cands: Vec<(PathBuf, Device, bool, bool, Map, bool)> = Vec::new();
    let mut any_direct = false;
    for (path, dev) in evdev::enumerate() {
        let Some(axes) = dev.supported_absolute_axes() else {
            continue;
        };
        let has_mtx = axes.contains(AbsoluteAxisCode::ABS_MT_POSITION_X);
        let has_mty = axes.contains(AbsoluteAxisCode::ABS_MT_POSITION_Y);
        let has_x = axes.contains(AbsoluteAxisCode::ABS_X);
        let has_y = axes.contains(AbsoluteAxisCode::ABS_Y);
        let is_mt = has_mtx && has_mty;
        if !(is_mt || (has_x && has_y)) {
            continue;
        }
        let (xcode, ycode) = if is_mt {
            (
                AbsoluteAxisCode::ABS_MT_POSITION_X,
                AbsoluteAxisCode::ABS_MT_POSITION_Y,
            )
        } else {
            (AbsoluteAxisCode::ABS_X, AbsoluteAxisCode::ABS_Y)
        };
        let has_major = axes.contains(AbsoluteAxisCode::ABS_MT_TOUCH_MAJOR)
            || axes.contains(AbsoluteAxisCode::ABS_MT_WIDTH_MAJOR);
        let mcode = if axes.contains(AbsoluteAxisCode::ABS_MT_TOUCH_MAJOR) {
            AbsoluteAxisCode::ABS_MT_TOUCH_MAJOR
        } else {
            AbsoluteAxisCode::ABS_MT_WIDTH_MAJOR
        };
        // INPUT_PROP_DIRECT(1)：直接对应屏幕；INPUT_PROP_POINTER(0)：指点设备(触摸板)
        let direct = dev.properties().contains(PropType(1));
        let pointer = dev.properties().contains(PropType(0));
        let is_touchscreen = direct || !pointer;
        if direct {
            any_direct = true;
        }
        let mut map = Map {
            lw: logical_w as f64,
            lh: logical_h as f64,
            xcode,
            ycode,
            mcode,
            xmin: 0.0,
            xmax: logical_w as f64,
            ymin: 0.0,
            ymax: logical_h as f64,
            mmin: 0.0,
            mmax: 0.0,
            has_major,
            has_tracking: axes.contains(AbsoluteAxisCode::ABS_MT_TRACKING_ID),
            is_mt,
        };
        if let Ok(it) = dev.get_absinfo() {
            for (code, info) in it {
                if code == xcode {
                    map.xmin = info.minimum() as f64;
                    map.xmax = info.maximum() as f64;
                } else if code == ycode {
                    map.ymin = info.minimum() as f64;
                    map.ymax = info.maximum() as f64;
                } else if has_major && code == mcode {
                    map.mmin = info.minimum() as f64;
                    map.mmax = info.maximum() as f64;
                }
            }
        }
        cands.push((path, dev, direct, is_touchscreen, map, is_mt));
    }

    let mut any = false;
    for (path, dev, direct, is_touchscreen, map, is_mt) in cands {
        if !is_touchscreen {
            log::info!("[EVDEV] 跳过指点设备(触摸板) {}", path.display());
            continue;
        }
        if any_direct && !direct {
            log::info!("[EVDEV] 跳过非直接输入设备 {}", path.display());
            continue;
        }
        let proto = if is_mt {
            if map.has_tracking {
                "B"
            } else {
                "A"
            }
        } else {
            "single"
        };
        log::info!(
            "[EVDEV] 触摸设备 {} '{}' 协议={} direct={} x=[{:.0},{:.0}] y=[{:.0},{:.0}] major={} max={:.0}",
            path.display(),
            dev.name().unwrap_or("?"),
            proto,
            direct,
            map.xmin,
            map.xmax,
            map.ymin,
            map.ymax,
            map.has_major,
            map.mmax
        );
        let tx2 = tx.clone();
        std::thread::spawn(move || read_loop(dev, map, tx2));
        any = true;
    }

    if !any {
        log::warn!("[EVDEV] 未发现可读的直接触摸设备（可能需要把用户加入 input 组）");
    }
    if any {
        Some(rx)
    } else {
        None
    }
}

/// 是否存在真实鼠标/触摸板（相对位移设备）——用于判断能否完全忽略 X 合成鼠标
pub fn has_relative_pointer() -> bool {
    for (_, dev) in evdev::enumerate() {
        if let Some(rel) = dev.supported_relative_axes() {
            if rel.contains(RelativeAxisCode::REL_X) && rel.contains(RelativeAxisCode::REL_Y) {
                return true;
            }
        }
    }
    false
}

fn read_loop(mut dev: Device, map: Map, tx: Sender<Vec<Contact>>) {
    let mut slots: Vec<Slot> = vec![Slot::default(); 64];
    let mut cur: usize = 0;
    // 协议 A 用
    let mut frame: Vec<Pending> = Vec::new();
    let mut prev: Vec<(i32, i32, i32, i32)> = Vec::new(); // (id, x, y, raw major)
    let mut next_id: i32 = 1;

    loop {
        let events = match dev.fetch_events() {
            Ok(e) => e,
            Err(_) => {
                log::warn!("[EVDEV] 触摸设备读取结束");
                return;
            }
        };
        let mut out: Vec<Contact> = Vec::new();
        for ev in events {
            match ev.event_type() {
                EventType::ABSOLUTE => {
                    let code = ev.code();
                    let v = ev.value();
                    if map.is_mt && !map.has_tracking {
                        // 协议 A：按帧累积 (X,Y,major)
                        if code == map.xcode.0 {
                            match frame.iter_mut().find(|p| p.x.is_none()) {
                                Some(p) => p.x = Some(v),
                                None => frame.push(Pending {
                                    x: Some(v),
                                    y: None,
                                    major: 0,
                                }),
                            }
                        } else if code == map.ycode.0 {
                            match frame.iter_mut().find(|p| p.y.is_none()) {
                                Some(p) => p.y = Some(v),
                                None => frame.push(Pending {
                                    x: None,
                                    y: Some(v),
                                    major: 0,
                                }),
                            }
                        } else if map.has_major && code == map.mcode.0 {
                            if let Some(last) = frame.last_mut() {
                                last.major = v;
                            }
                        }
                    } else if code == AbsoluteAxisCode::ABS_MT_SLOT.0 {
                        if v >= 0 && (v as usize) < slots.len() {
                            cur = v as usize;
                        }
                    } else if code == AbsoluteAxisCode::ABS_MT_TRACKING_ID.0 {
                        let s = &mut slots[cur];
                        if v == -1 {
                            if s.active && s.reported {
                                out.push(Contact {
                                    id: s.id,
                                    x: 0,
                                    y: 0,
                                    major: 0.0,
                                    phase: Phase::End,
                                });
                                s.reported = false;
                            }
                            s.active = false;
                        } else {
                            s.active = true;
                            s.id = v;
                            s.reported = false;
                        }
                    } else if code == map.xcode.0 {
                        slots[cur].x = v;
                    } else if code == map.ycode.0 {
                        slots[cur].y = v;
                    } else if map.has_major && code == map.mcode.0 {
                        slots[cur].major = v;
                    }
                }
                EventType::KEY => {
                    // 单点绝对设备（无 MT）：用 BTN_TOUCH 表示按下/抬起
                    if !map.is_mt && ev.code() == KeyCode::BTN_TOUCH.0 {
                        if ev.value() == 1 {
                            slots[0].active = true;
                            slots[0].id = 0;
                            slots[0].reported = false;
                        } else {
                            if slots[0].active && slots[0].reported {
                                out.push(Contact {
                                    id: slots[0].id,
                                    x: 0,
                                    y: 0,
                                    major: 0.0,
                                    phase: Phase::End,
                                });
                            }
                            slots[0].active = false;
                            slots[0].reported = false;
                        }
                    }
                }
                EventType::SYNCHRONIZATION => {
                    if map.is_mt && !map.has_tracking {
                        process_frame_a(&mut prev, &mut next_id, &frame, &map, &mut out);
                        frame.clear();
                    } else {
                        for s in slots.iter_mut() {
                            if !s.active {
                                continue;
                            }
                            let x = map.mapx(s.x);
                            let y = map.mapy(s.y);
                            let m = map.mapmajor(s.major);
                            if !s.reported {
                                out.push(Contact {
                                    id: s.id,
                                    x,
                                    y,
                                    major: m,
                                    phase: Phase::Begin,
                                });
                                s.reported = true;
                                s.rx = x;
                                s.ry = y;
                                s.rmajor = s.major;
                            } else if x != s.rx || y != s.ry || s.major != s.rmajor {
                                out.push(Contact {
                                    id: s.id,
                                    x,
                                    y,
                                    major: m,
                                    phase: Phase::Update,
                                });
                                s.rx = x;
                                s.ry = y;
                                s.rmajor = s.major;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if !out.is_empty() && tx.send(out).is_err() {
            return;
        }
    }
}

/// 协议 A 一帧：把新触点与上一帧按最近邻配对，产出 Begin/Update/End
fn process_frame_a(
    prev: &mut Vec<(i32, i32, i32, i32)>,
    next_id: &mut i32,
    frame: &[Pending],
    map: &Map,
    out: &mut Vec<Contact>,
) {
    let news: Vec<(i32, i32, i32)> = frame
        .iter()
        .filter_map(|p| match (p.x, p.y) {
            (Some(x), Some(y)) => Some((map.mapx(x), map.mapy(y), p.major)),
            _ => None,
        })
        .collect();
    let mut used = vec![false; prev.len()];
    let mut result: Vec<(i32, i32, i32, i32)> = Vec::new();
    const THRESH2: f64 = 300.0 * 300.0; // 跨帧最大匹配距离^2
    for &(x, y, major) in &news {
        let mut best: Option<usize> = None;
        let mut bestd = THRESH2;
        for (i, &(_, px, py, _)) in prev.iter().enumerate() {
            if used[i] {
                continue;
            }
            let dx = (x - px) as f64;
            let dy = (y - py) as f64;
            let d = dx * dx + dy * dy;
            if d < bestd {
                bestd = d;
                best = Some(i);
            }
        }
        let id = if let Some(i) = best {
            used[i] = true;
            let (id, px, py, pmajor) = prev[i];
            if x != px || y != py || major != pmajor {
                out.push(Contact {
                    id,
                    x,
                    y,
                    major: map.mapmajor(major),
                    phase: Phase::Update,
                });
            }
            id
        } else {
            let id = *next_id;
            *next_id = next_id.wrapping_add(1).max(1);
            out.push(Contact {
                id,
                x,
                y,
                major: map.mapmajor(major),
                phase: Phase::Begin,
            });
            id
        };
        result.push((id, x, y, major));
    }
    for (i, &(id, _, _, _)) in prev.iter().enumerate() {
        if !used[i] {
            out.push(Contact {
                id,
                x: 0,
                y: 0,
                major: 0.0,
                phase: Phase::End,
            });
        }
    }
    *prev = result;
}

/// 是否存在疑似触摸屏的事件设备（读 sysfs，无需打开设备）
/// 条件：有位置轴(ABS_MT_POSITION_X 或 ABS_X)，且不是指点设备(排除触摸板)
pub fn has_touch_node() -> bool {
    let Ok(rd) = std::fs::read_dir("/sys/class/input") else {
        return false;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("event") {
            continue;
        }
        let dir = e.path().join("device");
        let props = std::fs::read_to_string(dir.join("properties")).unwrap_or_default();
        let pointer = hex_bit(&props, 0); // INPUT_PROP_POINTER
        let direct = hex_bit(&props, 1); // INPUT_PROP_DIRECT
        let abs = std::fs::read_to_string(dir.join("capabilities/abs")).unwrap_or_default();
        // ABS_MT_POSITION_X(0x35) 或 ABS_X(0)
        if !(hex_bit(&abs, 0x35) || hex_bit(&abs, 0)) {
            continue;
        }
        if pointer && !direct {
            continue; // 触摸板/指点杆
        }
        return true;
    }
    false
}

/// 当前用户是否已在 input 组
pub fn in_input_group() -> bool {
    let gid = std::fs::read_to_string("/etc/group").ok().and_then(|s| {
        s.lines().find_map(|l| {
            let mut it = l.split(':');
            if it.next() == Some("input") {
                it.next();
                it.next().and_then(|g| g.parse::<u32>().ok())
            } else {
                None
            }
        })
    });
    let Some(gid) = gid else {
        return false;
    };
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Groups:") {
                return rest.split_whitespace().any(|g| g.parse::<u32>() == Ok(gid));
            }
        }
    }
    false
}

/// 检查 sysfs 十六进制位掩码里第 bit 位是否置位
fn hex_bit(words: &str, bit: u32) -> bool {
    let word = (bit / 64) as usize;
    let Some(w) = words.split_whitespace().nth(word) else {
        return false;
    };
    match u64::from_str_radix(w.trim_start_matches("0x"), 16) {
        Ok(v) => (v >> (bit % 64)) & 1 == 1,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn evdev_scan_no_panic() {
        // 本机可能没有触摸屏；只要枚举不 panic 即算通过（日志会打印发现的设备）
        let _ = super::spawn(1920, 1080);
    }
}
