# Read

Every listed codec is accepted in every listed container unless noted.

| Codec | RIFF | RIFX (BE) | RF64 / BW64 | Wave64 |
|---|---|---|---|---|
| PCM u8 / s16 / s24 / s32 | yes | yes | yes | yes |
| S24 in 4-byte containers | yes | yes | yes | yes |
| IEEE f32 / f64 | yes | yes | yes | yes |
| G.711 A-law / µ-law | yes | yes | yes | yes |
| MS-ADPCM, IMA/DVI ADPCM | yes | — | yes | yes |

Also:

- `WAVE_FORMAT_EXTENSIBLE` (PCM / IEEE / G.711 + Ambisonic GUIDs)
- Wild headers: `valid_bits = 0`, empty channel mask, short `data`,
  streaming `u32::MAX` sizes, PCM fmt sizes 16 / 18 / 40
- `TooLong` is decided from **bytes on disk**, not a lying `fact` / `ds64`

Headerless G.711 (rate and channel count stated by the caller):
`decode_g711` / `decode_g711_alaw` / `decode_g711_mulaw`.

From a path: `read` (split, archival caps) / `read_speech` (mix, 2 h) /
`read_with`. Molv drop-ins: `read_s16` / `read_f32`. From a `File` or any
`Read + Seek + Send`: `ByteSource::from_file` / `from_read_seek` +
`decode_with`. From a buffer: `decode_bytes`. From `impl Read` (slurp):
`decode_reader`. Streaming: `decode_streaming`.

Empty decode is `WavError::Empty`.
