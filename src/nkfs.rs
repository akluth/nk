use core::ptr;

use crate::{ata, serial, services};

const MAGIC: &[u8; 8] = b"NKFSv1\0\0";
const VERSION: u32 = 1;
const INODE_SIZE: usize = 128;
const MAX_FILE_SIZE: usize = 20 * 1024 * 1024;
const SMALL_FILE_CACHE_SIZE: usize = 512 * 1024;

const KIND_FILE: u16 = 1;
const KIND_DIR: u16 = 2;

static mut FILE_CACHE: [u8; MAX_FILE_SIZE] = [0; MAX_FILE_SIZE];
static mut FILE_CACHE_INODE: u32 = 0;
static mut FILE_CACHE_LEN: usize = 0;
static mut SMALL_FILE_CACHE: [u8; SMALL_FILE_CACHE_SIZE] = [0; SMALL_FILE_CACHE_SIZE];
static mut SMALL_FILE_CACHE_INODE: u32 = 0;
static mut SMALL_FILE_CACHE_LEN: usize = 0;
static mut DIR_BUFFER: [u8; ata::SECTOR_SIZE * 16] = [0; ata::SECTOR_SIZE * 16];
static mut EXTENT_BUFFER: [u8; ata::SECTOR_SIZE * 128] = [0; ata::SECTOR_SIZE * 128];

#[derive(Clone, Copy)]
struct Superblock {
    block_size: u32,
    inode_count: u32,
    inode_table_start: u32,
    root_inode: u32,
}

#[derive(Clone, Copy)]
pub struct Metadata {
    pub kind: u16,
    pub size: usize,
}

#[derive(Clone, Copy)]
struct Inode {
    number: u32,
    kind: u16,
    size: usize,
    extent_start: u32,
}

pub fn smoke_test() {
    if mount().is_some() {
        serial::write_line("nk: nkfs root volume mounted");
    } else {
        serial::write_line("nk: nkfs root volume missing");
    }
}

pub fn read_file(path: &[u8]) -> Option<&'static [u8]> {
    let fs = mount()?;
    let inode = resolve_path(fs, path)?;
    if inode.kind != KIND_FILE || inode.size > MAX_FILE_SIZE {
        return None;
    }
    unsafe {
        if inode.size <= SMALL_FILE_CACHE_SIZE
            && SMALL_FILE_CACHE_INODE == inode.number
            && SMALL_FILE_CACHE_LEN == inode.size
        {
            return Some(core::slice::from_raw_parts(
                ptr::addr_of!(SMALL_FILE_CACHE).cast(),
                SMALL_FILE_CACHE_LEN,
            ));
        }
        if FILE_CACHE_INODE == inode.number && FILE_CACHE_LEN == inode.size {
            return Some(core::slice::from_raw_parts(
                ptr::addr_of!(FILE_CACHE).cast(),
                FILE_CACHE_LEN,
            ));
        }
    }

    let (out, out_len) = if inode.size <= SMALL_FILE_CACHE_SIZE {
        (
            ptr::addr_of_mut!(SMALL_FILE_CACHE).cast(),
            SMALL_FILE_CACHE_SIZE,
        )
    } else {
        (ptr::addr_of_mut!(FILE_CACHE).cast(), MAX_FILE_SIZE)
    };
    read_extent(inode.extent_start, inode.size, out, out_len)?;
    unsafe {
        if inode.size <= SMALL_FILE_CACHE_SIZE {
            SMALL_FILE_CACHE_INODE = inode.number;
            SMALL_FILE_CACHE_LEN = inode.size;
            Some(core::slice::from_raw_parts(
                ptr::addr_of!(SMALL_FILE_CACHE).cast(),
                inode.size,
            ))
        } else {
            FILE_CACHE_INODE = inode.number;
            FILE_CACHE_LEN = inode.size;
            Some(core::slice::from_raw_parts(
                ptr::addr_of!(FILE_CACHE).cast(),
                inode.size,
            ))
        }
    }
}

pub fn preload_file(path: &[u8]) -> bool {
    read_file(path).is_some()
}

pub fn write_file_to_console(path: &[u8]) -> bool {
    let Some(data) = read_file(path) else {
        write_console(b"cat: not found\n");
        return false;
    };
    write_console(data);
    if !data.ends_with(b"\n") {
        write_console(b"\n");
    }
    true
}

pub fn write_dir_to_console(path: &[u8]) -> bool {
    let Some(data) = read_dir(path) else {
        write_console(b"ls: not a directory or not found\n");
        return false;
    };
    let mut offset = 0usize;
    let mut col = 0usize;
    while offset + 8 <= data.len() {
        let name_len = read_u16(data, offset + 4).unwrap_or(0) as usize;
        let next = align_up(offset + 8 + name_len, 4);
        if next > data.len() {
            break;
        }
        let name = &data[offset + 8..offset + 8 + name_len];
        if name != b"." && name != b".." {
            write_console(name);
            col += name.len();
            if col >= 64 {
                write_console(b"\n");
                col = 0;
            } else {
                write_console(b"  ");
                col += 2;
            }
        }
        offset = next;
    }
    if col != 0 {
        write_console(b"\n");
    }
    true
}

fn write_console(bytes: &[u8]) {
    for byte in bytes {
        serial::write_str_byte(*byte);
    }
    services::gui::console_write(bytes);
}

pub fn read_dir(path: &[u8]) -> Option<&'static [u8]> {
    let fs = mount()?;
    let inode = resolve_path(fs, path)?;
    if inode.kind != KIND_DIR || inode.size > ata::SECTOR_SIZE * 16 {
        return None;
    }
    read_extent(
        inode.extent_start,
        inode.size,
        ptr::addr_of_mut!(DIR_BUFFER).cast(),
        ata::SECTOR_SIZE * 16,
    )?;
    unsafe {
        Some(core::slice::from_raw_parts(
            ptr::addr_of!(DIR_BUFFER).cast(),
            inode.size,
        ))
    }
}

pub fn metadata(path: &[u8]) -> Option<Metadata> {
    let fs = mount()?;
    let inode = resolve_path(fs, path)?;
    Some(Metadata {
        kind: inode.kind,
        size: inode.size,
    })
}

pub fn exists(path: &[u8]) -> bool {
    metadata(path).is_some()
}

fn mount() -> Option<Superblock> {
    let mut sector = [0; ata::SECTOR_SIZE];
    if !ata::read_sector(0, &mut sector) {
        return None;
    }
    if &sector[0..8] != MAGIC {
        return None;
    }
    if read_u32(&sector, 8)? != VERSION {
        return None;
    }
    let block_size = read_u32(&sector, 12)?;
    if block_size as usize != ata::SECTOR_SIZE {
        return None;
    }
    Some(Superblock {
        block_size,
        inode_count: read_u32(&sector, 16)?,
        inode_table_start: read_u32(&sector, 20)?,
        root_inode: read_u32(&sector, 32)?,
    })
}

fn resolve_path(fs: Superblock, path: &[u8]) -> Option<Inode> {
    let mut inode_number = fs.root_inode;
    let mut cursor = 0usize;

    while cursor < path.len() && path[cursor] == b'/' {
        cursor += 1;
    }
    if cursor >= path.len() {
        return read_inode(fs, inode_number);
    }

    loop {
        let start = cursor;
        while cursor < path.len() && path[cursor] != b'/' && path[cursor] != 0 {
            cursor += 1;
        }
        let name = &path[start..cursor];
        if !name.is_empty() && name != b"." {
            let dir = read_inode(fs, inode_number)?;
            if dir.kind != KIND_DIR {
                return None;
            }
            inode_number = find_dir_entry(dir, name)?;
        }
        while cursor < path.len() && path[cursor] == b'/' {
            cursor += 1;
        }
        if cursor >= path.len() || path[cursor] == 0 {
            break;
        }
    }

    read_inode(fs, inode_number)
}

fn find_dir_entry(dir: Inode, name: &[u8]) -> Option<u32> {
    if dir.size > ata::SECTOR_SIZE * 16 {
        return None;
    }
    read_extent(
        dir.extent_start,
        dir.size,
        ptr::addr_of_mut!(DIR_BUFFER).cast(),
        ata::SECTOR_SIZE * 16,
    )?;
    let data = unsafe { core::slice::from_raw_parts(ptr::addr_of!(DIR_BUFFER).cast(), dir.size) };
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let inode = read_u32(data, offset)?;
        let name_len = read_u16(data, offset + 4)? as usize;
        let record_kind = read_u16(data, offset + 6)?;
        let next = align_up(offset + 8 + name_len, 4);
        if next > data.len() {
            return None;
        }
        if record_kind != 0 && data.get(offset + 8..offset + 8 + name_len)? == name {
            return Some(inode);
        }
        offset = next;
    }
    None
}

fn read_inode(fs: Superblock, inode_number: u32) -> Option<Inode> {
    if inode_number == 0 || inode_number > fs.inode_count {
        return None;
    }

    let byte_offset = fs.inode_table_start as usize * fs.block_size as usize
        + (inode_number as usize - 1) * INODE_SIZE;
    let lba = byte_offset / ata::SECTOR_SIZE;
    let offset = byte_offset % ata::SECTOR_SIZE;
    let mut sector = [0; ata::SECTOR_SIZE];
    if !ata::read_sector(lba as u32, &mut sector) {
        return None;
    }
    if offset + INODE_SIZE > ata::SECTOR_SIZE {
        return None;
    }
    Some(Inode {
        number: inode_number,
        kind: read_u16(&sector, offset)?,
        size: read_u64(&sector, offset + 8)? as usize,
        extent_start: read_u32(&sector, offset + 16)?,
    })
}

fn read_extent(start_block: u32, size: usize, out: *mut u8, out_len: usize) -> Option<()> {
    if size > out_len {
        return None;
    }
    let sectors = align_up(size, ata::SECTOR_SIZE) / ata::SECTOR_SIZE;
    let mut written = 0usize;
    while written < size {
        let remaining_sectors = sectors - (written / ata::SECTOR_SIZE);
        let chunk_sectors = remaining_sectors.min(128);
        let chunk_bytes = chunk_sectors * ata::SECTOR_SIZE;
        let buffer = unsafe {
            core::slice::from_raw_parts_mut(ptr::addr_of_mut!(EXTENT_BUFFER).cast(), chunk_bytes)
        };
        if !ata::read_sectors(
            start_block + (written / ata::SECTOR_SIZE) as u32,
            chunk_sectors,
            buffer,
        ) {
            return None;
        }
        let count = (size - written).min(chunk_bytes);
        unsafe {
            ptr::copy_nonoverlapping(ptr::addr_of!(EXTENT_BUFFER).cast(), out.add(written), count);
        }
        written += count;
    }
    Some(())
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let data = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([data[0], data[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let data = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let data = bytes.get(offset..offset + 8)?;
    Some(u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]))
}
