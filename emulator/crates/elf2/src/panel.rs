//! Front-panel switches (requirement 3.5-3.7).

/// Latching front-panel switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Run,
    Load,
    /// Memory protect.
    Mp,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrontPanel {
    pub run: bool,
    pub load: bool,
    pub mp: bool,
    /// IN button currently held.
    pub in_held: bool,
    /// One DMA-IN requested by an IN press in Load mode, not yet serviced.
    pub dma_in_pending: bool,
}

impl FrontPanel {
    /// /CLEAR and /WAIT levels (`true` = high) for the current switch positions:
    /// /CLEAR follows RUN, /WAIT is low while LOAD is on.
    pub fn control_lines(&self) -> (bool, bool) {
        (self.run, !self.load)
    }
}
