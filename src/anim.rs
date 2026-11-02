use std::time::Duration;

use crate::geometry::Size;

pub const DURATION_MS: u64 = 180;
/// Tope de avance por fotograma: un atasco pausa la animación en lugar de saltarla.
pub const MAX_STEP: Duration = Duration::from_millis(20);

/// Interpolación de tamaño entre dos estados, con ease-out cúbico, en tiempo virtual.
#[derive(Clone, Copy, Debug)]
pub struct SizeAnim {
    from: Size,
    to: Size,
    progress: Duration,
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn lerp(a: u32, b: u32, k: f32) -> u32 {
    (a as f32 + (b as f32 - a as f32) * k).round() as u32
}

impl SizeAnim {
    pub fn new(from: Size, to: Size) -> Self {
        Self {
            from,
            to,
            progress: Duration::ZERO,
        }
    }

    /// Avanza `dt` (como mucho `MAX_STEP`) y devuelve el tamaño y si terminó
    /// (entonces es exactamente `to`).
    pub fn advance(&mut self, dt: Duration) -> (Size, bool) {
        self.progress += dt.min(MAX_STEP);
        if self.progress >= Duration::from_millis(DURATION_MS) {
            return (self.to, true);
        }
        let t = self.progress.as_secs_f32() / (DURATION_MS as f32 / 1000.0);
        let k = ease_out_cubic(t);
        (
            Size {
                w: lerp(self.from.w, self.to.w, k),
                h: lerp(self.from.h, self.to.h, k),
            },
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PILL: Size = Size { w: 220, h: 36 };
    const OPEN: Size = Size { w: 520, h: 300 };
    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn easing_endpoints() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert_eq!(ease_out_cubic(0.5), 0.875);
    }

    #[test]
    fn zero_dt_stays_at_from() {
        let mut a = SizeAnim::new(PILL, OPEN);
        assert_eq!(a.advance(Duration::ZERO), (PILL, false));
    }

    #[test]
    fn stall_advances_only_one_step() {
        let mut stalled = SizeAnim::new(PILL, OPEN);
        let mut one_step = SizeAnim::new(PILL, OPEN);
        assert_eq!(stalled.advance(MS(100)), one_step.advance(MS(20)));
        assert!(!stalled.advance(MS(0)).1);
    }

    #[test]
    fn finishes_after_enough_frames_exactly_on_to() {
        let mut a = SizeAnim::new(PILL, OPEN);
        for _ in 0..8 {
            assert!(!a.advance(MS(20)).1);
        }
        // 9 x 20 ms = 180 ms
        assert_eq!(a.advance(MS(20)), (OPEN, true));
        assert_eq!(a.advance(MS(20)), (OPEN, true));
    }

    #[test]
    fn long_stalls_still_take_nine_frames() {
        let mut a = SizeAnim::new(PILL, OPEN);
        for _ in 0..8 {
            assert!(!a.advance(MS(500)).1);
        }
        assert_eq!(a.advance(MS(500)), (OPEN, true));
    }

    #[test]
    fn midway_is_eased_when_growing() {
        let mut a = SizeAnim::new(PILL, OPEN);
        let mut last = (PILL, false);
        for _ in 0..4 {
            last = a.advance(MS(20));
        }
        let (s, done) = last;
        assert!(!done);
        assert!(s.w > 370 && s.w < 520, "w = {}", s.w);
        assert!(s.h > 36 && s.h < 300);
    }

    #[test]
    fn shrinks_too() {
        let mut a = SizeAnim::new(OPEN, PILL);
        let mut s = (OPEN, false);
        for _ in 0..4 {
            s = a.advance(MS(20));
        }
        assert!(!s.1);
        assert!(s.0.w > 220 && s.0.w < 370, "w = {}", s.0.w);
        for _ in 0..5 {
            s = a.advance(MS(20));
        }
        assert_eq!(s, (PILL, true));
    }
}
