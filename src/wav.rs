//! WAVE family demux + pull-parser decode.
//!
//! See the crate-level docs for the support matrix and non-goals.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::ChannelMode;
use crate::error::{FormatKind, Result, WavError};
use crate::header::{self, SampleCodec, parse_header};
use crate::options::DecodeOptions;
use crate::pull::{decode_collect, ensure_adpcm_enabled, open_decode};
use crate::source::ByteSource;

pub use crate::convert::convert_s16_le_to_f32;
pub use crate::header::ProbeCodec;
pub use crate::pull::{StreamBlock, StreamInfo, decode_streaming};

/// Result of decoding a WAVE stream at its native sample rate.
///
/// `channels` holds exactly one mixed track in [`ChannelMode::Mono`], or one
/// track per channel in [`ChannelMode::Split`] (all of equal length).
#[derive(Debug, Clone)]
pub struct DecodedWav {
    /// Sample rate from the `fmt ` chunk (Hz).
    pub sample_rate: u32,
    /// Planar `f32`. Length 1 if mixed; otherwise one vec per channel.
    pub channels: Vec<Vec<f32>>,
}

impl DecodedWav {
    /// Number of output planes (`1` if mixed).
    #[must_use]
    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }

    /// Frames in the first plane (all planes are equal length).
    #[must_use]
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }
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

/// Byte-slice sniff (syom/sluh). Same markers as [`sniff_is_riff_wave`].
pub fn sniff_wav(data: &[u8]) -> bool {
    let mut source = ByteSource::from_slice(data);
    sniff_is_riff_wave(&mut source).unwrap_or(false)
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
        return Err(WavError::unsupported_codec(header.fmt.format_tag));
    }
    if header.fmt.codec.is_adpcm() {
        ensure_adpcm_enabled()?;
    }
    let declared_frames = if let Some(sc) = header.declared_sample_count {
        Some(sc)
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
            .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
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
///
/// `source_label` is stored on [`DecodeOptions`] only; it is not interpolated
/// into [`WavError`] messages.
pub fn decode(
    mss: &mut ByteSource<'_>,
    mode: ChannelMode,
    source_label: &str,
) -> Result<DecodedWav> {
    decode_with(
        mss,
        &DecodeOptions::speech()
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

/// Molv-compat: LE PCM16 **mono** `data` bytes, no f32 convert.
///
/// Stereo, RIFX, float, and other codecs are [`WavError::UnsupportedCodec`].
/// Empty `data` is [`WavError::Empty`]. Odd byte length is [`WavError::OddPcm`].
pub fn decode_s16(data: &[u8]) -> Result<(u32, Vec<u8>)> {
    decode_s16_from(&mut ByteSource::from_slice(data))
}

fn decode_s16_from(src: &mut ByteSource<'_>) -> Result<(u32, Vec<u8>)> {
    let header = parse_header(src)?;
    if header.fmt.codec != SampleCodec::S16
        || header.fmt.channels != 1
        || header.fmt.big_endian
        || header.fmt.sample_width != 2
    {
        return Err(WavError::unsupported_codec(header.fmt.format_tag));
    }
    let available = src
        .byte_len()
        .ok_or(WavError::StreamLengthUnknown)?
        .saturating_sub(header.data_pos);
    let want = header.declared_data_len.unwrap_or(available).min(available);
    if want == 0 {
        return Err(WavError::Empty);
    }
    if !want.is_multiple_of(2) {
        return Err(WavError::OddPcm);
    }
    let n = usize::try_from(want).map_err(|_| WavError::format(FormatKind::InvalidSize))?;
    if let Some(rest) = src.remaining_slice() {
        if rest.len() < n {
            return Err(WavError::format(FormatKind::Truncated));
        }
        return Ok((header.fmt.sample_rate, rest[..n].to_vec()));
    }
    let mut out = vec![0u8; n];
    src.read_buf_exact(&mut out)
        .map_err(|_| WavError::format(FormatKind::Truncated))?;
    Ok((header.fmt.sample_rate, out))
}

/// Molv-compat: mono f32 at the file's native rate (stereo mixed).
/// Empty PCM is [`WavError::Empty`]. No speech duration cap.
pub fn decode_f32(data: &[u8]) -> Result<(u32, Vec<f32>)> {
    let decoded = decode_bytes(
        data,
        DecodeOptions::unbounded().with_channel_mode(ChannelMode::Mono),
    )?;
    let mono = decoded
        .channels
        .into_iter()
        .next()
        .ok_or_else(|| WavError::format(FormatKind::InvalidOperation))?;
    if mono.is_empty() {
        return Err(WavError::Empty);
    }
    Ok((decoded.sample_rate, mono))
}

/// Read a WAVE file to planar `f32` (split channels, archival caps).
///
/// Mix-to-mono + 2 h speech caps: [`read_speech`].
pub fn read(path: impl AsRef<Path>) -> Result<DecodedWav> {
    read_with(path, &DecodeOptions::default())
}

/// [`read`] with [`DecodeOptions::speech`] (mix-to-mono, 2 h / 4 GiB).
pub fn read_speech(path: impl AsRef<Path>) -> Result<DecodedWav> {
    read_with(path, &DecodeOptions::speech())
}

/// [`read`] with explicit [`DecodeOptions`].
pub fn read_with(path: impl AsRef<Path>, opts: &DecodeOptions) -> Result<DecodedWav> {
    let file = std::fs::File::open(path)?;
    decode_with(&mut ByteSource::from_file(file), opts)
}

/// [`decode_s16`] from a filesystem path.
pub fn read_s16(path: &Path) -> Result<(u32, Vec<u8>)> {
    let file = std::fs::File::open(path)?;
    decode_s16_from(&mut ByteSource::from_file(file))
}

/// [`decode_f32`] from a filesystem path.
pub fn read_f32(path: &Path) -> Result<(u32, Vec<f32>)> {
    decode_f32(&std::fs::read(path)?)
}

/// Slurp `reader` then [`decode_bytes`]. The slurp stops at
/// `max_output_bytes + 1 MiB`.
pub fn decode_reader<R: Read>(reader: R, opts: &DecodeOptions) -> Result<DecodedWav> {
    let cap = opts.max_output_bytes.saturating_add(1024 * 1024);
    let mut limited = reader.take(cap.saturating_add(1));
    let mut data = Vec::new();
    limited.read_to_end(&mut data)?;
    let n = data.len() as u64;
    if n > cap {
        return Err(WavError::output_too_large(n, cap));
    }
    decode_bytes(&data, opts.clone())
}

#[cfg(test)]
#[path = "wav_tests.rs"]
mod tests;

#[cfg(all(test, not(miri)))]
#[path = "wav_proptest.rs"]
mod proptests;
