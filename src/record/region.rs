//! The recorded display and area in physical pixels, with sides rounded down to the even sizes encoders need.

use winit::monitor::MonitorHandle;

const MIN_SIDE: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub id: u32,
    pub origin: [i32; 2],
    pub size: [u32; 2],
    pub scale: f64,
}

impl Display {
    pub fn of(monitor: &MonitorHandle) -> Self {
        let (position, size) = (monitor.position(), monitor.size());
        Self {
            id: native_id(monitor),
            origin: [position.x, position.y],
            size: [size.width, size.height],
            scale: monitor.scale_factor(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub display: Display,
    pub origin: [u32; 2],
    pub size: [u32; 2],
}

impl Region {
    /// Clamps the area into the display, with even sides of at least 2.
    pub fn new(display: Display, origin: [u32; 2], size: [u32; 2]) -> Self {
        let origin = [0, 1].map(|axis| origin[axis].min(display.size[axis].saturating_sub(MIN_SIDE)));
        let size = [0, 1].map(|axis| {
            let room = display.size[axis].saturating_sub(origin[axis]);
            (size[axis].min(room) & !1).max(MIN_SIDE)
        });
        Self { display, origin, size }
    }

    #[cfg(test)]
    pub fn whole(display: Display) -> Self {
        Self::new(display, [0, 0], display.size)
    }

    pub fn is_whole(&self) -> bool {
        self.origin == [0, 0] && self.size == [0, 1].map(|axis| self.display.size[axis] & !1)
    }

    /// The area's top left corner in desktop coordinates.
    #[cfg(any(test, target_os = "windows"))]
    pub fn desktop_origin(&self) -> [i32; 2] {
        [0, 1].map(|axis| self.display.origin[axis] + self.origin[axis] as i32)
    }
}

#[cfg(target_os = "macos")]
fn native_id(monitor: &MonitorHandle) -> u32 {
    use winit::platform::macos::MonitorHandleExtMacOS as _;
    monitor.native_id()
}

#[cfg(not(target_os = "macos"))]
fn native_id(_monitor: &MonitorHandle) -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: Display = Display { id: 1, origin: [-1920, 0], size: [1920, 1080], scale: 1.25 };

    #[test]
    fn sides_are_even_and_on_display() {
        let region = Region::new(DISPLAY, [101, 33], [301, 1200]);
        assert_eq!((region.origin, region.size), ([101, 33], [300, 1046]));
        assert_eq!(region.desktop_origin(), [-1819, 33]);
        let corner = Region::new(DISPLAY, [5000, 5000], [9, 9]);
        assert_eq!((corner.origin, corner.size), ([1918, 1078], [2, 2]));
    }

    #[test]
    fn whole_display_is_recognised() {
        assert!(Region::whole(DISPLAY).is_whole());
        assert!(!Region::new(DISPLAY, [0, 0], [1920, 1000]).is_whole());
        let odd = Display { size: [1367, 769], ..DISPLAY };
        let whole = Region::whole(odd);
        assert_eq!(whole.size, [1366, 768]);
        assert!(whole.is_whole());
    }
}
