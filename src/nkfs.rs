use core::ptr;

use crate::{block, serial, services};

const MAGIC: &[u8; 8] = b"NKFSv1\0\0";
const VERSION: u32 = 1;
const INODE_SIZE: usize = 128;
const MAX_FILE_SIZE: usize = 20 * 1024 * 1024;
const SMALL_FILE_CACHE_SIZE: usize = 512 * 1024;
const RAM_FILE_CAP: usize = 256 * 1024;
const RAM_FILE_COUNT: usize = 32;

const KIND_FILE: u16 = 1;
const KIND_DIR: u16 = 2;

static mut FILE_CACHE: [u8; MAX_FILE_SIZE] = [0; MAX_FILE_SIZE];
static mut FILE_CACHE_INODE: u32 = 0;
static mut FILE_CACHE_LEN: usize = 0;
static mut MOUNT_CACHE: Option<Superblock> = None;
static mut SMALL_FILE_CACHE: [u8; SMALL_FILE_CACHE_SIZE] = [0; SMALL_FILE_CACHE_SIZE];
static mut SMALL_FILE_CACHE_INODE: u32 = 0;
static mut SMALL_FILE_CACHE_LEN: usize = 0;
static mut DIR_BUFFER: [u8; block::SECTOR_SIZE * 16] = [0; block::SECTOR_SIZE * 16];
static mut EXTENT_BUFFER: [u8; block::SECTOR_SIZE * 128] = [0; block::SECTOR_SIZE * 128];

struct RamFileCell(core::cell::UnsafeCell<[RamFile; RAM_FILE_COUNT]>);

unsafe impl Sync for RamFileCell {}

#[derive(Clone, Copy)]
struct RamFile {
    used: bool,
    path: [u8; 256],
    path_len: usize,
    data: [u8; RAM_FILE_CAP],
    len: usize,
}

impl RamFile {
    const fn empty() -> Self {
        Self {
            used: false,
            path: [0; 256],
            path_len: 0,
            data: [0; RAM_FILE_CAP],
            len: 0,
        }
    }
}

static RAM_FILES: RamFileCell = RamFileCell(core::cell::UnsafeCell::new(
    [RamFile::empty(); RAM_FILE_COUNT],
));

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
    if let Some(index) = ram_find(path) {
        return ram_file_slice(index);
    }

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
    if inode.kind != KIND_DIR || inode.size > block::SECTOR_SIZE * 16 {
        return None;
    }
    read_extent(
        inode.extent_start,
        inode.size,
        ptr::addr_of_mut!(DIR_BUFFER).cast(),
        block::SECTOR_SIZE * 16,
    )?;
    let size = append_ram_dir_entries(path, inode.size).unwrap_or(inode.size);
    unsafe {
        Some(core::slice::from_raw_parts(
            ptr::addr_of!(DIR_BUFFER).cast(),
            size,
        ))
    }
}

pub fn metadata(path: &[u8]) -> Option<Metadata> {
    if let Some(index) = ram_find(path) {
        let len = unsafe { (*RAM_FILES.0.get())[index].len };
        return Some(Metadata {
            kind: KIND_FILE,
            size: len,
        });
    }

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

pub fn open_writable_file(path: &[u8], truncate: bool) -> Option<usize> {
    if !valid_absolute_file_path(path) {
        return None;
    }
    if let Some(index) = ram_find(path) {
        if truncate {
            unsafe {
                (*RAM_FILES.0.get())[index].len = 0;
            }
        }
        return Some(index);
    }
    if !parent_directory_exists(path) {
        return None;
    }

    unsafe {
        let files = &mut *RAM_FILES.0.get();
        for index in 0..RAM_FILE_COUNT {
            if !files[index].used {
                files[index].used = true;
                files[index].path[..path.len()].copy_from_slice(path);
                files[index].path_len = path.len();
                files[index].len = 0;
                return Some(index);
            }
        }
    }
    None
}

pub fn ram_file_slice(index: usize) -> Option<&'static [u8]> {
    if index >= RAM_FILE_COUNT {
        return None;
    }
    unsafe {
        let file = &(*RAM_FILES.0.get())[index];
        if !file.used {
            return None;
        }
        Some(core::slice::from_raw_parts(file.data.as_ptr(), file.len))
    }
}

pub fn write_ram_file(index: usize, offset: usize, bytes: &[u8]) -> Option<usize> {
    if index >= RAM_FILE_COUNT {
        return None;
    }
    unsafe {
        let file = &mut (*RAM_FILES.0.get())[index];
        if !file.used || offset > RAM_FILE_CAP {
            return None;
        }
        let count = bytes.len().min(RAM_FILE_CAP - offset);
        file.data[offset..offset + count].copy_from_slice(&bytes[..count]);
        file.len = file.len.max(offset + count);
        Some(count)
    }
}

pub fn truncate_ram_file(index: usize, len: usize) -> bool {
    if index >= RAM_FILE_COUNT || len > RAM_FILE_CAP {
        return false;
    }
    unsafe {
        let file = &mut (*RAM_FILES.0.get())[index];
        if !file.used {
            return false;
        }
        if len > file.len {
            for byte in &mut file.data[file.len..len] {
                *byte = 0;
            }
        }
        file.len = len;
        true
    }
}

pub fn ram_file_index(path: &[u8]) -> Option<usize> {
    ram_find(path)
}

pub fn remove_ram_file(path: &[u8]) -> bool {
    let Some(index) = ram_find(path) else {
        return false;
    };
    unsafe {
        (*RAM_FILES.0.get())[index] = RamFile::empty();
    }
    true
}

fn mount() -> Option<Superblock> {
    unsafe {
        if let Some(fs) = MOUNT_CACHE {
            return Some(fs);
        }
    }

    let mut sector = [0; block::SECTOR_SIZE];
    if !block::read_sector(0, &mut sector) {
        return None;
    }
    if &sector[0..8] != MAGIC {
        return None;
    }
    if read_u32(&sector, 8)? != VERSION {
        return None;
    }
    let block_size = read_u32(&sector, 12)?;
    if block_size as usize != block::SECTOR_SIZE {
        return None;
    }
    let fs = Superblock {
        block_size,
        inode_count: read_u32(&sector, 16)?,
        inode_table_start: read_u32(&sector, 20)?,
        root_inode: read_u32(&sector, 32)?,
    };
    unsafe {
        MOUNT_CACHE = Some(fs);
    }
    Some(fs)
}

fn ram_find(path: &[u8]) -> Option<usize> {
    let path = normalized_path_bytes(path)?;
    unsafe {
        let files = &*RAM_FILES.0.get();
        for index in 0..RAM_FILE_COUNT {
            let file = &files[index];
            if file.used && file.path_len == path.len() && &file.path[..file.path_len] == path {
                return Some(index);
            }
        }
    }
    None
}

fn normalized_path_bytes(path: &[u8]) -> Option<&[u8]> {
    if path.is_empty() || path[0] != b'/' || path.len() > 256 {
        return None;
    }
    let mut len = path.len();
    while len > 1 && path[len - 1] == b'/' {
        len -= 1;
    }
    Some(&path[..len])
}

fn valid_absolute_file_path(path: &[u8]) -> bool {
    let Some(path) = normalized_path_bytes(path) else {
        return false;
    };
    path.len() > 1 && !path[path.len() - 1..].contains(&b'/')
}

fn parent_directory_exists(path: &[u8]) -> bool {
    let Some(path) = normalized_path_bytes(path) else {
        return false;
    };
    let mut slash = 0usize;
    for index in 1..path.len() {
        if path[index] == b'/' {
            slash = index;
        }
    }
    let parent = if slash == 0 { b"/" } else { &path[..slash] };
    let Some(fs) = mount() else {
        return false;
    };
    resolve_path(fs, parent)
        .map(|inode| inode.kind == KIND_DIR)
        .unwrap_or(false)
}

fn append_ram_dir_entries(path: &[u8], start: usize) -> Option<usize> {
    let dir = normalized_path_bytes(path)?;
    let mut offset = start;
    unsafe {
        let files = &*RAM_FILES.0.get();
        for file in files.iter() {
            if !file.used {
                continue;
            }
            let Some(name) = ram_child_name(dir, &file.path[..file.path_len]) else {
                continue;
            };
            if find_dir_name_in_buffer(start, name) {
                continue;
            }
            let record_len = align_up(8 + name.len(), 4);
            if offset + record_len > block::SECTOR_SIZE * 16 {
                return Some(offset);
            }
            let buffer = &mut *ptr::addr_of_mut!(DIR_BUFFER);
            write_u32(buffer, offset, 0x8000_0000 | file.path_len as u32);
            write_u16(buffer, offset + 4, name.len() as u16);
            write_u16(buffer, offset + 6, KIND_FILE);
            buffer[offset + 8..offset + 8 + name.len()].copy_from_slice(name);
            for index in offset + 8 + name.len()..offset + record_len {
                buffer[index] = 0;
            }
            offset += record_len;
        }
    }
    Some(offset)
}

fn ram_child_name<'a>(dir: &[u8], file_path: &'a [u8]) -> Option<&'a [u8]> {
    if dir == b"/" {
        let rest = file_path.get(1..)?;
        if rest.is_empty() || rest.contains(&b'/') {
            return None;
        }
        return Some(rest);
    }
    if file_path.len() <= dir.len() + 1
        || &file_path[..dir.len()] != dir
        || file_path[dir.len()] != b'/'
    {
        return None;
    }
    let rest = &file_path[dir.len() + 1..];
    if rest.is_empty() || rest.contains(&b'/') {
        return None;
    }
    Some(rest)
}

unsafe fn find_dir_name_in_buffer(size: usize, name: &[u8]) -> bool {
    let buffer = &*ptr::addr_of!(DIR_BUFFER);
    let mut offset = 0usize;
    while offset + 8 <= size {
        let name_len = read_u16(buffer, offset + 4).unwrap_or(0) as usize;
        let next = align_up(offset + 8 + name_len, 4);
        if next > size {
            break;
        }
        if buffer.get(offset + 8..offset + 8 + name_len) == Some(name) {
            return true;
        }
        offset = next;
    }
    false
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
    if dir.size > block::SECTOR_SIZE * 16 {
        return None;
    }
    read_extent(
        dir.extent_start,
        dir.size,
        ptr::addr_of_mut!(DIR_BUFFER).cast(),
        block::SECTOR_SIZE * 16,
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
    let lba = byte_offset / block::SECTOR_SIZE;
    let offset = byte_offset % block::SECTOR_SIZE;
    let mut sector = [0; block::SECTOR_SIZE];
    if !block::read_sector(lba as u32, &mut sector) {
        return None;
    }
    if offset + INODE_SIZE > block::SECTOR_SIZE {
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
    let sectors = align_up(size, block::SECTOR_SIZE) / block::SECTOR_SIZE;
    let mut written = 0usize;
    while written < size {
        let remaining_sectors = sectors - (written / block::SECTOR_SIZE);
        let chunk_sectors = remaining_sectors.min(128);
        let chunk_bytes = chunk_sectors * block::SECTOR_SIZE;
        let buffer = unsafe {
            core::slice::from_raw_parts_mut(ptr::addr_of_mut!(EXTENT_BUFFER).cast(), chunk_bytes)
        };
        if !block::read_sectors(
            start_block + (written / block::SECTOR_SIZE) as u32,
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

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    let raw = value.to_le_bytes();
    bytes[offset] = raw[0];
    bytes[offset + 1] = raw[1];
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    let raw = value.to_le_bytes();
    bytes[offset] = raw[0];
    bytes[offset + 1] = raw[1];
    bytes[offset + 2] = raw[2];
    bytes[offset + 3] = raw[3];
}
