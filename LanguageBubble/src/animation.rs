use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimState {
    Idle,
    FadeIn,
    Visible,
    FadeOut,
}

pub struct AnimController {
    pub state: AnimState,
    start: Instant,
    fade_from_opacity: f32,
    // Carousel/expanded slide
    pub slide_from: f32,
    pub slide_to: f32,
    slide_start: Instant,
    pub sliding: bool,
    // Expanded window slide
    pub win_slide_from_x: i32,
    pub win_slide_to_x: i32,
    win_slide_start: Instant,
    pub win_sliding: bool,
    pub win_slide_current_x: i32,
    // Label opacity tracking
    pub label_anim_start: Option<Instant>,
    pub prev_label_opacities: Vec<f32>,
}

const FADE_IN_MS: f64 = 120.0;
const FADE_OUT_MS: f64 = 250.0;
const SLIDE_MS: f64 = 180.0;
const WIN_SLIDE_MS: f64 = 180.0;
const LABEL_FADE_MS: f64 = 180.0;

impl AnimController {
    pub fn new() -> Self {
        Self::new_at(Instant::now())
    }

    fn new_at(now: Instant) -> Self {
        Self {
            state: AnimState::Idle,
            start: now,
            fade_from_opacity: 0.0,
            slide_from: 0.0,
            slide_to: 0.0,
            slide_start: now,
            sliding: false,
            win_slide_from_x: 0,
            win_slide_to_x: 0,
            win_slide_start: now,
            win_sliding: false,
            win_slide_current_x: 0,
            label_anim_start: None,
            prev_label_opacities: Vec::new(),
        }
    }

    pub fn begin_fade_in(&mut self) {
        self.begin_fade_in_at(Instant::now())
    }

    fn begin_fade_in_at(&mut self, now: Instant) {
        // If already partially visible (e.g. during fade-out), start from current opacity
        self.fade_from_opacity = self.opacity_at(now);
        self.state = AnimState::FadeIn;
        self.start = now;
    }

    pub fn begin_fade_out(&mut self) {
        self.begin_fade_out_at(Instant::now())
    }

    fn begin_fade_out_at(&mut self, now: Instant) {
        self.fade_from_opacity = self.opacity_at(now);
        self.state = AnimState::FadeOut;
        self.start = now;
    }

    pub fn set_visible(&mut self) {
        self.state = AnimState::Visible;
    }

    pub fn opacity(&self) -> f32 {
        self.opacity_at(Instant::now())
    }

    fn opacity_at(&self, now: Instant) -> f32 {
        match self.state {
            AnimState::Idle => 0.0,
            AnimState::Visible => 1.0,
            AnimState::FadeIn => {
                let t = now.duration_since(self.start).as_secs_f64() / (FADE_IN_MS / 1000.0);
                let t = t.min(1.0);
                let eased = ease_out_quart(t) as f32;
                self.fade_from_opacity + (1.0 - self.fade_from_opacity) * eased
            }
            AnimState::FadeOut => {
                let t = now.duration_since(self.start).as_secs_f64() / (FADE_OUT_MS / 1000.0);
                let t = t.min(1.0);
                let eased = ease_in_quad(t) as f32;
                self.fade_from_opacity * (1.0 - eased)
            }
        }
    }

    pub fn is_fade_complete(&self) -> bool {
        self.is_fade_complete_at(Instant::now())
    }

    fn is_fade_complete_at(&self, now: Instant) -> bool {
        match self.state {
            AnimState::FadeIn => {
                now.duration_since(self.start).as_secs_f64() >= FADE_IN_MS / 1000.0
            }
            AnimState::FadeOut => {
                now.duration_since(self.start).as_secs_f64() >= FADE_OUT_MS / 1000.0
            }
            _ => true,
        }
    }

    // Carousel row slide — always starts from current visual position on interruption
    pub fn begin_slide(&mut self, from: f32, to: f32) {
        self.begin_slide_at(from, to, Instant::now())
    }

    fn begin_slide_at(&mut self, from: f32, to: f32, now: Instant) {
        let actual_from = if self.sliding {
            self.slide_offset_at(now) // continue from current visual position
        } else {
            from
        };
        self.slide_from = actual_from;
        self.slide_to = to;
        self.slide_start = now;
        self.sliding = true;
    }

    pub fn slide_offset(&self) -> f32 {
        self.slide_offset_at(Instant::now())
    }

    fn slide_offset_at(&self, now: Instant) -> f32 {
        if !self.sliding {
            return self.slide_to;
        }
        let t = now.duration_since(self.slide_start).as_secs_f64() / (SLIDE_MS / 1000.0);
        let t = t.min(1.0);
        let eased = ease_out_quart(t) as f32;
        self.slide_from + (self.slide_to - self.slide_from) * eased
    }

    pub fn is_slide_complete(&self) -> bool {
        self.is_slide_complete_at(Instant::now())
    }

    fn is_slide_complete_at(&self, now: Instant) -> bool {
        !self.sliding || now.duration_since(self.slide_start).as_secs_f64() >= SLIDE_MS / 1000.0
    }

    pub fn finish_slide(&mut self) {
        self.sliding = false;
    }

    // Expanded window slide — always starts from current position on interruption
    pub fn begin_window_slide(&mut self, from_x: i32, to_x: i32) {
        self.begin_window_slide_at(from_x, to_x, Instant::now())
    }

    fn begin_window_slide_at(&mut self, from_x: i32, to_x: i32, now: Instant) {
        let actual_from = if self.win_sliding {
            self.win_slide_current_x // continue from where the window actually is
        } else {
            from_x
        };
        self.win_slide_from_x = actual_from;
        self.win_slide_to_x = to_x;
        self.win_slide_start = now;
        self.win_sliding = true;
        self.win_slide_current_x = actual_from;
    }

    pub fn window_slide_x(&mut self) -> i32 {
        self.window_slide_x_at(Instant::now())
    }

    fn window_slide_x_at(&mut self, now: Instant) -> i32 {
        if !self.win_sliding {
            return self.win_slide_to_x;
        }
        let t = now.duration_since(self.win_slide_start).as_secs_f64() / (WIN_SLIDE_MS / 1000.0);
        let t = t.min(1.0);
        let eased = ease_out_quart(t) as f32;
        let x = self.win_slide_from_x as f32
            + (self.win_slide_to_x - self.win_slide_from_x) as f32 * eased;
        self.win_slide_current_x = x as i32;
        self.win_slide_current_x
    }

    pub fn is_window_slide_complete(&self) -> bool {
        self.is_window_slide_complete_at(Instant::now())
    }

    fn is_window_slide_complete_at(&self, now: Instant) -> bool {
        !self.win_sliding
            || now.duration_since(self.win_slide_start).as_secs_f64() >= WIN_SLIDE_MS / 1000.0
    }

    pub fn finish_window_slide(&mut self) {
        self.win_sliding = false;
    }

    // --- Label opacity animation ---

    /// Snapshot current label opacities and start a new transition.
    pub fn begin_label_transition(&mut self, current_opacities: Vec<f32>) {
        self.begin_label_transition_at(current_opacities, Instant::now())
    }

    fn begin_label_transition_at(&mut self, current_opacities: Vec<f32>, now: Instant) {
        self.prev_label_opacities = current_opacities;
        self.label_anim_start = Some(now);
    }

    /// Get the interpolated opacity for a label.
    /// `target` is the final opacity (1.0 for selected, 0.3 for unselected).
    pub fn label_opacity(&self, index: usize, target: f32) -> f32 {
        self.label_opacity_at(index, target, Instant::now())
    }

    fn label_opacity_at(&self, index: usize, target: f32, now: Instant) -> f32 {
        let Some(start) = self.label_anim_start else {
            return target;
        };
        let from = self
            .prev_label_opacities
            .get(index)
            .copied()
            .unwrap_or(target);
        let elapsed = now.duration_since(start).as_secs_f64() * 1000.0;
        if elapsed >= LABEL_FADE_MS {
            return target;
        }
        let t = (elapsed / LABEL_FADE_MS) as f32;
        let eased = ease_out_quart(t as f64) as f32;
        from + (target - from) * eased
    }
}

// Smoother easing functions

/// Quartic ease-out: very smooth deceleration
fn ease_out_quart(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(4)
}

/// Quadratic ease-in: gentle acceleration
fn ease_in_quad(t: f64) -> f64 {
    t * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn after(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }
    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.00001,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn fade_boundaries_and_interruption_preserve_opacity() {
        let start = Instant::now();
        let mut anim = AnimController::new_at(start);
        close(anim.opacity_at(start), 0.0);
        anim.begin_fade_in_at(start);
        close(anim.opacity_at(start), 0.0);
        close(anim.opacity_at(after(start, 60)), 0.9375);
        assert!(!anim.is_fade_complete_at(after(start, 119)));
        assert!(anim.is_fade_complete_at(after(start, 120)));
        close(anim.opacity_at(after(start, 120)), 1.0);
        anim.begin_fade_out_at(after(start, 120));
        close(anim.opacity_at(after(start, 245)), 0.75);
        assert!(!anim.is_fade_complete_at(after(start, 369)));
        assert!(anim.is_fade_complete_at(after(start, 370)));
        close(anim.opacity_at(after(start, 500)), 0.0);
        anim.begin_fade_in_at(after(start, 245));
        close(anim.opacity_at(after(start, 245)), 0.75);
        close(anim.opacity_at(after(start, 365)), 1.0);
    }

    #[test]
    fn carousel_interruption_starts_at_current_visual_offset() {
        let start = Instant::now();
        let mut anim = AnimController::new_at(start);
        anim.begin_slide_at(0.0, 100.0, start);
        close(anim.slide_offset_at(after(start, 90)), 93.75);
        assert!(!anim.is_slide_complete_at(after(start, 179)));
        assert!(anim.is_slide_complete_at(after(start, 180)));
        anim.begin_slide_at(-999.0, 200.0, after(start, 90));
        close(anim.slide_offset_at(after(start, 90)), 93.75);
        close(anim.slide_offset_at(after(start, 270)), 200.0);
        anim.finish_slide();
        assert!(anim.is_slide_complete_at(after(start, 270)));
        close(anim.slide_offset_at(after(start, 270)), 200.0);
    }

    #[test]
    fn window_slide_preserves_integer_truncation_and_last_applied_position() {
        let start = Instant::now();
        let mut anim = AnimController::new_at(start);
        anim.begin_window_slide_at(-100, 0, start);
        assert_eq!(anim.window_slide_x_at(after(start, 90)), -6);
        // Interruption continues at last applied position, not a later sample.
        anim.begin_window_slide_at(999, 100, after(start, 120));
        assert_eq!(anim.window_slide_x_at(after(start, 120)), -6);
        assert!(!anim.is_window_slide_complete_at(after(start, 299)));
        assert!(anim.is_window_slide_complete_at(after(start, 300)));
        assert_eq!(anim.window_slide_x_at(after(start, 300)), 100);
        anim.finish_window_slide();
        assert_eq!(anim.window_slide_x_at(after(start, 400)), 100);
    }

    #[test]
    fn label_transition_uses_snapshot_fallback_and_exact_endpoint() {
        let start = Instant::now();
        let mut anim = AnimController::new_at(start);
        close(anim.label_opacity_at(0, 0.3, start), 0.3);
        anim.begin_label_transition_at(vec![1.0, 0.3], start);
        close(anim.label_opacity_at(0, 0.3, start), 1.0);
        close(anim.label_opacity_at(0, 0.3, after(start, 90)), 0.34375);
        close(anim.label_opacity_at(2, 0.3, after(start, 90)), 0.3);
        close(anim.label_opacity_at(0, 0.3, after(start, 180)), 0.3);
        let current = anim.label_opacity_at(0, 0.3, after(start, 90));
        anim.begin_label_transition_at(vec![current], after(start, 90));
        close(anim.label_opacity_at(0, 1.0, after(start, 90)), current);
        close(anim.label_opacity_at(0, 1.0, after(start, 270)), 1.0);
    }
}
