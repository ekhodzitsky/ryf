//! Inverse-quantizer constants from ITU-T G.726 (1990), section 4.2.
//! Index is the codeword. The sign bit is the top bit of that codeword.

pub(super) struct RateTables {
    pub iquant: &'static [i16],
    pub w: &'static [i16],
    pub f: &'static [u8],
}

const IQUANT_16: [i16; 4] = [116, 365, 365, 116];
const W_16: [i16; 4] = [-22, 439, 439, -22];
const F_16: [u8; 4] = [0, 7, 7, 0];

const IQUANT_24: [i16; 8] = [i16::MIN, 135, 273, 373, 373, 273, 135, i16::MIN];
const W_24: [i16; 8] = [-4, 30, 137, 582, 582, 137, 30, -4];
const F_24: [u8; 8] = [0, 1, 2, 7, 7, 2, 1, 0];

const IQUANT_32: [i16; 16] = [
    i16::MIN,
    4,
    135,
    213,
    273,
    323,
    373,
    425,
    425,
    373,
    323,
    273,
    213,
    135,
    4,
    i16::MIN,
];
const W_32: [i16; 16] = [
    -12, 18, 41, 64, 112, 198, 355, 1122, 1122, 355, 198, 112, 64, 41, 18, -12,
];
const F_32: [u8; 16] = [0, 0, 0, 1, 1, 1, 3, 7, 7, 3, 1, 1, 1, 0, 0, 0];

const IQUANT_40: [i16; 32] = [
    i16::MIN,
    -66,
    28,
    104,
    169,
    224,
    274,
    318,
    358,
    395,
    429,
    459,
    488,
    514,
    539,
    566,
    566,
    539,
    514,
    488,
    459,
    429,
    395,
    358,
    318,
    274,
    224,
    169,
    104,
    28,
    -66,
    i16::MIN,
];
const W_40: [i16; 32] = [
    14, 14, 24, 39, 40, 41, 58, 100, 141, 179, 219, 280, 358, 440, 529, 696, 696, 529, 440, 358,
    280, 219, 179, 141, 100, 58, 41, 40, 39, 24, 14, 14,
];
const F_40: [u8; 32] = [
    0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 3, 4, 5, 6, 6, 6, 6, 5, 4, 3, 2, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
];

pub(super) fn tables(bits: u8) -> RateTables {
    match bits {
        2 => RateTables {
            iquant: &IQUANT_16,
            w: &W_16,
            f: &F_16,
        },
        3 => RateTables {
            iquant: &IQUANT_24,
            w: &W_24,
            f: &F_24,
        },
        5 => RateTables {
            iquant: &IQUANT_40,
            w: &W_40,
            f: &F_40,
        },
        _ => RateTables {
            iquant: &IQUANT_32,
            w: &W_32,
            f: &F_32,
        },
    }
}
