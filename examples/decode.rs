//! Decode WAVE to planar f32.
//!
//! ```sh
//! cargo run --example decode
//! cargo run --example decode -- speech.wav
//! ```

fn main() -> ryf::Result<()> {
    let decoded = if let Some(path) = std::env::args().nth(1) {
        ryf::read(path)?
    } else {
        let wav = ryf::encode_s16(&ryf::f32_to_s16le(&[0.25, -0.5, 0.0]), 16_000)?;
        ryf::decode_bytes(&wav, ryf::DecodeOptions::speech())?
    };
    println!(
        "{} Hz, {} plane(s), {} frames",
        decoded.sample_rate,
        decoded.num_channels(),
        decoded.frames()
    );
    Ok(())
}
