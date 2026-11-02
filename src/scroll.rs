use std::time::{Duration, Instant};

/// Píxeles de desplazamiento continuo que equivalen a un paso de página.
const PIXELS_PER_STEP: f32 = 40.0;
/// Enfriamiento por defecto: tras un paso se ignoran los eventos y el acumulador se reinicia
/// tras este tiempo sin eventos.
const COOLDOWN: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Delta {
    /// Rueda con muescas (unidades de línea).
    Lines(f32),
    /// Desplazamiento continuo (panel táctil), en píxeles.
    Pixels(f32),
}

/// Convierte el flujo de eventos de rueda en como máximo un paso de página por gesto.
#[derive(Debug)]
pub struct ScrollGate {
    cooldown: Duration,
    acc: f32,
    last_event: Option<Instant>,
    blocked_until: Option<Instant>,
}

impl Default for ScrollGate {
    fn default() -> Self {
        Self::with_cooldown(COOLDOWN)
    }
}

impl ScrollGate {
    /// Igual que `default` pero con otro enfriamiento (mayor = menos pasos por segundo).
    pub fn with_cooldown(cooldown: Duration) -> Self {
        ScrollGate {
            cooldown,
            acc: 0.0,
            last_event: None,
            blocked_until: None,
        }
    }

    /// Devuelve +1 (página siguiente, `y < 0`), -1 (anterior, `y > 0`) o 0.
    pub fn feed(&mut self, delta: Delta, now: Instant) -> i32 {
        if self.blocked_until.is_some_and(|b| now < b) {
            self.last_event = Some(now);
            return 0;
        }
        if self
            .last_event
            .is_some_and(|l| now.duration_since(l) > self.cooldown)
        {
            self.acc = 0.0;
        }
        self.last_event = Some(now);
        self.acc += match delta {
            Delta::Lines(y) => y,
            Delta::Pixels(y) => y / PIXELS_PER_STEP,
        };
        if self.acc.abs() < 1.0 {
            return 0;
        }
        let step = if self.acc < 0.0 { 1 } else { -1 };
        self.acc = 0.0;
        self.blocked_until = Some(now + self.cooldown);
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
    }

    #[test]
    fn one_notch_delivered_twice_is_one_step() {
        let t = Instant::now();
        let mut g = ScrollGate::default();
        assert_eq!(g.feed(Delta::Lines(-1.0), t), 1);
        assert_eq!(g.feed(Delta::Lines(-1.0), ms(t, 5)), 0);
    }

    #[test]
    fn short_cooldown_allows_fast_notches_but_dedupes_duplicates() {
        let t = Instant::now();
        let mut g = ScrollGate::with_cooldown(Duration::from_millis(40));
        assert_eq!(g.feed(Delta::Lines(-1.0), t), 1);
        assert_eq!(g.feed(Delta::Lines(-1.0), ms(t, 5)), 0);
        assert_eq!(g.feed(Delta::Lines(-1.0), ms(t, 80)), 1);
        // El gate por defecto sigue bloqueando a 80 ms.
        let mut d = ScrollGate::default();
        assert_eq!(d.feed(Delta::Lines(-1.0), t), 1);
        assert_eq!(d.feed(Delta::Lines(-1.0), ms(t, 80)), 0);
    }

    #[test]
    fn two_notches_400ms_apart_are_two_steps() {
        let t = Instant::now();
        let mut g = ScrollGate::default();
        assert_eq!(g.feed(Delta::Lines(-1.0), t), 1);
        assert_eq!(g.feed(Delta::Lines(-1.0), ms(t, 400)), 1);
    }

    #[test]
    fn small_pixel_deltas_accumulate() {
        let t = Instant::now();
        let mut g = ScrollGate::default();
        assert_eq!(g.feed(Delta::Pixels(-15.0), t), 0);
        assert_eq!(g.feed(Delta::Pixels(-15.0), ms(t, 10)), 0);
        assert_eq!(g.feed(Delta::Pixels(-15.0), ms(t, 20)), 1);
    }

    #[test]
    fn long_swipe_is_one_step_then_cooldown() {
        let t = Instant::now();
        let mut g = ScrollGate::default();
        let steps: i32 = (0..10)
            .map(|i| g.feed(Delta::Pixels(-30.0), ms(t, i * 10)))
            .sum();
        assert_eq!(steps, 1);
    }

    #[test]
    fn accumulator_resets_after_idle() {
        let t = Instant::now();
        let mut g = ScrollGate::default();
        assert_eq!(g.feed(Delta::Pixels(-30.0), t), 0);
        assert_eq!(g.feed(Delta::Pixels(-30.0), ms(t, 300)), 0);
    }

    #[test]
    fn direction_sign_is_preserved() {
        let t = Instant::now();
        let mut g = ScrollGate::default();
        assert_eq!(g.feed(Delta::Lines(1.0), t), -1);
        assert_eq!(g.feed(Delta::Pixels(50.0), ms(t, 500)), -1);
        assert_eq!(g.feed(Delta::Pixels(-50.0), ms(t, 1000)), 1);
    }

    #[test]
    fn opposite_deltas_cancel() {
        let t = Instant::now();
        let mut g = ScrollGate::default();
        assert_eq!(g.feed(Delta::Pixels(-30.0), t), 0);
        assert_eq!(g.feed(Delta::Pixels(30.0), ms(t, 10)), 0);
        assert_eq!(g.feed(Delta::Pixels(-30.0), ms(t, 20)), 0);
    }
}
