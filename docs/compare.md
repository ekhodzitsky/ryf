# Compare

ryf is a **WAVE family ingest** crate: messy containers, telephony codecs,
planar `f32`, RAM/duration caps, **zero** default-feature deps.

It is **not** a player (rodio/cpal/miniaudio), not a resample pipeline, not
ffmpeg. Product path is pure Rust. C is only an optional Criterion peer.

## Timed (in-process, same bytes)

Same classic RIFF clip (2 s @ 16 kHz to mixed planar `f32`). Numbers:
[benchmarks.md](benchmarks.md).

| | [ryf](https://github.com/ekhodzitsky/ryf) | [hound](https://github.com/ruuda/hound) 3.5 | [symphonia](https://github.com/pdeljanov/Symphonia) 0.5 | [wavers](https://github.com/jmg049/wavers) 1.5 | [dr_wav](https://github.com/mackron/dr_libs) 0.14 |
|---|---|---|---|---|---|
| Lang | Rust | Rust | Rust | Rust | **C** (header-only) |
| Job | WAVE family ingest | PCM/IEEE RIFF iterator | multi-format demux | PCM/IEEE reader | WAV load/write |
| RIFF PCM / IEEE f32 | yes | yes | yes (`wav`+`pcm`) | yes | yes |
| RIFX / RF64 / BW64 / Wave64 | yes | no | limited | no | RF64 + Wave64; not RIFX write |
| G.711 | yes | no | yes | no | yes |
| G.722 | yes | no | no | no | no |
| GSM 06.10 / wav49 | yes | no | no | no | no |
| MS / IMA ADPCM | yes (`adpcm`) | no | limited | no | yes |
| Write | RIFF/RF64/RIFX PCM/IEEE/G.711; extensible PCM/IEEE | PCM/IEEE, extensible | **no** | PCM/IEEE (path) | PCM/IEEE/A-law/mu-law (no ADPCM write) |
| Default deps | **none** | none | several | several | none (single `.h`) |
| Caps | yes | no | no | no | no |
| In harness | always | always | always | always | `--features bench-c` |

hound is the default WAV crate (~1k reverse-deps). PCM/IEEE RIFF only. Last
release 2023.

Symphonia is a media pipeline. The bench enables **only** `wav` + `pcm`.
No encode.

wavers is a later PCM/IEEE reader (bulk SIMD convert). Author archived it in
favour of [`audio_samples_io`](https://crates.io/crates/audio_samples_io).
Encode is file-path API - **decode-only** in Criterion.

**dr_wav** is the C speed king for this job: public domain / MIT-0, one
header, memory in/out. Vendored at `native/dr_wav.h` (mackron/dr_libs
`dfe8377`, v0.14.6) and compiled only with `bench-c`. Not on the product
path. Decode: `drwav_open_memory_and_read_pcm_frames_f32`. Encode:
`drwav_init_memory_write_sequential_pcm_frames`.

## C / C++ (not all timed)

| Library | Why it is (not) in the harness |
|---|---|
| **dr_wav** | Timed. See above. |
| [libsndfile](https://github.com/libsndfile/libsndfile) | Industry WAV/AIFF/FLAC/... I/O (LGPL). System lib + pkg-config; skipped so CI stays green with no C toolchain on the default path. |
| [miniaudio](https://github.com/mackron/miniaudio) | Device I/O + decoder. The WAV backend **is** dr_wav. Timing miniaudio would re-time dr_wav plus a player. |
| **ffmpeg** `libavformat` / `libavcodec` | Correctness **oracle** (subprocess on `PATH`). Not a library dep, not timed. |
| [AudioFile.h](https://github.com/adamstark/AudioFile) (C++) | Header-only PCM/IEEE. Same niche as hound; not in-process from Rust without another FFI. |
| JUCE / Qt Multimedia | App frameworks. Qt vendors dr_wav. |

## Other languages (not timed)

In-process Criterion is Rust and C. These stacks are documented, not raced:

| Runtime | Typical WAV stack | Notes vs ryf |
|---|---|---|
| Python | `wave` (stdlib, PCM), `scipy.io.wavfile`, `soundfile` (libsndfile) | GC + C backend. `soundfile` **is** libsndfile. |
| Go | `go-audio/wav`, `youpy/go-wav` | PCM/IEEE RIFF. No family codecs. |
| JS / TS | `wavefile`, Web Audio `decodeAudioData` | Browser/node. Not a Unix ingest crate. |
| C# | NAudio, CSCore | Windows-first. |
| Java | `javax.sound.sampled` | PCM RIFF. |

## Other Rust WAVE crates (not timed)

PCM/IEEE RIFF readers - same job as hound/wavers, not a family codec:

| Crate | Why not in the harness |
|---|---|
| [`wav`](https://crates.io/crates/wav) 1.0 | **Deprecated**; author says use hound. PCM only. LGPL. |
| [`wav-codec`](https://crates.io/crates/wav-codec) 0.1 | New zero-dep PCM/IEEE iterator. No G.711/ADPCM/RF64. |
| [`wav_io`](https://crates.io/crates/wav_io) | PCM utility + resample + silence split. Different product. |
| [`riff-wave`](https://crates.io/crates/riff-wave) | Canonical PCM only. 2016-2022. |
| [`waveadapter`](https://crates.io/crates/waveadapter) | CamillaDSP-style container to `audioadapter` buffers. A-law/ADPCM round-trip as **raw bytes**, not decoded. |
| [`nwav`](https://crates.io/crates/nwav) | `no_std` **metadata** parser (~100 lines). Not a codec. |
| [`pure_wav`](https://crates.io/crates/pure_wav) | `no_std` header walker. AGPL. Not a codec. |
| [`rezin-wav`](https://crates.io/crates/rezin-wav) | Zero-dep PCM 16/24 to `i32` stream. No f32, no telephony. |
| [`pacmog`](https://crates.io/crates/pacmog) | Embedded `include_bytes!` PCM/IMA player. MCU, not ingest. |

WAVE-family or multi-format (overlap, different product):

| Crate | Why not in the harness |
|---|---|
| [`audio_samples_io`](https://crates.io/crates/audio_samples_io) | Successor of wavers. WAV (RF64/BW64) + FLAC + AIFF. Path / `Read+Seek`. Extra crates (`audio_samples`, ...). |
| [`bwavfile`](https://crates.io/crates/bwavfile) | Broadcast-WAV / RF64 / iXML / ADM. Film metadata, not G.711/ADPCM. Last release 2023. |
| [`oxideav-basic`](https://crates.io/crates/oxideav-basic) | Closest **family** overlap: RIFF + RF64/BW64, WAVEFORMATEXTENSIBLE, G.711 dispatch. 0.0.x media stack, many crates. |
| [`oxideav-g711`](https://crates.io/crates/oxideav-g711) / [`oxideav-adpcm`](https://crates.io/crates/oxideav-adpcm) | Standalone codecs for that stack. Not a WAVE ingest crate. |
| [`decibri-decode`](https://crates.io/crates/decibri-decode) | WAV + AIFF + FLAC + headerless PCM/G.711. Broader than WAVE. |
| [`rustwav`](https://crates.io/crates/rustwav) | Kitchen-sink (MP3/Opus/FLAC + resample). Custom license, many deps. |
| [`shravan`](https://crates.io/crates/shravan) | WAV+FLAC+AIFF+... GPL-3. Different product. |
| [`rff-format-wav`](https://crates.io/crates/rff-format-wav) | WAV demux in a "remade ffmpeg" stack. PCM via a sibling crate. |
| [`audio-file`](https://crates.io/crates/audio-file) / [`symphonium`](https://crates.io/crates/symphonium) / [`audrey`](https://github.com/RustAudio/audrey) | Multi-format loaders on Symphonia / hound. |
| [`creek`](https://crates.io/crates/creek) | Realtime disk streamer; decode is Symphonia. |
| [`dasp-rs`](https://crates.io/crates/dasp-rs) | DSP/MIR; WAV I/O is a side door (and it resamples). |
| [`oxiaudio-encode`](https://crates.io/crates/oxiaudio-encode) | WAV encode **via hound**. |
| [`audio-codec-algorithms`](https://crates.io/crates/audio-codec-algorithms) / [`audio-codec`](https://crates.io/crates/audio-codec) / [`ezk-g711`](https://crates.io/crates/ezk-g711) | A-law/mu-law/G.722 **bitstreams**, no RIFF. ryf decodes G.711/G.722/GSM in-tree. |

Not codecs: [rodio](https://crates.io/crates/rodio) (playback via hound/symphonia),
[cpal](https://crates.io/crates/cpal) (host I/O),
[maudio](https://crates.io/crates/maudio) (miniaudio bindings - C).

If a crate is a WAV **library** aimed at PCM RIFF or the WAVE family, it
belongs in the speed table or in these lists with a reason. Gaps: open an
issue.
