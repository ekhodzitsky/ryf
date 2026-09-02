//! FFI to vendored dr_wav. Compiled only with `--features bench-c`.

use std::ffi::{c_int, c_uint, c_void};

unsafe extern "C" {
    fn ryf_drwav_decode_f32(
        data: *const c_void,
        data_size: usize,
        out: *mut *mut f32,
        frames: *mut u64,
        channels: *mut c_uint,
    ) -> c_int;
    fn ryf_drwav_free(p: *mut c_void);
    fn ryf_drwav_encode_s16(
        pcm: *const i16,
        frames: u64,
        channels: c_uint,
        sample_rate: c_uint,
        out: *mut *mut c_void,
        out_size: *mut usize,
    ) -> c_int;
    fn ryf_drwav_encode_f32(
        pcm: *const f32,
        frames: u64,
        channels: c_uint,
        sample_rate: c_uint,
        out: *mut *mut c_void,
        out_size: *mut usize,
    ) -> c_int;
}

pub fn decode_mixed_f32(wav: &[u8]) -> Vec<f32> {
    let mut ptr: *mut f32 = std::ptr::null_mut();
    let mut frames = 0u64;
    let mut ch = 0u32;
    let ok = unsafe {
        ryf_drwav_decode_f32(
            wav.as_ptr().cast(),
            wav.len(),
            &mut ptr,
            &mut frames,
            &mut ch,
        )
    };
    assert!(ok != 0 && !ptr.is_null(), "dr_wav decode");
    let n = (frames as usize)
        .checked_mul(ch as usize)
        .expect("dr_wav frame*ch");
    let interleaved = unsafe { std::slice::from_raw_parts(ptr, n) };
    let mixed = crate::mix_interleaved_f32(interleaved, ch as usize);
    unsafe { ryf_drwav_free(ptr.cast()) };
    mixed
}

pub fn encode_s16(samples: &[i16], rate: u32) -> Vec<u8> {
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut size = 0usize;
    let ok = unsafe {
        ryf_drwav_encode_s16(
            samples.as_ptr(),
            samples.len() as u64,
            1,
            rate,
            &mut ptr,
            &mut size,
        )
    };
    assert!(ok != 0 && !ptr.is_null(), "dr_wav encode s16");
    let out = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) }.to_vec();
    unsafe { ryf_drwav_free(ptr) };
    out
}

pub fn encode_f32(samples: &[f32], rate: u32) -> Vec<u8> {
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut size = 0usize;
    let ok = unsafe {
        ryf_drwav_encode_f32(
            samples.as_ptr(),
            samples.len() as u64,
            1,
            rate,
            &mut ptr,
            &mut size,
        )
    };
    assert!(ok != 0 && !ptr.is_null(), "dr_wav encode f32");
    let out = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) }.to_vec();
    unsafe { ryf_drwav_free(ptr) };
    out
}
