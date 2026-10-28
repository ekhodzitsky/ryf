//! FFI to vendored dr_wav. Compiled only with `--features bench-c`.

use std::ffi::{c_int, c_uint, c_void};

#[allow(unsafe_code)]
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

#[allow(unsafe_code)]
pub fn decode_mixed_f32(wav: &[u8]) -> Vec<f32> {
    let mut ptr: *mut f32 = std::ptr::null_mut();
    let mut frames = 0u64;
    let mut ch = 0u32;
    // SAFETY: borrowed input and initialized output parameters remain valid throughout the C call.
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
    // SAFETY: successful dr_wav decode returns frames * channels initialized floats.
    let interleaved = unsafe { std::slice::from_raw_parts(ptr, n) };
    let mixed = crate::mix_interleaved_f32(interleaved, ch as usize);
    // SAFETY: release the original C allocation once, after its last borrow.
    unsafe { ryf_drwav_free(ptr.cast()) };
    mixed
}

#[allow(unsafe_code)]
pub fn encode_s16(samples: &[i16], rate: u32) -> Vec<u8> {
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut size = 0usize;
    // SAFETY: borrowed input and initialized output parameters remain valid throughout the C call.
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
    // SAFETY: successful memory encoding returns exactly size initialized bytes.
    let out = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) }.to_vec();
    // SAFETY: release the original C allocation once, after its last borrow.
    unsafe { ryf_drwav_free(ptr) };
    out
}

#[allow(unsafe_code)]
pub fn encode_f32(samples: &[f32], rate: u32) -> Vec<u8> {
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut size = 0usize;
    // SAFETY: borrowed input and initialized output parameters remain valid throughout the C call.
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
    // SAFETY: successful memory encoding returns exactly size initialized bytes.
    let out = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) }.to_vec();
    // SAFETY: release the original C allocation once, after its last borrow.
    unsafe { ryf_drwav_free(ptr) };
    out
}
