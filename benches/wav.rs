//! In-process WAVE decode/encode vs hound, Symphonia, wavers, and (optional) dr_wav.
//!
//! Same classic RIFF bytes. Decode is measured through to mixed planar `f32`
//! (the STT output). hound yields typed samples; the bench converts i16 with
//! `/ 32768` and mixes stereo the same way ryf does (sum / n). Symphonia is
//! probed as WAVE (`wav` + `pcm` features only) from a zero-copy slice.
//! wavers reads `f32` from a `Cursor` (`Wav::new` + `read`).
//! dr_wav (`--features bench-c`) is in-process C: memory decode to interleaved
//! `f32`, then the same mix; sequential memory write for encode.
//!
//! ffmpeg is a **correctness oracle**, not a speed peer (process spawn).
//! Symphonia does not encode. wavers encode is path-only — not in this bench.

use std::io::{Cursor, Read, Seek, SeekFrom};
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SyError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ryf::{DecodeOptions, WriteSpec, decode_bytes, encode, encode_f32, encode_s16};

#[cfg(feature = "bench-c")]
#[path = "drwav.rs"]
mod drwav;

const RATE: u32 = 16_000;
const SECS: u32 = 2;
const FRAMES: usize = (RATE * SECS) as usize;

fn s16_mono_wav() -> Vec<u8> {
    let mut pcm = Vec::with_capacity(FRAMES * 2);
    for i in 0..FRAMES {
        let s = (i as i16).wrapping_mul(13);
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    encode_s16(&pcm, RATE).expect("encode s16 mono")
}

fn s16_stereo_wav() -> Vec<u8> {
    let mut pcm = Vec::with_capacity(FRAMES * 4);
    for i in 0..FRAMES {
        let l = (i as i16).wrapping_mul(13);
        let r = l.wrapping_neg();
        pcm.extend_from_slice(&l.to_le_bytes());
        pcm.extend_from_slice(&r.to_le_bytes());
    }
    encode(WriteSpec::s16(RATE, 2), &pcm).expect("encode s16 stereo")
}

fn f32_mono_wav() -> Vec<u8> {
    let samples: Vec<f32> = (0..FRAMES).map(|i| (i as f32) * 0.00001).collect();
    encode_f32(&samples, RATE, 1).expect("encode f32 mono")
}

fn s16_tone() -> Vec<i16> {
    (0..FRAMES).map(|i| (i as i16).wrapping_mul(13)).collect()
}

fn i16_as_le_bytes(samples: &[i16]) -> Vec<u8> {
    let n = samples.len() * 2;
    #[cfg(target_endian = "little")]
    {
        let mut out = Vec::with_capacity(n);
        // SAFETY: `i16` is plain bits; length is `samples.len() * 2`.
        let bytes = unsafe { std::slice::from_raw_parts(samples.as_ptr().cast::<u8>(), n) };
        out.extend_from_slice(bytes);
        out
    }
    #[cfg(target_endian = "big")]
    {
        let mut out = Vec::with_capacity(n);
        for &s in samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }
}

fn hound_s16_to_mixed_f32(wav: &[u8]) -> Vec<f32> {
    let mut reader = hound::WavReader::new(Cursor::new(wav)).expect("hound open");
    let ch = usize::from(reader.spec().channels).max(1);
    let interleaved: Vec<i16> = reader
        .samples::<i16>()
        .map(|s| s.expect("hound sample"))
        .collect();
    if ch == 1 {
        interleaved
            .into_iter()
            .map(|s| s as f32 / 32_768.0)
            .collect()
    } else {
        let n = ch as f32;
        interleaved
            .chunks_exact(ch)
            .map(|frame| frame.iter().map(|&s| s as f32 / 32_768.0).sum::<f32>() / n)
            .collect()
    }
}

fn hound_f32_mono(wav: &[u8]) -> Vec<f32> {
    let mut reader = hound::WavReader::new(Cursor::new(wav)).expect("hound open");
    reader
        .samples::<f32>()
        .map(|s| s.expect("hound sample"))
        .collect()
}

fn hound_encode_s16(samples: &[i16]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec).expect("hound writer");
        for &s in samples {
            w.write_sample(s).expect("hound write");
        }
        w.finalize().expect("hound finalize");
    }
    cursor.into_inner()
}

fn hound_encode_f32(samples: &[f32]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec).expect("hound writer");
        for &s in samples {
            w.write_sample(s).expect("hound write");
        }
        w.finalize().expect("hound finalize");
    }
    cursor.into_inner()
}

/// Zero-copy `MediaSource` over a `'static` slice (same as ryf `from_slice`).
struct SliceSrc {
    inner: Cursor<&'static [u8]>,
}

impl Read for SliceSrc {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Seek for SliceSrc {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl MediaSource for SliceSrc {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.inner.get_ref().len() as u64)
    }
}

fn leak(bytes: Vec<u8>) -> &'static [u8] {
    Box::leak(bytes.into_boxed_slice())
}

fn wavers_decode_mixed_f32(wav: &'static [u8]) -> Vec<f32> {
    let mut w: wavers::Wav<f32> =
        wavers::Wav::new(Box::new(Cursor::new(wav))).expect("wavers open");
    let ch = usize::from(w.n_channels());
    let samples = w.read().expect("wavers read");
    mix_interleaved_f32(&samples, ch)
}

fn mix_interleaved_f32(samples: &[f32], ch: usize) -> Vec<f32> {
    if ch <= 1 {
        samples.to_vec()
    } else {
        let n = ch as f32;
        samples
            .chunks_exact(ch)
            .map(|frame| frame.iter().sum::<f32>() / n)
            .collect()
    }
}

fn symphonia_decode_mixed_f32(wav: &'static [u8]) -> Vec<f32> {
    let mss = MediaSourceStream::new(
        Box::new(SliceSrc {
            inner: Cursor::new(wav),
        }),
        Default::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension("wav");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .expect("symphonia probe");
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .expect("symphonia track");
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .expect("symphonia decoder");
    let mut sample_buf = None;
    let mut interleaved = Vec::new();
    let mut ch = 1usize;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SyError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => panic!("symphonia packet: {e}"),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet).expect("symphonia decode");
        ch = decoded.spec().channels.count();
        let buf = sample_buf.get_or_insert_with(|| {
            SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec())
        });
        buf.copy_interleaved_ref(decoded);
        interleaved.extend_from_slice(buf.samples());
    }
    mix_interleaved_f32(&interleaved, ch)
}

fn ryf_decode_mono(wav: &[u8]) -> Vec<f32> {
    decode_bytes(wav, DecodeOptions::speech())
        .expect("ryf decode")
        .channels
        .into_iter()
        .next()
        .expect("mono plane")
}

fn benches(c: &mut Criterion) {
    let opts = DecodeOptions::speech();
    let s16_mono = leak(s16_mono_wav());
    let s16_st = leak(s16_stereo_wav());
    let f32_mono = leak(f32_mono_wav());
    let tone = s16_tone();
    let f32_tone: Vec<f32> = tone.iter().map(|&s| s as f32 / 32_768.0).collect();

    let mut decode = c.benchmark_group("decode");
    decode.throughput(Throughput::Bytes(s16_mono.len() as u64));
    decode.bench_function("ryf/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(ryf_decode_mono(s16_mono)));
    });
    decode.bench_function("hound/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(hound_s16_to_mixed_f32(s16_mono)));
    });
    decode.bench_function("symphonia/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(symphonia_decode_mixed_f32(s16_mono)));
    });
    decode.bench_function("wavers/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(wavers_decode_mixed_f32(s16_mono)));
    });
    #[cfg(feature = "bench-c")]
    decode.bench_function("dr_wav/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(drwav::decode_mixed_f32(s16_mono)));
    });
    decode.throughput(Throughput::Bytes(s16_st.len() as u64));
    decode.bench_function("ryf/s16_stereo_mix_2s", |b| {
        b.iter(|| {
            std::hint::black_box(
                decode_bytes(s16_st, opts.clone())
                    .expect("ryf decode")
                    .channels[0]
                    .len(),
            )
        });
    });
    decode.bench_function("hound/s16_stereo_mix_2s", |b| {
        b.iter(|| std::hint::black_box(hound_s16_to_mixed_f32(s16_st).len()));
    });
    decode.bench_function("symphonia/s16_stereo_mix_2s", |b| {
        b.iter(|| std::hint::black_box(symphonia_decode_mixed_f32(s16_st).len()));
    });
    decode.bench_function("wavers/s16_stereo_mix_2s", |b| {
        b.iter(|| std::hint::black_box(wavers_decode_mixed_f32(s16_st).len()));
    });
    #[cfg(feature = "bench-c")]
    decode.bench_function("dr_wav/s16_stereo_mix_2s", |b| {
        b.iter(|| std::hint::black_box(drwav::decode_mixed_f32(s16_st).len()));
    });
    decode.throughput(Throughput::Bytes(f32_mono.len() as u64));
    decode.bench_function("ryf/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(ryf_decode_mono(f32_mono)));
    });
    decode.bench_function("hound/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(hound_f32_mono(f32_mono)));
    });
    decode.bench_function("symphonia/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(symphonia_decode_mixed_f32(f32_mono)));
    });
    decode.bench_function("wavers/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(wavers_decode_mixed_f32(f32_mono)));
    });
    #[cfg(feature = "bench-c")]
    decode.bench_function("dr_wav/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(drwav::decode_mixed_f32(f32_mono)));
    });
    decode.finish();

    let mut encode_g = c.benchmark_group("encode");
    encode_g.throughput(Throughput::Elements(FRAMES as u64));
    encode_g.bench_function("ryf/s16_mono_2s", |b| {
        b.iter(|| {
            let pcm = i16_as_le_bytes(&tone);
            std::hint::black_box(encode_s16(&pcm, RATE).expect("ryf encode"))
        });
    });
    encode_g.bench_function("hound/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(hound_encode_s16(&tone)));
    });
    #[cfg(feature = "bench-c")]
    encode_g.bench_function("dr_wav/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(drwav::encode_s16(&tone, RATE)));
    });
    encode_g.bench_function("ryf/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(encode_f32(&f32_tone, RATE, 1).expect("ryf encode")));
    });
    encode_g.bench_function("hound/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(hound_encode_f32(&f32_tone)));
    });
    #[cfg(feature = "bench-c")]
    encode_g.bench_function("dr_wav/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(drwav::encode_f32(&f32_tone, RATE)));
    });
    encode_g.finish();

    let mulaw: Vec<u8> = (0..FRAMES).map(|i| (i % 256) as u8).collect();
    let mut g711 = c.benchmark_group("g711");
    g711.throughput(Throughput::Bytes(mulaw.len() as u64));
    g711.bench_function("ryf/decode_g711_mulaw_2s", |b| {
        b.iter(|| std::hint::black_box(ryf::decode_g711_mulaw(&mulaw, RATE).expect("g711")));
    });
    g711.finish();

    let mut stream = c.benchmark_group("stream");
    stream.throughput(Throughput::Bytes(s16_mono.len() as u64));
    stream.bench_function("ryf/decode_streaming_s16_mono_2s", |b| {
        b.iter(|| {
            let mut src = ryf::ByteSource::from_slice(s16_mono);
            let mut n = 0usize;
            ryf::decode_streaming(&mut src, &opts, |block| {
                n += block.frames;
                Ok(())
            })
            .expect("stream");
            std::hint::black_box(n)
        });
    });
    stream.finish();
}

fn criterion_config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(40)
}

criterion_group! {
    name = wav;
    config = criterion_config();
    targets = benches
}
criterion_main!(wav);
