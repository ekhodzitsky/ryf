//! Write a tiny PCM16 WAVE file.
//!
//! ```sh
//! cargo run --example encode -- out.wav
//! ```

fn main() -> ryf::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "out.wav".into());
    let pcm = ryf::f32_to_s16le(&[0.0, 0.25, -0.5]);
    ryf::write_s16(path.as_ref(), &pcm, 16_000)?;
    println!("wrote {path} (16 kHz PCM16 mono)");
    Ok(())
}
