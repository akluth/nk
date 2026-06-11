use crate::{arch, serial};

const DATA: u16 = 0x1f0;
const SECTOR_COUNT: u16 = 0x1f2;
const LBA_LOW: u16 = 0x1f3;
const LBA_MID: u16 = 0x1f4;
const LBA_HIGH: u16 = 0x1f5;
const DRIVE: u16 = 0x1f6;
const STATUS_COMMAND: u16 = 0x1f7;

const STATUS_ERR: u8 = 1 << 0;
const STATUS_DRQ: u8 = 1 << 3;
const STATUS_BSY: u8 = 1 << 7;
const CMD_READ_SECTORS: u8 = 0x20;

pub const SECTOR_SIZE: usize = 512;

pub fn read_sector(lba: u32, out: &mut [u8; SECTOR_SIZE]) -> bool {
    read_sectors(lba, 1, out)
}

pub fn read_sectors(lba: u32, sectors: usize, out: &mut [u8]) -> bool {
    if sectors == 0 || sectors > 255 || out.len() < sectors * SECTOR_SIZE {
        return false;
    }

    unsafe {
        arch::outb(DRIVE, 0xe0 | ((lba >> 24) as u8 & 0x0f));
        arch::outb(SECTOR_COUNT, sectors as u8);
        arch::outb(LBA_LOW, lba as u8);
        arch::outb(LBA_MID, (lba >> 8) as u8);
        arch::outb(LBA_HIGH, (lba >> 16) as u8);
        arch::outb(STATUS_COMMAND, CMD_READ_SECTORS);

        for sector in 0..sectors {
            if !wait_drq() {
                return false;
            }
            let offset = sector * SECTOR_SIZE;
            for chunk in out[offset..offset + SECTOR_SIZE].chunks_exact_mut(2) {
                let word = arch::inw(DATA);
                chunk[0] = word as u8;
                chunk[1] = (word >> 8) as u8;
            }
        }
    }

    true
}

pub fn smoke_test() {
    let mut sector = [0; SECTOR_SIZE];
    if read_sector(0, &mut sector) {
        serial::write_line("nk: ata pio disk readable");
    } else {
        serial::write_line("nk: ata pio disk missing");
    }
}

unsafe fn wait_drq() -> bool {
    for _ in 0..100_000 {
        let status = arch::inb(STATUS_COMMAND);
        if status & STATUS_ERR != 0 {
            return false;
        }
        if status & STATUS_BSY == 0 && status & STATUS_DRQ != 0 {
            return true;
        }
    }

    false
}
