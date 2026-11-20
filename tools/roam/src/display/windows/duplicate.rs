//! What's on a display, from DXGI desktop duplication, as the recorder takes it
//! (src/record/windows/dxgi.rs): the whole output copied to a staging texture for the CPU, with
//! the pointer drawn on in software, since duplicated frames leave it out.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HMODULE, LUID};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
    DXGI_OUTDUPL_POINTER_SHAPE_INFO, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR,
    DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, IDXGIAdapter1,
    IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
};
use windows::core::Interface;

use crate::display::cursor::{Kind, Pointer, Shape};
use crate::display::{PointerAt, PointerShape};

const BYTES_PER_PIXEL: usize = 4;
const RECONNECT_EVERY: Duration = Duration::from_millis(250);

pub struct Duplicator {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    output: IDXGIOutput1,
    duplication: Option<IDXGIOutputDuplication>,
    reconnect_at: Instant,
    staging: Option<ID3D11Texture2D>,
    size: [usize; 2],
    origin: [i32; 2],
    clean: Vec<u8>,
    composed: Vec<u8>,
    fresh: bool,
    /// Something changed since the last `frame`.
    pub changed: bool,
    pointer: Pointer,
    shape: Vec<u8>,
    /// The cursor's spot in the pointer image.
    hotspot: [i32; 2],
    /// Whether the pointer goes apart from the pictures, which then leave it out, and what's
    /// new about it since the last `take_pointer`.
    apart: bool,
    moved: bool,
    reshaped: bool,
    /// A pointer never drawn, for the pictures when it goes apart.
    none: Pointer,
    /// Whether the desktop stays on the graphics card, in `desk`, instead of coming off it.
    on_card: bool,
    desk: Option<ID3D11Texture2D>,
    luid: LUID,
}

impl Duplicator {
    /// Duplicates the output Windows calls `name` (`\\.\DISPLAY3`).
    pub fn open(name: &[u16; 32]) -> Result<Self, String> {
        let (adapter, output) = find_output(name)?;
        let desc = unsafe { output.GetDesc() }.map_err(|err| err.message())?;
        let r = desc.DesktopCoordinates;
        let size = [(r.right - r.left).max(2) as usize, (r.bottom - r.top).max(2) as usize];
        let (device, context) = create_device(&adapter)?;
        let luid = unsafe { adapter.GetDesc1() }.map(|desc| desc.AdapterLuid).unwrap_or_default();
        let duplication = unsafe { output.DuplicateOutput(&device) }.map_err(|err| err.message())?;
        Ok(Self {
            device,
            context,
            output,
            duplication: Some(duplication),
            reconnect_at: Instant::now(),
            staging: None,
            size,
            origin: [r.left, r.top],
            clean: vec![0; size[0] * size[1] * BYTES_PER_PIXEL],
            composed: vec![0; size[0] * size[1] * BYTES_PER_PIXEL],
            fresh: false,
            changed: true,
            pointer: Pointer::default(),
            shape: Vec::new(),
            hotspot: [0, 0],
            apart: false,
            moved: false,
            reshaped: false,
            none: Pointer::default(),
            on_card: false,
            desk: None,
            luid,
        })
    }

    pub fn size(&self) -> [usize; 2] {
        self.size
    }

    /// Sends the pointer apart from the pictures from now on: they leave it out, and a move
    /// alone is no change to encode.
    pub fn pointer_apart(&mut self) {
        self.apart = true;
        self.moved = true;
        self.reshaped = self.pointer.shape.is_some();
        self.changed = true;
    }

    /// What's new about the pointer since the last call, when it goes apart: where it is on
    /// the display, and its shape when that changed.
    pub fn take_pointer(&mut self) -> (Option<PointerAt>, Option<PointerShape>) {
        if !self.apart {
            return (None, None);
        }
        let shape = std::mem::take(&mut self.reshaped)
            .then(|| self.pointer.shape.as_ref().map(Shape::rgba))
            .flatten()
            .map(|(size, rgba)| PointerShape { size, rgba })
            .filter(PointerShape::sound);
        let at = std::mem::take(&mut self.moved).then(|| PointerAt {
            at: [self.pointer.at[0] - self.origin[0], self.pointer.at[1] - self.origin[1]],
            visible: self.pointer.visible && self.pointer.shape.is_some(),
        });
        (at, shape)
    }

    /// What the engine needs to place the pointer image from its own mouse hook: the cursor's
    /// spot in the image, and whether it shows. `None` until the shape is known.
    pub fn cursor(&self) -> Option<([i32; 2], bool)> {
        self.pointer.shape.as_ref()?;
        Some((self.hotspot, self.pointer.visible))
    }

    /// Takes updates until something changes or `deadline` comes: a change goes to the encoder
    /// the moment Windows has it, not at the next tick.
    pub fn wait(&mut self, deadline: Instant) -> Result<(), String> {
        loop {
            let now = Instant::now();
            let Some(duplication) = self.connected(now) else {
                std::thread::sleep(deadline.saturating_duration_since(now).min(Duration::from_millis(50)));
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
                    if self.changed || self.moved {
                        return Ok(());
                    }
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

    pub fn device(&self) -> (&ID3D11Device, &ID3D11DeviceContext) {
        (&self.device, &self.context)
    }

    /// The graphics card the display is on.
    pub fn luid(&self) -> LUID {
        self.luid
    }

    /// Keeps the desktop on the graphics card from now on, or brings it off again.
    pub fn keep_on_card(&mut self, on_card: bool) {
        if self.on_card == on_card {
            return;
        }
        self.on_card = on_card;
        if !on_card && let Some(desk) = self.desk.clone() {
            // The picture on the card is the latest: the copy for the processor catches up.
            if let Ok(staging) = self.staging() {
                unsafe { self.context.CopyResource(&staging, &desk) };
                self.fresh = true;
            }
        }
        self.changed = true;
    }

    /// The display as it is now on the graphics card, before the pointer goes on, with the
    /// pointer and where the display is. Nothing until it has a first picture.
    pub fn on_card(&mut self) -> Option<(&ID3D11Texture2D, &Pointer, [i32; 2])> {
        self.changed = false;
        let pointer = if self.apart { &self.none } else { &self.pointer };
        Some((self.desk.as_ref()?, pointer, self.origin))
    }

    fn staging(&mut self) -> Result<ID3D11Texture2D, String> {
        if let Some(staging) = &self.staging {
            return Ok(staging.clone());
        }
        let made = texture(&self.device, self.size, true)?;
        self.staging = Some(made.clone());
        Ok(made)
    }

    fn desk(&mut self) -> Result<ID3D11Texture2D, String> {
        if let Some(desk) = &self.desk {
            return Ok(desk.clone());
        }
        let made = texture(&self.device, self.size, false)?;
        self.desk = Some(made.clone());
        Ok(made)
    }

    /// The display as it is now, pointer and all, in BGRA.
    pub fn frame(&mut self) -> &[u8] {
        if std::mem::take(&mut self.fresh) {
            self.read_staging();
        }
        if std::mem::take(&mut self.changed) {
            self.composed.copy_from_slice(&self.clean);
            if !self.apart {
                self.pointer.draw(&mut self.composed, self.size, self.origin);
            }
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
        if info.LastPresentTime != 0
            && let Some(resource) = resource
        {
            let texture: ID3D11Texture2D = resource.cast().map_err(|err| err.message())?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe { texture.GetDesc(&mut desc) };
            if desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM
                && desc.Width as usize >= self.size[0]
                && desc.Height as usize >= self.size[1]
            {
                if self.on_card {
                    let desk = self.desk()?;
                    unsafe { self.context.CopyResource(&desk, &texture) };
                } else {
                    let staging = self.staging()?;
                    unsafe { self.context.CopyResource(&staging, &texture) };
                    self.fresh = true;
                }
                self.changed = true;
            }
        }
        if info.LastMouseUpdateTime != 0 {
            let position = info.PointerPosition;
            self.pointer.visible = position.Visible.as_bool();
            // Relative to the output; `draw` takes away the output's origin.
            self.pointer.at = [position.Position.x + self.origin[0], position.Position.y + self.origin[1]];
            self.pointer_changed();
        }
        if info.PointerShapeBufferSize > 0 {
            self.read_shape(duplication, info.PointerShapeBufferSize);
            self.reshaped = true;
            self.pointer_changed();
        }
        Ok(())
    }

    /// The pointer moved or changed: news to send when it goes apart, else a change to the
    /// picture.
    fn pointer_changed(&mut self) {
        if self.apart {
            self.moved = true;
        } else {
            self.changed = true;
        }
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
        self.hotspot = [info.HotSpot.x, info.HotSpot.y];
        self.pointer.shape = Some(Shape {
            kind,
            width: info.Width as usize,
            height: info.Height as usize,
            pitch: info.Pitch as usize,
            pixels: self.shape[..needed.min(bytes) as usize].to_vec(),
        });
    }

    fn read_staging(&mut self) {
        let Some(staging) = &self.staging else { return };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        if unsafe { self.context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }.is_err() {
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
        unsafe { self.context.Unmap(staging, 0) };
    }
}

fn find_output(name: &[u16; 32]) -> Result<(IDXGIAdapter1, IDXGIOutput1), String> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(|err| err.message())?;
    for a in 0u32.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(a) }) else { break };
        for o in 0u32.. {
            let Ok(output) = (unsafe { adapter.EnumOutputs(o) }) else { break };
            let Ok(desc) = (unsafe { output.GetDesc() }) else { continue };
            if desc.DeviceName == *name && desc.AttachedToDesktop.as_bool() {
                let output = output.cast::<IDXGIOutput1>().map_err(|err| err.message())?;
                return Ok((adapter, output));
            }
        }
    }
    Err("Windows hasn't put the virtual display on a graphics card yet".into())
}

fn create_device(adapter: &IDXGIAdapter1) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    // Video support lets the card's video processor and encoder share the device; a card
    // without it still duplicates.
    let mut why = String::new();
    for flags in
        [D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_CREATE_DEVICE_BGRA_SUPPORT]
    {
        let (mut device, mut context) = (None, None);
        let made = unsafe {
            D3D11CreateDevice(
                adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                flags,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        };
        match (made, device, context) {
            (Ok(()), Some(device), Some(context)) => {
                // The encoder works the device from its own threads.
                if let Ok(multithread) = context.cast::<ID3D11Multithread>() {
                    let _ = unsafe { multithread.SetMultithreadProtected(true) };
                }
                return Ok((device, context));
            }
            (Err(err), ..) => why = err.message(),
            _ => why = "no Direct3D device".into(),
        }
    }
    Err(why)
}

/// A BGRA texture the size of the display: for reading off the card when `staging`, else for
/// the desktop to wait on it.
fn texture(device: &ID3D11Device, [width, height]: [usize; 2], staging: bool) -> Result<ID3D11Texture2D, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width as u32,
        Height: height as u32,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: if staging { D3D11_USAGE_STAGING } else { D3D11_USAGE_DEFAULT },
        BindFlags: 0,
        CPUAccessFlags: if staging { D3D11_CPU_ACCESS_READ.0 as u32 } else { 0 },
        MiscFlags: 0,
    };
    let mut texture = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }.map_err(|err| err.message())?;
    texture.ok_or_else(|| "no staging texture".into())
}
