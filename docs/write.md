# Write

Little-endian WAVE. Classic RIFF when sizes fit in `u32`; **RF64** when they
do not (or via `encode_rf64` / `WavWriter::new_rf64`). No RIFX, no ADPCM
encode. Channels `1..=26` (same ceiling as decode). Empty payload is a valid
header (44-byte integer PCM, 58-byte f32). `WavWriter::new` does **not**
auto-upgrade: past 4 GiB it returns `RiffTooLarge`; use `new_rf64`.

| | Notes |
|---|---|
| `encode` / `write` + `WriteSpec` | U8, S16, packed S24, S32, IEEE f32; auto RF64 |
| `encode_rf64` / `write_rf64` | force RF64 (`ds64`) |
| `encode_s16` / `write_s16` | molv drop-in, PCM16 **mono** |
| `encode_f32` / `write_f32` | interleaved f32 |
| `WavWriter` / `new_rf64` / `create_rf64` | streaming; sizes patched on `finalize` or drop |

No `WAVEFORMATEXTENSIBLE` on write (plain `fmt ` only).

Decode scale for integer PCM is `/ 32768` (s16), `/ 2^23` (s24), `/ 2^31`
(s32). Encode pack `f32_to_s16le` uses peak **32767** (`±1.0 → ±32767`).
That is the molv write path; it is **not** the inverse of decode.
Round-trip through encode then decode is lossless on *length and rate*,
not bit-exact on sample values.
