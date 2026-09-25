// Sidera - platform-independent annotation state
// Copyright (C) 2026 Carl_Jin
// SPDX-License-Identifier: GPL-3.0-only

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

pub const MAX_UNDO: usize = 12;
pub const WPS_CACHE_CAPACITY: usize = 20;
pub const WHITEBOARD_CACHE_CAPACITY: usize = 10;
pub const OTHER_CACHE_CAPACITY: usize = 2;
pub const WPS_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Cursor,
    Pen,
    Eraser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Rgba {
    pub const fn opaque(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeKind {
    Ink,
    Eraser,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub points: Vec<Point>,
    pub color: Rgba,
    pub width: f32,
    pub kind: StrokeKind,
}

impl Stroke {
    pub fn ink(points: Vec<Point>, color: Rgba, width: f32) -> Self {
        Self {
            points,
            color,
            width,
            kind: StrokeKind::Ink,
        }
    }

    pub fn eraser(points: Vec<Point>, width: f32) -> Self {
        Self {
            points,
            color: Rgba::opaque(0, 0, 0),
            width,
            kind: StrokeKind::Eraser,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Next,
    Previous,
}

impl Command {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Next => "NEXT",
            Self::Previous => "PREV",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WpsEvent {
    pub name: String,
    pub position: Option<u32>,
    pub click: Option<i32>,
}

impl WpsEvent {
    pub fn parse(line: &str) -> Option<Self> {
        let mut parts = line.split_whitespace();
        if parts.next()? != "EVENT" {
            return None;
        }
        let name = parts.next()?.to_owned();
        let mut position = None;
        let mut click = None;
        for token in parts {
            if let Some(value) = token.strip_prefix("pos=") {
                position = value.parse::<u32>().ok();
            } else if let Some(value) = token.strip_prefix("click=") {
                click = value.parse::<i32>().ok();
            }
        }
        Some(Self {
            name,
            position,
            click,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PageCache {
    capacity: usize,
    evict_oldest: bool,
    pages: BTreeMap<u32, Vec<Stroke>>,
}

impl PageCache {
    pub fn new(capacity: usize, evict_oldest: bool) -> Self {
        Self {
            capacity,
            evict_oldest,
            pages: BTreeMap::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn contains(&self, page: u32) -> bool {
        self.pages.contains_key(&page)
    }

    pub fn get(&self, page: u32) -> Option<&[Stroke]> {
        self.pages.get(&page).map(Vec::as_slice)
    }

    pub fn set_capacity(&mut self, capacity: usize, evict_oldest: bool) {
        self.capacity = capacity;
        self.evict_oldest = evict_oldest;
        while self.pages.len() > self.capacity {
            if let Some(oldest) = self.pages.keys().next().copied() {
                self.pages.remove(&oldest);
            } else {
                break;
            }
        }
    }

    pub fn save(&mut self, page: u32, strokes: &[Stroke]) -> bool {
        if strokes.is_empty() || self.capacity == 0 {
            return false;
        }
        if !self.pages.contains_key(&page) && self.pages.len() >= self.capacity {
            if !self.evict_oldest {
                return false;
            }
            if let Some(oldest) = self.pages.keys().next().copied() {
                self.pages.remove(&oldest);
            }
        }
        self.pages.insert(page, strokes.to_vec());
        true
    }

    pub fn remove(&mut self, page: u32) -> bool {
        self.pages.remove(&page).is_some()
    }

    pub fn clear(&mut self) {
        self.pages.clear();
    }
}

#[derive(Debug, Clone)]
pub struct UndoHistory {
    max_depth: usize,
    snapshots: Vec<Vec<Stroke>>,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new(MAX_UNDO)
    }
}

impl UndoHistory {
    pub fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            snapshots: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    pub fn push(&mut self, strokes: &[Stroke]) {
        if self.max_depth == 0 {
            return;
        }
        if self.snapshots.len() >= self.max_depth {
            self.snapshots.remove(0);
        }
        self.snapshots.push(strokes.to_vec());
    }

    pub fn pop(&mut self) -> Option<Vec<Stroke>> {
        self.snapshots.pop()
    }
}

#[derive(Debug, Clone)]
pub struct AnnotationState {
    pub mode: Mode,
    pub whiteboard: bool,
    pub current_page: u32,
    pub saved_slide: u32,
    pub page_has_ink: bool,
    pub current_strokes: Vec<Stroke>,
    pub slide_cache: PageCache,
    pub whiteboard_cache: PageCache,
    pub undo: UndoHistory,
    pub wps_connected: bool,
    pub wps_real_position: Option<u32>,
    wps_last_seen: Option<Instant>,
    commands: VecDeque<Command>,
}

impl Default for AnnotationState {
    fn default() -> Self {
        Self::new()
    }
}

impl AnnotationState {
    pub fn new() -> Self {
        Self {
            mode: Mode::Cursor,
            whiteboard: false,
            current_page: 1,
            saved_slide: 1,
            page_has_ink: false,
            current_strokes: Vec::new(),
            slide_cache: PageCache::new(OTHER_CACHE_CAPACITY, true),
            whiteboard_cache: PageCache::new(WHITEBOARD_CACHE_CAPACITY, true),
            undo: UndoHistory::default(),
            wps_connected: false,
            wps_real_position: None,
            wps_last_seen: None,
            commands: VecDeque::new(),
        }
    }

    pub fn set_presentation_active(&mut self, active: bool) {
        if self.whiteboard {
            return;
        }
        if active {
            self.slide_cache.set_capacity(WPS_CACHE_CAPACITY, false);
        } else {
            self.slide_cache.set_capacity(OTHER_CACHE_CAPACITY, true);
        }
    }

    pub fn begin_stroke(&mut self) {
        self.undo.push(&self.current_strokes);
    }

    pub fn add_stroke(&mut self, stroke: Stroke) {
        if stroke.kind == StrokeKind::Ink {
            self.page_has_ink = true;
        }
        self.current_strokes.push(stroke);
    }

    pub fn undo_last(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.current_strokes = previous;
        self.page_has_ink = self
            .current_strokes
            .iter()
            .any(|stroke| stroke.kind == StrokeKind::Ink);
        true
    }

    pub fn save_current_page(&mut self) -> bool {
        if !self.page_has_ink || self.current_strokes.is_empty() {
            return false;
        }
        if self.whiteboard {
            self.whiteboard_cache
                .save(self.current_page, &self.current_strokes)
        } else {
            self.slide_cache
                .save(self.current_page, &self.current_strokes)
        }
    }

    pub fn load_page(&mut self, page: u32) {
        let strokes = if self.whiteboard {
            self.whiteboard_cache.get(page)
        } else {
            self.slide_cache.get(page)
        };
        self.current_strokes = strokes.map_or_else(Vec::new, ToOwned::to_owned);
        self.current_page = page;
        self.page_has_ink = self
            .current_strokes
            .iter()
            .any(|stroke| stroke.kind == StrokeKind::Ink);
        self.undo.clear();
    }

    pub fn clear_current_page(&mut self) {
        if self.whiteboard {
            self.whiteboard_cache.remove(self.current_page);
        } else {
            self.slide_cache.remove(self.current_page);
        }
        self.current_strokes.clear();
        self.page_has_ink = false;
        self.undo.clear();
    }

    pub fn clear_all_pages(&mut self) {
        self.slide_cache.clear();
        self.whiteboard_cache.clear();
        self.current_page = 1;
        self.current_strokes.clear();
        self.page_has_ink = false;
        self.wps_real_position = None;
        self.undo.clear();
        self.commands.clear();
    }

    pub fn set_whiteboard(&mut self, enabled: bool) {
        if self.whiteboard == enabled {
            return;
        }
        if enabled {
            self.save_current_page();
            self.saved_slide = self.current_page;
            self.whiteboard_cache.clear();
            self.whiteboard = true;
            self.current_page = 1;
            self.current_strokes.clear();
            self.page_has_ink = false;
        } else {
            self.whiteboard_cache.clear();
            self.whiteboard = false;
            self.current_page = self.saved_slide;
            self.load_page(self.current_page);
        }
        self.undo.clear();
    }

    pub fn mark_bridge_seen(&mut self) {
        self.wps_connected = true;
        self.wps_last_seen = Some(Instant::now());
    }

    pub fn mark_bridge_disconnected(&mut self) {
        self.wps_connected = false;
        self.wps_real_position = None;
        self.wps_last_seen = None;
    }

    pub fn expire_bridge_if_stale(&mut self, now: Instant) -> bool {
        if self.wps_connected
            && self
                .wps_last_seen
                .is_some_and(|last_seen| now.duration_since(last_seen) > WPS_HEARTBEAT_TIMEOUT)
        {
            self.mark_bridge_disconnected();
            true
        } else {
            false
        }
    }

    pub fn enqueue(&mut self, command: Command) {
        self.commands.push_back(command);
    }

    pub fn dequeue(&mut self) -> Option<Command> {
        self.commands.pop_front()
    }

    pub fn queued_commands(&self) -> usize {
        self.commands.len()
    }

    pub fn apply_wps_event(&mut self, event: &WpsEvent) -> bool {
        if self.whiteboard {
            return false;
        }
        if event.name == "SlideShowBegin" {
            self.clear_all_pages();
            self.set_presentation_active(true);
            let page = event.position.unwrap_or(1).max(1);
            self.wps_real_position = Some(page);
            self.load_page(page);
            return true;
        }
        if event.name == "SlideShowEnd" {
            self.clear_all_pages();
            self.set_presentation_active(false);
            self.wps_real_position = None;
            return true;
        }
        let Some(page) = event.position.filter(|page| *page > 0) else {
            return false;
        };
        if self.wps_real_position == Some(page) {
            return false;
        }
        if self.wps_real_position.is_some() {
            self.save_current_page();
        }
        self.set_presentation_active(true);
        self.wps_real_position = Some(page);
        self.load_page(page);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(x: f32) -> Stroke {
        Stroke::ink(vec![Point { x, y: 1.0 }], Rgba::opaque(255, 0, 0), 3.0)
    }

    #[test]
    fn parses_wps_event_with_negative_click() {
        let event = WpsEvent::parse("EVENT SlideShowState pos=4 click=-1").unwrap();
        assert_eq!(event.name, "SlideShowState");
        assert_eq!(event.position, Some(4));
        assert_eq!(event.click, Some(-1));
    }

    #[test]
    fn cache_evicts_oldest_only_when_configured() {
        let mut cache = PageCache::new(2, true);
        assert!(cache.save(1, &[ink(1.0)]));
        assert!(cache.save(2, &[ink(2.0)]));
        assert!(cache.save(3, &[ink(3.0)]));
        assert!(!cache.contains(1));
        assert!(cache.contains(3));

        cache.set_capacity(2, false);
        assert!(!cache.save(4, &[ink(4.0)]));
        assert!(!cache.contains(4));
    }

    #[test]
    fn animation_does_not_switch_annotation_page() {
        let mut state = AnnotationState::new();
        assert!(
            state.apply_wps_event(&WpsEvent::parse("EVENT SlideShowBegin pos=1 click=0").unwrap())
        );
        state.begin_stroke();
        state.add_stroke(ink(10.0));
        assert!(
            !state.apply_wps_event(
                &WpsEvent::parse("EVENT SlideShowNextClick pos=1 click=1").unwrap()
            )
        );
        assert_eq!(state.current_page, 1);
        assert_eq!(state.current_strokes.len(), 1);
        assert!(
            state.apply_wps_event(
                &WpsEvent::parse("EVENT SlideShowNextSlide pos=2 click=0").unwrap()
            )
        );
        assert_eq!(state.current_page, 2);
        assert_eq!(state.slide_cache.get(1).unwrap().len(), 1);
    }

    #[test]
    fn undo_restores_previous_strokes() {
        let mut state = AnnotationState::new();
        state.begin_stroke();
        state.add_stroke(ink(1.0));
        state.begin_stroke();
        state.add_stroke(ink(2.0));
        assert!(state.undo_last());
        assert_eq!(state.current_strokes.len(), 1);
        assert_eq!(state.current_strokes[0].points[0].x, 1.0);
    }

    #[test]
    fn bridge_connection_expires_after_heartbeat_timeout() {
        let mut state = AnnotationState::new();
        state.mark_bridge_seen();
        state.wps_real_position = Some(4);
        let last_seen = state.wps_last_seen.unwrap();

        assert!(!state.expire_bridge_if_stale(last_seen + WPS_HEARTBEAT_TIMEOUT));
        assert!(state.wps_connected);
        assert!(
            state.expire_bridge_if_stale(
                last_seen + WPS_HEARTBEAT_TIMEOUT + Duration::from_millis(1)
            )
        );
        assert!(!state.wps_connected);
        assert_eq!(state.wps_real_position, None);
    }

    #[test]
    fn whiteboard_strokes_are_session_local() {
        let mut state = AnnotationState::new();
        state.begin_stroke();
        state.add_stroke(ink(1.0));
        state.set_whiteboard(true);
        state.begin_stroke();
        state.add_stroke(ink(2.0));
        assert!(state.save_current_page());
        assert!(state.whiteboard_cache.contains(1));

        state.set_whiteboard(false);
        assert!(!state.whiteboard);
        assert!(state.whiteboard_cache.is_empty());
        assert_eq!(state.current_strokes.len(), 1);
        assert_eq!(state.current_strokes[0].points[0].x, 1.0);
    }
}
