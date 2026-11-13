//! Frames from DXGI desktop duplication. Only the recorded area is copied into a small staging texture for the CPU,
//! and the pointer is drawn on in software.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
    DXGI_OUTDUPL_POINTER_SHAPE_INFO, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR,
    DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, IDXGIAdapter1,
    IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
use windows::core::Interface;

use super::cursor::{Kind, Pointer, Shape};
use crate::record::Region;

const BYTES_PER_PIXEL: usize = 4;
/// While the duplication is lost, how often we try to get it back. The last frame repeats meanwhile.
const RECONNECT_EVERY: Duration = Duration::from_millis(250);

pub(super) struct Duplicator {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    output: IDXGIOutput1,
    duplication: Option<IDXGIOutputDuplication>,
    reconnect_at: Instant,
    staging: ID3D11Texture2D,
    crop: D3D11_BOX,
    offset: [i32; 2],
    size: [usize; 2],
    /// The area as captured. The pointer is drawn on a copy, `composed`.
    clean: Vec<u8>,
    composed: Vec<u8>,
    /// A new image is waiting in the staging texture.
    fresh: bool,
    /// The composed frame needs drawing again.
    stale: bool,
    pointer: Pointer,
    shape: Vec<u8>,
}

impl Duplicator {
    pub(super) fn open(region: &Region) -> Result<Self, String> {
        let (adapter, output) = find_output(region)?;
        let (device, context) = create_device(&adapter)?;
        let duplication = unsafe { output.DuplicateOutput(&device) }.map_err(|err| err.message())?;
        let size = region.size.map(|side| side as usize);
        let staging = staging(&device, region.size)?;
        let [left, top] = region.origin;
        let crop =
            D3D11_BOX { left, top, front: 0, right: left + region.size[0], bottom: top + region.size[1], back: 1 };
        Ok(Self {
            device,
            context,
            output,
            duplication: Some(duplication),
            reconnect_at: Instant::now(),
            staging,
            crop,
            offset: region.origin.map(|side| side as i32),
            size,
            clean: vec![0; size[0] * size[1] * BYTES_PER_PIXEL],
            composed: vec![0; size[0] * size[1] * BYTES_PER_PIXEL],
            fresh: false,
            stale: true,
            pointer: Pointer::default(),
            shape: Vec::new(),
        })
    }

    /// Takes desktop updates as they come until the deadline. While the duplication is lost it just sleeps.
    pub(super) fn wait(&mut self, deadline: Instant) -> Result<(), String> {
        loop {
            let now = Instant::now();
            let Some(duplication) = self.connected(now) else {
                std::thread::sleep(deadline.saturating_duration_since(now));
                return Ok(());
            };
            let timeout = deadline.saturating_duration_since(now).as_millis() as u32;
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;
            match unsafe { duplication.AcquireNextFrame(timeout, &mut info, &mut resource) } {
                Ok(()) => {
                    let taken = self.take(&duplication, &info, resource);
                    let _ = unsafe { duplication.ReleaseFrame() };
                    taken?;
                }
                Err(err) if err.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(()),
                Err(err) if err.code() == DXGI_ERROR_ACCESS_LOST => self.duplication = None,
                Err(err) => return Err(err.message()),
            }
            if Instant::now() >= deadline {
                return Ok(());
            }
        }
    }

    pub(super) fn frame(&mut self) -> &[u8] {
        if std::mem::take(&mut self.fresh) {
            self.read_staging();
        }
        if std::mem::take(&mut self.stale) {
            self.composed.copy_from_slice(&self.clean);
            self.pointer.draw(&mut self.composed, self.size, self.offset);
        }
        &self.composed
    }

    fn connected(&mut self, now: Instant) -> Option<IDXGIOutputDuplication> {
        if self.duplication.is_none() && now >= self.reconnect_at {
            self.reconnect_at = now + RECONNECT_EVERY;
            self.duplication = unsafe { self.output.DuplicateOutput(&self.device) }.ok();
        }
        self.duplication.clone()
    }

    fn take(
        &mut self,
        duplication: &IDXGIOutputDuplication,
        info: &DXGI_OUTDUPL_FRAME_INFO,
        resource: Option<IDXGIResource>,
    ) -> Result<(), String> {
        // A zero present time means the desktop image didn't change, so there's nothing new to copy.
        if info.LastPresentTime != 0
            && let Some(resource) = resource
        {
            let texture: ID3D11Texture2D = resource.cast().map_err(|err| err.message())?;
            if self.fits(&texture) {
                unsafe { self.context.CopySubresourceRegion(&self.staging, 0, 0, 0, 0, &texture, 0, Some(&self.crop)) };
                self.fresh = true;
                self.stale = true;
            }
        }
        if info.LastMouseUpdateTime != 0 {
            let position = info.PointerPosition;
            self.pointer.visible = position.Visible.as_bool();
            self.pointer.at = [position.Position.x, position.Position.y];
            self.stale = true;
        }
        if info.PointerShapeBufferSize > 0 {
            self.read_shape(duplication, info.PointerShapeBufferSize);
            self.stale = true;
        }
        Ok(())
    }

    fn fits(&self, texture: &ID3D11Texture2D) -> bool {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM && desc.Width >= self.crop.right && desc.Height >= self.crop.bottom
    }

    fn read_shape(&mut self, duplication: &IDXGIOutputDuplication, bytes: u32) {
        self.shape.resize(bytes as usize, 0);
        let mut needed = 0;
        let mut info = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
        let read =
            unsafe { duplication.GetFramePointerShape(bytes, self.shape.as_mut_ptr().cast(), &mut needed, &mut info) };
        if read.is_err() {
            return;
        }
        let kind = match info.Type as i32 {
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 => Kind::Monochrome,
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 => Kind::Color,
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 => Kind::MaskedColor,
            _ => return,
        };
        self.pointer.shape = Some(Shape {
            kind,
            width: info.Width as usize,
            height: info.Height as usize,
            pitch: info.Pitch as usize,
            pixels: self.shape[..needed.min(bytes) as usize].to_vec(),
        });
    }

    fn read_staging(&mut self) {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        if unsafe { self.context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }.is_err() {
            return;
        }
        let row = self.size[0] * BYTES_PER_PIXEL;
        let pitch = mapped.RowPitch as usize;
        if !mapped.pData.is_null() && pitch >= row {
            let base = mapped.pData.cast::<u8>().cast_const();
            for (y, line) in self.clean.chunks_exact_mut(row).enumerate() {
                line.copy_from_slice(unsafe { std::slice::from_raw_parts(base.add(y * pitch), row) });
            }
        }
        unsafe { self.context.Unmap(&self.staging, 0) };
    }
}

fn find_output(region: &Region) -> Result<(IDXGIAdapter1, IDXGIOutput1), String> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(|err| err.message())?;
    for a in 0u32.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(a) }) else { break };
        for o in 0u32.. {
            let Ok(output) = (unsafe { adapter.EnumOutputs(o) }) else { break };
            let Ok(desc) = (unsafe { output.GetDesc() }) else { continue };
            if !desc.AttachedToDesktop.as_bool() || !covers(desc.DesktopCoordinates, region) {
                continue;
            }
            if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY && desc.Rotation != DXGI_MODE_ROTATION_UNSPECIFIED {
                return Err("the display is rotated".into());
            }
            let output = output.cast::<IDXGIOutput1>().map_err(|err| err.message())?;
            return Ok((adapter, output));
        }
    }
    Err("no display output matches the monitor".into())
}

fn covers(rect: RECT, region: &Region) -> bool {
    let display = region.display;
    [rect.left, rect.top] == display.origin
        && [(rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32] == display.size
}

fn create_device(adapter: &IDXGIAdapter1) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    let (mut device, mut context) = (None, None);
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(|err| err.message())?;
    device.zip(context).ok_or_else(|| "no Direct3D device".into())
}

fn staging(device: &ID3D11Device, [width, height]: [u32; 2]) -> Result<ID3D11Texture2D, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut texture = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }.map_err(|err| err.message())?;
    texture.ok_or_else(|| "no staging texture".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Display;

    fn region(origin: [i32; 2], size: [u32; 2]) -> Region {
        Region::new(Display { id: 0, origin, size, scale: 1.0 }, [0, 0], size)
    }

    #[test]
    fn output_covers_exact_place_and_size() {
        let rect = RECT { left: -1920, top: 0, right: 0, bottom: 1080 };
        assert!(covers(rect, &region([-1920, 0], [1920, 1080])));
        assert!(!covers(rect, &region([0, 0], [1920, 1080])), "another display");
        assert!(!covers(rect, &region([-1920, 0], [2560, 1440])), "another size");
    }
}
