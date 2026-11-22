//! Memory map: RAM from 0000, ROM images that overlay it, unmapped addresses,
//! and the front-panel memory-protect gate (requirements 3.1, 3.2, 3.4, 3.7).

use crate::error::MachineError;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Unmapped,
    Ram,
    Rom,
}

/// A ROM image placed in the address space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRom {
    pub name: String,
    pub base: u16,
    pub len: usize,
    pub sha256: String,
}

/// Lower-case hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub struct MemoryMap {
    bytes: Vec<u8>,
    kind: Vec<Kind>,
    unmapped_read: u8,
    ram_size: u32,
    roms: Vec<LoadedRom>,
    /// Memory protect switch: blocks all writes while on.
    pub protect: bool,
}

impl MemoryMap {
    pub fn new(ram_size: u32, unmapped_read: u8) -> Self {
        let mut kind = vec![Kind::Unmapped; 0x10000];
        kind[..ram_size as usize].fill(Kind::Ram);
        MemoryMap {
            bytes: vec![0; 0x10000],
            kind,
            unmapped_read,
            ram_size,
            roms: Vec::new(),
            protect: false,
        }
    }

    /// CPU or DMA read.
    #[inline]
    pub fn read(&self, addr: u16) -> u8 {
        match self.kind[addr as usize] {
            Kind::Unmapped => self.unmapped_read,
            _ => self.bytes[addr as usize],
        }
    }

    /// CPU or DMA write: only RAM, and only while memory protect is off.
    #[inline]
    pub fn write(&mut self, addr: u16, val: u8) {
        if !self.protect && self.kind[addr as usize] == Kind::Ram {
            self.bytes[addr as usize] = val;
        }
    }

    /// Debugger / file-load write: ignores memory protect, writes RAM and ROM
    /// contents, ignores unmapped addresses. Returns the number of bytes stored.
    pub fn poke(&mut self, addr: u16, data: &[u8]) -> usize {
        let mut n = 0;
        for (k, &b) in data.iter().enumerate() {
            let a = (addr as usize + k) & 0xFFFF;
            if self.kind[a] != Kind::Unmapped {
                self.bytes[a] = b;
                n += 1;
            }
        }
        n
    }

    /// Address has RAM or ROM.
    pub fn is_mapped(&self, addr: u16) -> bool {
        self.kind[addr as usize] != Kind::Unmapped
    }

    /// All 64K of stored bytes (RAM and ROM contents; unmapped addresses hold 00).
    pub fn raw(&self) -> &[u8] {
        &self.bytes
    }

    /// Debugger read of a range (wraps at FFFF).
    pub fn peek(&self, addr: u16, len: usize) -> Vec<u8> {
        (0..len)
            .map(|k| self.read(addr.wrapping_add(k as u16)))
            .collect()
    }

    /// Place a ROM image. Its bytes overlay RAM and are read-only to the CPU.
    pub fn load_rom(
        &mut self,
        name: &str,
        base: u16,
        data: &[u8],
    ) -> Result<&LoadedRom, MachineError> {
        let end = usize::from(base) + data.len();
        if data.is_empty() || end > 0x10000 {
            return Err(MachineError::TooLarge {
                base,
                len: data.len(),
            });
        }
        // Replacing a ROM with the same name is allowed; overlapping another is not.
        self.remove_rom(name);
        for r in &self.roms {
            let (b, e) = (usize::from(r.base), usize::from(r.base) + r.len);
            if usize::from(base) < e && b < end {
                return Err(MachineError::RomOverlap {
                    name: name.to_string(),
                    other: r.name.clone(),
                });
            }
        }
        self.bytes[usize::from(base)..end].copy_from_slice(data);
        self.kind[usize::from(base)..end].fill(Kind::Rom);
        self.roms.push(LoadedRom {
            name: name.to_string(),
            base,
            len: data.len(),
            sha256: sha256_hex(data),
        });
        Ok(self.roms.last().expect("just pushed"))
    }

    /// Remove a ROM by name; the addresses revert to RAM (contents 00) or unmapped.
    pub fn remove_rom(&mut self, name: &str) {
        if let Some(i) = self.roms.iter().position(|r| r.name == name) {
            let r = self.roms.remove(i);
            for a in usize::from(r.base)..usize::from(r.base) + r.len {
                if (a as u32) < self.ram_size {
                    self.kind[a] = Kind::Ram;
                    self.bytes[a] = 0;
                } else {
                    self.kind[a] = Kind::Unmapped;
                }
            }
        }
    }

    pub fn roms(&self) -> &[LoadedRom] {
        &self.roms
    }
}
