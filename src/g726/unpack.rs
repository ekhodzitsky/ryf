//! Codeword unpacking. State is at most 7 unused bits plus the byte
//! currently being consumed.

use super::G726Packing;

#[derive(Clone, Copy, Debug)]
pub(super) struct Unpacker {
    pending: u32,
    nbits: u8,
    width: u8,
    msb_first: bool,
}

impl Unpacker {
    pub(super) fn new(width: u8, packing: G726Packing) -> Self {
        Self {
            pending: 0,
            nbits: 0,
            width,
            msb_first: packing == G726Packing::Aal2,
        }
    }

    pub(super) fn pending_bits(self) -> u8 {
        self.nbits
    }

    pub(super) fn clear(&mut self) {
        self.pending = 0;
        self.nbits = 0;
    }

    pub(super) fn push_byte(&mut self, byte: u8) {
        if self.msb_first {
            self.pending = (self.pending << 8) | u32::from(byte);
        } else {
            self.pending |= u32::from(byte) << self.nbits;
        }
        self.nbits += 8;
    }

    pub(super) fn try_take(&mut self) -> Option<u8> {
        if self.nbits < self.width {
            return None;
        }
        let code = if self.msb_first {
            let shift = self.nbits - self.width;
            let code = (self.pending >> shift) & ((1u32 << self.width) - 1);
            self.nbits -= self.width;
            self.pending = if self.nbits == 0 {
                0
            } else {
                self.pending & ((1u32 << self.nbits) - 1)
            };
            code
        } else {
            let mask = (1u32 << self.width) - 1;
            let code = self.pending & mask;
            self.pending >>= self.width;
            self.nbits -= self.width;
            code
        };
        Some(code as u8)
    }
}
