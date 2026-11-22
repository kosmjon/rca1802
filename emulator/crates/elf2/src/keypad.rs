//! Hex keypad and two-digit hex display (requirement 4).

/// 16-key hex keypad. The 74C923 encoder and 74C173 latches keep the last two keys
/// pressed as one byte: each key shifts the previous digit into the high nibble.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HexKeypad {
    pub(crate) latch: u8,
}

impl HexKeypad {
    /// Key `nibble` (0-F) pressed. Values above 0F use only the low nibble.
    pub fn press(&mut self, nibble: u8) {
        self.latch = (self.latch << 4) | (nibble & 0x0F);
    }

    /// Byte put on the bus by INP 4 or a Load-mode DMA-IN.
    pub fn byte(&self) -> u8 {
        self.latch
    }
}

/// Two-digit seven-segment display, latched by OUT 4 and by Load-mode DMA-IN.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HexDisplay {
    pub(crate) value: u8,
}

impl HexDisplay {
    /// Latch a value; returns true if the shown value changed.
    pub fn latch(&mut self, v: u8) -> bool {
        let changed = v != self.value;
        self.value = v;
        changed
    }

    pub fn value(&self) -> u8 {
        self.value
    }
}
