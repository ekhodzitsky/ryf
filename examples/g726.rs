//! Headerless G.726 at 32 kbit/s, RFC 3551 packing.
//!
//! ```sh
//! cargo run --example g726
//! ```

fn main() -> ryf::Result<()> {
    let pcm = ryf::decode_g726(
        &[0x00, 0x11],
        ryf::G726Rate::Kbps32,
        ryf::G726Packing::Rfc3551,
        Some(4),
        &ryf::DecodeOptions::speech(),
    )?;
    println!("{} Hz, {} frames", pcm.sample_rate, pcm.frames());
    Ok(())
}
