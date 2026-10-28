//! Exact-size G.711 collect using shared source windows.
use super::input::for_each_input;
use crate::convert::split_g711;
use crate::{ByteSource, Result};
pub(super) fn fill_split(
    mss: &mut ByteSource<'_>,
    total: usize,
    channels: usize,
    max: usize,
    rate: u32,
    table: &'static [f32; 256],
) -> Result<Vec<Vec<f32>>> {
    let mut out: Vec<Vec<f32>> = (0..channels).map(|_| vec![0.0; total]).collect();
    for_each_input(mss, total, channels, max, rate, |bytes, range| {
        let mut planes: Vec<&mut [f32]> = out.iter_mut().map(|p| &mut p[range.clone()]).collect();
        split_g711(bytes, &mut planes, table);
        Ok(())
    })?;
    Ok(out)
}
