//! Startup presentation. Completion follows real setup results; time only
//! controls readability and the visual handoff to the toolbar.
use std::time::{Duration, Instant};
use std::cell::RefCell;
use tiny_skia::Pixmap;

pub const CARD_WIDTH: i32 = 560;
pub const CARD_HEIGHT: i32 = 448;
const READ_TIME: Duration = Duration::from_millis(1100);
const READY_TIME: Duration = Duration::from_millis(250);
const FADE_TIME: Duration = Duration::from_millis(220);
const REVEAL_DELAY: Duration = Duration::from_millis(140);
const REVEAL_TIME: Duration = Duration::from_millis(380);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState { Preparing, Disabled, Waiting, Connected, Unavailable }

impl LinkState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preparing => "正在准备课件联动",
            Self::Disabled => "已按设置关闭",
            Self::Waiting => "服务已准备，等待 WPS 接入",
            Self::Connected => "已接入 WPS",
            Self::Unavailable => "联动暂不可用，可继续批注",
        }
    }
}

pub struct Launch {
    pub desktop: String,
    pub resources_ok: bool,
    pub hotkey: bool,
    pub link: LinkState,
    // Reuse the rendered card during its fade; fonts are not rasterized per frame.
    pub card: RefCell<Option<Pixmap>>,
    started: Option<Instant>,
    ready_at: Option<Instant>,
    panel_alpha: f32,
    reveal: f32,
}

impl Default for Launch {
    fn default() -> Self {
        Self { desktop: String::new(), resources_ok: true, hotkey: false,
            link: LinkState::Preparing, card: RefCell::new(None), started: None, ready_at: None,
            panel_alpha: 0.0, reveal: 1.0 }
    }
}

impl Launch {
    pub fn begin(desktop: String, resources_ok: bool, hotkey: bool, now: Instant) -> Self {
        Self { desktop, resources_ok, hotkey, link: LinkState::Preparing, card: RefCell::new(None),
            started: Some(now), ready_at: None, panel_alpha: 1.0, reveal: 0.0 }
    }
    pub fn active(&self) -> bool { self.started.is_some() }
    pub fn panel_alpha(&self) -> f32 { self.panel_alpha }
    pub fn reveal(&self) -> f32 { self.reveal }
    pub fn ready(&self) -> bool { self.ready_at.is_some() }
    pub fn completed(&self) -> usize { if self.ready() { 4 } else { 3 } }

    pub fn report_link(&mut self, link: LinkState, now: Instant) -> bool {
        if !self.active() { return false; }
        let changed = self.link != link;
        self.link = link;
        if changed { self.card.get_mut().take(); }
        if link != LinkState::Preparing && self.ready_at.is_none() { self.ready_at = Some(now); }
        changed
    }

    pub fn advance_at(&mut self, now: Instant) -> bool {
        let (Some(started), Some(ready)) = (self.started, self.ready_at) else { return false; };
        let exit = (started + READ_TIME).max(ready + READY_TIME);
        let elapsed = now.saturating_duration_since(exit);
        let fade = (elapsed.as_secs_f32() / FADE_TIME.as_secs_f32()).clamp(0.0, 1.0);
        let reveal = (elapsed.saturating_sub(REVEAL_DELAY).as_secs_f32()
            / REVEAL_TIME.as_secs_f32()).clamp(0.0, 1.0);
        let old = (self.panel_alpha, self.reveal);
        self.panel_alpha = 1.0 - fade * fade * (3.0 - 2.0 * fade);
        self.reveal = 1.0 - (1.0 - reveal).powi(3);
        if reveal >= 1.0 { self.finish(); }
        old != (self.panel_alpha, self.reveal)
    }

    pub fn finish(&mut self) {
        self.started = None;
        self.panel_alpha = 0.0;
        self.reveal = 1.0;
        self.card.get_mut().take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_cannot_invent_completion_and_optional_wps_never_blocks_startup() {
        let start = Instant::now();
        for link in [LinkState::Waiting, LinkState::Disabled, LinkState::Unavailable, LinkState::Connected] {
            let mut launch = Launch::begin("GNOME".into(), true, true, start);
            launch.advance_at(start + Duration::from_secs(10));
            assert!(launch.active() && !launch.ready());
            assert_eq!(launch.reveal(), 0.0);
            let ready = start + Duration::from_secs(10);
            launch.report_link(link, ready);
            launch.advance_at(ready + Duration::from_millis(200));
            assert_eq!(launch.panel_alpha(), 1.0);
            launch.advance_at(ready + Duration::from_secs(1));
            assert!(!launch.active());
            assert_eq!((launch.panel_alpha(), launch.reveal()), (0.0, 1.0));
        }
    }

    #[test]
    fn handoff_is_monotonic_and_heartbeats_do_not_restart_it() {
        let start = Instant::now();
        let mut launch = Launch::begin("X11".into(), true, false, start);
        launch.report_link(LinkState::Waiting, start);
        let (mut alpha, mut reveal) = (1.0, 0.0);
        for ms in (0..=1800).step_by(20) {
            let now = start + Duration::from_millis(ms);
            launch.report_link(LinkState::Waiting, now);
            launch.advance_at(now);
            assert!(launch.panel_alpha() <= alpha && launch.reveal() >= reveal);
            (alpha, reveal) = (launch.panel_alpha(), launch.reveal());
        }
        assert!(!launch.active());
        assert!(!launch.report_link(LinkState::Connected, start + Duration::from_secs(10)));
        assert!(!launch.advance_at(start + Duration::from_secs(10)));
    }
}
