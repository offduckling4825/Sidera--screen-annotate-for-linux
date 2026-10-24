//! The drawing tool drawer is independent of manually tucking the whole sidebar.
//! A frame samples its animation once, so painting and hit testing share geometry.
use std::time::{Duration, Instant};

const DURATION: Duration = Duration::from_millis(220);

#[derive(Clone, Copy)]
struct Transition { started: Instant, from: f32 }

#[derive(Default)]
pub struct DrawingTools {
    expanded: bool,
    fraction: f32,
    transition: Option<Transition>,
    auto_collapse_pending: bool,
}

impl DrawingTools {
    pub fn expanded(&self) -> bool { self.expanded }
    pub fn fraction(&self) -> f32 { self.fraction }
    pub fn animating(&self) -> bool { self.transition.is_some() }

    pub fn request_auto_collapse(&mut self) {
        self.auto_collapse_pending = self.expanded;
    }

    pub fn take_auto_collapse_request(&mut self) -> bool {
        std::mem::take(&mut self.auto_collapse_pending)
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.set_expanded_at(expanded, Instant::now());
    }

    pub fn set_expanded_at(&mut self, expanded: bool, now: Instant) {
        // A subsequent explicit tool choice supersedes a queued empty-page check.
        self.auto_collapse_pending = false;
        if self.expanded == expanded { return; }
        self.advance_at(now);
        self.expanded = expanded;
        self.transition = Some(Transition { started: now, from: self.fraction });
    }

    pub fn advance_at(&mut self, now: Instant) -> bool {
        let Some(transition) = self.transition else { return false };
        let t = (now.saturating_duration_since(transition.started).as_secs_f32()
            / DURATION.as_secs_f32()).clamp(0.0, 1.0);
        let target = if self.expanded { 1.0 } else { 0.0 };
        // Ease out without overshooting; reversing starts at the visible position.
        self.fraction = transition.from + (target - transition.from) * (1.0 - (1.0 - t).powi(3));
        if t >= 1.0 { self.settle(); }
        true
    }

    pub fn settle(&mut self) {
        self.fraction = if self.expanded { 1.0 } else { 0.0 };
        self.transition = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PresentationAction { Escape, Start, End }

const PRESENTATION_DURATION: Duration = Duration::from_millis(240);

#[derive(Clone, Copy)]
struct PresentationFrame { started: Instant, from: [f32; 3] }

/// Visual state only: commands and hit testing use the current action immediately.
pub struct PresentationTransition {
    action: PresentationAction,
    weights: [f32; 3],
    transition: Option<PresentationFrame>,
}

impl Default for PresentationTransition {
    fn default() -> Self {
        Self { action: PresentationAction::Escape, weights: [1.0, 0.0, 0.0], transition: None }
    }
}

impl PresentationTransition {
    pub fn weights(&self) -> [f32; 3] { self.weights }

    pub fn set_action_at(&mut self, action: PresentationAction, now: Instant) {
        // Repeated WPS reports must not restart a transition that is in flight.
        if self.action == action { return; }
        self.advance_at(now);
        self.action = action;
        self.transition = Some(PresentationFrame { started: now, from: self.weights });
    }

    pub fn advance_at(&mut self, now: Instant) -> bool {
        let Some(frame) = self.transition else { return false };
        let t = (now.saturating_duration_since(frame.started).as_secs_f32()
            / PRESENTATION_DURATION.as_secs_f32()).clamp(0.0, 1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        for (index, weight) in self.weights.iter_mut().enumerate() {
            let target = if index == self.action as usize { 1.0 } else { 0.0 };
            *weight = frame.from[index] + (target - frame.from[index]) * eased;
        }
        if t >= 1.0 { self.settle(); }
        true
    }

    pub fn settle(&mut self) {
        self.weights = [0.0; 3];
        self.weights[self.action as usize] = 1.0;
        self.transition = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_transition_keeps_the_visible_mix_when_retargeted() {
        let mut animation = PresentationTransition::default();
        let start = Instant::now();
        animation.set_action_at(PresentationAction::Start, start);
        let interrupted = start + Duration::from_millis(80);
        animation.advance_at(interrupted);
        let visible = animation.weights();
        assert!(visible[0] > 0.0 && visible[1] > 0.0);
        animation.set_action_at(PresentationAction::End, interrupted);
        assert_eq!(animation.weights(), visible);
        animation.advance_at(interrupted + Duration::from_millis(60));
        let visible = animation.weights();
        animation.set_action_at(PresentationAction::Escape, interrupted + Duration::from_millis(60));
        assert_eq!(animation.weights(), visible);
        // A heartbeat with the same action cannot postpone the final frame.
        animation.set_action_at(PresentationAction::Escape, interrupted + Duration::from_millis(180));
        assert!(animation.advance_at(interrupted + Duration::from_millis(300)));
        assert_eq!(animation.weights(), [1.0, 0.0, 0.0]);
        assert!(!animation.advance_at(start + Duration::from_secs(60)));
    }

    #[test]
    fn reversing_is_continuous_and_idle_ticks_never_reopen_the_drawer() {
        let mut tools = DrawingTools::default();
        let start = Instant::now();
        tools.set_expanded_at(true, start);
        let reverse = start + Duration::from_millis(70);
        tools.advance_at(reverse);
        let visible = tools.fraction();
        assert!(visible > 0.0 && visible < 1.0);
        tools.set_expanded_at(false, reverse);
        assert_eq!(tools.fraction(), visible);
        tools.advance_at(reverse + Duration::from_millis(60));
        assert!(tools.fraction() < visible && tools.fraction() > 0.0);
        tools.advance_at(reverse + DURATION);
        assert_eq!(tools.fraction(), 0.0);
        assert!(!tools.animating());
        tools.advance_at(start + Duration::from_secs(600));
        assert!(!tools.expanded());
        assert_eq!(tools.fraction(), 0.0);
    }
}
