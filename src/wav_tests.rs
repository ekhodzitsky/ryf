//! Differential tests: this decoder must produce BIT-EXACT f32 output
//! with the ffmpeg CLI decoder on every well-formed input
//! (PCM/µ-law/A-law/float WAV are lossless). The ffmpeg oracle decodes
//! through a temp file; per-channel output is mixed in-test with the
//! same left-to-right sum / n arithmetic the decoder uses for
//! [`ChannelMode::Mono`]. Comparison is at the file's native sample
//! rate — product resampling (16 kHz) lives in `the consumer`.
//!
//! Documented, deliberate divergences from the old symphonia pipeline
//! (malformed input only, Err-vs-Ok):
//!
//! 1. `data` chunk declares more bytes than the file contains: symphonia
//!    errored on the short read (`Error reading packet`), the in-tree
//!    decoder clamps to the real bytes and decodes the frames that exist
//!    (per the roadmap contract).
//! 2. `data` chunk length is the streaming marker `u32::MAX`: symphonia
//!    read until EOF and errored on the final short read; the in-tree
//!    decoder clamps to the real bytes and succeeds.
//! 3. WAVE_FORMAT_EXTENSIBLE fmt chunks longer than 40 bytes: symphonia
//!    left the surplus unread and desynchronized its chunk walk (it
//!    then errored out); the in-tree decoder skips the surplus and
//!    succeeds.
//! 4. Malformed LIST/INFO internals: symphonia parses INFO sub-chunks
//!    and errors; the in-tree decoder skips list bodies (metadata is
//!    never consumed by the decode path) and succeeds.
//!
//! In every other case: well-formed input is gated bit-exact against
//! ffmpeg; own-Err on a well-formed input is a test failure.

#[path = "wav_tests_adpcm.rs"]
mod adpcm;
#[path = "wav_tests_builders.rs"]
mod builders;
#[path = "wav_tests_containers.rs"]
mod containers;
#[path = "wav_tests_ffmpeg.rs"]
mod ffmpeg;
#[path = "wav_tests_ffmpeg_ext.rs"]
mod ffmpeg_ext;
#[path = "wav_tests_more.rs"]
mod more;
#[path = "wav_tests_oracle.rs"]
mod oracle;
#[path = "wav_tests_seed.rs"]
mod seed;

pub(crate) use adpcm::adpcm_fixture;
pub(crate) use builders::*;
pub(crate) use oracle::*;
pub(crate) use seed::*;
