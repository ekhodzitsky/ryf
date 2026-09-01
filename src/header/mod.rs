//! WAVE header demux: containers, `fmt ` / `data` walk, Wave64.

use crate::error::{Result, WavError};

#[cfg(feature = "adpcm")]
use crate::adpcm::{ImaAdpcmParams, MsAdpcmParams};

#[cfg(not(feature = "adpcm"))]
pub(crate) mod adpcm_types {
    #![allow(dead_code)]
    #[derive(Debug, Clone)]
    pub struct MsAdpcmParams {
        pub block_align: u16,
        pub samples_per_block: u16,
        pub channels: usize,
        pub coefs: Vec<(i16, i16)>,
    }
    #[derive(Debug, Clone, Copy)]
    pub struct ImaAdpcmParams {
        pub block_align: u16,
        pub samples_per_block: u16,
        pub channels: usize,
    }
}
#[cfg(not(feature = "adpcm"))]
use adpcm_types::{ImaAdpcmParams, MsAdpcmParams};

pub(crate) enum Container {
    /// Classic little-endian RIFF/WAVE.
    Riff,
    /// Big-endian RIFX/WAVE (PCM multi-byte samples are BE).
    Rifx,
    /// RF64/BW64 with `ds64` (LE).
    Rf64,
}

impl Container {
    pub(crate) fn big_endian(self) -> bool {
        matches!(self, Container::Rifx)
    }
}

// Sony Wave64 GUIDs (on-disk byte order). Public domain / Microsoft SDK.
#[rustfmt::skip]
pub(crate) const W64_GUID_RIFF: [u8; 16] = [
    0x72, 0x69, 0x66, 0x66, 0x2E, 0x91, 0xCF, 0x11,
    0xA5, 0xD6, 0x28, 0xDB, 0x04, 0xC1, 0x00, 0x00,
];
#[rustfmt::skip]
pub(crate) const W64_GUID_WAVE: [u8; 16] = [
    0x77, 0x61, 0x76, 0x65, 0xF3, 0xAC, 0xD3, 0x11,
    0x8C, 0xD1, 0x00, 0xC0, 0x4F, 0x8E, 0xDB, 0x8A,
];
#[rustfmt::skip]
pub(crate) const W64_GUID_FMT: [u8; 16] = [
    0x66, 0x6D, 0x74, 0x20, 0xF3, 0xAC, 0xD3, 0x11,
    0x8C, 0xD1, 0x00, 0xC0, 0x4F, 0x8E, 0xDB, 0x8A,
];
#[rustfmt::skip]
pub(crate) const W64_GUID_DATA: [u8; 16] = [
    0x64, 0x61, 0x74, 0x61, 0xF3, 0xAC, 0xD3, 0x11,
    0x8C, 0xD1, 0x00, 0xC0, 0x4F, 0x8E, 0xDB, 0x8A,
];
#[rustfmt::skip]
pub(crate) const W64_GUID_FACT: [u8; 16] = [
    0x66, 0x61, 0x63, 0x74, 0xF3, 0xAC, 0xD3, 0x11,
    0x8C, 0xD1, 0x00, 0xC0, 0x4F, 0x8E, 0xDB, 0x8A,
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeCodec {
    PcmU8,
    PcmS16,
    PcmS24,
    PcmS32,
    Float32,
    Float64,
    ALaw,
    MuLaw,
    MsAdpcm,
    ImaAdpcm,
    Unsupported,
}

/// Sample-level codec selected from the `fmt ` chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleCodec {
    U8,
    S16,
    /// Packed 24-bit (3-byte containers).
    S24,
    /// 24-bit PCM in 4-byte LE containers (S24_LE / WAVEFORMATEXTENSIBLE).
    S24_4,
    S32,
    F32,
    F64,
    ALaw,
    MuLaw,
    MsAdpcm,
    ImaAdpcm,
    Unsupported,
}

impl SampleCodec {
    pub(crate) fn probe(self) -> ProbeCodec {
        match self {
            SampleCodec::U8 => ProbeCodec::PcmU8,
            SampleCodec::S16 => ProbeCodec::PcmS16,
            SampleCodec::S24 | SampleCodec::S24_4 => ProbeCodec::PcmS24,
            SampleCodec::S32 => ProbeCodec::PcmS32,
            SampleCodec::F32 => ProbeCodec::Float32,
            SampleCodec::F64 => ProbeCodec::Float64,
            SampleCodec::ALaw => ProbeCodec::ALaw,
            SampleCodec::MuLaw => ProbeCodec::MuLaw,
            SampleCodec::MsAdpcm => ProbeCodec::MsAdpcm,
            SampleCodec::ImaAdpcm => ProbeCodec::ImaAdpcm,
            SampleCodec::Unsupported => ProbeCodec::Unsupported,
        }
    }

    pub(crate) fn is_adpcm(self) -> bool {
        matches!(self, SampleCodec::MsAdpcm | SampleCodec::ImaAdpcm)
    }
}

/// Parsed `fmt ` chunk fields needed for decoding.
pub(crate) struct FmtFields {
    pub(crate) codec: SampleCodec,
    pub(crate) channels: usize,
    pub(crate) sample_rate: u32,
    /// Bytes per container sample of a single channel (PCM path).
    pub(crate) sample_width: usize,
    /// ADPCM block size and layout (ignored for PCM).
    pub(crate) adpcm_ms: Option<MsAdpcmParams>,
    pub(crate) adpcm_ima: Option<ImaAdpcmParams>,
    /// Sample multi-byte fields are big-endian (RIFX).
    pub(crate) big_endian: bool,
}

/// Parsed stream header up to the start of the `data` chunk.
pub(crate) struct WavHeader {
    pub(crate) fmt: FmtFields,
    /// Declared `data` chunk length in bytes; `None` for unknown/streaming.
    pub(crate) declared_data_len: Option<u64>,
    /// Absolute stream position of the first data byte.
    pub(crate) data_pos: u64,
    /// Optional sample count from `fact` / `ds64` (ADPCM / RF64).
    pub(crate) declared_sample_count: Option<u64>,
}

/// WAVE format tags (mmreg.h).
pub(crate) const WAVE_FORMAT_PCM: u16 = 0x0001;
pub(crate) const WAVE_FORMAT_ADPCM_MS: u16 = 0x0002;
pub(crate) const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
pub(crate) const WAVE_FORMAT_ALAW: u16 = 0x0006;
pub(crate) const WAVE_FORMAT_MULAW: u16 = 0x0007;
pub(crate) const WAVE_FORMAT_ADPCM_IMA: u16 = 0x0011;
pub(crate) const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// SubFormat GUIDs for WAVE_FORMAT_EXTENSIBLE (ksmedia.h), in on-disk byte
/// order. The µ-law subtype tag is 0x04 in the GUID form even though the
/// plain format tag is 0x0007 — kept identical to the historical demuxer.
#[rustfmt::skip]
pub(crate) const KSDATAFORMAT_SUBTYPE_PCM: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];
#[rustfmt::skip]
pub(crate) const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: [u8; 16] = [
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];
#[rustfmt::skip]
pub(crate) const KSDATAFORMAT_SUBTYPE_ALAW: [u8; 16] = [
    0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];
#[rustfmt::skip]
pub(crate) const KSDATAFORMAT_SUBTYPE_MULAW: [u8; 16] = [
    0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];
#[rustfmt::skip]
pub(crate) const KSDATAFORMAT_SUBTYPE_AMBISONIC_PCM: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x21, 0x07, 0xd3, 0x11,
    0x86, 0x44, 0xc8, 0xc1, 0xca, 0x00, 0x00, 0x00,
];
#[rustfmt::skip]
pub(crate) const KSDATAFORMAT_SUBTYPE_AMBISONIC_IEEE_FLOAT: [u8; 16] = [
    0x03, 0x00, 0x00, 0x00, 0x21, 0x07, 0xd3, 0x11,
    0x86, 0x44, 0xc8, 0xc1, 0xca, 0x00, 0x00, 0x00,
];

/// Resolve container width from `block_align` when consistent; else bits/8.
fn container_width(block_align: u16, num_channels: u16, bits_per_sample: u16) -> Result<usize> {
    if num_channels == 0 {
        return Err(WavError::format("riff: invalid channel count"));
    }
    let min_w = (bits_per_sample as usize).div_ceil(8);
    if min_w == 0 || min_w > 8 {
        return Err(WavError::format("wav: unsupported bits per sample"));
    }
    if block_align > 0 && block_align.is_multiple_of(num_channels) {
        let w = (block_align / num_channels) as usize;
        // Container must be at least the packed width and a multiple of the
        // natural sample size family we understand (1..8).
        if w >= min_w && w <= 8 {
            return Ok(w);
        }
    }
    Ok(min_w)
}

fn pcm_codec_for(bits_per_sample: u16, width: usize) -> Result<(SampleCodec, usize)> {
    match (bits_per_sample, width) {
        (8, w) if w >= 1 => Ok((SampleCodec::U8, w)),
        (16, w) if w >= 2 => Ok((SampleCodec::S16, w)),
        (24, 3) => Ok((SampleCodec::S24, 3)),
        (24, w) if w >= 4 => Ok((SampleCodec::S24_4, w)),
        (32, w) if w >= 4 => Ok((SampleCodec::S32, w)),
        _ => Err(WavError::format(format!(
            "wav: unsupported PCM layout bits={bits_per_sample} width={width}"
        ))),
    }
}

/// Map a plain-format WAVE channel count to a channel number: 1..=26.
fn map_wave_channel_count(count: u16) -> Result<usize> {
    if (1..=26).contains(&count) {
        Ok(usize::from(count))
    } else {
        Err(WavError::format("riff: invalid channel count"))
    }
}

/// Map a channel count to Ambisonic B-format components.
fn map_ambisonic_channel_count(count: u16) -> Result<usize> {
    match count {
        1..=9 | 11 | 16 => Ok(usize::from(count)),
        _ => Err(WavError::format("wav: invalid ambisonic channel count")),
    }
}

/// Correct a WAVE channel mask that is not valid for the stated number of
/// channels.
fn fix_wave_channel_mask(channel_mask: u32, num_channels: u16) -> Option<u32> {
    let n = u32::from(num_channels);
    if n > 32 {
        return None;
    }

    let mut mask = channel_mask;
    let pop = mask.count_ones();

    if n > pop {
        let diff = n - pop;
        let shift = 32 - mask.leading_zeros();
        if diff >= 32 {
            mask = u32::MAX;
        } else if shift + diff > 32 {
            return None;
        } else {
            mask |= ((1u32 << diff) - 1) << shift;
        }
    } else if pop > n {
        while mask.count_ones() != n {
            let highest_one = 31 - mask.leading_zeros();
            mask &= !(1u32 << highest_one);
        }
    }

    Some(mask)
}

mod fmt;
mod riff;

pub(crate) use riff::parse_header;

#[cfg(test)]
#[path = "../header_tests.rs"]
mod header_tests;
