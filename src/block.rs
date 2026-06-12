use crate::{ata, serial, virtio};

pub const SECTOR_SIZE: usize = ata::SECTOR_SIZE;

#[derive(Clone, Copy)]
enum Backend {
    None,
    Virtio,
    Ata,
}

static mut BACKEND: Backend = Backend::None;

pub fn init() {
    unsafe {
        if virtio::block_ready() {
            BACKEND = Backend::Virtio;
            let mut sector = [0; SECTOR_SIZE];
            if read_sector(0, &mut sector) {
                serial::write_str("nk: root block backend virtio-blk magic=");
                for byte in &sector[..8] {
                    serial::write_hex_u16(*byte as u16);
                }
                serial::write_line("");
                return;
            }
            serial::write_line("nk: virtio-blk root probe failed");
        }

        let mut sector = [0; SECTOR_SIZE];
        if ata::read_sector(0, &mut sector) {
            BACKEND = Backend::Ata;
            serial::write_line("nk: root block backend ata-pio fallback");
        } else {
            BACKEND = Backend::None;
            serial::write_line("nk: no readable root block backend");
        }
    }
}

pub fn read_sector(lba: u32, out: &mut [u8; SECTOR_SIZE]) -> bool {
    read_sectors(lba, 1, out)
}

pub fn read_sectors(lba: u32, sectors: usize, out: &mut [u8]) -> bool {
    unsafe {
        match BACKEND {
            Backend::Virtio => virtio::read_block_sectors(lba, sectors, out),
            Backend::Ata => ata::read_sectors(lba, sectors, out),
            Backend::None => false,
        }
    }
}
