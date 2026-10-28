# Public API review before 1.0

Reviewed after 0.7.4. This is the compatibility baseline for further review;
it does not declare a 1.0 release or change the WAVE acceptance policy.

## Audio and container contracts

Decode returns planar f32 at native rate. Split is the library default;
mono mixing sums channels left to right and divides by their count. G.722
emits 16000 Hz, G.726 emits 8000 Hz. No resampling is performed.
PCM16 byte helpers are mono and strict about odd payloads. General planar
decode discards an incomplete PCM frame. Empty planar WAVE succeeds with
empty planes; raw helpers and headerless telephony reject empty output.

Probe is metadata inspection, not a successful-decode certificate. It checks
codec support, enabled features and sample-rate policy, but does not enforce
duration or output-byte budgets or validate every compressed block. Smaller
positive fact/ds64 counts trim available frames; larger counts do not create
audio. A length snapshot can become stale if the underlying file changes.

## Limits and errors

Use speech options or explicit limits for untrusted inputs. The default
archival options are generous and are not a process-memory quota.
Duration uses floor(seconds * min(file rate, decode-rate ceiling)), with an
effective rate of at least 1. Nonpositive or nonfinite duration products
produce a zero-frame budget. Existing builders remain infallible; changing
invalid-option handling requires an explicit compatibility decision.
TooLong expresses both fields in actual output seconds: max_secs is the
effective frame budget divided by the native output rate. It can therefore
be smaller than max_duration_secs after the rate ceiling or frame rounding.

max_output_bytes counts logical planar output across channels. Streaming
enforces this logical budget too, even though it retains only bounded blocks.
Collect additionally rejects any individual allocation exceeding isize::MAX
bytes. OutputTooLarge reports the limit exceeded, which can be the platform
limit rather than the configured budget. Allocator exhaustion below these
limits is not guaranteed to become a recoverable error. decode_reader also
enforces a separate input ceiling of max_output_bytes + 1 MiB (saturating).
It buffers at most that ceiling plus one byte and rejects a larger input
before decoding. OutputTooLarge.max then reports the input ceiling. A large
multichannel input can exceed it even when mixed output fits the planar
budget; decode_bytes and seekable sources do not have this input ceiling.
Raw PCM helpers do not accept DecodeOptions.

Match typed errors and their fields rather than parsing Display strings.
UnexpectedEof during packet reads is Format(Truncated); other transport
errors retain Io and its original source. Multiple invalid properties need
not have a universally fixed error precedence. Public non_exhaustive types
require fallback matches. ChannelMode and G711Law remain closed enums.

## Source and incremental state

Fallible ByteSource constructors report initialization failures. Original
constructors retain best-effort length discovery and rewind for compatibility.
Probe and sniff attempt rewind on both success and error; a failed rewind
cannot guarantee cursor zero. When parsing already failed, its error wins.
Streaming errors can follow delivered blocks and cursor movement. A complete
retry requires an explicit rewind, and callback slices live only for the call.

G726Decoder push can retain complete codewords when its output fills. Drain
with empty input before finish; finish discards all pending bits and keeps
the predictor, while reset restores both predictor and input state.

## Writer lifecycle

Path constructors validate before truncation. Generic constructors require
cursor zero and an empty sink; they cannot truncate pre-existing trailing
bytes. RIFX accepts little-endian input and emits big-endian samples.

Successful writes commit data_bytes. Any mutating I/O error makes the writer
terminal, including promotion and finalize failures. Validation before I/O
leaves it usable. finalize patches sizes and flushes; successful repeats are
harmless. Drop attempts completion only while open and cannot report errors.
There is no rollback, atomic replacement or durable-storage guarantee.

RIFF promotion relocates the existing recording and requires read access.
Use forced RF64 for predictable long-recording latency. Compact RIFF headers
remain the default. Reserved JUNK, weaker writer trait bounds, sink extraction
and new codec encoders are separate API proposals, not readiness requirements.

## Evidence and remaining limits

The real-file tests cross both classic and extensible RIFF size thresholds,
write beyond 2^32 payload bytes, verify every PCM byte and ds64 sizes, and
exercise a partial relocation error at the actual threshold. Large files
are temporary and excluded from normal tests and git.

The portable check executes public conversions and all four writable
containers with 1, 2, 3 and 26 channels, scalar/SIMD comparisons, ADPCM fixtures
and every PCM16 conversion value. Its 32-bit regressions check an oversized
collect request and bounded streaming from an 8 GiB virtual source, without
allocating the whole input or output. QEMU execution
checks semantics, not native hardware timing or complete OS behavior.

Bounded sanitizer fuzz campaigns and Miri complement the regression suite.
Neither is a proof of all inputs. Native ARM execution is additionally covered
by macOS ARM64 CI. Big-endian execution, allocator-failure injection and a
sustained external fuzz service remain
additional validation opportunities. Keep the existing API contracts stable
unless a proposed change has a specific compatibility rationale.
