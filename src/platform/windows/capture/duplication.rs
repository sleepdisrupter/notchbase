//! Grabs a monitor with DXGI Desktop Duplication and reads it back through a staging texture. No rotated displays.

use windows::Win32::Foundation::{HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput,
    IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
use windows::core::Interface;

use super::{Area, BYTES_PER_PIXEL};

const FRAME_WAIT_MS: u32 = 120;
const ATTEMPTS: usize = 4;

struct Frame<'a> {
    duplication: &'a IDXGIOutputDuplication,
    resource: IDXGIResource,
}

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        let _ = unsafe { self.duplication.ReleaseFrame() };
    }
}

pub(super) fn grab_bgra(area: Area) -> Result<Vec<u8>, String> {
    let (adapter, output) = find_output(area)?;
    let (device, context) = create_device(&adapter)?;
    let output: IDXGIOutput1 = output.cast().map_err(|err| err.message())?;
    let duplication = unsafe { output.DuplicateOutput(&device) }.map_err(|err| err.message())?;
    let frame = acquire(&duplication)?;
    let texture: ID3D11Texture2D = frame.resource.cast().map_err(|err| err.message())?;
    let staging = copy_to_staging(&device, &context, &texture, area)?;
    drop(frame);
    read_rows(&context, &staging, area)
}

fn find_output(area: Area) -> Result<(IDXGIAdapter1, IDXGIOutput), String> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(|err| err.message())?;
    for a in 0u32.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(a) }) else { break };
        for o in 0u32.. {
            let Ok(output) = (unsafe { adapter.EnumOutputs(o) }) else { break };
            let Ok(desc) = (unsafe { output.GetDesc() }) else { continue };
            if !desc.AttachedToDesktop.as_bool() || !matches(desc.DesktopCoordinates, area) {
                continue;
            }
            if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY && desc.Rotation != DXGI_MODE_ROTATION_UNSPECIFIED {
                return Err("the display is rotated".into());
            }
            return Ok((adapter, output));
        }
    }
    Err("no display output matches the monitor".into())
}

fn matches(rect: RECT, area: Area) -> bool {
    rect.left == area.x
        && rect.top == area.y
        && rect.right - rect.left == area.width
        && rect.bottom - rect.top == area.height
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

fn acquire(duplication: &IDXGIOutputDuplication) -> Result<Frame<'_>, String> {
    for _ in 0..ATTEMPTS {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        match unsafe { duplication.AcquireNextFrame(FRAME_WAIT_MS, &mut info, &mut resource) } {
            Ok(()) => match resource {
                Some(resource) if info.LastPresentTime != 0 => return Ok(Frame { duplication, resource }),
                Some(resource) => drop(Frame { duplication, resource }),
                None => {
                    let _ = unsafe { duplication.ReleaseFrame() };
                }
            },
            Err(err) if err.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
            Err(err) => return Err(err.message()),
        }
    }
    Err("no desktop frame arrived".into())
}

fn copy_to_staging(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
    area: Area,
) -> Result<ID3D11Texture2D, String> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut desc) };
    if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
        return Err(format!("unexpected desktop format {}", desc.Format.0));
    }
    if (desc.Width, desc.Height) != (area.width as u32, area.height as u32) {
        return Err("the desktop image doesn't match the monitor size".into());
    }
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: desc.Width,
        Height: desc.Height,
        MipLevels: 1,
        ArraySize: 1,
        Format: desc.Format,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut staging = None;
    unsafe { device.CreateTexture2D(&staging_desc, None, Some(&mut staging)) }.map_err(|err| err.message())?;
    let staging = staging.ok_or("no staging texture")?;
    unsafe { context.CopyResource(&staging, texture) };
    Ok(staging)
}

fn read_rows(context: &ID3D11DeviceContext, staging: &ID3D11Texture2D, area: Area) -> Result<Vec<u8>, String> {
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }.map_err(|err| err.message())?;
    let row_bytes = area.width as usize * BYTES_PER_PIXEL;
    let pitch = mapped.RowPitch as usize;
    let mut pixels = Vec::with_capacity(area.bytes());
    if !mapped.pData.is_null() && pitch >= row_bytes {
        let base = mapped.pData.cast::<u8>().cast_const();
        for row in 0..area.height as usize {
            let line = unsafe { std::slice::from_raw_parts(base.add(row * pitch), row_bytes) };
            pixels.extend_from_slice(line);
        }
    }
    unsafe { context.Unmap(staging, 0) };
    if pixels.len() == area.bytes() { Ok(pixels) } else { Err("the desktop image couldn't be read".into()) }
}
