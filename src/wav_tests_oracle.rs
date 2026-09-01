use crate::ChannelMode;
use crate::error::Result;
use crate::source::ByteSource;

// --- decode oracles ---

/// Own decoder via the public dispatched entry point.
pub fn own_mono(data: &[u8]) -> Result<Vec<f32>> {
    // Native-rate mono (no resample — resampling is a product concern).
    own_mono_native(data).map(|(_, v)| v)
}

pub fn own_channels(data: &[u8]) -> Result<Vec<Vec<f32>>> {
    own_channels_native(data).map(|(_, v)| v)
}

// Native-rate own decodes (no resample): the cheap comparison used by
// the big matrix. Multi-rate coverage is in `test_diff_multi_rate`.

pub fn decode_file_mono(path: &str) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path)?;
    let mut source = ByteSource::from_file(file);
    let d = crate::decode(&mut source, ChannelMode::Mono, "file")?;
    Ok(d.channels.into_iter().next().unwrap_or_default())
}

pub fn decode_file_channels(path: &str) -> Result<Vec<Vec<f32>>> {
    let file = std::fs::File::open(path)?;
    let mut source = ByteSource::from_file(file);
    let d = crate::decode(&mut source, ChannelMode::Split, "file")?;
    Ok(d.channels)
}

pub fn own_mono_native(data: &[u8]) -> Result<(u32, Vec<f32>)> {
    let mut source = ByteSource::from_slice(data);
    let decoded = crate::decode(&mut source, ChannelMode::Mono, "diff")?;
    Ok((
        decoded.sample_rate,
        decoded.channels.into_iter().next().unwrap_or_default(),
    ))
}

pub fn own_channels_native(data: &[u8]) -> Result<(u32, Vec<Vec<f32>>)> {
    let mut source = ByteSource::from_slice(data);
    let decoded = crate::decode(&mut source, ChannelMode::Split, "diff")?;
    Ok((decoded.sample_rate, decoded.channels))
}

/// Whether ffmpeg + ffprobe are available for the differential gates.
pub fn ffmpeg_available() -> bool {
    ["ffmpeg", "ffprobe"].iter().all(|tool| {
        std::process::Command::new(tool)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// ffmpeg CLI oracle: decode the WAV bytes (written to a temp file) to
/// native-rate f32le, deinterleaved per channel. PCM/µ-law/A-law/float
/// WAV are lossless, so this is a bit-exact reference. Returns (rate,
/// channels).
pub fn ffmpeg_native_channels(label: &str, data: &[u8]) -> Result<(u32, Vec<Vec<f32>>)> {
    // Unique temp file per case: tests run multi-threaded, so a
    // pid-derived name would collide across tests.
    let tmp = tempfile::Builder::new()
        .prefix("ryf-diff-")
        .suffix(".wav")
        .tempfile()?;
    std::fs::write(tmp.path(), data)?;
    let probe = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0"])
        .args([
            "-show_entries",
            "stream=sample_rate,channels",
            "-of",
            "csv=p=0",
        ])
        .arg(tmp.path())
        .output()?;
    assert!(probe.status.success(), "{label}: ffprobe failed");
    let meta = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    let mut it = meta.split(',');
    let rate: u32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let channels: usize = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    assert!(rate > 0 && channels > 0, "{label}: ffprobe parsed {meta:?}");
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-xerror", "-i"])
        .arg(tmp.path())
        .args(["-f", "f32le", "-"])
        .output()?;
    assert!(
        out.status.success(),
        "{label}: ffmpeg decode failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let interleaved: Vec<f32> = out
        .stdout
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect();
    let mut chans = vec![Vec::with_capacity(interleaved.len() / channels); channels];
    for frame in interleaved.chunks_exact(channels) {
        for (c, &s) in frame.iter().enumerate() {
            chans[c].push(s);
        }
    }
    Ok((rate, chans))
}

/// Mono mix of per-channel vectors with the same accumulation order the
/// decoder's inline mix uses (sum left-to-right, divide by n).
pub fn mix_mono(channels: &[Vec<f32>]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }
    if channels.len() == 1 {
        return channels[0].clone();
    }
    let n = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    let n_ch = channels.len() as f32;
    (0..n)
        .map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / n_ch)
        .collect()
}

#[track_caller]
pub fn assert_bit_exact(label: &str, a: &[f32], b: &[f32]) {
    assert_eq!(
        a.len(),
        b.len(),
        "{label}: length mismatch {} vs {}",
        a.len(),
        b.len()
    );
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        let (xb, yb) = (x.to_bits(), y.to_bits());
        assert_eq!(
            xb, yb,
            "{label}: sample {i} diverged: {xb:#010x} vs {yb:#010x} ({x} vs {y})"
        );
    }
}

/// Well-formed input: own decode and the ffmpeg oracle must succeed and
/// agree bit-exactly on the mono path at native sample rate
/// (ffmpeg's per-channel output is mixed with the decoder's arithmetic).
#[track_caller]
pub fn assert_mono_bit_exact(label: &str, data: &[u8]) {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not available, skipping {label}");
        return;
    }
    let own = own_mono(data)
        .unwrap_or_else(|e| panic!("{label}: own decoder rejected well-formed: {e:#}"));
    let (_rate, ff) =
        ffmpeg_native_channels(label, data).unwrap_or_else(|e| panic!("{label}: ffmpeg: {e:#}"));
    let expect = mix_mono(&ff);
    assert_bit_exact(label, &own, &expect);
}

/// Well-formed input: own decode and the ffmpeg oracle must succeed and
/// agree bit-exactly on the per-channel path at native sample rate.
#[track_caller]
pub fn assert_channels_bit_exact(label: &str, data: &[u8]) {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not available, skipping {label}");
        return;
    }
    let own = own_channels(data)
        .unwrap_or_else(|e| panic!("{label}: own channels rejected well-formed: {e:#}"));
    let (_rate, ff) =
        ffmpeg_native_channels(label, data).unwrap_or_else(|e| panic!("{label}: ffmpeg: {e:#}"));
    assert_eq!(own.len(), ff.len(), "{label}: channel count mismatch");
    for (c, (x, y)) in own.iter().zip(ff.iter()).enumerate() {
        assert_bit_exact(&format!("{label} ch{c}"), x, y);
    }
}

#[track_caller]
pub fn assert_bit_exact_both_modes(label: &str, data: &[u8]) {
    assert_mono_bit_exact(label, data);
    assert_channels_bit_exact(label, data);
}

/// Well-formed input: own decode and the ffmpeg oracle must succeed and
/// agree bit-exactly at the native sample rate (mono and per-channel),
/// before any resampling.
#[track_caller]
pub fn assert_native_bit_exact_both_modes(label: &str, data: &[u8]) {
    if !ffmpeg_available() {
        eprintln!("ffmpeg/ffprobe not available, skipping {label}");
        return;
    }
    let (rate_f, ff) =
        ffmpeg_native_channels(label, data).unwrap_or_else(|e| panic!("{label}: ffmpeg: {e:#}"));
    let (rate_o, own) = own_mono_native(data)
        .unwrap_or_else(|e| panic!("{label}: own native decode rejected well-formed: {e:#}"));
    assert_eq!(rate_o, rate_f, "{label}: native rate mismatch");
    assert_bit_exact(label, &own, &mix_mono(&ff));
    let (rate_o, own) = own_channels_native(data)
        .unwrap_or_else(|e| panic!("{label}: own native split rejected well-formed: {e:#}"));
    assert_eq!(rate_o, rate_f, "{label}: native rate mismatch (split)");
    assert_eq!(own.len(), ff.len(), "{label}: channel count mismatch");
    for (c, (x, y)) in own.iter().zip(ff.iter()).enumerate() {
        assert_bit_exact(&format!("{label} ch{c}"), x, y);
    }
}

/// Malformed input which the own decoder must fail.
#[track_caller]
pub fn assert_both_err(label: &str, data: &[u8]) {
    let own = own_mono(data);
    assert!(own.is_err(), "{label}: own decoder must reject, got Ok");
}
