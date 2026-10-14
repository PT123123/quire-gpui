use gpui::{Pixels, px as gpui_px};
use std::sync::atomic::{AtomicU32, Ordering};

static UI_SCALE: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
const ZOOM_STEPS: [f32; 13] = [
    0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0,
];

pub fn get() -> f32 {
    f32::from_bits(UI_SCALE.load(Ordering::Relaxed))
}

pub fn set(scale: f32) {
    UI_SCALE.store(scale.clamp(0.5, 3.0).to_bits(), Ordering::Relaxed);
}

pub fn step(scale: f32, direction: i32) -> f32 {
    if direction > 0 {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|candidate| *candidate > scale + 0.001)
            .unwrap_or(3.0)
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|candidate| *candidate < scale - 0.001)
            .unwrap_or(0.5)
    }
}

pub fn px(value: f32) -> Pixels {
    gpui_px(value * get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_shared_and_clamped() {
        set(1.2);
        assert!((get() - 1.2).abs() < f32::EPSILON);
        assert!((f32::from(px(10.)) - 12.).abs() < f32::EPSILON);
        assert_eq!(step(1., 1), 1.1);
        assert_eq!(step(1., -1), 0.9);
        assert_eq!(step(3., 1), 3.);
        assert_eq!(step(0.5, -1), 0.5);
        assert_eq!(step(1.02, 1), 1.1);
        assert_eq!(step(1.02, -1), 1.);
        set(4.);
        assert_eq!(get(), 3.);
        set(0.1);
        assert_eq!(get(), 0.5);
        set(1.);
    }
}
