//! WAVE family demux + pull-parser decode.
//!
//! See the crate-level docs for the support matrix and non-goals.

use std::io::{Read, Seek, SeekFrom};

use crate::ChannelMode;
use crate::error::{Result, WavError};
use crate::header::{self, SampleCodec, parse_header};
use crate::options::DecodeOptions;
use crate::pull::{decode_collect, ensure_adpcm_enabled, open_decode};
use crate::source::ByteSource;

pub use crate::convert::{convert_s16_le_to_f32, convert_s16_mono_pub};
pub use crate::header::ProbeCodec;
pub use crate::pull::{StreamBlock, StreamInfo, decode_streaming};

/// Result of decoding a RIFF/WAVE stream at its native sample rate.
///
/// `channels` holds exactly one mixed track in [`ChannelMode::Mono`], or one
/// track per channel in [`ChannelMode::Split`] (all of equal length).
#[derive(Debug, Clone)]
pub struct DecodedWav {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

/// Lightweight header probe without decoding PCM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavProbe {
    pub sample_rate: u32,
    pub channels: usize,
    /// Container bytes per sample of one channel.
    pub sample_width: usize,
    pub codec: ProbeCodec,
    /// Declared PCM frames from the data chunk, when known.
    pub declared_frames: Option<u64>,
    /// Absolute stream position of the first data byte.
    pub data_pos: u64,
}

/// Read the stream head and report whether it marks a WAVE container:
/// `RIFF` / `RIFX` / `RF64` / `BW64` + `"WAVE"`, or Sony Wave64 (GUID riff/wave).
/// The stream is always rewound to position 0 afterwards.
pub fn sniff_is_riff_wave(mss: &mut ByteSource<'_>) -> Result<bool> {
    // 40 bytes covers W64 (16 GUID + 8 size + 16 WAVE GUID).
    let mut prefix = [0u8; 40];
    let mut filled = 0usize;
    while filled < prefix.len() {
        match mss.read(&mut prefix[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) => return Err(WavError::Io(e)),
        }
    }

    let is_classic = filled >= 12
        && matches!(&prefix[0..4], b"RIFF" | b"RIFX" | b"RF64" | b"BW64")
        && &prefix[8..12] == b"WAVE";
    let is_w64 = filled >= 40
        && prefix[0..16] == header::W64_GUID_RIFF
        && prefix[24..40] == header::W64_GUID_WAVE;

    mss.seek(SeekFrom::Start(0))?;
    Ok(is_classic || is_w64)
}

/// Probe with speech-ingest default caps.
pub fn probe(mss: &mut ByteSource<'_>) -> Result<WavProbe> {
    probe_with(mss, &DecodeOptions::default())
}

/// Probe a RIFF/WAVE stream (positioned at offset 0) without decoding PCM.
pub fn probe_with(mss: &mut ByteSource<'_>, opts: &DecodeOptions) -> Result<WavProbe> {
    let header = parse_header(mss)?;
    let sample_rate = header.fmt.sample_rate;
    if sample_rate == 0 || sample_rate > opts.max_sample_rate {
        return Err(WavError::sample_rate(sample_rate, opts.max_sample_rate));
    }
    if matches!(header.fmt.codec, SampleCodec::Unsupported) {
        return Err(WavError::UnsupportedCodec);
    }
    if header.fmt.codec.is_adpcm() {
        ensure_adpcm_enabled()?;
    }
    let declared_frames = if let Some(sc) = header.declared_sample_count {
        let ch = header.fmt.channels as u64;
        Some(if ch > 0 && sc % ch == 0 { sc / ch } else { sc })
    } else if header.fmt.codec.is_adpcm() {
        let (ba, spb) = match (header.fmt.adpcm_ms.as_ref(), header.fmt.adpcm_ima.as_ref()) {
            (Some(p), _) => (p.block_align as u64, p.samples_per_block as u64),
            (_, Some(p)) => (p.block_align as u64, p.samples_per_block as u64),
            _ => (0, 0),
        };
        header
            .declared_data_len
            .and_then(|d| d.checked_div(ba).map(|blocks| blocks * spb))
    } else {
        let frame_bytes = header
            .fmt
            .sample_width
            .checked_mul(header.fmt.channels)
            .filter(|&n| n > 0)
            .ok_or_else(|| WavError::format("wav: invalid frame size"))?;
        header.declared_data_len.map(|d| d / frame_bytes as u64)
    };
    Ok(WavProbe {
        sample_rate,
        channels: header.fmt.channels,
        sample_width: header.fmt.sample_width,
        codec: header.fmt.codec.probe(),
        declared_frames,
        data_pos: header.data_pos,
    })
}

/// Decode with explicit channel mode and a source label (speech-ingest caps).
pub fn decode(
    mss: &mut ByteSource<'_>,
    mode: ChannelMode,
    source_label: &str,
) -> Result<DecodedWav> {
    decode_with(
        mss,
        &DecodeOptions::default()
            .with_channel_mode(mode)
            .with_source_label(source_label),
    )
}

/// Decode a full in-memory WAVE buffer (zero-copy over `data`; no heap clone).
pub fn decode_bytes(data: &[u8], opts: DecodeOptions) -> Result<DecodedWav> {
    let mut source = ByteSource::from_slice(data);
    decode_with(&mut source, &opts)
}

/// Decode a RIFF/WAVE stream at native sample rate under `opts`.
///
/// Hot paths (mono s16 / mono f32 / stereo mix) write into one exact-size
/// allocation with file-sized scratch; streaming API shares the same convert
/// kernels via [`crate::pull::decode_streaming`].
pub fn decode_with(mss: &mut ByteSource<'_>, opts: &DecodeOptions) -> Result<DecodedWav> {
    let plan = open_decode(mss, opts)?;
    let sample_rate = plan.sample_rate;
    let channels = decode_collect(mss, &plan)?;
    Ok(DecodedWav {
        sample_rate,
        channels,
    })
}

#[cfg(test)]
#[path = "wav_tests.rs"]
mod tests;

#[cfg(all(test, not(miri)))]
#[path = "wav_proptest.rs"]
mod proptests;
