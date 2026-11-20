//! Pictures that stay on the graphics card, as game streaming keeps them: the duplicated
//! desktop is copied card to card, the pointer drawn on with only its own patch coming off
//! the card, and the card's video processor turns it into NV12 for the encoder. No full
//! picture is read back or converted on the processor.

use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_FLAG, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VIDEO_ENCODER, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
    D3D11_VIDEO_PROCESSOR_COLOR_SPACE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
    D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    ID3D11VideoContext, ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator,
    ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::core::Interface;

use crate::display::cursor::Pointer;

const BYTES_PER_PIXEL: usize = 4;
/// NV12 pictures in turn: the encoder still reads one while the next is made.
const POOL: usize = 6;
/// The video processor's format support flags.
const FORMAT_INPUT: u32 = 1;
const FORMAT_OUTPUT: u32 = 2;
/// Full-range RGB in; BT.709 limited range out, as `nv12.rs` makes it and the Mac expects.
/// The legacy colour space bits: Usage, RGB_Range, YCbCr_Matrix, YCbCr_xvYCC, Nominal_Range.
const RGB_FULL: u32 = 0;
const YUV_709_LIMITED: u32 = (1 << 2) | (1 << 4);

/// Makes NV12 pictures on the graphics card from the desktop and the pointer.
pub struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    video: ID3D11VideoContext,
    processor: ID3D11VideoProcessor,
    size: [u32; 2],
    /// The desktop with the pointer on it, and the video processor's view of it.
    composed: ID3D11Texture2D,
    input: ID3D11VideoProcessorInputView,
    /// Where the pointer's patch comes off the card, and its pixels.
    patch: Option<(ID3D11Texture2D, [usize; 2])>,
    pixels: Vec<u8>,
    pool: Vec<(ID3D11Texture2D, ID3D11VideoProcessorOutputView)>,
    next: usize,
}

// SAFETY: used on the capture thread only; the device is multithread protected for the encoder.
unsafe impl Send for Gpu {}

impl Gpu {
    pub fn new(device: &ID3D11Device, context: &ID3D11DeviceContext, size: [u32; 2], fps: u32) -> Result<Self, String> {
        fn err(what: &'static str) -> impl Fn(windows::core::Error) -> String {
            move |err| format!("{what}: {}", err.message())
        }
        let video_device: ID3D11VideoDevice = device.cast().map_err(err("No video on this graphics device"))?;
        let video: ID3D11VideoContext = context.cast().map_err(err("No video on this graphics device"))?;
        let rate = DXGI_RATIONAL { Numerator: fps, Denominator: 1 };
        let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: rate,
            InputWidth: size[0],
            InputHeight: size[1],
            OutputFrameRate: rate,
            OutputWidth: size[0],
            OutputHeight: size[1],
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        // SAFETY: plain creation calls on a device we hold; everything made is owned here.
        unsafe {
            let enumerator: ID3D11VideoProcessorEnumerator =
                video_device.CreateVideoProcessorEnumerator(&content).map_err(err("No video processor"))?;
            let supports = |format: DXGI_FORMAT, flag: u32| {
                enumerator.CheckVideoProcessorFormat(format).is_ok_and(|flags| flags & flag != 0)
            };
            if !supports(DXGI_FORMAT_B8G8R8A8_UNORM, FORMAT_INPUT) || !supports(DXGI_FORMAT_NV12, FORMAT_OUTPUT) {
                return Err("The video processor can't turn BGRA into NV12".into());
            }
            let processor = video_device.CreateVideoProcessor(&enumerator, 0).map_err(err("No video processor"))?;
            video.VideoProcessorSetStreamFrameFormat(&processor, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            video.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
            video.VideoProcessorSetStreamColorSpace(
                &processor,
                0,
                &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: RGB_FULL },
            );
            video.VideoProcessorSetOutputColorSpace(
                &processor,
                &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: YUV_709_LIMITED },
            );
            let composed = texture(
                device,
                size,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                &[D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_RENDER_TARGET],
            )?;
            let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 },
                },
            };
            let mut input = None;
            video_device
                .CreateVideoProcessorInputView(&composed, &enumerator, &input_desc, Some(&mut input))
                .map_err(err("The video processor can't read the desktop"))?;
            let input = input.ok_or("The video processor can't read the desktop")?;
            let output_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            let mut pool = Vec::with_capacity(POOL);
            for _ in 0..POOL {
                // The encoder reads these: say so where the card allows it.
                let nv12 = texture(
                    device,
                    size,
                    DXGI_FORMAT_NV12,
                    &[D3D11_BIND_RENDER_TARGET | D3D11_BIND_VIDEO_ENCODER, D3D11_BIND_RENDER_TARGET],
                )?;
                let mut view = None;
                video_device
                    .CreateVideoProcessorOutputView(&nv12, &enumerator, &output_desc, Some(&mut view))
                    .map_err(err("The video processor can't write NV12"))?;
                pool.push((nv12, view.ok_or("The video processor can't write NV12")?));
            }
            Ok(Self {
                device: device.clone(),
                context: context.clone(),
                video,
                processor,
                size,
                composed,
                input,
                patch: None,
                pixels: Vec::new(),
                pool,
                next: 0,
            })
        }
    }

    /// `desk` (the desktop at `origin`) with the pointer on it, as an NV12 texture for the
    /// encoder.
    pub fn convert(
        &mut self,
        desk: &ID3D11Texture2D,
        pointer: &Pointer,
        origin: [i32; 2],
    ) -> Result<ID3D11Texture2D, String> {
        // SAFETY: copies between textures of the same size and format, all ours.
        unsafe { self.context.CopyResource(&self.composed, desk) };
        let size = [self.size[0] as usize, self.size[1] as usize];
        if let Some(area) = pointer.area(size, origin) {
            self.draw_pointer(desk, pointer, origin, area)?;
        }
        let (nv12, view) = self.pool[self.next].clone();
        self.next = (self.next + 1) % self.pool.len();
        let stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            pInputSurface: std::mem::ManuallyDrop::new(Some(self.input.clone())),
            ..Default::default()
        };
        let streams = [stream];
        // SAFETY: one stream, its views made for this processor.
        let blt = unsafe { self.video.VideoProcessorBlt(&self.processor, &view, 0, &streams) };
        let [stream] = streams;
        drop(std::mem::ManuallyDrop::into_inner(stream.pInputSurface));
        blt.map_err(|err| format!("The video processor didn't convert the picture: {}", err.message()))?;
        Ok(nv12)
    }

    /// Takes the patch under the pointer off the card, draws the pointer on it as the processor
    /// path does, and puts it back on the composed picture.
    fn draw_pointer(
        &mut self,
        desk: &ID3D11Texture2D,
        pointer: &Pointer,
        origin: [i32; 2],
        [x, y, w, h]: [usize; 4],
    ) -> Result<(), String> {
        let staging = match &self.patch {
            Some((staging, [pw, ph])) if *pw >= w && *ph >= h => staging.clone(),
            _ => {
                let made = staging(&self.device, [w.max(64), h.max(64)])?;
                self.patch = Some((made.clone(), [w.max(64), h.max(64)]));
                made
            }
        };
        let area = D3D11_BOX {
            left: x as u32,
            top: y as u32,
            front: 0,
            right: (x + w) as u32,
            bottom: (y + h) as u32,
            back: 1,
        };
        let row = w * BYTES_PER_PIXEL;
        self.pixels.resize(row * h, 0);
        // SAFETY: the box lies inside both textures; the mapped rows are read within their pitch.
        unsafe {
            self.context.CopySubresourceRegion(&staging, 0, 0, 0, 0, desk, 0, Some(&area));
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|err| format!("Couldn't read under the pointer: {}", err.message()))?;
            let pitch = mapped.RowPitch as usize;
            let base = mapped.pData.cast::<u8>().cast_const();
            if !base.is_null() && pitch >= row {
                for (line, out) in self.pixels.chunks_exact_mut(row).enumerate() {
                    out.copy_from_slice(std::slice::from_raw_parts(base.add(line * pitch), row));
                }
            }
            self.context.Unmap(&staging, 0);
        }
        pointer.draw(&mut self.pixels, [w, h], [origin[0] + x as i32, origin[1] + y as i32]);
        // SAFETY: as above; the pixels are `h` rows of `row` bytes.
        unsafe {
            self.context.UpdateSubresource(&self.composed, 0, Some(&area), self.pixels.as_ptr().cast(), row as u32, 0);
        }
        Ok(())
    }
}

/// A texture on the card, with the first of `binds` the card takes.
fn texture(
    device: &ID3D11Device,
    [width, height]: [u32; 2],
    format: DXGI_FORMAT,
    binds: &[D3D11_BIND_FLAG],
) -> Result<ID3D11Texture2D, String> {
    let mut why = String::new();
    for bind in binds {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: bind.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut made = None;
        // SAFETY: a plain creation call.
        match unsafe { device.CreateTexture2D(&desc, None, Some(&mut made)) } {
            Ok(()) => {
                if let Some(made) = made {
                    return Ok(made);
                }
            }
            Err(err) => why = err.message(),
        }
    }
    Err(format!("Couldn't make a picture on the graphics card: {why}"))
}

fn staging(device: &ID3D11Device, [width, height]: [usize; 2]) -> Result<ID3D11Texture2D, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width as u32,
        Height: height as u32,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut made = None;
    // SAFETY: a plain creation call.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut made)) }.map_err(|err| err.message())?;
    made.ok_or_else(|| "Couldn't make a picture on the graphics card".into())
}
