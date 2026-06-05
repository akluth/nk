use core::arch::asm;

use crate::serial;

pub type VirtAddr = u64;
pub type PhysAddr = u64;

#[derive(Clone, Copy)]
pub enum Syscall {
    Yield = 0,
}

#[derive(Clone, Copy)]
pub struct Mapping {
    pub virt: VirtAddr,
    pub phys: PhysAddr,
    pub len: u64,
    pub flags: MappingFlags,
}

#[derive(Clone, Copy)]
pub struct MappingFlags {
    bits: u64,
}

impl MappingFlags {
    pub const READ: Self = Self { bits: 1 << 0 };
    pub const WRITE: Self = Self { bits: 1 << 1 };
    pub const EXECUTE: Self = Self { bits: 1 << 2 };
    pub const USER: Self = Self { bits: 1 << 3 };

    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    pub const fn bits(self) -> u64 {
        self.bits
    }
}

pub struct AddressSpace {
    mappings: [Option<Mapping>; 16],
}

impl AddressSpace {
    pub const fn new() -> Self {
        Self {
            mappings: [None; 16],
        }
    }

    pub fn map(&mut self, mapping: Mapping) -> bool {
        for slot in &mut self.mappings {
            if slot.is_none() {
                *slot = Some(mapping);
                return true;
            }
        }

        false
    }

    pub fn validation_token(&self) -> u64 {
        let mut token = 0;

        for mapping in &self.mappings {
            if let Some(mapping) = mapping {
                token ^= mapping.virt;
                token ^= mapping.phys;
                token ^= mapping.len;
                token ^= mapping.flags.bits();
            }
        }

        token
    }
}

pub fn init() {
    let mut address_space = AddressSpace::new();
    let mapped = address_space.map(Mapping {
        virt: 0x0000_4000_0000,
        phys: 0,
        len: 0x1000,
        flags: MappingFlags::READ
            .union(MappingFlags::WRITE)
            .union(MappingFlags::EXECUTE)
            .union(MappingFlags::USER),
    });

    if mapped && address_space.validation_token() != 0 {
        serial::write_line("nk: user address-space model ready");
    }
}

pub fn smoke_test_syscall() {
    unsafe {
        asm!(
            "int 0x80",
            in("rax") Syscall::Yield as u64,
            options(nostack, preserves_flags)
        );
    }
}
