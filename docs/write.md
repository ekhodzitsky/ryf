# Write

Little-endian WAVE. Classic RIFF when sizes fit in `u32`; **RF64** when they
do not (or via `encode_rf64` / `WavWriter::new_rf64`). No RIFX, no ADPCM
encode. Channels `1..=26` (same ceiling as decode). Empty payload is a valid
header (44-byte integer PCM, 58-byte f32). `WavWriter::new` auto-promotes
to RF64 when the payload would overflow `u32` (same rule as `encode`).
`new_rf64` forces RF64 from the first byte.

| | Notes |
|---|---|
| `encode` / `write` + `WriteSpec` | U8, S16, packed S24, S32, IEEE f32; auto RF64 |
| `encode_rf64` / `write_rf64` | force RF64 (`ds64`) |
| `encode_s16` / `write_s16` | molv drop-in, PCM16 **mono** |
| `encode_f32` / `write_f32` | interleaved f32 |
| `WavWriter` / `new_rf64` / `create_rf64` | streaming; sizes patched on `finalize` or drop |

No `WAVEFORMATEXTENSIBLE` on write (plain `fmt ` only).

Decode and `f32_to_s16le` share the s16 scale `/ 32768` (`* 32768` then
clamp to `i16`). `-1.0` encodes as `-32768`; `+1.0` clamps to `32767`.
Round-trip is lossless on length and rate; `+1.0` is not bit-exact.
