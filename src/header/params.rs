//! Mutually exclusive codec-specific format parameters.
use super::{G726Params, ImaAdpcmParams, MsAdpcmParams, Tag0064Facts};

pub(crate) enum CodecParams {
    None,
    Ms(MsAdpcmParams),
    Ima(ImaAdpcmParams),
    G726(G726Params),
    Tag0064(Tag0064Facts),
}
impl super::FmtFields {
    #[cfg(feature = "adpcm")]
    pub(crate) fn ms_params(&self) -> Option<&MsAdpcmParams> {
        if let CodecParams::Ms(p) = &self.params {
            Some(p)
        } else {
            None
        }
    }
    #[cfg(feature = "adpcm")]
    pub(crate) fn ima_params(&self) -> Option<&ImaAdpcmParams> {
        if let CodecParams::Ima(p) = &self.params {
            Some(p)
        } else {
            None
        }
    }
    pub(crate) fn g726_params(&self) -> Option<&G726Params> {
        if let CodecParams::G726(p) = &self.params {
            Some(p)
        } else {
            None
        }
    }
    pub(crate) fn tag_0064_facts(&self) -> Option<&Tag0064Facts> {
        if let CodecParams::Tag0064(p) = &self.params {
            Some(p)
        } else {
            None
        }
    }
}
