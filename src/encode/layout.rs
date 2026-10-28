//! Header selection shared by writer construction, promotion, and finalization.
use super::{WriteSpec, header::*};
use crate::Result;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Layout {
    Riff,
    Rf64,
    Rf64Extensible,
    Rifx,
    Extensible,
}

impl Layout {
    pub(super) fn header(self, spec: WriteSpec, data: u64) -> Result<Vec<u8>> {
        validate_spec(spec)?;
        self.file_len(spec, data)?;
        let frames = data / frame_bytes(spec)? as u64;
        let mut out = Vec::with_capacity(116);
        match self {
            Self::Riff => push_header(
                &mut out,
                spec,
                u32::try_from(data).map_err(|_| crate::WavError::RiffTooLarge)?,
            )?,
            Self::Rifx => push_rifx_header(
                &mut out,
                spec,
                u32::try_from(data).map_err(|_| crate::WavError::RiffTooLarge)?,
            )?,
            Self::Extensible => push_extensible_header(
                &mut out,
                spec,
                u32::try_from(data).map_err(|_| crate::WavError::RiffTooLarge)?,
            )?,
            Self::Rf64 => push_rf64_header(&mut out, spec, data, frames)?,
            Self::Rf64Extensible => push_rf64_extensible_header(&mut out, spec, data, frames)?,
        }
        Ok(out)
    }
    pub(super) fn header_len(self, spec: WriteSpec) -> u64 {
        match self {
            Self::Riff | Self::Rifx => u64::from(riff_prefix(spec)) + 8,
            Self::Extensible => u64::from(extensible_riff_prefix(spec)) + 8,
            Self::Rf64 => rf64_header_len(spec),
            Self::Rf64Extensible => rf64_extensible_header_len(spec),
        }
    }
    pub(super) fn file_len(self, spec: WriteSpec, data: u64) -> Result<u64> {
        self.header_len(spec)
            .checked_add(data)
            .and_then(|n| n.checked_add(data & 1))
            .ok_or(crate::WavError::RiffTooLarge)
    }
    pub(super) fn overflows(self, spec: WriteSpec, data: u64) -> bool {
        match self {
            Self::Riff | Self::Rifx => needs_rf64(spec, data),
            Self::Extensible => needs_rf64_extensible(spec, data),
            _ => false,
        }
    }
    pub(super) fn promoted(self) -> Self {
        if self == Self::Extensible {
            Self::Rf64Extensible
        } else {
            Self::Rf64
        }
    }
}
