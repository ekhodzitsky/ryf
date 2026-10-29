//! Headerless ITU-T G.726 ADPCM. Mono, 8000 Hz, 16/24/32/40 kbit/s.
//!
//! Raw bytes do not identify the bitrate, the packing, or a padded tail.
//! Those are arguments. Output is planar `f32` at 8000 Hz (16-bit linear
//! PCM scaled by `1/32768`, same as the rest of this crate).

mod decode;
mod tables;
mod unpack;
mod wave;

use crate::DecodedWav;
use crate::convert::I16_SCALE;
use crate::error::{FormatKind, Result, WavError};
use crate::options::DecodeOptions;

pub(crate) use wave::{fmt_fixed, fmt_tag_0064, resolve_tag};

pub use decode::G726Decoder;

/// Native G.726 PCM rate.
pub(crate) const RATE: u32 = 8_000;

/// G.726 bitrate. Each rate is a fixed codeword width at 8000 Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum G726Rate {
    /// 16 kbit/s, 2-bit codewords.
    Kbps16,
    /// 24 kbit/s, 3-bit codewords.
    Kbps24,
    /// 32 kbit/s, 4-bit codewords.
    Kbps32,
    /// 40 kbit/s, 5-bit codewords.
    Kbps40,
}

impl G726Rate {
    /// Codeword width in bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        match self {
            Self::Kbps16 => 2,
            Self::Kbps24 => 3,
            Self::Kbps32 => 4,
            Self::Kbps40 => 5,
        }
    }

    /// Bitrate in bits per second.
    #[must_use]
    pub const fn bits_per_sec(self) -> u32 {
        self.bits() as u32 * RATE
    }

    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        match bits {
            2 => Some(Self::Kbps16),
            3 => Some(Self::Kbps24),
            4 => Some(Self::Kbps32),
            5 => Some(Self::Kbps40),
            _ => None,
        }
    }

    /// Smallest byte block that ends on a codeword boundary.
    #[must_use]
    pub const fn byte_quantum(self) -> u16 {
        match self {
            Self::Kbps24 => 3,
            Self::Kbps40 => 5,
            Self::Kbps16 | Self::Kbps32 => 1,
        }
    }
}

/// How codewords sit in octets.
///
/// The payload does not say which one was used. Callers select it.
/// The two orders are bitwise opposites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum G726Packing {
    /// RFC 3551 section 4.5.4. The first codeword occupies the least
    /// significant bits of the first octet. ffmpeg calls this `g726le`
    /// (little-endian, right-justified). Sun AU uses this order.
    Rfc3551,
    /// ITU-T I.366.2 Annex E (AAL2). The first codeword occupies the most
    /// significant bits of the first octet. This is the opposite of RFC 3551.
    /// ffmpeg calls this `g726` (big-endian, left-justified). WAVE tag
    /// `0x0045` uses this order.
    Aal2,
}

/// How to read WAVE tag `0x0064`.
///
/// mmreg names that tag `WAVE_FORMAT_G726_ADPCM` (APICOM). Asterisk, SBC,
/// and Cisco write G.722 in it. This crate already decodes the 64 kbit/s
/// form as G.722. The tag does not carry a G.726 packing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Tag0064 {
    /// `nAvgBytesPerSec == 8000 * nChannels` is G.722 (output 16 kHz).
    /// Any other layout is [`crate::FormatKind::AmbiguousCodec`]. A G.726
    /// reading has to be requested with [`Self::G726`], because the tag
    /// does not record the packing.
    #[default]
    Auto,
    /// Force the Asterisk/SBC G.722 path (64 kbit/s, output 16 kHz).
    G722,
    /// Force G.726. Bitrate comes from `wBitsPerSample` when it is 2, 3, 4,
    /// or 5, otherwise from `nAvgBytesPerSec` when that is 2, 3, 4, or 5
    /// kbit/s per channel at 8000 Hz. The two fields must agree when both
    /// are present. `packing` is required.
    G726 {
        /// Codeword order. The `fmt ` chunk does not store it.
        packing: G726Packing,
    },
}

/// Complete codewords in `bytes` at `bits` per codeword.
#[inline]
pub(crate) fn pcm_frames(bytes: u64, bits: u8) -> u64 {
    if bits == 0 {
        0
    } else {
        bytes.saturating_mul(8) / u64::from(bits)
    }
}

/// `fact` / ds64 sample count wins when it is smaller than the byte estimate.
#[inline]
pub(crate) fn pcm_frames_capped(bytes: u64, bits: u8, declared: Option<u64>) -> u64 {
    let frames = pcm_frames(bytes, bits);
    match declared {
        Some(sc) if sc > 0 => frames.min(sc),
        _ => frames,
    }
}

/// Decode headerless G.726 to planar `f32` at 8000 Hz.
///
/// ```
/// # fn main() -> ryf::Result<()> {
/// let pcm = ryf::decode_g726(
///     &[0x00, 0x00],
///     ryf::G726Rate::Kbps32,
///     ryf::G726Packing::Rfc3551,
///     Some(4),
///     &ryf::DecodeOptions::speech(),
/// )?;
/// assert_eq!(pcm.sample_rate, 8_000);
/// assert_eq!(pcm.frames(), 4);
/// # Ok(())
/// # }
/// ```
///
/// `sample_count` is the number of codewords to emit.
///
/// - `None`: every complete codeword. A trailing partial codeword (unused
///   bits in the last octet that do not finish a codeword) is
///   [`WavError::Format`] [`FormatKind::InvalidSize`]. Well-formed RFC
///   payloads end on a codeword boundary, so this is the normal case.
/// - `Some(n)`: emit `n` samples and ignore the rest, including later
///   complete codewords and unused bits. `n` larger than the complete
///   codewords in `data`, or `n == 0`, is [`FormatKind::InvalidSize`].
///
/// Empty `data` is [`WavError::Empty`]. The bitrate and packing are never
/// inferred from the bytes.
pub fn decode_g726(
    data: &[u8],
    rate: G726Rate,
    packing: G726Packing,
    sample_count: Option<usize>,
    opts: &DecodeOptions,
) -> Result<DecodedWav> {
    let frames = decode_frames(data, rate, sample_count, opts)?;
    let mut plane = vec![0.0f32; frames];
    let mut dec = G726Decoder::new(rate, packing);
    let mut scratch = [0i16; 4096];
    let mut offset = 0;
    let mut consumed = 0;
    while offset < frames {
        let want = (frames - offset).min(scratch.len());
        let (n, used) = dec.push(&data[consumed..], &mut scratch[..want]);
        if n == 0 {
            return Err(WavError::format(FormatKind::InvalidSize));
        }
        for (dst, sample) in plane[offset..offset + n].iter_mut().zip(&scratch[..n]) {
            *dst = f32::from(*sample) * I16_SCALE;
        }
        offset += n;
        consumed += used;
    }
    Ok(DecodedWav {
        sample_rate: RATE,
        channels: vec![plane],
    })
}

/// Headerless G.726, every complete codeword, [`DecodeOptions::speech`].
pub fn decode_g726_mono(data: &[u8], rate: G726Rate, packing: G726Packing) -> Result<DecodedWav> {
    decode_g726(data, rate, packing, None, &DecodeOptions::speech())
}

fn decode_frames(
    data: &[u8],
    rate: G726Rate,
    sample_count: Option<usize>,
    opts: &DecodeOptions,
) -> Result<usize> {
    if data.is_empty() {
        return Err(WavError::Empty);
    }
    if RATE > opts.max_sample_rate {
        return Err(WavError::sample_rate(RATE, opts.max_sample_rate));
    }
    let bits = rate.bits();
    let bit_len = data
        .len()
        .checked_mul(8)
        .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
    let width = usize::from(bits);
    let complete = bit_len / width;
    let leftover = (bit_len % width) as u8;
    let frames = match sample_count {
        Some(0) => return Err(WavError::format(FormatKind::InvalidSize)),
        Some(n) if n > complete => return Err(WavError::format(FormatKind::InvalidSize)),
        Some(n) => n,
        None if leftover != 0 => return Err(WavError::format(FormatKind::InvalidSize)),
        None => complete,
    };
    let max_samples = opts.max_frames(RATE);
    crate::pull::check_duration(frames, max_samples, RATE)?;
    let out_bytes = (frames as u64).saturating_mul(4);
    if out_bytes > opts.max_output_bytes {
        return Err(WavError::output_too_large(out_bytes, opts.max_output_bytes));
    }
    crate::error::check_vec_capacity::<f32>(frames)?;
    Ok(frames)
}

#[cfg(test)]
pub(crate) fn decode_i16(
    data: &[u8],
    rate: G726Rate,
    packing: G726Packing,
    sample_count: Option<usize>,
    opts: &DecodeOptions,
) -> Result<Vec<i16>> {
    let frames = decode_frames(data, rate, sample_count, opts)?;
    let mut dec = G726Decoder::new(rate, packing);
    let mut pcm = vec![0; frames];
    let (got, _) = dec.push(data, &mut pcm);
    if got != frames {
        return Err(WavError::format(FormatKind::InvalidSize));
    }
    Ok(pcm)
}

#[cfg(test)]
#[path = "../g726_tests.rs"]
mod g726_tests;
