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
        Self {
            state: AnimState::Idle,
            start: Instant::now(),
            fade_from_opacity: 0.0,
            slide_from: 0.0,
            slide_to: 0.0,
            slide_start: Instant::now(),
            sliding: false,
            win_slide_from_x: 0,
            win_slide_to_x: 0,
            win_slide_start: Instant::now(),
            win_sliding: false,
            win_slide_current_x: 0,
            label_anim_start: None,
            prev_label_opacities: Vec::new(),
        }
    }

    pub fn begin_fade_in(&mut self) {
        // If already partially visible (e.g. during fade-out), start from current opacity
        self.fade_from_opacity = self.opacity();
        self.state = AnimState::FadeIn;
        self.start = Instant::now();
    }

    pub fn begin_fade_out(&mut self) {
        self.fade_from_opacity = self.opacity();
        self.state = AnimState::FadeOut;
        self.start = Instant::now();
    }

    pub fn set_visible(&mut self) {
        self.state = AnimState::Visible;
    }

    pub fn opacity(&self) -> f32 {
        match self.state {
            AnimState::Idle => 0.0,
            AnimState::Visible => 1.0,
            AnimState::FadeIn => {
                let t = self.start.elapsed().as_secs_f64() / (FADE_IN_MS / 1000.0);
                let t = t.min(1.0);
                let eased = ease_out_quart(t) as f32;
                self.fade_from_opacity + (1.0 - self.fade_from_opacity) * eased
            }
            AnimState::FadeOut => {
                let t = self.start.elapsed().as_secs_f64() / (FADE_OUT_MS / 1000.0);
                let t = t.min(1.0);
                let eased = ease_in_quad(t) as f32;
                self.fade_from_opacity * (1.0 - eased)
            }
        }
    }

    pub fn is_fade_complete(&self) -> bool {
        match self.state {
            AnimState::FadeIn => self.start.elapsed().as_secs_f64() >= FADE_IN_MS / 1000.0,
            AnimState::FadeOut => self.start.elapsed().as_secs_f64() >= FADE_OUT_MS / 1000.0,
            _ => true,
        }
    }

    // Carousel row slide — always starts from current visual position on interruption
    pub fn begin_slide(&mut self, from: f32, to: f32) {
        let actual_from = if self.sliding {
            self.slide_offset() // continue from current visual position
        } else {
            from
        };
        self.slide_from = actual_from;
        self.slide_to = to;
        self.slide_start = Instant::now();
        self.sliding = true;
    }

    pub fn slide_offset(&self) -> f32 {
        if !self.sliding {
            return self.slide_to;
        }
        let t = self.slide_start.elapsed().as_secs_f64() / (SLIDE_MS / 1000.0);
        let t = t.min(1.0);
        let eased = ease_out_quart(t) as f32;
        self.slide_from + (self.slide_to - self.slide_from) * eased
    }

    pub fn is_slide_complete(&self) -> bool {
        !self.sliding || self.slide_start.elapsed().as_secs_f64() >= SLIDE_MS / 1000.0
    }

    pub fn finish_slide(&mut self) {
        self.sliding = false;
    }

    // Expanded window slide — always starts from current position on interruption
    pub fn begin_window_slide(&mut self, from_x: i32, to_x: i32) {
        let actual_from = if self.win_sliding {
            self.win_slide_current_x // continue from where the window actually is
        } else {
            from_x
        };
        self.win_slide_from_x = actual_from;
        self.win_slide_to_x = to_x;
        self.win_slide_start = Instant::now();
        self.win_sliding = true;
        self.win_slide_current_x = actual_from;
    }

    pub fn window_slide_x(&mut self) -> i32 {
        if !self.win_sliding {
            return self.win_slide_to_x;
        }
        let t = self.win_slide_start.elapsed().as_secs_f64() / (WIN_SLIDE_MS / 1000.0);
        let t = t.min(1.0);
        let eased = ease_out_quart(t) as f32;
        let x = self.win_slide_from_x as f32
            + (self.win_slide_to_x - self.win_slide_from_x) as f32 * eased;
        self.win_slide_current_x = x as i32;
        self.win_slide_current_x
    }

    pub fn is_window_slide_complete(&self) -> bool {
        !self.win_sliding
            || self.win_slide_start.elapsed().as_secs_f64() >= WIN_SLIDE_MS / 1000.0
    }

    pub fn finish_window_slide(&mut self) {
        self.win_sliding = false;
    }

    // --- Label opacity animation ---

    /// Snapshot current label opacities and start a new transition.
    pub fn begin_label_transition(&mut self, current_opacities: Vec<f32>) {
        self.prev_label_opacities = current_opacities;
        self.label_anim_start = Some(Instant::now());
    }

    /// Get the interpolated opacity for a label.
    /// `target` is the final opacity (1.0 for selected, 0.3 for unselected).
    pub fn label_opacity(&self, index: usize, target: f32) -> f32 {
        let Some(start) = self.label_anim_start else {
            return target;
        };
        let from = self
            .prev_label_opacities
            .get(index)
            .copied()
            .unwrap_or(target);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
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
