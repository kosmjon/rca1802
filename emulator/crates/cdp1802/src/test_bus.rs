//! A simple bus for CPU tests: 64K RAM, settable EF lines and request signals,
//! and a log of every I/O operation.

use crate::{Bus, Signals};
use std::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoEvent {
    Out(u8, u8),
    Inp(u8),
    Q(bool),
    DmaIn(u8),
    DmaOut(u8),
}

pub struct TestBus {
    pub mem: Vec<u8>,
    pub ef: [bool; 4],
    pub signals: Signals,
    /// Value returned by `inp`, per port 1..=7 (index 0 unused).
    pub inp_value: [u8; 8],
    /// Value supplied by `dma_in`.
    pub dma_in_value: u8,
    pub log: Vec<IoEvent>,
    pub ticked: u64,
}

impl TestBus {
    pub fn new() -> Self {
        TestBus {
            mem: std::vec![0; 0x10000],
            ef: [false; 4],
            signals: Signals::default(),
            inp_value: [0; 8],
            dma_in_value: 0,
            log: Vec::new(),
            ticked: 0,
        }
    }

    /// Copy `bytes` into memory at `addr`.
    pub fn load(&mut self, addr: u16, bytes: &[u8]) {
        for (k, b) in bytes.iter().enumerate() {
            self.mem[(addr as usize + k) & 0xFFFF] = *b;
        }
    }
}

impl Bus for TestBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[addr as usize]
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.mem[addr as usize] = val;
    }
    fn out(&mut self, port: u8, val: u8) {
        self.log.push(IoEvent::Out(port, val));
    }
    fn inp(&mut self, port: u8) -> u8 {
        self.log.push(IoEvent::Inp(port));
        self.inp_value[port as usize]
    }
    fn ef(&self, line: u8) -> bool {
        self.ef[(line - 1) as usize]
    }
    fn set_q(&mut self, q: bool) {
        self.log.push(IoEvent::Q(q));
    }
    fn dma_in(&mut self) -> u8 {
        self.log.push(IoEvent::DmaIn(self.dma_in_value));
        self.dma_in_value
    }
    fn dma_out(&mut self, val: u8) {
        self.log.push(IoEvent::DmaOut(val));
    }
    fn signals(&self) -> Signals {
        self.signals
    }
    fn tick(&mut self, cycles: u32) {
        self.ticked += u64::from(cycles);
    }
}
