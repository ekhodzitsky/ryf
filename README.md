# ryf

just wav.

Pure-Rust **WAVE family** reader. `std` only. No encode.

## Support matrix

| Containers | Codecs |
|------------|--------|
| RIFF / WAVE | PCM int 8 / 16 / 24 / 32 (incl. S24 in 4-byte containers) |
| RIFX (big-endian) | IEEE float 32 / 64 |
| RF64 / BW64 (`ds64`) | G.711 A-law / µ-law |
| Sony Wave64 | MS-ADPCM, IMA/DVI ADPCM (`adpcm` feature) |

Also: `WAVE_FORMAT_EXTENSIBLE` (PCM / float / G.711 + Ambisonic GUIDs),
wild-file quirks (`valid_bits=0`, empty channel mask, short `data`, streaming
`u32::MAX` sizes).

## Non-goals

- Encoding / writing WAVE
- GSM, MPEG-in-WAV, and other exotic codecs
- Resampling
- Async I/O
- `mmap`, `zeroize`, `tracing`

## Dependencies

**Default (`adpcm` + `simd`): zero external crates** — pure `std`.

## Features

| Feature | Default | Effect |
|---------|---------|--------|
| `adpcm` | yes | MS + IMA ADPCM |
| `simd` | yes | NEON / SSE s16→f32, stereo mix/split, f32 copy (bit-exact) |

## API

```rust,no_run
use ryf::{ChannelMode, DecodeOptions, decode_bytes};

let opts = DecodeOptions::speech().with_channel_mode(ChannelMode::Mono);
let data = std::fs::read("speech.wav")?;
let decoded = decode_bytes(&data, opts)?;
println!("{} Hz, {} frames", decoded.sample_rate, decoded.channels[0].len());
# Ok::<(), ryf::WavError>(())
```

- [`DecodeOptions::speech`] — 2 h, 192 kHz, 4 GiB planar f32 (default)
- [`DecodeOptions::unbounded`] — archival; still caps RAM from a lying header
- [`decode_streaming`] — ~256 KiB source blocks, peak RAM O(block)

Typed errors: [`WavError`] (no `anyhow`, no `thiserror`).

## Layout

```
src/
  lib.rs       public re-exports, ChannelMode
  error.rs     WavError
  options.rs   DecodeOptions
  source.rs    ByteSource (inlined; no extra crate)
  header/      RIFF/RIFX/RF64/W64 demux
  convert/     s16/f32/G.711 convert + SIMD
  pull/        O(block) pull-parser
  wav.rs       sniff / probe / decode
  adpcm/       MS + IMA (feature)
```

## Correctness

Differential suite vs **ffmpeg** (bit-exact f32 on lossless PCM / G.711) when
`ffmpeg` is on `PATH`. Duration is rejected (`TooLong`) on the *actual* PCM
length. Split output is bounded by `max_output_bytes` (default 4 GiB).

## License

MIT
