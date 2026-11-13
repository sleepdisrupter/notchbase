//! GPU use summed over every 3D engine from PDH, the GPU's name from DXGI, and the system drive's free space.

use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIAdapter1, IDXGIFactory1};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Performance::{
    PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER,
    PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW,
};
use windows::core::{PCWSTR, w};

use super::Disk;

const GPU_3D_UTILIZATION: PCWSTR = w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage");
const PDH_OK: u32 = 0;
const ITEM_SIZE: usize = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
/// Engines can come and go between sizing the buffer and reading it, so try twice.
const READ_ATTEMPTS: usize = 2;
const FALLBACK_DRIVE: &str = "C:";

struct Query(PDH_HQUERY);

impl Drop for Query {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.0) };
    }
}

pub(super) struct Gpu {
    query: Query,
    counter: PDH_HCOUNTER,
    items: Vec<PDH_FMT_COUNTERVALUE_ITEM_W>,
}

impl Gpu {
    pub(super) fn open() -> Option<Self> {
        let mut handle = PDH_HQUERY::default();
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut handle) } != PDH_OK {
            return None;
        }
        let query = Query(handle);
        let mut counter = PDH_HCOUNTER::default();
        if unsafe { PdhAddEnglishCounterW(query.0, GPU_3D_UTILIZATION, 0, &mut counter) } != PDH_OK {
            return None;
        }
        // PDH rates need two collections, so take the first one now.
        unsafe { PdhCollectQueryData(query.0) };
        Some(Self { query, counter, items: Vec::new() })
    }

    pub(super) fn sample(&mut self) -> Option<f32> {
        if unsafe { PdhCollectQueryData(self.query.0) } != PDH_OK {
            return None;
        }
        let busy: f64 = self
            .engines()?
            .iter()
            .filter(|item| matches!(item.FmtValue.CStatus, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA))
            .map(|item| unsafe { item.FmtValue.Anonymous.doubleValue })
            .filter(|percent| percent.is_finite() && *percent > 0.0)
            .sum();
        Some((busy / 100.0).clamp(0.0, 1.0) as f32)
    }

    fn engines(&mut self) -> Option<&[PDH_FMT_COUNTERVALUE_ITEM_W]> {
        for _ in 0..READ_ATTEMPTS {
            let (mut bytes, mut count) = (0u32, 0u32);
            match unsafe { PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, None) } {
                // Success without a buffer means there are no engines to read.
                PDH_OK => return Some(&[]),
                PDH_MORE_DATA => {}
                _ => return None,
            }
            let len = (bytes as usize).div_ceil(ITEM_SIZE).max(1);
            self.items.clear();
            self.items.resize(len, PDH_FMT_COUNTERVALUE_ITEM_W::default());
            let mut bytes = (len * ITEM_SIZE) as u32;
            let buffer = Some(self.items.as_mut_ptr());
            match unsafe { PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, buffer) }
            {
                PDH_OK => return Some(&self.items[..(count as usize).min(len)]),
                PDH_MORE_DATA => {}
                _ => return None,
            }
        }
        None
    }
}

pub(super) fn gpu_name() -> Option<String> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    (0..)
        .map_while(|index| unsafe { factory.EnumAdapters1(index) }.ok())
        .find_map(|adapter| hardware_adapter_name(&adapter))
}

fn hardware_adapter_name(adapter: &IDXGIAdapter1) -> Option<String> {
    let desc = unsafe { adapter.GetDesc1() }.ok()?;
    if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
        return None;
    }
    let name = wide_until_nul(&desc.Description);
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn wide_until_nul(wide: &[u16]) -> String {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

pub(super) fn system_disk() -> Option<Disk> {
    let root: Vec<u16> = format!("{}\\", system_drive()).encode_utf16().chain(Some(0)).collect();
    let (mut total, mut free) = (0u64, 0u64);
    unsafe { GetDiskFreeSpaceExW(PCWSTR(root.as_ptr()), None, Some(&mut total), Some(&mut free)) }.ok()?;
    (total > 0).then_some(Disk { free: free.min(total), total })
}

fn system_drive() -> String {
    std::env::var("SystemDrive")
        .ok()
        .map(|drive| drive.trim().trim_end_matches('\\').to_owned())
        .filter(|drive| !drive.is_empty())
        .unwrap_or_else(|| FALLBACK_DRIVE.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_strings_stop_at_the_first_nul() {
        let wide: Vec<u16> = "GPU\0junk".encode_utf16().collect();
        assert_eq!(wide_until_nul(&wide), "GPU");
        assert_eq!(wide_until_nul(&[0x41, 0x42]), "AB");
    }

    #[test]
    fn gpu_counter_reports_a_fraction() {
        let Some(mut gpu) = Gpu::open() else { return };
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(gpu.sample().is_none_or(|busy| (0.0..=1.0).contains(&busy)));
        assert!(gpu_name().is_none_or(|name| !name.is_empty() && name == name.trim()));
    }
}
