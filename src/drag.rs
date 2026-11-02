//! Máquina de arrastre de la pastilla: distingue un clic de un arrastre (en 2D).

/// Distancia (px, en 2D) a partir de la cual un movimiento es un arrastre.
pub const THRESHOLD: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragEv {
    /// Botón pulsado; (x, y) del puntero.
    Press(f32, f32),
    /// Puntero movido; (x, y).
    Move(f32, f32),
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragOut {
    None,
    /// Se soltó sin haber pasado del umbral.
    Click,
    /// Arrastre en curso: nuevo centro (x, y) de la pastilla.
    Moved(f32, f32),
    /// Fin del arrastre: centro final (x, y).
    Done(f32, f32),
}

pub struct Drag {
    center: (f32, f32),
    /// (x, y) de la pulsación; None si no hay botón pulsado.
    start: Option<(f32, f32)>,
    active: bool,
    /// Último centro calculado (el que devuelve `Done`).
    last_center: (f32, f32),
}

impl Drag {
    pub fn new(center_x: f32, center_y: f32) -> Self {
        Self {
            center: (center_x, center_y),
            start: None,
            active: false,
            last_center: (center_x, center_y),
        }
    }

    pub fn on(&mut self, ev: DragEv) -> DragOut {
        match ev {
            DragEv::Press(x, y) => {
                self.start = Some((x, y));
                self.active = false;
                self.last_center = self.center;
                DragOut::None
            }
            DragEv::Move(x, y) => {
                let Some((sx, sy)) = self.start else {
                    return DragOut::None;
                };
                let (dx, dy) = (x - sx, y - sy);
                if !self.active && dx.hypot(dy) <= THRESHOLD {
                    return DragOut::None;
                }
                self.active = true;
                self.last_center = (self.center.0 + dx, self.center.1 + dy);
                DragOut::Moved(self.last_center.0, self.last_center.1)
            }
            DragEv::Release => {
                if self.start.take().is_none() {
                    return DragOut::None;
                }
                if std::mem::take(&mut self.active) {
                    DragOut::Done(self.last_center.0, self.last_center.1)
                } else {
                    DragOut::Click
                }
            }
        }
    }

    /// Ya pasó del umbral: es un arrastre y no un clic.
    pub fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: f32 = 960.0;
    const CY: f32 = 18.0;

    fn drag() -> Drag {
        Drag::new(C, CY)
    }

    #[test]
    fn small_jitter_is_a_click() {
        let mut d = drag();
        assert_eq!(d.on(DragEv::Press(100.0, 10.0)), DragOut::None);
        assert_eq!(d.on(DragEv::Move(102.0, 14.0)), DragOut::None);
        assert!(!d.is_active());
        assert_eq!(d.on(DragEv::Release), DragOut::Click);
    }

    #[test]
    fn drag_moves_and_finishes() {
        let mut d = drag();
        d.on(DragEv::Press(100.0, 10.0));
        assert_eq!(
            d.on(DragEv::Move(110.0, 10.0)),
            DragOut::Moved(C + 10.0, CY)
        );
        assert!(d.is_active());
        assert_eq!(d.on(DragEv::Move(90.0, 10.0)), DragOut::Moved(C - 10.0, CY));
        assert_eq!(
            d.on(DragEv::Move(110.0, 10.0)),
            DragOut::Moved(C + 10.0, CY)
        );
        assert_eq!(d.on(DragEv::Release), DragOut::Done(C + 10.0, CY));
    }

    #[test]
    fn vertical_drag_moves_y() {
        let mut d = drag();
        d.on(DragEv::Press(100.0, 10.0));
        assert_eq!(
            d.on(DragEv::Move(100.0, 20.0)),
            DragOut::Moved(C, CY + 10.0)
        );
        assert_eq!(d.on(DragEv::Release), DragOut::Done(C, CY + 10.0));
    }

    #[test]
    fn diagonal_distance_counts_in_2d() {
        let mut d = drag();
        d.on(DragEv::Press(0.0, 0.0));
        // 4 + 4: cada eje bajo el umbral, pero la distancia 2D (5,66) también.
        assert_eq!(d.on(DragEv::Move(4.0, 4.0)), DragOut::None);
        // 5 + 5: 7,07 > 6.
        assert_eq!(
            d.on(DragEv::Move(5.0, 5.0)),
            DragOut::Moved(C + 5.0, CY + 5.0)
        );
    }

    #[test]
    fn jitter_back_after_activation_stays_a_drag() {
        let mut d = drag();
        d.on(DragEv::Press(100.0, 10.0));
        d.on(DragEv::Move(120.0, 10.0));
        assert_eq!(d.on(DragEv::Move(101.0, 10.0)), DragOut::Moved(C + 1.0, CY));
        assert_eq!(d.on(DragEv::Release), DragOut::Done(C + 1.0, CY));
    }

    #[test]
    fn release_without_press_is_nothing() {
        let mut d = drag();
        assert_eq!(d.on(DragEv::Release), DragOut::None);
        assert_eq!(d.on(DragEv::Move(500.0, 5.0)), DragOut::None);
    }

    #[test]
    fn negative_and_far_coordinates_work() {
        let mut d = drag();
        d.on(DragEv::Press(100.0, 10.0));
        assert_eq!(
            d.on(DragEv::Move(-300.0, 410.0)),
            DragOut::Moved(C - 400.0, CY + 400.0)
        );
        assert_eq!(d.on(DragEv::Release), DragOut::Done(C - 400.0, CY + 400.0));
    }
}
