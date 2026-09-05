# Read

Every listed codec is accepted in every listed container unless noted.

| Codec | RIFF | RIFX (BE) | RF64 / BW64 | Wave64 |
|---|---|---|---|---|
| PCM u8 / s16 / s24 / s32 | yes | yes | yes | yes |
| S24 in 4-byte containers | yes | yes | yes | yes |
| IEEE f32 / f64 | yes | yes | yes | yes |
| G.711 A-law / mu-law | yes | yes | yes | yes |
| G.722 64 kbit/s only (tags `0x0064` / `0x0065` / `0x028F`) | yes | yes | yes | yes |
| GSM 06.10 / wav49 (`0x0031`, 65-byte blocks) | yes | yes | yes | yes |
| MS-ADPCM, IMA/DVI ADPCM | yes | yes | yes | yes |

Also:

- `WAVE_FORMAT_EXTENSIBLE` (PCM / IEEE / G.711 + Ambisonic GUIDs).
  Numeric `fmt ` fields follow the container endian. SubFormat GUID is
  always the Microsoft on-disk 16 bytes (including RIFX).
- Wild headers: `valid_bits = 0`, empty channel mask, mask bits 18+ /
  `SPEAKER_ALL`, short `data`, `data` before `fmt `, first `fmt ` wins,
  RIFF size 0, `fact` longer than 4 bytes, streaming `u32::MAX` sizes,
  `fmt ` surplus after the 16-byte core (PCM 20, Wave64 pad, IEEE 17/18
  with wild `cbSize`, IMA extra longer than 2, extensible `cbSize`
  longer than 22)
- `TooLong` is decided from **bytes on disk**, not a lying `fact` / `ds64`
- RF64 `ds64` size table (EBU Tech 3306) sizes non-`data` `0xFFFFFFFF`
  chunks. Wave64 `data` length is `chunk_size - 24`; pad bytes stuffed
  into that size are PCM (ffmpeg)

Headerless G.711 (rate and channel count stated by the caller):
`decode_g711` / `decode_g711_alaw` / `decode_g711_mulaw`.

Headerless G.722 (**64 kbit/s only**; `sample_rate` 8000 or 16000, output
always 16 kHz): `decode_g722` / `decode_g722_mono`. 56/48 kbit/s packed
streams are not decoded. WAVE G.722 always reports 16 kHz (SDP `fmt ` 8000
or any other accepted header rate). `0x0064` is the Asterisk/SBC alias
(mmreg lists that tag as G.726).

Headerless Microsoft GSM 06.10 / wav49 (`sample_rate` 8000, 65-byte
blocks): `decode_gsm` / `decode_gsm_mono`. WAVE tag `0x0031`. Output keeps
the `fmt ` rate (8 kHz on Asterisk wav49). 33-byte toast frames and MSN
variable block sizes (41-64) are not decoded.

From a path: `read` (split, archival caps) / `read_speech` (mix, 2 h;
duration uses min(file rate, 48 kHz)) / `read_with`. Mono PCM16 / mixed
f32 helpers: `read_s16` / `read_f32`. From a `File` or any
`Read + Seek + Send`: `ByteSource::from_file` / `from_read_seek` +
`decode_with`. From a buffer: `decode_bytes`. From `impl Read` (slurp):
`decode_reader`. Streaming: `decode_streaming`. Probe: `probe` (library
defaults) / `probe_with`. Sniff: `sniff_wav`.

`decode_bytes` / `read` of an empty `data` chunk return 0 frames (one
empty plane if mixed). `decode_f32` / `decode_s16` / headerless G.711 /
G.722 / GSM return `WavError::Empty`.
