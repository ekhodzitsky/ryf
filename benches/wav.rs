//! In-process WAVE decode/encode vs [hound](https://github.com/ruuda/hound).
//!
//! Same classic RIFF bytes. Decode is measured through to mixed planar `f32`
//! (the STT output). hound yields typed samples; the bench converts i16 with
//! `/ 32768` and mixes stereo the same way ryf does (sum / n).
//!
//! ffmpeg is a **correctness oracle**, not a speed peer (process spawn).
//! symphonia is a multi-format pipeline, not a WAVE specialist — omitted.

use std::io::Cursor;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ryf::{DecodeOptions, WriteSpec, decode_bytes, encode, encode_f32, encode_s16};

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
    let s16_mono = s16_mono_wav();
    let s16_st = s16_stereo_wav();
    let f32_mono = f32_mono_wav();
    let tone = s16_tone();
    let f32_tone: Vec<f32> = tone.iter().map(|&s| s as f32 / 32_768.0).collect();

    let mut decode = c.benchmark_group("decode");
    decode.throughput(Throughput::Bytes(s16_mono.len() as u64));
    decode.bench_function("ryf/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(ryf_decode_mono(&s16_mono)));
    });
    decode.bench_function("hound/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(hound_s16_to_mixed_f32(&s16_mono)));
    });
    decode.throughput(Throughput::Bytes(s16_st.len() as u64));
    decode.bench_function("ryf/s16_stereo_mix_2s", |b| {
        b.iter(|| {
            std::hint::black_box(
                decode_bytes(&s16_st, opts.clone())
                    .expect("ryf decode")
                    .channels[0]
                    .len(),
            )
        });
    });
    decode.bench_function("hound/s16_stereo_mix_2s", |b| {
        b.iter(|| std::hint::black_box(hound_s16_to_mixed_f32(&s16_st).len()));
    });
    decode.throughput(Throughput::Bytes(f32_mono.len() as u64));
    decode.bench_function("ryf/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(ryf_decode_mono(&f32_mono)));
    });
    decode.bench_function("hound/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(hound_f32_mono(&f32_mono)));
    });
    decode.finish();

    let mut encode_g = c.benchmark_group("encode");
    encode_g.throughput(Throughput::Elements(FRAMES as u64));
    encode_g.bench_function("ryf/s16_mono_2s", |b| {
        b.iter(|| {
            let mut pcm = Vec::with_capacity(tone.len() * 2);
            for &s in &tone {
                pcm.extend_from_slice(&s.to_le_bytes());
            }
            std::hint::black_box(encode_s16(&pcm, RATE).expect("ryf encode"))
        });
    });
    encode_g.bench_function("hound/s16_mono_2s", |b| {
        b.iter(|| std::hint::black_box(hound_encode_s16(&tone)));
    });
    encode_g.bench_function("ryf/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(encode_f32(&f32_tone, RATE, 1).expect("ryf encode")));
    });
    encode_g.bench_function("hound/f32_mono_2s", |b| {
        b.iter(|| std::hint::black_box(hound_encode_f32(&f32_tone)));
    });
    encode_g.finish();

    let mut stream = c.benchmark_group("stream");
    stream.throughput(Throughput::Bytes(s16_mono.len() as u64));
    stream.bench_function("ryf/decode_streaming_s16_mono_2s", |b| {
        b.iter(|| {
            let mut src = ryf::ByteSource::from_slice(&s16_mono);
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
