//! `fmt ` chunk body (PCM / IEEE / G.711 / ADPCM / EXTENSIBLE).

use super::riff::{read_u16_endian, read_u32_endian};
use super::{
    FmtFields, KSDATAFORMAT_SUBTYPE_ALAW, KSDATAFORMAT_SUBTYPE_AMBISONIC_IEEE_FLOAT,
    KSDATAFORMAT_SUBTYPE_AMBISONIC_PCM, KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
    KSDATAFORMAT_SUBTYPE_MULAW, KSDATAFORMAT_SUBTYPE_PCM, SampleCodec, WAVE_FORMAT_ADPCM_IMA,
    WAVE_FORMAT_ADPCM_MS, WAVE_FORMAT_ALAW, WAVE_FORMAT_EXTENSIBLE, WAVE_FORMAT_IEEE_FLOAT,
    WAVE_FORMAT_MULAW, WAVE_FORMAT_PCM, container_width, fix_wave_channel_mask,
    map_ambisonic_channel_count, map_wave_channel_count, pcm_codec_for,
};
use crate::error::{Result, WavError};
use crate::source::ByteSource;

#[cfg(feature = "adpcm")]
use crate::adpcm::{ImaAdpcmParams, MsAdpcmParams};

#[cfg(not(feature = "adpcm"))]
use super::adpcm_types::{ImaAdpcmParams, MsAdpcmParams};

pub(super) fn parse_fmt_chunk(
    mss: &mut ByteSource<'_>,
    chunk_len: u32,
    big_endian: bool,
) -> Result<FmtFields> {
    if chunk_len < 16 {
        return Err(WavError::format("wav: malformed fmt chunk"));
    }

    let format = read_u16_endian(mss, big_endian)?;
    let num_channels = read_u16_endian(mss, big_endian)?;
    let sample_rate = read_u32_endian(mss, big_endian)?;
    let _avg_bytes_per_sec = read_u32_endian(mss, big_endian)?;
    let block_align = read_u16_endian(mss, big_endian)?;
    let bits_per_sample = read_u16_endian(mss, big_endian)?;

    match format {
        WAVE_FORMAT_PCM => {
            // Canonical lengths (same as historical path); 40 is a common wild size.
            match chunk_len {
                16 => {}
                18 => {
                    let _cb = read_u16_endian(mss, big_endian)?;
                }
                40 => {
                    mss.ignore_bytes(24)?;
                }
                _ => return Err(WavError::format("wav: malformed fmt_pcm chunk")),
            }

            let width = container_width(block_align, num_channels, bits_per_sample)?;
            let (codec, width) = pcm_codec_for(bits_per_sample, width)?;
            let channels = map_wave_channel_count(num_channels)?;
            Ok(FmtFields {
                codec,
                channels,
                sample_rate,
                sample_width: width,
                adpcm_ms: None,
                adpcm_ima: None,
                big_endian: false,
            })
        }
        WAVE_FORMAT_IEEE_FLOAT => {
            match chunk_len {
                16 => {}
                18 => {
                    let extra_size = read_u16_endian(mss, big_endian)?;
                    if extra_size != 0 {
                        return Err(WavError::format(
                            "wav: extra data not expected for fmt_ieee chunk",
                        ));
                    }
                }
                40 => {
                    mss.ignore_bytes(24)?;
                }
                n if n > 18 => {
                    mss.ignore_bytes(u64::from(n - 16))?;
                }
                _ => return Err(WavError::format("wav: malformed fmt_ieee chunk")),
            }

            let width = container_width(block_align, num_channels, bits_per_sample)?;
            let (codec, width) = match (bits_per_sample, width) {
                (32, 4) => (SampleCodec::F32, 4),
                (64, 8) => (SampleCodec::F64, 8),
                (32, w) if w >= 4 => (SampleCodec::F32, w),
                (64, w) if w >= 8 => (SampleCodec::F64, w),
                _ => {
                    return Err(WavError::format(
                        "wav: bits per sample for fmt_ieee must be 32 or 64 bits",
                    ));
                }
            };

            let channels = map_wave_channel_count(num_channels)?;
            Ok(FmtFields {
                codec,
                channels,
                sample_rate,
                sample_width: width,
                adpcm_ms: None,
                adpcm_ima: None,
                big_endian: false,
            })
        }
        WAVE_FORMAT_ALAW | WAVE_FORMAT_MULAW => {
            // Canonical is 18; accept 16 and longer wild sizes.
            if chunk_len < 16 {
                return Err(WavError::format("wav: malformed fmt_alaw chunk"));
            }
            if chunk_len >= 18 {
                let extra_size = read_u16_endian(mss, big_endian)?;
                let already = 18u32;
                let rem = chunk_len.saturating_sub(already);
                let skip = u64::from(extra_size).min(u64::from(rem));
                if skip > 0 {
                    mss.ignore_bytes(skip)?;
                }
                if rem > extra_size as u32 {
                    mss.ignore_bytes(u64::from(rem - u32::from(extra_size)))?;
                }
            } else if chunk_len > 16 {
                mss.ignore_bytes(u64::from(chunk_len - 16))?;
            }
            let codec = if format == WAVE_FORMAT_ALAW {
                SampleCodec::ALaw
            } else {
                SampleCodec::MuLaw
            };
            let channels = map_wave_channel_count(num_channels)?;
            Ok(FmtFields {
                codec,
                channels,
                sample_rate,
                sample_width: 1,
                adpcm_ms: None,
                adpcm_ima: None,
                big_endian: false,
            })
        }
        WAVE_FORMAT_ADPCM_MS | WAVE_FORMAT_ADPCM_IMA => {
            if bits_per_sample != 4 {
                return Err(WavError::format(
                    "wav: bits per sample for fmt_adpcm must be 4 bits",
                ));
            }
            if chunk_len < 20 {
                return Err(WavError::format("wav: malformed fmt_adpcm chunk"));
            }
            let channels = map_wave_channel_count(num_channels)?;
            if channels > 2 {
                return Err(WavError::format("wav: ADPCM supports at most 2 channels"));
            }
            if block_align == 0 {
                return Err(WavError::format("wav: ADPCM block_align is zero"));
            }
            let extra_size = u64::from(read_u16_endian(mss, big_endian)?);
            match format {
                WAVE_FORMAT_ADPCM_MS => {
                    // samplesPerBlock (2) + numCoefs (2) + coefs (4 * n)
                    if extra_size < 32 {
                        return Err(WavError::format("wav: malformed fmt_adpcm chunk"));
                    }
                    let samples_per_block = read_u16_endian(mss, big_endian)?;
                    let num_coefs = read_u16_endian(mss, big_endian)?;
                    if num_coefs == 0 || num_coefs > 256 {
                        return Err(WavError::format("wav: MS-ADPCM invalid coefficient count"));
                    }
                    let coef_bytes = u64::from(num_coefs) * 4;
                    if extra_size < 4 + coef_bytes {
                        return Err(WavError::format(
                            "wav: MS-ADPCM coefficient table truncated",
                        ));
                    }
                    let mut coefs = Vec::with_capacity(num_coefs as usize);
                    for _ in 0..num_coefs {
                        let c1 = read_u16_endian(mss, big_endian)? as i16;
                        let c2 = read_u16_endian(mss, big_endian)? as i16;
                        coefs.push((c1, c2));
                    }
                    let rest = extra_size - 4 - coef_bytes;
                    if rest > 0 {
                        mss.ignore_bytes(rest)?;
                    }
                    Ok(FmtFields {
                        codec: SampleCodec::MsAdpcm,
                        channels,
                        sample_rate,
                        sample_width: 1, // unused for ADPCM; blocks drive layout
                        adpcm_ms: Some(MsAdpcmParams {
                            block_align,
                            samples_per_block,
                            channels,
                            coefs,
                        }),
                        adpcm_ima: None,
                        big_endian: false,
                    })
                }
                WAVE_FORMAT_ADPCM_IMA => {
                    if extra_size != 2 {
                        return Err(WavError::format("wav: malformed fmt_adpcm chunk"));
                    }
                    let samples_per_block = read_u16_endian(mss, big_endian)?;
                    Ok(FmtFields {
                        codec: SampleCodec::ImaAdpcm,
                        channels,
                        sample_rate,
                        sample_width: 1,
                        adpcm_ms: None,
                        adpcm_ima: Some(ImaAdpcmParams {
                            block_align,
                            samples_per_block,
                            channels,
                        }),
                        big_endian: false,
                    })
                }
                _ => unreachable!(),
            }
        }
        WAVE_FORMAT_EXTENSIBLE => {
            if chunk_len < 40 {
                return Err(WavError::format("wav: malformed fmt_ext chunk"));
            }

            let extra_size = read_u16_endian(mss, big_endian)?;
            if extra_size != 22 {
                return Err(WavError::format(
                    "wav: extra data size not 22 bytes for fmt_ext chunk",
                ));
            }

            let mut valid_bits_per_sample = read_u16_endian(mss, big_endian)?;
            // Wild files: valid_bits == 0 means "use container bits" (hound).
            if valid_bits_per_sample == 0 {
                valid_bits_per_sample = bits_per_sample;
            }

            if bits_per_sample & 0x7 != 0 {
                return Err(WavError::format(
                    "wav: bits per coded sample for fmt_ext must be a multiple of 8",
                ));
            }

            let channel_mask = read_u32_endian(mss, big_endian)?;

            let mut sub_format_guid = [0u8; 16];
            mss.read_buf_exact(&mut sub_format_guid)?;

            if chunk_len > 40 {
                mss.ignore_bytes(u64::from(chunk_len - 40))?;
            }

            let is_ambisonic = matches!(
                sub_format_guid,
                KSDATAFORMAT_SUBTYPE_AMBISONIC_PCM | KSDATAFORMAT_SUBTYPE_AMBISONIC_IEEE_FLOAT
            );

            let container_bits = bits_per_sample;
            let width = container_width(block_align, num_channels, container_bits)?;

            let (codec, width) = match sub_format_guid {
                KSDATAFORMAT_SUBTYPE_PCM | KSDATAFORMAT_SUBTYPE_AMBISONIC_PCM => {
                    if valid_bits_per_sample > container_bits {
                        return Err(WavError::format(
                            "wav: valid bits per sample for fmt_ext PCM sub-type must be <= bits per sample",
                        ));
                    }
                    // Layout is driven by the container (`wBitsPerSample` +
                    // `block_align`). Partial `wValidBitsPerSample` is accepted
                    // but ignored for conversion (historical bit-exact path):
                    // e.g. valid=24 in a 32-bit container still decodes as S32.
                    // True S24_LE (24-bit samples in 4-byte containers) is
                    // selected when the container itself is 24-bit wide in 4
                    // bytes (`bits=24`, `width=4`).
                    if !(matches!(container_bits, 8 | 16 | 24 | 32)
                        && width >= (container_bits as usize).div_ceil(8))
                    {
                        return Err(WavError::format(
                            "wav: bits per sample for fmt_ext PCM sub-type must be 8, 16, 24, or 32 bits",
                        ));
                    }
                    pcm_codec_for(container_bits, width)?
                }
                KSDATAFORMAT_SUBTYPE_IEEE_FLOAT | KSDATAFORMAT_SUBTYPE_AMBISONIC_IEEE_FLOAT => {
                    if valid_bits_per_sample != container_bits {
                        return Err(WavError::format(
                            "wav: valid bits per sample for fmt_ext IEEE sub-type must equal bits per sample",
                        ));
                    }
                    match container_bits {
                        32 => (SampleCodec::F32, width.max(4)),
                        64 => (SampleCodec::F64, width.max(8)),
                        _ => {
                            return Err(WavError::format(
                                "wav: bits per sample for fmt_ext IEEE sub-type must be 32 or 64 bits",
                            ));
                        }
                    }
                }
                KSDATAFORMAT_SUBTYPE_ALAW => {
                    if container_bits != 8 {
                        return Err(WavError::format(
                            "wav: bits per sample for fmt_ext a-law sub-type must be 8 bits",
                        ));
                    }
                    // Accept honest valid=8 and historical quirk valid=16.
                    if valid_bits_per_sample != 8 && valid_bits_per_sample != 16 {
                        (SampleCodec::Unsupported, 1)
                    } else {
                        (SampleCodec::ALaw, 1)
                    }
                }
                KSDATAFORMAT_SUBTYPE_MULAW => {
                    if container_bits != 8 {
                        return Err(WavError::format(
                            "wav: bits per sample for fmt_ext u-law sub-type must be 8 bits",
                        ));
                    }
                    if valid_bits_per_sample != 8 && valid_bits_per_sample != 16 {
                        (SampleCodec::Unsupported, 1)
                    } else {
                        (SampleCodec::MuLaw, 1)
                    }
                }
                _ => return Err(WavError::format("wav: unsupported fmt_ext sub-type")),
            };

            // Prefer header channel count; use mask fix-up when mask is non-zero
            // and consistent. Broken mask with honest nChannels falls back.
            let channels = if is_ambisonic {
                map_ambisonic_channel_count(num_channels)?
            } else {
                // Prefer nChannels (speech-ingest). Mask is used only to reject
                // impossible high bits when non-zero and unfixable.
                match (
                    channel_mask,
                    fix_wave_channel_mask(channel_mask, num_channels),
                ) {
                    (0, _) => map_wave_channel_count(num_channels)?,
                    (_, Some(fixed)) if fixed >> 18 == 0 => map_wave_channel_count(num_channels)?,
                    (_, Some(_)) => {
                        return Err(WavError::format(
                            "wav: too many channels in mask for fmt_ext",
                        ));
                    }
                    (_, None) => {
                        // Cannot fit mask; still accept honest nChannels when possible.
                        map_wave_channel_count(num_channels)?
                    }
                }
            };

            Ok(FmtFields {
                codec,
                channels,
                sample_rate,
                sample_width: width,
                adpcm_ms: None,
                adpcm_ima: None,
                big_endian: false,
            })
        }
        _ => Err(WavError::format("wav: unsupported wave format")),
    }
}
