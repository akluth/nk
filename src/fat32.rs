use crate::{ata, serial};

const MAX_FILE_SIZE: usize = 1024 * 1024;
const END_OF_CHAIN: u32 = 0x0fff_fff8;

static mut FILE_BUFFER: [u8; MAX_FILE_SIZE] = [0; MAX_FILE_SIZE];

#[derive(Clone, Copy)]
struct Fat32 {
    sectors_per_cluster: u8,
    root_cluster: u32,
    fat_start: u32,
    data_start: u32,
}

#[derive(Clone, Copy)]
struct DirectoryEntry {
    cluster: u32,
    size: u32,
}

pub fn read_file(name: &[u8; 11]) -> Option<&'static [u8]> {
    let fs = mount()?;
    let entry = find_root_entry(fs, name)?;
    let len = read_chain(fs, entry.cluster, entry.size as usize)?;
    serial::write_line("nk: fat32 file loaded");

    unsafe { Some(&FILE_BUFFER[..len]) }
}

pub fn smoke_test() {
    if mount().is_some() {
        serial::write_line("nk: fat32 volume mounted");
    } else {
        serial::write_line("nk: fat32 volume missing");
    }
}

fn mount() -> Option<Fat32> {
    let mut sector = [0; ata::SECTOR_SIZE];
    if !ata::read_sector(0, &mut sector) {
        return None;
    }
    if sector[510] != 0x55 || sector[511] != 0xaa {
        return None;
    }

    if read_u16(&sector, 11)? as usize != ata::SECTOR_SIZE {
        return None;
    }
    let sectors_per_cluster = sector[13];
    let reserved_sectors = read_u16(&sector, 14)?;
    let fat_count = sector[16];
    let sectors_per_fat = read_u32(&sector, 36)?;
    let root_cluster = read_u32(&sector, 44)?;
    let fat_start = reserved_sectors as u32;
    let data_start = fat_start + fat_count as u32 * sectors_per_fat;

    Some(Fat32 {
        sectors_per_cluster,
        root_cluster,
        fat_start,
        data_start,
    })
}

fn find_root_entry(fs: Fat32, name: &[u8; 11]) -> Option<DirectoryEntry> {
    let mut sector = [0; ata::SECTOR_SIZE];
    let first_sector = cluster_lba(fs, fs.root_cluster);
    for offset in 0..fs.sectors_per_cluster as u32 {
        if !ata::read_sector(first_sector + offset, &mut sector) {
            return None;
        }
        for entry in sector.chunks_exact(32) {
            if entry[0] == 0 {
                return None;
            }
            if entry[0] == 0xe5 || entry[11] == 0x0f {
                continue;
            }
            if &entry[0..11] == name {
                let high = read_u16(entry, 20)? as u32;
                let low = read_u16(entry, 26)? as u32;
                return Some(DirectoryEntry {
                    cluster: (high << 16) | low,
                    size: read_u32(entry, 28)?,
                });
            }
        }
    }

    None
}

fn read_chain(fs: Fat32, start_cluster: u32, size: usize) -> Option<usize> {
    if size > MAX_FILE_SIZE {
        return None;
    }

    let mut cluster = start_cluster;
    let mut written = 0;
    let mut sector = [0; ata::SECTOR_SIZE];
    while cluster < END_OF_CHAIN && written < size {
        let first_sector = cluster_lba(fs, cluster);
        for offset in 0..fs.sectors_per_cluster as u32 {
            if written >= size {
                break;
            }
            if !ata::read_sector(first_sector + offset, &mut sector) {
                return None;
            }
            let copy = (size - written).min(ata::SECTOR_SIZE);
            unsafe {
                FILE_BUFFER[written..written + copy].copy_from_slice(&sector[..copy]);
            }
            written += copy;
        }
        cluster = fat_entry(fs, cluster)?;
    }

    Some(written)
}

fn fat_entry(fs: Fat32, cluster: u32) -> Option<u32> {
    let mut sector = [0; ata::SECTOR_SIZE];
    let offset = cluster * 4;
    let lba = fs.fat_start + offset / ata::SECTOR_SIZE as u32;
    let sector_offset = (offset % ata::SECTOR_SIZE as u32) as usize;
    if !ata::read_sector(lba, &mut sector) {
        return None;
    }

    Some(read_u32(&sector, sector_offset)? & 0x0fff_ffff)
}

fn cluster_lba(fs: Fat32, cluster: u32) -> u32 {
    fs.data_start + (cluster - 2) * fs.sectors_per_cluster as u32
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let data = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([data[0], data[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let data = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}
