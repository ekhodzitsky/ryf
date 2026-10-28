# Write

Little-endian WAVE, or RIFX via `encode_rifx`. Classic RIFF when sizes fit
in `u32`; **RF64** when they do not (or via `encode_rf64` /
`WavWriter::new_rf64`). G.711 A-law / mu-law write: `encode_alaw` /
`encode_mulaw`. `WAVEFORMATEXTENSIBLE` PCM/IEEE/G.711: `encode_extensible` /
`WavWriter::new_extensible`. `encode_extensible` uses RF64 when `u32`
sizes overflow. No ADPCM / G.722 / G.726 / GSM encode. Channels `1..=26` (same
ceiling as decode). Empty payload is a valid header (44-byte integer PCM,
58-byte f32 / G.711).
`WavWriter::new` and `new_extensible` auto-promote to RF64 when the payload
would overflow `u32` (same rule as `encode` / `encode_extensible`).
`new_rf64` forces classic RF64 from the first byte.

| | Notes |
|---|---|
| `encode` / `write` + `WriteSpec` | U8, S16, packed S24, S32, IEEE f32, A-law, mu-law; auto RF64 |
| `encode_rf64` / `write_rf64` | force RF64 (`ds64`) |
| `encode_rifx` / `WavWriter::new_rifx` | big-endian RIFX (PCM / IEEE / G.711) |
| `encode_extensible` / `WavWriter::new_extensible` | `WAVEFORMATEXTENSIBLE` PCM/IEEE/G.711; auto-RF64 |
| `encode_alaw` / `encode_mulaw` | compress interleaved f32 to G.711 WAVE |
| `encode_s16` / `write_s16` | PCM16 **mono** helper |
| `encode_f32` / `write_f32` | interleaved f32 |
| `WavWriter` / `new_rf64` / `new_rifx` / `new_extensible` | streaming; sizes patched on `finalize` or drop |

Decode and `f32_to_s16le` share the s16 scale `/ 32768` (`* 32768` then
clamp to `i16`). `-1.0` encodes as `-32768`; `+1.0` clamps to `32767`.
Round-trip is lossless on length and rate; `+1.0` is not bit-exact.

`WavWriter::create*` validates the complete header before opening or truncating
the destination. RIFF and extensible path writers open the file for reading
and writing because promotion relocates existing payload bytes. Promotion is
linear in the existing recording size; use `new_rf64` / `create_rf64` up front
when long-recording latency matters. The default RIFF header remains compact.

Generic `new*` constructors require cursor zero and reject another origin
before writing. Supply an empty sink; generic sinks cannot be truncated and
pre-existing trailing bytes are not removed. Successful finalization leaves
the cursor immediately after the WAVE payload and its alignment pad.

A partial I/O failure is terminal. Further writes and `finalize` return
`InvalidOperation`, and Drop does not retry finalization. Discard that incomplete
file: no rollback or durability guarantee is implied. Validation failures
before I/O leave an existing writer usable. Always call `finalize` explicitly
to observe completion errors; repeated successful calls are harmless.

G.711 encoding quantizes PCM16 on a 14-bit grid using reconstruction-level
midpoints, with ties selecting the higher magnitude. Negative inputs round
down to the grid. All PCM16 inputs are checked against fixed ffmpeg 7.0.2
encoder fingerprints, including sign and segment boundaries.
