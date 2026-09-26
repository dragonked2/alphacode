//! Animated transitions — a state machine for smooth value interpolation.
//!
//! # Why this exists
//!
//! Widgets need smooth animations for panel open/close, message fade-in,
//! progress bar fills, and cursor blinking. Instead of hand-rolling timing
//! math in every widget, `Transition` provides a reusable state machine
//! that tracks progress and returns interpolated values.
//!
//! # Usage
//!
//! ```ignore
//! use crate::alphacode_tui_style::transition::{Transition, TransitionState};
//!
//! // Create a transition for a panel sliding open
//! let mut slide = Transition::new(0.0, 1.0, Duration::from_millis(300));
//!
//! // In your render loop:
//! slide.tick(elapsed);
//! let offset = slide.value(); // 0.0 → 1.0 with easing
//!
//! if slide.is_done() {
//!     // Animation complete
//! }
//! ```

use std::time::Duration;

use super::easing::Easing;

/// A smooth transition between two values.
#[derive(Debug, Clone)]
pub struct Transition {
    /// Start value.
    from: f32,
    /// End value.
    to: f32,
    /// Current interpolated value.
    current: f32,
    /// Total duration of the transition.
    duration: Duration,
    /// Elapsed time since transition started.
    elapsed: Duration,
    /// Easing function to apply.
    easing: Easing,
    /// Whether the transition has completed.
    done: bool,
    /// Whether the transition is currently active.
    active: bool,
}

impl Transition {
    /// Create a new transition from `from` to `to` over the given duration.
    pub fn new(from: f32, to: f32, duration: Duration) -> Self {
        Self {
            from,
            to,
            current: from,
            duration,
            elapsed: Duration::ZERO,
            easing: Easing::EaseOutCubic,
            done: false,
            active: true,
        }
    }

    /// Create a transition with a custom easing function.
    pub fn with_easing(from: f32, to: f32, duration: Duration, easing: Easing) -> Self {
        Self {
            from,
            to,
            current: from,
            duration,
            elapsed: Duration::ZERO,
            easing,
            done: false,
            active: true,
        }
    }

    /// Create a snappy spring transition (for UI reactions).
    pub fn snappy(from: f32, to: f32) -> Self {
        Self::with_easing(from, to, Duration::from_millis(200), Easing::SpringSnappy)
    }

    /// Create a gentle transition (for smooth, luxurious animations).
    pub fn gentle(from: f32, to: f32) -> Self {
        Self::with_easing(from, to, Duration::from_millis(500), Easing::SpringGentle)
    }

    /// Create a bouncy transition (for playful effects).
    pub fn bouncy(from: f32, to: f32) -> Self {
        Self::with_easing(from, to, Duration::from_millis(400), Easing::SpringBouncy)
    }

    /// Create a slide transition (for panel open/close).
    pub fn slide(from: f32, to: f32) -> Self {
        Self::with_easing(from, to, Duration::from_millis(250), Easing::EaseOutCubic)
    }

    /// Create a fade transition (for opacity changes).
    pub fn fade(from: f32, to: f32) -> Self {
        Self::with_easing(from, to, Duration::from_millis(200), Easing::EaseInOutSine)
    }

    /// Create a typewriter transition (for text reveal).
    pub fn typewriter(duration: Duration) -> Self {
        Self::new(0.0, 1.0, duration)
    }

    /// Advance the transition by the given elapsed time.
    pub fn tick(&mut self, elapsed: Duration) {
        if !self.active || self.done {
            return;
        }

        self.elapsed += elapsed;

        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.current = self.to;
            self.done = true;
            self.active = false;
            return;
        }

        let t = self.elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let eased = self.easing.apply(t);
        self.current = self.from + (self.to - self.from) * eased;
    }

    /// Get the current interpolated value.
    pub fn value(&self) -> f32 {
        self.current
    }

    /// Get the raw progress (0.0..=1.0) without easing.
    pub fn progress(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        (self.elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// Whether the transition has completed.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Whether the transition is currently active (in progress).
    pub fn is_active(&self) -> bool {
        self.active && !self.done
    }

    /// Reset the transition to its starting value.
    pub fn reset(&mut self) {
        self.current = self.from;
        self.elapsed = Duration::ZERO;
        self.done = false;
        self.active = true;
    }

    /// Jump to the end value immediately.
    pub fn finish(&mut self) {
        self.current = self.to;
        self.elapsed = self.duration;
        self.done = true;
        self.active = false;
    }

    /// Start a new transition from the current value to a new target.
    pub fn transition_to(&mut self, to: f32, duration: Duration) {
        self.from = self.current;
        self.to = to;
        self.duration = duration;
        self.elapsed = Duration::ZERO;
        self.done = false;
        self.active = true;
    }

    /// Change the easing function.
    pub fn set_easing(&mut self, easing: Easing) {
        self.easing = easing;
    }

    /// Get the start value.
    pub fn from_value(&self) -> f32 {
        self.from
    }

    /// Get the target value.
    pub fn to_value(&self) -> f32 {
        self.to
    }
}

/// A bidirectional transition that can animate forward and reverse.
#[derive(Debug, Clone)]
pub struct BidirectionalTransition {
    /// The underlying transition.
    transition: Transition,
    /// Whether we're currently going forward.
    forward: bool,
}

impl BidirectionalTransition {
    /// Create a new bidirectional transition.
    pub fn new(initial: f32, duration: Duration) -> Self {
        let mut transition = Transition::new(initial, initial, duration);
        // There is no direction-specific work to do until the caller asks
        // for forward/reverse. Starting as completed lets the first request
        // establish its direction regardless of the initial value.
        transition.finish();
        Self {
            transition,
            forward: true,
        }
    }

    /// Animate forward (open/expand/show).
    pub fn forward(&mut self, duration: Duration) {
        if !self.forward || self.transition.is_done() {
            self.transition.transition_to(1.0, duration);
            self.forward = true;
        }
    }

    /// Animate reverse (close/collapse/hide).
    pub fn reverse(&mut self, duration: Duration) {
        if self.forward || self.transition.is_done() {
            self.transition.transition_to(0.0, duration);
            self.forward = false;
        }
    }

    /// Advance the transition.
    pub fn tick(&mut self, elapsed: Duration) {
        self.transition.tick(elapsed);
    }

    /// Get the current value.
    pub fn value(&self) -> f32 {
        self.transition.value()
    }

    /// Whether the transition is currently going forward.
    pub fn is_forward(&self) -> bool {
        self.forward
    }

    /// Whether the transition is done.
    pub fn is_done(&self) -> bool {
        self.transition.is_done()
    }

    /// Whether the panel should be visible (forward progress > 0).
    pub fn is_visible(&self) -> bool {
        self.transition.value() > 0.01
    }
}

/// A repeating transition (for pulsing, blinking, breathing effects).
#[derive(Debug, Clone)]
pub struct PulseTransition {
    /// Current phase (0.0..=1.0).
    phase: f32,
    /// Speed in cycles per second.
    speed: f32,
    /// Easing applied to each cycle.
    easing: Easing,
    /// Whether the pulse is active.
    active: bool,
}

impl PulseTransition {
    /// Create a new pulse transition.
    pub fn new(speed: f32, easing: Easing) -> Self {
        Self {
            phase: 0.0,
            speed,
            easing,
            active: true,
        }
    }

    /// Create a smooth breathing pulse (sinusoidal, ~0.5 Hz).
    pub fn breathing() -> Self {
        Self::new(0.5, Easing::EaseInOutSine)
    }

    /// Create a fast blink pulse (~2 Hz).
    pub fn blink() -> Self {
        Self::new(2.0, Easing::Linear)
    }

    /// Create a subtle heartbeat pulse (~1 Hz).
    pub fn heartbeat() -> Self {
        Self::new(1.0, Easing::EaseInOutCubic)
    }

    /// Advance the pulse by the given elapsed time.
    pub fn tick(&mut self, elapsed: Duration) {
        if !self.active {
            return;
        }
        let delta = elapsed.as_secs_f32() * self.speed;
        self.phase = (self.phase + delta) % 1.0;
    }

    /// Get the current pulse value (0.0..=1.0).
    pub fn value(&self) -> f32 {
        self.easing.apply(self.phase)
    }

    /// Get a value that oscillates between -1.0 and 1.0.
    pub fn oscillate(&self) -> f32 {
        self.value() * 2.0 - 1.0
    }

    /// Whether the pulse is currently above the midpoint.
    pub fn is_up(&self) -> bool {
        self.phase < 0.5
    }

    /// Pause the pulse.
    pub fn pause(&mut self) {
        self.active = false;
    }

    /// Resume the pulse.
    pub fn resume(&mut self) {
        self.active = true;
    }

    /// Reset the pulse to the beginning.
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }
}

/// A sequenced transition — multiple stages played in order.
#[derive(Debug, Clone)]
pub struct SequenceTransition {
    stages: Vec<(f32, Duration)>,
    current_stage: usize,
    elapsed_in_stage: Duration,
    done: bool,
}

impl SequenceTransition {
    /// Create a new sequence from (value, duration) pairs.
    pub fn new(stages: Vec<(f32, Duration)>) -> Self {
        let done = stages.is_empty();
        Self {
            stages,
            current_stage: 0,
            elapsed_in_stage: Duration::ZERO,
            done,
        }
    }

    /// Advance the sequence.
    pub fn tick(&mut self, elapsed: Duration) {
        if self.done || self.stages.is_empty() {
            return;
        }

        self.elapsed_in_stage += elapsed;
        let (_, duration) = &self.stages[self.current_stage];

        while self.elapsed_in_stage >= *duration {
            self.elapsed_in_stage -= *duration;
            self.current_stage += 1;
            if self.current_stage >= self.stages.len() {
                self.done = true;
                return;
            }
        }
    }

    /// Get the current value.
    pub fn value(&self) -> f32 {
        if self.stages.is_empty() {
            return 0.0;
        }
        let idx = self.current_stage.min(self.stages.len() - 1);
        self.stages[idx].0
    }

    /// Whether the sequence has completed.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Reset the sequence to the beginning.
    pub fn reset(&mut self) {
        self.current_stage = 0;
        self.elapsed_in_stage = Duration::ZERO;
        self.done = self.stages.is_empty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_interpolates_correctly() {
        let mut t = Transition::new(0.0, 100.0, Duration::from_millis(1000));
        t.tick(Duration::from_millis(500));
        // At 50% with EaseOutCubic, value should be > 50 (ease-out speeds up start)
        assert!(t.value() > 40.0 && t.value() < 100.0);
    }

    #[test]
    fn transition_completes() {
        let mut t = Transition::new(0.0, 1.0, Duration::from_millis(100));
        t.tick(Duration::from_millis(200));
        assert!(t.is_done());
        assert_eq!(t.value(), 1.0);
    }

    #[test]
    fn pulse_oscillates() {
        let mut p = PulseTransition::new(1.0, Easing::Linear);
        p.tick(Duration::from_millis(500));
        let val = p.value();
        assert!((0.0..=1.0).contains(&val));
        assert!((-1.0..=1.0).contains(&p.oscillate()));
    }

    #[test]
    fn bidirectional_forward_reverse() {
        let mut b = BidirectionalTransition::new(0.0, Duration::from_millis(100));
        b.forward(Duration::from_millis(100));
        b.tick(Duration::from_millis(100));
        assert!(b.value() > 0.5);
        assert!(b.is_forward());

        b.reverse(Duration::from_millis(100));
        b.tick(Duration::from_millis(100));
        assert!(b.value() < 0.5);
        assert!(!b.is_forward());
    }
}
