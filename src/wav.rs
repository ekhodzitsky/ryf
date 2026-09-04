//! WAVE family demux + pull-parser decode.
//!
//! See the crate-level docs for the support matrix and non-goals.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::ChannelMode;
use crate::error::{FormatKind, Result, WavError};
use crate::header::{self, SampleCodec, parse_header};
use crate::options::DecodeOptions;
use crate::pull::{adpcm_frames_capped, decode_collect, ensure_adpcm_enabled, open_decode};
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
    /// Native PCM rate (Hz). G.722 is always 16 kHz, even if `fmt ` differs.
    /// GSM keeps the `fmt ` rate (8 kHz on wav49).
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
    /// Native PCM rate (Hz). G.722 is always 16 kHz; GSM keeps `fmt `.
    pub sample_rate: u32,
    pub channels: usize,
    /// Container bytes per sample of one channel (G.722: 1 encoded byte;
    /// GSM: 65-byte MS-GSM block).
    pub sample_width: usize,
    pub codec: ProbeCodec,
    /// Declared PCM frames from the data chunk, when known.
    pub declared_frames: Option<u64>,
    /// Absolute stream position of the first data byte.
    pub data_pos: u64,
}

/// Read the stream head and report whether it marks a WAVE container:
/// `RIFF` / `RIFX` / `RF64` / `BW64` + `"WAVE"`, or Sony Wave64 (GUID riff/wave).
/// The stream is always rewound to position 0 afterwards, including on I/O
/// error (rewind failure is returned only when the prefix read succeeded).
pub fn sniff_is_riff_wave(mss: &mut ByteSource<'_>) -> Result<bool> {
    let result = sniff_wave_prefix(mss);
    let rewind = mss.seek(SeekFrom::Start(0));
    match result {
        Ok(v) => rewind.map(|_| v).map_err(WavError::Io),
        Err(e) => {
            let _ = rewind;
            Err(e)
        }
    }
}

fn sniff_wave_prefix(mss: &mut ByteSource<'_>) -> Result<bool> {
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
    Ok(is_classic || is_w64)
}

/// Byte-slice sniff. Same markers as [`sniff_is_riff_wave`].
pub fn sniff_wav(data: &[u8]) -> bool {
    let mut source = ByteSource::from_slice(data);
    sniff_is_riff_wave(&mut source).unwrap_or(false)
}

/// Probe with library defaults ([`DecodeOptions::default`]: split, archival
/// caps). Speech caps: [`probe_with`] + [`DecodeOptions::speech`].
pub fn probe(mss: &mut ByteSource<'_>) -> Result<WavProbe> {
    probe_with(mss, &DecodeOptions::default())
}

/// Probe a RIFF/WAVE stream (positioned at offset 0) without decoding PCM.
/// The stream is always rewound to position 0 afterwards, including on error
/// (rewind failure is returned only when the probe itself succeeded).
pub fn probe_with(mss: &mut ByteSource<'_>, opts: &DecodeOptions) -> Result<WavProbe> {
    let result = probe_inner(mss, opts);
    let rewind = mss.seek(SeekFrom::Start(0));
    match result {
        Ok(v) => rewind.map(|_| v).map_err(WavError::Io),
        Err(e) => {
            let _ = rewind;
            Err(e)
        }
    }
}

fn probe_inner(mss: &mut ByteSource<'_>, opts: &DecodeOptions) -> Result<WavProbe> {
    let header = parse_header(mss)?;
    let header_rate = header.fmt.sample_rate;
    if header_rate == 0 || header_rate > opts.max_sample_rate {
        return Err(WavError::sample_rate(header_rate, opts.max_sample_rate));
    }
    if matches!(header.fmt.codec, SampleCodec::Unsupported) {
        return Err(WavError::unsupported_codec(header.fmt.format_tag));
    }
    let sample_rate = crate::g722::output_rate(header.fmt.codec, header_rate);
    if sample_rate > opts.max_sample_rate {
        return Err(WavError::sample_rate(sample_rate, opts.max_sample_rate));
    }
    if header.fmt.codec.is_adpcm() {
        ensure_adpcm_enabled()?;
    }
    // Same clamp as `open_decode`: lying `data` size cannot exceed the file.
    let data_len = match header.declared_data_len {
        Some(d) => Some(
            d.min(
                mss.byte_len()
                    .map(|n| n.saturating_sub(header.data_pos))
                    .unwrap_or(d),
            ),
        ),
        None => mss.byte_len().map(|n| n.saturating_sub(header.data_pos)),
    };
    let declared_frames = if header.fmt.codec == SampleCodec::G722 {
        match data_len {
            Some(d) => Some(crate::g722::pcm_frames_capped(
                d,
                header.fmt.channels,
                header.declared_sample_count,
            )),
            None => header.declared_sample_count,
        }
    } else if header.fmt.codec == SampleCodec::Gsm {
        match data_len {
            Some(d) => Some(crate::gsm::pcm_frames_capped(
                d,
                header.declared_sample_count,
            )),
            None => header.declared_sample_count,
        }
    } else if header.fmt.codec.is_adpcm() {
        let (ba, ch, ima) = match (header.fmt.adpcm_ms.as_ref(), header.fmt.adpcm_ima.as_ref()) {
            (Some(p), _) => (u64::from(p.block_align), p.channels as u64, false),
            (_, Some(p)) => (u64::from(p.block_align), p.channels as u64, true),
            _ => (0, 1, false),
        };
        data_len.map(|d| adpcm_frames_capped(d, ba, ch, ima, header.declared_sample_count))
    } else {
        let frame_bytes = header
            .fmt
            .sample_width
            .checked_mul(header.fmt.channels)
            .filter(|&n| n > 0)
            .ok_or_else(|| WavError::format(FormatKind::InvalidSize))?;
        data_len.map(|d| {
            let actual = d / frame_bytes as u64;
            match header.declared_sample_count {
                Some(sc) if sc > 0 => actual.min(sc),
                _ => actual,
            }
        })
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

/// LE PCM16 **mono** `data` bytes, no f32 convert.
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
    if header.fmt.sample_rate == 0 {
        return Err(WavError::sample_rate(0, 1));
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

/// Mono f32 at the file's native rate (stereo mixed).
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
