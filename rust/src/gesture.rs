// ============================================================
// Sidera - 手掌/大触点橡皮手势引擎（移植自 C++ widget.h 的 FSM）
// 单点=画笔；多点/大触点=橡皮，锁存到全部抬手
// ============================================================
use crate::app::{now_ms, App, PALM_ERASE_WIDTH};
use crate::paint::{stroke_segment, stroke_tapered};
use crate::ui::R;

#[derive(Clone, Copy, PartialEq)]
pub enum TouchKind {
    Begin,
    Update,
    End,
}

fn dirty_of(a: (i32, i32), b: (i32, i32), w: i32) -> R {
    let x1 = a.0.min(b.0) - w;
    let y1 = a.1.min(b.1) - w;
    let x2 = a.0.max(b.0) + w;
    let y2 = a.1.max(b.1) + w;
    R {
        x: x1,
        y: y1,
        w: x2 - x1,
        h: y2 - y1,
    }
}

pub fn union(a: Option<R>, b: R) -> R {
    match a {
        None => b,
        Some(a) => {
            let x1 = a.x.min(b.x);
            let y1 = a.y.min(b.y);
            let x2 = (a.x + a.w).max(b.x + b.w);
            let y2 = (a.y + a.h).max(b.y + b.h);
            R {
                x: x1,
                y: y1,
                w: x2 - x1,
                h: y2 - y1,
            }
        }
    }
}

/// 处理一个触摸事件；返回需要重绘的区域
pub fn touch_event(
    app: &mut App,
    id: i32,
    x: i32,
    y: i32,
    kind: TouchKind,
    diameter: f64,
) -> Option<R> {
    let mut now = app.t_prev_pos.clone();
    match kind {
        TouchKind::Begin | TouchKind::Update => {
            now.insert(id, (x, y));
        }
        TouchKind::End => {
            now.remove(&id);
        }
    }
    let mut dirty: Option<R> = None;

    let mode = app.mode;
    let mode_erase_only = mode == 2;

    // 新手势
    if app.t_prev_pos.is_empty() && !now.is_empty() {
        app.push_undo();
        app.t_moved = true;
        app.t_begin_pos = now.values().next().cloned().unwrap_or((0, 0));
        app.t_begin_role = 0;
        app.stroke_start_ms = now_ms();
        app.last_pen_w = app.timed_pen_width();
    }

    let n = now.len();
    let any_large = diameter >= app.large_touch_threshold;
    let large_trigger = (!app.erase_by_finger) && (any_large || n >= 2);
    let multi_trigger = app.erase_by_finger && n >= 2;
    if large_trigger || multi_trigger {
        app.t_palm_latched = true;
    }
    if large_trigger {
        app.palm_erase_w = ((diameter * app.large_erase_scale10 as f64 / 10.0).round() as i32)
            .clamp(PALM_ERASE_WIDTH, 1024);
    } else {
        app.palm_erase_w = PALM_ERASE_WIDTH;
    }
    if app.t_begin_role == 0 && !now.is_empty() {
        app.t_begin_role = if mode_erase_only || app.t_palm_latched {
            2
        } else {
            1
        };
        let bp = app.t_begin_pos;
        if app.t_begin_role == 1 {
            stroke_tapered(app, bp, bp, app.last_pen_w, app.last_pen_w);
        } else {
            let w = if mode == 2 {
                app.eraser_width()
            } else {
                app.palm_erase_w
            };
            stroke_segment(app, bp, bp, true, w);
        }
    }

    let mut ids: Vec<i32> = now.keys().cloned().collect();
    ids.sort_unstable();
    let pen_id = if !mode_erase_only && !app.t_palm_latched && !ids.is_empty() {
        ids[0]
    } else {
        -1
    };

    let mut role_map = std::collections::HashMap::new();
    for &i in &ids {
        let role = if mode_erase_only || app.t_palm_latched {
            2
        } else if i == pen_id {
            1
        } else {
            0
        };
        role_map.insert(i, role);
    }

    for &i in &ids {
        let cur = now[&i];
        let cur_role = role_map[&i];
        let have_old = app.t_prev_pos.contains_key(&i);
        let old_role = app.t_prev_role.get(&i).copied().unwrap_or(-1);
        let old_pos = app.t_prev_pos.get(&i).copied().unwrap_or(cur);
        if !have_old || cur_role != old_role {
            continue;
        }
        if old_pos == cur || cur_role == 0 {
            continue;
        }
        let w = if cur_role == 1 {
            app.timed_pen_width()
        } else if mode == 2 {
            app.eraser_width()
        } else {
            app.palm_erase_w
        };
        if cur_role == 1 {
            stroke_tapered(app, old_pos, cur, app.last_pen_w, w);
        } else {
            stroke_segment(app, old_pos, cur, true, w);
        }
        app.last_pen_w = w;
        app.t_moved = true;
        dirty = Some(union(dirty, dirty_of(old_pos, cur, w)));
    }

    // 圆形橡皮预览
    let prev_preview = app.t_prev_preview.clone();
    app.palm_erase_preview.clear();
    for &i in &ids {
        if role_map.get(&i).copied().unwrap_or(2) == 2 {
            app.palm_erase_preview.push(now[&i]);
        }
    }
    if !prev_preview.is_empty() || !app.palm_erase_preview.is_empty() {
        for p in prev_preview.iter().chain(app.palm_erase_preview.iter()) {
            let pad = if mode == 2 {
                app.eraser_width()
            } else {
                app.palm_erase_w
            } + 8;
            dirty = Some(union(
                dirty,
                R {
                    x: p.0 - pad,
                    y: p.1 - pad,
                    w: pad * 2,
                    h: pad * 2,
                },
            ));
        }
    }
    app.t_prev_preview = app.palm_erase_preview.clone();

    // 更新状态
    app.t_prev_pos = now;
    app.t_prev_role = role_map;

    if app.t_prev_pos.is_empty() {
        if !app.t_moved && !app.t_palm_latched && mode != 0 && app.t_begin_role != 0 {
            let bp = app.t_begin_pos;
            if app.t_begin_role == 1 {
                let w = app.timed_pen_width();
                stroke_segment(app, bp, bp, false, w);
            } else {
                let w = if mode == 2 {
                    app.eraser_width()
                } else {
                    app.palm_erase_w
                };
                stroke_segment(app, bp, bp, true, w);
            }
        }
        app.reset_palm_gesture();
        dirty = Some(R {
            x: 0,
            y: 0,
            w: app.screen_w,
            h: app.screen_h,
        });
    }

    dirty
}
