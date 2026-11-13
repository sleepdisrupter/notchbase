//! Core Audio's buffer list, read as a slice.

use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AudioBuffer {
    pub channels: u32,
    pub bytes: u32,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct AudioBufferList {
    pub count: u32,
    pub buffers: [AudioBuffer; 1],
}

impl AudioBufferList {
    pub unsafe fn buffers<'a>(list: *const Self) -> &'a [AudioBuffer] {
        if list.is_null() {
            return &[];
        }
        unsafe {
            let count = (*list).count as usize;
            std::slice::from_raw_parts(std::ptr::addr_of!((*list).buffers).cast::<AudioBuffer>(), count)
        }
    }
}

#[cfg(test)]
impl AudioBuffer {
    pub fn over(samples: &mut [f32], channels: u32) -> Self {
        Self { channels, bytes: size_of_val(samples) as u32, data: samples.as_mut_ptr().cast() }
    }
}
