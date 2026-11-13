//! Captures a monitor with DXGI Desktop Duplication, falling back to GDI when that fails or comes back black.

mod duplication;
mod gdi;

use winit::monitor::MonitorHandle;

use crate::platform::Capture;

const BYTES_PER_PIXEL: usize = 4;
const OPAQUE: u8 = 255;
const MOSTLY_BLACK: f32 = 0.98;
const SAMPLE_EVERY: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Area {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Area {
    fn of(monitor: &MonitorHandle) -> Option<Self> {
        let origin = monitor.position();
        let size = monitor.size();
        let width = i32::try_from(size.width).ok().filter(|&w| w > 0)?;
        let height = i32::try_from(size.height).ok().filter(|&h| h > 0)?;
        Some(Self { x: origin.x, y: origin.y, width, height })
    }

    fn bytes(self) -> usize {
        self.width as usize * self.height as usize * BYTES_PER_PIXEL
    }
}

pub fn capture_monitor(monitor: &MonitorHandle) -> Result<Capture, String> {
    let area = Area::of(monitor).ok_or("monitor has no usable size")?;
    let mut rgba = grab_bgra(area)?;
    for px in rgba.chunks_exact_mut(BYTES_PER_PIXEL) {
        px.swap(0, 2);
        px[3] = OPAQUE;
    }
    Ok(Capture { width: area.width as usize, height: area.height as usize, rgba })
}

pub fn screen_capture_access(_request: bool) -> bool {
    true
}

fn grab_bgra(area: Area) -> Result<Vec<u8>, String> {
    let composed = duplication::grab_bgra(area);
    if let Ok(pixels) = &composed
        && black_share(pixels) < MOSTLY_BLACK
    {
        return composed;
    }
    match (composed, gdi::grab_bgra(area)) {
        (Ok(gpu), Ok(gdi)) => Ok(if black_share(&gdi) < black_share(&gpu) { gdi } else { gpu }),
        (Ok(pixels), Err(_)) => Ok(pixels),
        (Err(err), fallback) => {
            eprintln!("notchbase: GPU screen capture unavailable ({err}), using GDI");
            fallback
        }
    }
}

fn black_share(bgra: &[u8]) -> f32 {
    let (mut black, mut seen) = (0usize, 0usize);
    for px in bgra.chunks_exact(BYTES_PER_PIXEL).step_by(SAMPLE_EVERY) {
        seen += 1;
        black += usize::from(px[..3] == [0, 0, 0]);
    }
    if seen == 0 { 1.0 } else { black as f32 / seen as f32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearly_black_counts_as_black() {
        let mut pixels = [0u8, 0, 0, 255].repeat(700);
        assert_eq!(black_share(&pixels), 1.0);
        pixels[..4].copy_from_slice(&[9, 9, 9, 255]);
        assert!(black_share(&pixels) >= MOSTLY_BLACK, "a lone cursor doesn't make it a picture");
        let picture = [30u8, 40, 50, 255].repeat(700);
        assert_eq!(black_share(&picture), 0.0);
        assert_eq!(black_share(&[]), 1.0);
    }
}
