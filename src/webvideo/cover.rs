//! The cover beside a download: the page's thumbnail, fetched once on its own thread and cut to a 120 pixel square.

use std::sync::Arc;
use std::time::Duration;

use egui::ColorImage;
use image::imageops::FilterType;
use notchbase_net::Mode;

use super::Shared;

const THREAD: &str = "notchbase-video-cover";
const USER_AGENT: &str = notchbase_net::user_agent!();
const TIMEOUT: Duration = Duration::from_secs(15);
const LARGEST: u64 = 8 * 1024 * 1024;
const SIDE: u32 = 120;

pub(super) fn fetch_for(shared: Arc<Shared>, id: u64, address: String) {
    let work = move || {
        if let Some(cover) = fetch(&address) {
            shared.change(id, |download| download.cover = Some(Arc::new(cover)));
        }
    };
    let _ = std::thread::Builder::new().name(THREAD.into()).spawn(work);
}

fn fetch(address: &str) -> Option<ColorImage> {
    let agent = notchbase_net::agent(TIMEOUT, USER_AGENT, Mode::Follow);
    let mut response = agent.get(address).call().ok()?;
    let bytes = response.body_mut().with_config().limit(LARGEST).read_to_vec().ok()?;
    square(&bytes)
}

fn square(bytes: &[u8]) -> Option<ColorImage> {
    let image = image::load_from_memory(bytes).ok()?;
    let side = image.width().min(image.height());
    if side == 0 {
        return None;
    }
    let (x, y) = ((image.width() - side) / 2, (image.height() - side) / 2);
    let square = image.crop_imm(x, y, side, side).resize_exact(SIDE, SIDE, FilterType::Triangle).into_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([SIDE as usize; 2], square.as_raw()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{ImageFormat, Rgb, RgbImage};

    use super::*;

    fn encoded(image: &RgbImage, format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn wide_thumbnail_keeps_middle_square() {
        let red_middle = |x: u32, _| if (70..250).contains(&x) { Rgb([200, 30, 30]) } else { Rgb([0, 0, 0]) };
        let frame = RgbImage::from_fn(320, 180, red_middle);
        let cover = square(&encoded(&frame, ImageFormat::Png)).expect("a cover");
        assert_eq!(cover.size, [SIDE as usize; 2]);
        let red = |pixel: egui::Color32| pixel.r() > 150 && pixel.g() < 80;
        assert!(red(cover.pixels[0]) && red(cover.pixels[cover.pixels.len() - 1]), "the black sides are cut away");
    }

    #[test]
    fn tall_reel_squares_but_rubbish_doesnt() {
        let reel = RgbImage::from_pixel(90, 160, Rgb([20, 120, 200]));
        let cover = square(&encoded(&reel, ImageFormat::Jpeg)).expect("a cover");
        assert_eq!(cover.size, [SIDE as usize; 2]);
        assert!(square(b"<html>not a picture</html>").is_none());
    }

    #[test]
    #[ignore = "fetches a real thumbnail from YouTube"]
    fn real_thumbnail_becomes_a_cover() {
        for address in [
            "https://i.ytimg.com/vi/jNQXAC9IVRw/hqdefault.jpg",
            "https://i.ytimg.com/vi_webp/jNQXAC9IVRw/hqdefault.webp",
        ] {
            let cover = fetch(address).unwrap_or_else(|| panic!("a cover from {address}"));
            assert_eq!(cover.size, [SIDE as usize; 2]);
        }
    }
}
