use core::cell::UnsafeCell;

use crate::{ata, serial, virtio};

pub const SECTOR_SIZE: usize = ata::SECTOR_SIZE;

#[derive(Clone, Copy)]
enum Backend {
    None,
    Virtio,
    Ata,
}

struct BackendCell(UnsafeCell<Backend>);

unsafe impl Sync for BackendCell {}

static BACKEND: BackendCell = BackendCell(UnsafeCell::new(Backend::None));

pub fn init() {
    if virtio::block_ready() {
        set_backend(Backend::Virtio);
        let mut sector = [0; SECTOR_SIZE];
        if read_sector(0, &mut sector) {
            serial::write_line("nk: root block backend virtio-blk");
            return;
        }
        serial::write_line("nk: virtio-blk root probe failed");
    }

    let mut sector = [0; SECTOR_SIZE];
    if ata::read_sector(0, &mut sector) {
        set_backend(Backend::Ata);
        serial::write_line("nk: root block backend ata-pio fallback");
    } else {
        set_backend(Backend::None);
        serial::write_line("nk: no readable root block backend");
    }
}

pub fn read_sector(lba: u32, out: &mut [u8; SECTOR_SIZE]) -> bool {
    read_sectors(lba, 1, out)
}

pub fn read_sectors(lba: u32, sectors: usize, out: &mut [u8]) -> bool {
    match backend() {
        Backend::Virtio => virtio::read_block_sectors(lba, sectors, out),
        Backend::Ata => ata::read_sectors(lba, sectors, out),
        Backend::None => false,
    }
}

fn set_backend(backend: Backend) {
    unsafe {
        *BACKEND.0.get() = backend;
    }
}

fn backend() -> Backend {
    unsafe { *BACKEND.0.get() }
}
