//! Easing functions for smooth animations.
//!
//! All functions take `t` in `0.0..=1.0` and return `0.0..=1.0`.
//! Based on Robert Penner's easing equations and modern extensions.
//!
//! # Usage
//!
//! ```ignore
//! use crate::alphacode_tui_style::easing;
//!
//! let progress = easing::ease_out_cubic(t);
//! let color = blend_color(from, to, progress);
//! ```

/// Linear interpolation — no easing.
#[inline]
pub fn linear(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

// ── Quadratic ───────────────────────────────────────────────────────────────

/// Ease-in: accelerates from zero velocity.
#[inline]
pub fn ease_in_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

/// Ease-out: decelerates to zero velocity.
#[inline]
pub fn ease_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * (2.0 - t)
}

/// Ease-in-out: accelerates then decelerates.
#[inline]
pub fn ease_in_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        -1.0 + (4.0 - 2.0 * t) * t
    }
}

// ── Cubic ───────────────────────────────────────────────────────────────────

/// Ease-in (cubic): slow start, fast end.
#[inline]
pub fn ease_in_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t
}

/// Ease-out (cubic): fast start, slow end.
#[inline]
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let t1 = t - 1.0;
    t1 * t1 * t1 + 1.0
}

/// Ease-in-out (cubic).
#[inline]
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let t1 = 2.0 * t - 2.0;
        0.5 * t1 * t1 * t1 + 1.0
    }
}

// ── Quartic ─────────────────────────────────────────────────────────────────

/// Ease-in (quartic).
#[inline]
pub fn ease_in_quart(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * t
}

/// Ease-out (quartic).
#[inline]
pub fn ease_out_quart(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let t1 = t - 1.0;
    1.0 - t1 * t1 * t1 * t1
}

/// Ease-in-out (quartic).
#[inline]
pub fn ease_in_out_quart(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        8.0 * t * t * t * t
    } else {
        let t1 = t - 1.0;
        1.0 - 8.0 * t1 * t1 * t1 * t1
    }
}

// ── Quintic ─────────────────────────────────────────────────────────────────

/// Ease-in (quintic).
#[inline]
pub fn ease_in_quint(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * t * t
}

/// Ease-out (quintic).
#[inline]
pub fn ease_out_quint(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let t1 = t - 1.0;
    t1 * t1 * t1 * t1 * t1 + 1.0
}

// ── Sinusoidal ──────────────────────────────────────────────────────────────

/// Ease-in (sinusoidal).
#[inline]
pub fn ease_in_sine(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (t * std::f32::consts::FRAC_PI_2).cos()
}

/// Ease-out (sinusoidal).
#[inline]
pub fn ease_out_sine(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (t * std::f32::consts::FRAC_PI_2).sin()
}

/// Ease-in-out (sinusoidal).
#[inline]
pub fn ease_in_out_sine(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    -0.5 * ((std::f32::consts::PI * t).cos() - 1.0)
}

// ── Exponential ─────────────────────────────────────────────────────────────

/// Ease-in (exponential).
#[inline]
pub fn ease_in_expo(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 {
        0.0
    } else {
        2.0_f32.powf(10.0 * (t - 1.0))
    }
}

/// Ease-out (exponential).
#[inline]
pub fn ease_out_expo(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 1.0 {
        1.0
    } else {
        1.0 - 2.0_f32.powf(-10.0 * t)
    }
}

/// Ease-in-out (exponential).
#[inline]
pub fn ease_in_out_expo(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 {
        return 0.0;
    }
    if t == 1.0 {
        return 1.0;
    }
    if t < 0.5 {
        0.5 * 2.0_f32.powf(20.0 * t - 10.0)
    } else {
        1.0 - 0.5 * 2.0_f32.powf(-20.0 * t + 10.0)
    }
}

// ── Circular ────────────────────────────────────────────────────────────────

/// Ease-in (circular).
#[inline]
pub fn ease_in_circ(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t * t).sqrt()
}

/// Ease-out (circular).
#[inline]
pub fn ease_out_circ(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (1.0 - (t - 1.0).powi(2)).sqrt()
}

/// Ease-in-out (circular).
#[inline]
pub fn ease_in_out_circ(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        0.5 * (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt())
    } else {
        0.5 * ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0)
    }
}

// ── Back ────────────────────────────────────────────────────────────────────

/// Ease-in (back): slight overshoot on entry.
#[inline]
pub fn ease_in_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    const S: f32 = 1.70158;
    t * t * ((S + 1.0) * t - S)
}

/// Ease-out (back): slight overshoot on exit.
#[inline]
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    const S: f32 = 1.70158;
    let t1 = t - 1.0;
    t1 * t1 * ((S + 1.0) * t1 + S) + 1.0
}

/// Ease-in-out (back).
#[inline]
pub fn ease_in_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    const S: f32 = 1.70158 * 1.525;
    if t < 0.5 {
        let u = 2.0 * t;
        0.5 * (u * u * ((S + 1.0) * u - S))
    } else {
        let u = 2.0 * t - 2.0;
        0.5 * (u * u * ((S + 1.0) * u + S) + 2.0)
    }
}

// ── Elastic ─────────────────────────────────────────────────────────────────

/// Ease-in (elastic): bouncy spring effect.
#[inline]
pub fn ease_in_elastic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 {
        return 0.0;
    }
    if t == 1.0 {
        return 1.0;
    }
    let p = 0.3;
    let s = p / 4.0;
    let t = t - 1.0;
    -(2.0_f32.powf(10.0 * t) * ((t - s) * (2.0 * std::f32::consts::PI) / p).sin())
}

/// Ease-out (elastic).
#[inline]
pub fn ease_out_elastic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 {
        return 0.0;
    }
    if t == 1.0 {
        return 1.0;
    }
    let p = 0.3;
    let s = p / 4.0;
    2.0_f32.powf(-10.0 * t) * ((t - s) * (2.0 * std::f32::consts::PI) / p).sin() + 1.0
}

// ── Bounce ──────────────────────────────────────────────────────────────────

fn bounce_out(t: f32) -> f32 {
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}

/// Ease-in (bounce).
#[inline]
pub fn ease_in_bounce(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - bounce_out(1.0 - t)
}

/// Ease-out (bounce).
#[inline]
pub fn ease_out_bounce(t: f32) -> f32 {
    bounce_out(t.clamp(0.0, 1.0))
}

/// Ease-in-out (bounce).
#[inline]
pub fn ease_in_out_bounce(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        0.5 * (1.0 - bounce_out(1.0 - 2.0 * t))
    } else {
        0.5 * bounce_out(2.0 * t - 1.0) + 0.5
    }
}

// ── Springs ─────────────────────────────────────────────────────────────────

/// Critically damped spring — no overshoot, fast settle.
/// `damping` controls decay speed (0.5 = fast, 0.1 = slow).
#[inline]
pub fn spring_critically_damped(t: f32, damping: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let d = damping.max(0.01);
    1.0 - (-d * t * 8.0).exp() * (1.0 + d * t * 8.0)
}

/// Underdamped spring — oscillates before settling.
/// `frequency` controls oscillation speed, `decay` controls damping.
#[inline]
pub fn spring_underdamped(t: f32, frequency: f32, decay: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let freq = frequency.max(0.1);
    let dec = decay.max(0.01);
    let envelope = (-dec * t * 8.0).exp();
    let oscillation = (t * freq * std::f32::consts::PI * 2.0).cos();
    1.0 - envelope * oscillation
}

/// Snappy spring — quick settle with minimal overshoot.
#[inline]
pub fn spring_snappy(t: f32) -> f32 {
    spring_underdamped(t, 3.0, 4.0)
}

/// Bouncy spring — noticeable overshoot and oscillation.
#[inline]
pub fn spring_bouncy(t: f32) -> f32 {
    spring_underdamped(t, 4.0, 2.5)
}

/// Gentle spring — slow, luxurious settle.
#[inline]
pub fn spring_gentle(t: f32) -> f32 {
    spring_critically_damped(t, 0.3)
}

// ── Easing function pointer type ────────────────────────────────────────────

/// Type alias for easing functions.
pub type EasingFn = fn(f32) -> f32;

/// All named easing curves for runtime selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Easing {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInQuint,
    EaseOutQuint,
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    EaseInElastic,
    EaseOutElastic,
    EaseInBounce,
    EaseOutBounce,
    EaseInOutBounce,
    SpringCriticallyDamped,
    SpringSnappy,
    SpringBouncy,
    SpringGentle,
}

impl Easing {
    /// Apply this easing to a normalized time value.
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Self::Linear => linear(t),
            Self::EaseInQuad => ease_in_quad(t),
            Self::EaseOutQuad => ease_out_quad(t),
            Self::EaseInOutQuad => ease_in_out_quad(t),
            Self::EaseInCubic => ease_in_cubic(t),
            Self::EaseOutCubic => ease_out_cubic(t),
            Self::EaseInOutCubic => ease_in_out_cubic(t),
            Self::EaseInQuart => ease_in_quart(t),
            Self::EaseOutQuart => ease_out_quart(t),
            Self::EaseInOutQuart => ease_in_out_quart(t),
            Self::EaseInQuint => ease_in_quint(t),
            Self::EaseOutQuint => ease_out_quint(t),
            Self::EaseInSine => ease_in_sine(t),
            Self::EaseOutSine => ease_out_sine(t),
            Self::EaseInOutSine => ease_in_out_sine(t),
            Self::EaseInExpo => ease_in_expo(t),
            Self::EaseOutExpo => ease_out_expo(t),
            Self::EaseInOutExpo => ease_in_out_expo(t),
            Self::EaseInCirc => ease_in_circ(t),
            Self::EaseOutCirc => ease_out_circ(t),
            Self::EaseInOutCirc => ease_in_out_circ(t),
            Self::EaseInBack => ease_in_back(t),
            Self::EaseOutBack => ease_out_back(t),
            Self::EaseInOutBack => ease_in_out_back(t),
            Self::EaseInElastic => ease_in_elastic(t),
            Self::EaseOutElastic => ease_out_elastic(t),
            Self::EaseInBounce => ease_in_bounce(t),
            Self::EaseOutBounce => ease_out_bounce(t),
            Self::EaseInOutBounce => ease_in_out_bounce(t),
            Self::SpringCriticallyDamped => spring_critically_damped(t, 0.5),
            Self::SpringSnappy => spring_snappy(t),
            Self::SpringBouncy => spring_bouncy(t),
            Self::SpringGentle => spring_gentle(t),
        }
    }

    /// Get the function pointer for this easing.
    pub fn fn_ptr(self) -> EasingFn {
        match self {
            Self::Linear => linear,
            Self::EaseInQuad => ease_in_quad,
            Self::EaseOutQuad => ease_out_quad,
            Self::EaseInOutQuad => ease_in_out_quad,
            Self::EaseInCubic => ease_in_cubic,
            Self::EaseOutCubic => ease_out_cubic,
            Self::EaseInOutCubic => ease_in_out_cubic,
            Self::EaseInQuart => ease_in_quart,
            Self::EaseOutQuart => ease_out_quart,
            Self::EaseInOutQuart => ease_in_out_quart,
            Self::EaseInQuint => ease_in_quint,
            Self::EaseOutQuint => ease_out_quint,
            Self::EaseInSine => ease_in_sine,
            Self::EaseOutSine => ease_out_sine,
            Self::EaseInOutSine => ease_in_out_sine,
            Self::EaseInExpo => ease_in_expo,
            Self::EaseOutExpo => ease_out_expo,
            Self::EaseInOutExpo => ease_in_out_expo,
            Self::EaseInCirc => ease_in_circ,
            Self::EaseOutCirc => ease_out_circ,
            Self::EaseInOutCirc => ease_in_out_circ,
            Self::EaseInBack => ease_in_back,
            Self::EaseOutBack => ease_out_back,
            Self::EaseInOutBack => ease_in_out_back,
            Self::EaseInElastic => ease_in_elastic,
            Self::EaseOutElastic => ease_out_elastic,
            Self::EaseInBounce => ease_in_bounce,
            Self::EaseOutBounce => ease_out_bounce,
            Self::EaseInOutBounce => ease_in_out_bounce,
            Self::SpringCriticallyDamped => |t| spring_critically_damped(t, 0.5),
            Self::SpringSnappy => spring_snappy,
            Self::SpringBouncy => spring_bouncy,
            Self::SpringGentle => spring_gentle,
        }
    }

    /// Human-readable label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::EaseInQuad => "ease-in-quad",
            Self::EaseOutQuad => "ease-out-quad",
            Self::EaseInOutQuad => "ease-in-out-quad",
            Self::EaseInCubic => "ease-in-cubic",
            Self::EaseOutCubic => "ease-out-cubic",
            Self::EaseInOutCubic => "ease-in-out-cubic",
            Self::EaseInQuart => "ease-in-quart",
            Self::EaseOutQuart => "ease-out-quart",
            Self::EaseInOutQuart => "ease-in-out-quart",
            Self::EaseInQuint => "ease-in-quint",
            Self::EaseOutQuint => "ease-out-quint",
            Self::EaseInSine => "ease-in-sine",
            Self::EaseOutSine => "ease-out-sine",
            Self::EaseInOutSine => "ease-in-out-sine",
            Self::EaseInExpo => "ease-in-expo",
            Self::EaseOutExpo => "ease-out-expo",
            Self::EaseInOutExpo => "ease-in-out-expo",
            Self::EaseInCirc => "ease-in-circ",
            Self::EaseOutCirc => "ease-out-circ",
            Self::EaseInOutCirc => "ease-in-out-circ",
            Self::EaseInBack => "ease-in-back",
            Self::EaseOutBack => "ease-out-back",
            Self::EaseInOutBack => "ease-in-out-back",
            Self::EaseInElastic => "ease-in-elastic",
            Self::EaseOutElastic => "ease-out-elastic",
            Self::EaseInBounce => "ease-in-bounce",
            Self::EaseOutBounce => "ease-out-bounce",
            Self::EaseInOutBounce => "ease-in-out-bounce",
            Self::SpringCriticallyDamped => "spring-critically-damped",
            Self::SpringSnappy => "spring-snappy",
            Self::SpringBouncy => "spring-bouncy",
            Self::SpringGentle => "spring-gentle",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_easing_at_zero_returns_zero() {
        for easing in [
            Easing::Linear,
            Easing::EaseInQuad,
            Easing::EaseOutCubic,
            Easing::SpringSnappy,
        ] {
            let val = easing.apply(0.0);
            assert!(
                val.abs() < 0.01,
                "{} at t=0 should be ~0, got {}",
                easing.label(),
                val
            );
        }
    }

    #[test]
    fn all_easing_at_one_returns_one() {
        for easing in [
            Easing::Linear,
            Easing::EaseInQuad,
            Easing::EaseOutCubic,
            Easing::SpringSnappy,
        ] {
            let val = easing.apply(1.0);
            assert!(
                (val - 1.0).abs() < 0.05,
                "{} at t=1 should be ~1, got {}",
                easing.label(),
                val
            );
        }
    }

    #[test]
    fn smoothstep_is_monotonic() {
        let mut prev = 0.0;
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let val = ease_in_out_cubic(t);
            assert!(val >= prev, "smoothstep should be monotonic at t={t}");
            prev = val;
        }
    }

    #[test]
    fn bounce_never_exceeds_bounds() {
        for i in 0..=200 {
            let t = i as f32 / 200.0;
            let val = ease_out_bounce(t);
            assert!(
                (-0.01..=1.01).contains(&val),
                "bounce out of bounds at t={t}: {val}"
            );
        }
    }
}
