//! One value for output and one for input.

use super::DeviceKind;

#[derive(Clone, Default, PartialEq)]
pub struct PerKind<T> {
    output: T,
    input: T,
}

impl<T> PerKind<T> {
    pub fn from_fn(mut make: impl FnMut(DeviceKind) -> T) -> Self {
        Self { output: make(DeviceKind::Output), input: make(DeviceKind::Input) }
    }

    pub fn get(&self, kind: DeviceKind) -> &T {
        match kind {
            DeviceKind::Output => &self.output,
            DeviceKind::Input => &self.input,
        }
    }

    pub fn get_mut(&mut self, kind: DeviceKind) -> &mut T {
        match kind {
            DeviceKind::Output => &mut self.output,
            DeviceKind::Input => &mut self.input,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_owns_its_own_slot() {
        let mut slots = PerKind::from_fn(|kind| vec![kind]);
        slots.get_mut(DeviceKind::Input).push(DeviceKind::Output);
        assert_eq!(slots.get(DeviceKind::Output), &[DeviceKind::Output]);
        assert_eq!(slots.get(DeviceKind::Input), &[DeviceKind::Input, DeviceKind::Output]);
    }
}
