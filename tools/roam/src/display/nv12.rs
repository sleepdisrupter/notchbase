//! BGRA to NV12 for the H.264 encoder (as the recorder does, in src/record/windows/nv12.rs), and NV12 back to
//! BGRA for drawing decoded pictures. BGRA to NV12 uses with integer BT.709 limited range math. Bands of row pairs convert in
//! parallel, and an oversized area is first box-filtered to half size.

use std::num::NonZeroUsize;

const BYTES_PER_PIXEL: usize = 4;
const MAX_WORKERS: usize = 8;

pub struct Converter {
    source: [usize; 2],
    output: [usize; 2],
    workers: usize,
    halved: Vec<u8>,
    nv12: Vec<u8>,
}

impl Converter {
    pub fn new(source: [u32; 2], output: [u32; 2]) -> Self {
        let [source, output] = [source, output].map(|size| size.map(|side| side as usize));
        let halving = output != source;
        Self {
            source,
            output,
            workers: workers(),
            halved: if halving { vec![0; output[0] * output[1] * BYTES_PER_PIXEL] } else { Vec::new() },
            nv12: vec![0; output[0] * output[1] * 3 / 2],
        }
    }

    pub fn convert(&mut self, bgra: &[u8]) -> &[u8] {
        if self.output == self.source {
            to_nv12(bgra, self.output, self.workers, &mut self.nv12);
        } else {
            halve(bgra, self.source, self.output, &mut self.halved);
            to_nv12(&self.halved, self.output, self.workers, &mut self.nv12);
        }
        &self.nv12
    }
}

/// BT.709 limited range in 8.8 fixed point, like `chroma` below.
fn luma(b: i32, g: i32, r: i32) -> u8 {
    (((47 * r + 157 * g + 16 * b + 128) >> 8) + 16) as u8
}

fn chroma(b: i32, g: i32, r: i32) -> [u8; 2] {
    let u = ((-26 * r - 86 * g + 112 * b + 128) >> 8) + 128;
    let v = ((112 * r - 102 * g - 10 * b + 128) >> 8) + 128;
    [u.clamp(16, 240) as u8, v.clamp(16, 240) as u8]
}

fn workers() -> usize {
    std::thread::available_parallelism().map_or(1, NonZeroUsize::get).clamp(1, MAX_WORKERS)
}

fn to_nv12(bgra: &[u8], [width, height]: [usize; 2], workers: usize, out: &mut [u8]) {
    let (luma_plane, chroma_plane) = out.split_at_mut(width * height);
    let stride = width * BYTES_PER_PIXEL;
    let pairs = (height / 2).div_ceil(workers).max(1);
    let lumas = luma_plane.chunks_mut((2 * width * pairs).max(1));
    let chromas = chroma_plane.chunks_mut((width * pairs).max(1)).chain(std::iter::repeat_with(Default::default));
    let sources = bgra.chunks((2 * stride * pairs).max(1));
    // Each band writes its own slices of both planes, so the workers need no locking.
    std::thread::scope(|scope| {
        for ((luma, chroma), source) in lumas.zip(chromas).zip(sources) {
            scope.spawn(move || band(source, width, luma, chroma));
        }
    });
}

fn band(bgra: &[u8], width: usize, luma_rows: &mut [u8], chroma_rows: &mut [u8]) {
    let stride = width * BYTES_PER_PIXEL;
    for (row, line) in bgra.chunks_exact(stride).zip(luma_rows.chunks_exact_mut(width)) {
        for (dst, px) in line.iter_mut().zip(row.chunks_exact(BYTES_PER_PIXEL)) {
            *dst = luma(i32::from(px[0]), i32::from(px[1]), i32::from(px[2]));
        }
    }
    for (rows, uv) in bgra.chunks_exact(2 * stride).zip(chroma_rows.chunks_exact_mut(width)) {
        let (top, bottom) = rows.split_at(stride);
        for (x, dst) in uv.chunks_exact_mut(2).enumerate() {
            let at = 2 * x * BYTES_PER_PIXEL;
            let sum = |channel: usize| {
                let pick = |line: &[u8], offset: usize| i32::from(line[at + offset + channel]);
                pick(top, 0) + pick(top, BYTES_PER_PIXEL) + pick(bottom, 0) + pick(bottom, BYTES_PER_PIXEL)
            };
            let [u, v] = chroma((sum(0) + 2) / 4, (sum(1) + 2) / 4, (sum(2) + 2) / 4);
            dst[0] = u;
            dst[1] = v;
        }
    }
}

/// Box-filters each 2 by 2 block of pixels into one.
fn halve(bgra: &[u8], [width, _]: [usize; 2], [out_width, out_height]: [usize; 2], out: &mut [u8]) {
    let stride = width * BYTES_PER_PIXEL;
    for y in 0..out_height {
        let top = &bgra[2 * y * stride..(2 * y + 1) * stride];
        let bottom = &bgra[(2 * y + 1) * stride..(2 * y + 2) * stride];
        for x in 0..out_width {
            let at = 2 * x * BYTES_PER_PIXEL;
            let dst = (y * out_width + x) * BYTES_PER_PIXEL;
            for channel in 0..3 {
                let sum = u32::from(top[at + channel])
                    + u32::from(top[at + BYTES_PER_PIXEL + channel])
                    + u32::from(bottom[at + channel])
                    + u32::from(bottom[at + BYTES_PER_PIXEL + channel]);
                out[dst + channel] = ((sum + 2) / 4) as u8;
            }
            out[dst + 3] = u8::MAX;
        }
    }
}

/// NV12 (BT.709, limited range, `stride` bytes a row in both planes, the luma plane `rows`
/// tall, which a decoder may pad past `height`) to BGRA, rows of pairs in parallel.
pub fn to_bgra(nv12: &[u8], [width, height]: [usize; 2], stride: usize, rows: usize, out: &mut [u8]) {
    let (luma_plane, chroma_plane) = nv12.split_at((stride * rows.max(height)).min(nv12.len()));
    let pairs = (height / 2).div_ceil(workers()).max(1);
    let lines = out.chunks_mut((2 * width * BYTES_PER_PIXEL * pairs).max(1));
    let lumas = luma_plane.chunks(2 * stride * pairs);
    let chromas = chroma_plane.chunks(stride * pairs);
    std::thread::scope(|scope| {
        for ((rows, luma), chroma) in lines.zip(lumas).zip(chromas) {
            scope.spawn(move || back_band(luma, chroma, width, stride, rows));
        }
    });
}

fn back_band(luma: &[u8], chroma: &[u8], width: usize, stride: usize, out: &mut [u8]) {
    let row = width * BYTES_PER_PIXEL;
    for (y, line) in out.chunks_exact_mut(row).enumerate() {
        let (Some(ys), Some(uv)) = (luma.get(y * stride..y * stride + width), chroma.get((y / 2) * stride..)) else {
            break;
        };
        for (x, px) in line.chunks_exact_mut(BYTES_PER_PIXEL).enumerate() {
            let c = 298 * (i32::from(ys[x]) - 16);
            let at = x & !1;
            let (u, v) = (
                i32::from(uv.get(at).copied().unwrap_or(128)) - 128,
                i32::from(uv.get(at + 1).copied().unwrap_or(128)) - 128,
            );
            let r = (c + 459 * v + 128) >> 8;
            let g = (c - 55 * u - 136 * v + 128) >> 8;
            let b = (c + 541 * u + 128) >> 8;
            px[0] = b.clamp(0, 255) as u8;
            px[1] = g.clamp(0, 255) as u8;
            px[2] = r.clamp(0, 255) as u8;
            px[3] = u8::MAX;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(size: [usize; 2], bgra: [u8; 4]) -> Vec<u8> {
        bgra.repeat(size[0] * size[1])
    }

    #[test]
    fn primaries_hit_studio_range_bt709() {
        assert_eq!(luma(0, 0, 0), 16);
        assert_eq!(luma(255, 255, 255), 235);
        assert_eq!(chroma(255, 255, 255), [128, 128]);
        assert_eq!(chroma(0, 0, 0), [128, 128]);
        let red = (luma(0, 0, 255), chroma(0, 0, 255));
        assert_eq!(red, (63, [102, 240]));
        let blue = (luma(255, 0, 0), chroma(255, 0, 0));
        assert_eq!(blue, (32, [240, 118]));
    }

    #[test]
    fn solid_frame_fills_both_planes() {
        let mut converter = Converter::new([4, 2], [4, 2]);
        let nv12 = converter.convert(&solid([4, 2], [255, 255, 255, 255])).to_vec();
        assert_eq!(nv12.len(), 12);
        assert!(nv12[..8].iter().all(|&y| y == 235));
        assert!(nv12[8..].iter().all(|&c| c == 128));
    }

    #[test]
    fn chroma_averages_each_2x2_block() {
        let mut frame = solid([2, 2], [0, 0, 0, 255]);
        frame[0..4].copy_from_slice(&[0, 0, 255, 255]);
        frame[12..16].copy_from_slice(&[0, 0, 255, 255]);
        let nv12 = Converter::new([2, 2], [2, 2]).convert(&frame).to_vec();
        assert_eq!(&nv12[..4], &[63, 16, 16, 63]);
        assert_eq!(&nv12[4..], &chroma(0, 0, 128));
    }

    #[test]
    fn worker_count_changes_nothing() {
        for size in [[6, 7], [8, 10], [10, 1], [2, 2]] {
            let frame: Vec<u8> = (0..size[0] * size[1] * 4).map(|i| (i * 37 % 251) as u8).collect();
            let alone = {
                let mut out = vec![0; size[0] * size[1] * 3 / 2];
                to_nv12(&frame, size, 1, &mut out);
                out
            };
            for workers in [2, 3, 8] {
                let mut out = vec![0; alone.len()];
                to_nv12(&frame, size, workers, &mut out);
                assert_eq!(out, alone, "{size:?} across {workers}");
            }
        }
    }

    #[test]
    fn colours_survive_the_round_trip() {
        for bgra in [[255, 255, 255, 255], [0, 0, 0, 255], [0, 0, 255, 255], [40, 200, 90, 255]] {
            let nv12 = Converter::new([4, 2], [4, 2]).convert(&solid([4, 2], bgra)).to_vec();
            let mut back = vec![0; 4 * 2 * 4];
            to_bgra(&nv12, [4, 2], 4, 2, &mut back);
            for px in back.chunks(4) {
                for channel in 0..3 {
                    assert!(px[channel].abs_diff(bgra[channel]) <= 3, "{bgra:?} came back {px:?}");
                }
            }
        }
    }

    #[test]
    fn oversized_frames_are_halved_first() {
        let mut frame = solid([4, 4], [0, 0, 0, 255]);
        for px in frame.chunks_exact_mut(4).step_by(2) {
            px.copy_from_slice(&[255, 255, 255, 255]);
        }
        let mut converter = Converter::new([4, 4], [2, 2]);
        let nv12 = converter.convert(&frame).to_vec();
        assert_eq!(nv12.len(), 6);
        assert!(nv12[..4].iter().all(|&y| y == luma(128, 128, 128)));
    }
}
