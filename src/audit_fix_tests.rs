use crate::{ByteSource, ChannelMode, DecodeOptions, Result, WavError};
#[cfg(feature = "adpcm")]
use std::io::{self, Cursor, Read, Seek, SeekFrom};

fn duration_error<T: std::fmt::Debug>(result: Result<T>, observed: f64, maximum: f64) {
    match result {
        Err(WavError::TooLong {
            observed_secs,
            max_secs,
        }) => {
            assert_eq!(observed_secs, observed);
            assert_eq!(max_secs, maximum);
            assert!(observed_secs > max_secs);
        }
        other => panic!("expected TooLong, got {other:?}"),
    }
}

#[test]
fn duration_errors_use_the_effective_frame_budget() -> Result<()> {
    let opts = DecodeOptions::default()
        .with_max_duration_secs(1.0)
        .with_max_decode_sample_rate(8000);
    let wav = crate::encode_s16(&vec![0; 12000 * 2], 16000)?;
    duration_error(crate::decode_bytes(&wav, opts.clone()), 0.75, 0.5);
    duration_error(
        crate::decode_streaming(&mut ByteSource::from_slice(&wav), &opts, |_| Ok(())),
        0.75,
        0.5,
    );
    duration_error(
        crate::decode_g711(&vec![0; 12000], crate::G711Law::MuLaw, 16000, 1, &opts),
        0.75,
        0.5,
    );
    duration_error(
        crate::decode_g722(&vec![0; 6000], 8000, 1, &opts),
        0.75,
        0.5,
    );
    let opts = opts.with_max_decode_sample_rate(4000);
    duration_error(
        crate::decode_g726(
            &vec![0; 1500],
            crate::G726Rate::Kbps16,
            crate::G726Packing::Aal2,
            None,
            &opts,
        ),
        0.75,
        0.5,
    );
    duration_error(crate::decode_gsm(&vec![0; 15 * 65], 8000, &opts), 0.6, 0.5);
    // A fractional frame is not part of the effective budget either.
    let wav = crate::encode_s16(&[0; 4], 3)?;
    let opts = opts.with_max_duration_secs(0.5);
    duration_error(crate::decode_bytes(&wav, opts), 2.0 / 3.0, 1.0 / 3.0);
    Ok(())
}

#[test]
fn reader_input_ceiling_is_distinct_from_planar_output() -> Result<()> {
    let frames = 40960;
    let wav = crate::encode(crate::WriteSpec::s16(8000, 26), &vec![0; frames * 52])?;
    let opts = DecodeOptions::default()
        .with_channel_mode(ChannelMode::Mono)
        .with_max_output_bytes(262144);
    assert_eq!(crate::decode_bytes(&wav, opts.clone())?.frames(), frames);
    let mut reader = std::io::Cursor::new(&wav);
    let cap = opts.max_output_bytes + 1024 * 1024;
    assert!(
        matches!(crate::decode_reader(&mut reader, &opts), Err(WavError::OutputTooLarge { bytes, max }) if bytes == cap + 1 && max == cap)
    );
    assert_eq!(reader.position(), cap + 1);
    assert!(reader.position() < wav.len() as u64);
    Ok(())
}

#[cfg(feature = "adpcm")]
fn adpcm(ima: bool, channels: u16, target: usize) -> (Vec<u8>, usize) {
    fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], bytes: &[u8]) {
        out.extend_from_slice(tag);
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    let block = usize::from(channels) * 8;
    let spb = if ima { 9 } else { 4 };
    let mut fmt = Vec::new();
    for n in [if ima { 17u16 } else { 2 }, channels] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    for n in [8000u32, 8000 * block as u32 / spb as u32] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    for n in [block as u16, 4, if ima { 2 } else { 8 }, spb as u16] {
        fmt.extend_from_slice(&n.to_le_bytes());
    }
    if !ima {
        for n in [1i16, 256, 0] {
            fmt.extend_from_slice(&n.to_le_bytes());
        }
    }
    let mut data = vec![0; block * 3];
    if !ima {
        data[block * 2] = 255;
    } // A malformed block beyond the logical end.
    let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
    chunk(&mut wav, b"fmt ", &fmt);
    chunk(&mut wav, b"fact", &(target as u32).to_le_bytes());
    let data_pos = wav.len() + 8;
    chunk(&mut wav, b"data", &data);
    let size = wav.len() as u32 - 8;
    wav[4..8].copy_from_slice(&size.to_le_bytes());
    (wav, data_pos + target.div_ceil(spb) * block)
}

#[cfg(feature = "adpcm")]
struct Fence {
    bytes: Cursor<Vec<u8>>,
    end: u64,
}
#[cfg(feature = "adpcm")]
impl Read for Fence {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let left = self.end.saturating_sub(self.bytes.position()) as usize;
        if left == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "read past fact target",
            ));
        }
        let n = bytes.len().min(left);
        self.bytes.read(&mut bytes[..n])
    }
}
#[cfg(feature = "adpcm")]
impl Seek for Fence {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.bytes.seek(position)
    }
}

#[cfg(feature = "adpcm")]
#[test]
fn adpcm_fact_boundaries_do_not_read_discarded_blocks() -> Result<()> {
    for ima in [false, true] {
        for channels in [1, 2] {
            let spb = if ima { 9 } else { 4 };
            for target in [1, spb - 1, spb, spb + 1, spb * 2] {
                let (wav, end) = adpcm(ima, channels, target);
                for mode in [ChannelMode::Mono, ChannelMode::Split] {
                    let opts = DecodeOptions::default()
                        .with_channel_mode(mode)
                        .with_max_duration_secs((target as f64 + 0.25) / 8000.0);
                    assert_eq!(crate::decode_bytes(&wav, opts.clone())?.frames(), target);
                    for reader in [false, true] {
                        let mut source = if reader {
                            ByteSource::try_from_read_seek(
                                Fence {
                                    bytes: Cursor::new(wav.clone()),
                                    end: end as u64,
                                },
                                None,
                            )?
                        } else {
                            ByteSource::from_slice(&wav)
                        };
                        assert_eq!(crate::decode_with(&mut source, &opts)?.frames(), target);
                        assert_eq!(source.pos(), end as u64);
                        source.rewind()?;
                        let mut delivered = 0;
                        let info = crate::decode_streaming(&mut source, &opts, |block| {
                            delivered += block.frames;
                            Ok(())
                        })?;
                        assert_eq!(info.frames, target);
                        assert_eq!(delivered, target);
                        assert_eq!(source.pos(), end as u64);
                    }
                }
            }
        }
    }
    Ok(())
}
