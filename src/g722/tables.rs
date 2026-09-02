//! ITU-T G.722 tables (same values as FFmpeg `g722.c`, ITU bit-exact).

pub(super) const INV_LOG2: [i32; 32] = [
    2048, 2093, 2139, 2186, 2233, 2282, 2332, 2383, 2435, 2489, 2543, 2599, 2656, 2714, 2774, 2834,
    2896, 2960, 3025, 3091, 3158, 3228, 3298, 3371, 3444, 3520, 3597, 3676, 3756, 3838, 3922, 4008,
];

/// 6-bit low-band inverse quantizer (64 kbit/s).
pub(super) const LOW_INV_QUANT6: [i32; 64] = [
    -17, -17, -17, -17, -3101, -2738, -2376, -2088, -1873, -1689, -1535, -1399, -1279, -1170,
    -1072, -982, -899, -822, -750, -682, -618, -558, -501, -447, -396, -347, -300, -254, -211,
    -170, -130, -91, 3101, 2738, 2376, 2088, 1873, 1689, 1535, 1399, 1279, 1170, 1072, 982, 899,
    822, 750, 682, 618, 558, 501, 447, 396, 347, 300, 254, 211, 170, 130, 91, 54, 17, -54, -17,
];

/// 4-bit low-band inverse quantizer (predictor update).
pub(super) const LOW_INV_QUANT4: [i32; 16] = [
    0, -2557, -1612, -1121, -786, -530, -323, -150, 2557, 1612, 1121, 786, 530, 323, 150, 0,
];

pub(super) const HIGH_INV_QUANT: [i32; 4] = [-926, -202, 926, 202];

/// `wl[rl42[ilow]]` for 4-bit low-band indices.
pub(super) const LOW_LOG_FACTOR_STEP: [i32; 16] = [
    -60, 3042, 1198, 538, 334, 172, 58, -30, 3042, 1198, 538, 334, 172, 58, -30, -60,
];

/// Indexed by `ihigh & 1`.
pub(super) const HIGH_LOG_FACTOR_STEP: [i32; 2] = [798, -214];

/// QMF even taps (ITU Table 11). Odd taps are this table reversed.
pub(super) const QMF_EVEN: [i32; 12] = [3, -11, 12, 32, -210, 951, 3876, -805, 362, -156, 53, -11];
pub(super) const QMF_ODD: [i32; 12] = [-11, 53, -156, 362, -805, 3876, 951, -210, 32, 12, -11, 3];
