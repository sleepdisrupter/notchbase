//! A color that eases toward the sampled one, for the bezel and pill tints. It starts on its first target and
//! snaps to the target once the difference would round away.

use egui::Color32;

/// Time constant of the easing, in seconds.
const TIME: f32 = 0.08;

#[derive(Default)]
pub(super) struct Tint {
    value: [f32; 3],
    target: [f32; 3],
    ready: bool,
}

impl Tint {
    pub(super) fn update(&mut self, target: Color32, dt: f32) -> Color32 {
        let target = [target.r() as f32, target.g() as f32, target.b() as f32];
        if self.ready {
            let k = 1.0 - (-dt / TIME).exp();
            for (v, t) in self.value.iter_mut().zip(target) {
                *v += (t - *v) * k;
            }
        } else {
            self.value = target;
            self.ready = true;
        }
        self.target = target;
        if self.is_settled() {
            self.value = target;
        }
        let [r, g, b] = self.value.map(|v| v.round().clamp(0.0, 255.0) as u8);
        Color32::from_rgb(r, g, b)
    }

    fn is_settled(&self) -> bool {
        self.value.iter().zip(self.target).all(|(v, t)| (v - t).abs() < 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_eases_to_target_and_settles() {
        let mut tint = Tint::default();
        assert_eq!(tint.update(Color32::from_rgb(10, 20, 30), 1.0 / 60.0), Color32::from_rgb(10, 20, 30));
        assert!(tint.is_settled());
        let first = tint.update(Color32::from_rgb(210, 20, 30), 1.0 / 60.0);
        assert!(first.r() > 10 && first.r() < 210);
        assert!(!tint.is_settled());
        for _ in 0..120 {
            tint.update(Color32::from_rgb(210, 20, 30), 1.0 / 60.0);
        }
        assert!(tint.is_settled());
    }
}
