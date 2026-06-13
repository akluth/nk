use core::ptr;

use crate::{block, serial, services};

const MAGIC: &[u8; 8] = b"NKFSv1\0\0";
const VERSION: u32 = 1;
const INODE_SIZE: usize = 128;
const MAX_FILE_SIZE: usize = 20 * 1024 * 1024;
const SMALL_FILE_CACHE_SIZE: usize = 512 * 1024;
const RAM_FILE_CAP: usize = 256 * 1024;
const RAM_FILE_COUNT: usize = 32;
const MAX_INODES: u32 = 256;

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
    inode: u32,
    extent_start: u32,
    extent_blocks: u32,
    dirty: bool,
}

impl RamFile {
    const fn empty() -> Self {
        Self {
            used: false,
            path: [0; 256],
            path_len: 0,
            data: [0; RAM_FILE_CAP],
            len: 0,
            inode: 0,
            extent_start: 0,
            extent_blocks: 0,
            dirty: false,
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
    inode_table_blocks: u32,
    data_start: u32,
    root_inode: u32,
    next_free_block: u32,
    total_blocks: u32,
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
    extent_blocks: u32,
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
        let kind = read_u16(data, offset + 6).unwrap_or(0);
        if kind != 0 && name != b"." && name != b".." {
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
    let fs = mount()?;
    let path = normalized_path_bytes(path)?;
    let inode = resolve_path(fs, path);
    unsafe {
        let files = &mut *RAM_FILES.0.get();
        for index in 0..RAM_FILE_COUNT {
            if !files[index].used {
                files[index].used = true;
                files[index].path[..path.len()].copy_from_slice(path);
                files[index].path_len = path.len();
                if let Some(inode) = inode {
                    if inode.kind != KIND_FILE || inode.size > RAM_FILE_CAP {
                        files[index] = RamFile::empty();
                        return None;
                    }
                    files[index].inode = inode.number;
                    files[index].extent_start = inode.extent_start;
                    files[index].extent_blocks = inode.extent_blocks;
                    if truncate {
                        files[index].len = 0;
                        files[index].dirty = true;
                        if !flush_writable_file(index) {
                            files[index] = RamFile::empty();
                            return None;
                        }
                    } else {
                        files[index].len = inode.size;
                        if inode.size > 0 {
                            read_extent(
                                inode.extent_start,
                                inode.size,
                                files[index].data.as_mut_ptr(),
                                RAM_FILE_CAP,
                            )?;
                        }
                    }
                    return Some(index);
                }

                let Some((inode_number, extent_start)) = create_disk_file(fs, path) else {
                    files[index] = RamFile::empty();
                    return None;
                };
                files[index].inode = inode_number;
                files[index].extent_start = extent_start;
                files[index].extent_blocks = 1;
                files[index].len = 0;
                files[index].dirty = false;
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
        file.dirty = true;
        if !flush_writable_file(index) {
            return None;
        }
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
        file.dirty = true;
        flush_writable_file(index)
    }
}

pub fn ram_file_index(path: &[u8]) -> Option<usize> {
    ram_find(path)
}

pub fn remove_ram_file(path: &[u8]) -> bool {
    let path = normalized_path_bytes(path).unwrap_or(path);
    if !remove_disk_file(path) {
        return false;
    }
    unsafe {
        if let Some(index) = ram_find(path) {
            (*RAM_FILES.0.get())[index] = RamFile::empty();
        }
    }
    true
}

pub fn create_dir(path: &[u8]) -> bool {
    let Some(fs) = mount() else {
        return false;
    };
    let Some(path) = normalized_path_bytes(path) else {
        return false;
    };
    if path == b"/" {
        return false;
    }
    let Some((parent_path, name)) = split_parent_name(path) else {
        return false;
    };
    let Some(parent) = resolve_path(fs, parent_path) else {
        return false;
    };
    if parent.kind != KIND_DIR || find_dir_entry(parent, name).is_some() {
        return false;
    }

    let Some((fs, inode_number)) = allocate_inode(fs) else {
        return false;
    };
    let Some((fs, extent_start)) = allocate_blocks(fs, 1) else {
        return false;
    };

    let Some(size) = write_empty_dir(extent_start, inode_number, parent.number) else {
        return false;
    };
    let inode = Inode {
        number: inode_number,
        kind: KIND_DIR,
        size,
        extent_start,
        extent_blocks: 1,
    };
    if !write_inode(fs, inode) || !append_dir_entry(fs, parent, name, inode_number, KIND_DIR) {
        return false;
    }
    clear_caches();
    true
}

pub fn remove_dir(path: &[u8]) -> bool {
    let Some(fs) = mount() else {
        return false;
    };
    let Some(path) = normalized_path_bytes(path) else {
        return false;
    };
    if path == b"/" {
        return false;
    }
    let Some((parent_path, name)) = split_parent_name(path) else {
        return false;
    };
    let Some(parent) = resolve_path(fs, parent_path) else {
        return false;
    };
    let Some(inode_number) = find_dir_entry(parent, name) else {
        return false;
    };
    let Some(inode) = read_inode(fs, inode_number) else {
        return false;
    };
    if inode.kind != KIND_DIR || !dir_is_empty(inode) {
        return false;
    }
    if remove_dir_entry(parent, name, KIND_DIR).is_none() || !release_inode(fs, inode) {
        return false;
    }
    clear_caches();
    true
}

pub fn rename_path(old_path: &[u8], new_path: &[u8]) -> bool {
    let Some(fs) = mount() else {
        return false;
    };
    let Some(old_path) = normalized_path_bytes(old_path) else {
        return false;
    };
    let Some(new_path) = normalized_path_bytes(new_path) else {
        return false;
    };
    if old_path == b"/" || new_path == b"/" || old_path == new_path {
        return false;
    }
    let Some((old_parent_path, old_name)) = split_parent_name(old_path) else {
        return false;
    };
    let Some((new_parent_path, new_name)) = split_parent_name(new_path) else {
        return false;
    };
    let Some(old_parent) = resolve_path(fs, old_parent_path) else {
        return false;
    };
    let Some(new_parent) = resolve_path(fs, new_parent_path) else {
        return false;
    };
    if old_parent.kind != KIND_DIR
        || new_parent.kind != KIND_DIR
        || find_dir_entry(new_parent, new_name).is_some()
    {
        return false;
    }
    let Some(inode_number) = find_dir_entry(old_parent, old_name) else {
        return false;
    };
    let Some(inode) = read_inode(fs, inode_number) else {
        return false;
    };
    if inode.kind != KIND_FILE && inode.kind != KIND_DIR {
        return false;
    }
    if inode.kind == KIND_DIR && path_is_descendant(new_path, old_path) {
        return false;
    }
    if !append_dir_entry(fs, new_parent, new_name, inode_number, inode.kind) {
        return false;
    }
    if remove_dir_entry(old_parent, old_name, inode.kind).is_none() {
        let _ = remove_dir_entry(new_parent, new_name, inode.kind);
        return false;
    }
    update_ram_paths(old_path, new_path);
    clear_caches();
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
        inode_table_blocks: read_u32(&sector, 24)?,
        data_start: read_u32(&sector, 28)?,
        root_inode: read_u32(&sector, 32)?,
        next_free_block: read_u32(&sector, 36)?,
        total_blocks: read_u32(&sector, 40).unwrap_or(read_u32(&sector, 36)?),
    };
    unsafe {
        MOUNT_CACHE = Some(fs);
    }
    Some(fs)
}

fn write_mount(fs: Superblock) -> bool {
    let mut sector = [0u8; block::SECTOR_SIZE];
    if !block::read_sector(0, &mut sector) {
        return false;
    }
    write_u32(&mut sector, 16, fs.inode_count);
    write_u32(&mut sector, 20, fs.inode_table_start);
    write_u32(&mut sector, 24, fs.inode_table_blocks);
    write_u32(&mut sector, 28, fs.data_start);
    write_u32(&mut sector, 32, fs.root_inode);
    write_u32(&mut sector, 36, fs.next_free_block);
    write_u32(&mut sector, 40, fs.total_blocks);
    if !block::write_sector(0, &sector) {
        return false;
    }
    unsafe {
        MOUNT_CACHE = Some(fs);
    }
    true
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

fn create_disk_file(fs: Superblock, path: &[u8]) -> Option<(u32, u32)> {
    let (parent_path, name) = split_parent_name(path)?;
    let parent = resolve_path(fs, parent_path)?;
    if parent.kind != KIND_DIR || find_dir_entry(parent, name).is_some() {
        return None;
    }

    let (fs, inode_number) = allocate_inode(fs)?;
    let (fs, extent_start) = allocate_blocks(fs, 1)?;
    let inode = Inode {
        number: inode_number,
        kind: KIND_FILE,
        size: 0,
        extent_start,
        extent_blocks: 1,
    };
    if !write_inode(fs, inode) || !append_dir_entry(fs, parent, name, inode_number, KIND_FILE) {
        return None;
    }

    let zero = [0u8; block::SECTOR_SIZE];
    if !block::write_sector(extent_start, &zero) {
        return None;
    }
    Some((inode_number, extent_start))
}

fn split_parent_name(path: &[u8]) -> Option<(&[u8], &[u8])> {
    let path = normalized_path_bytes(path)?;
    let mut slash = 0usize;
    for index in 1..path.len() {
        if path[index] == b'/' {
            slash = index;
        }
    }
    let parent = if slash == 0 { b"/" } else { &path[..slash] };
    let name = &path[slash + 1..];
    if name.is_empty() || name.contains(&b'/') || name.len() > 255 {
        return None;
    }
    Some((parent, name))
}

fn append_dir_entry(fs: Superblock, mut dir: Inode, name: &[u8], inode: u32, kind: u16) -> bool {
    if dir.kind != KIND_DIR || dir.size > block::SECTOR_SIZE * 16 {
        return false;
    }
    let record_len = align_up(8 + name.len(), 4);
    if read_extent(
        dir.extent_start,
        dir.size,
        ptr::addr_of_mut!(DIR_BUFFER).cast(),
        block::SECTOR_SIZE * 16,
    )
    .is_none()
    {
        return false;
    }
    unsafe {
        let buffer = &mut *ptr::addr_of_mut!(DIR_BUFFER);
        let mut offset = 0usize;
        while offset + 8 <= dir.size {
            let name_len = read_u16(buffer, offset + 4).unwrap_or(0) as usize;
            let entry_kind = read_u16(buffer, offset + 6).unwrap_or(0);
            let entry_len = align_up(8 + name_len, 4);
            let next = offset + entry_len;
            if next > dir.size {
                return false;
            }
            if entry_kind == 0 && entry_len >= record_len {
                write_dir_record(buffer, offset, entry_len, name, inode, kind);
                if !write_extent(dir.extent_start, dir.size, &buffer[..dir.size]) {
                    return false;
                }
                clear_caches();
                return true;
            }
            offset = next;
        }
        let next_size = dir.size + record_len;
        if next_size > dir.extent_blocks as usize * block::SECTOR_SIZE
            || next_size > block::SECTOR_SIZE * 16
        {
            return false;
        }
        write_dir_record(buffer, dir.size, record_len, name, inode, kind);
        if !write_extent(dir.extent_start, next_size, &buffer[..next_size]) {
            return false;
        }
        dir.size = next_size;
    }
    write_inode(fs, dir)
}

fn remove_disk_file(path: &[u8]) -> bool {
    let Some(fs) = mount() else {
        return false;
    };
    let Some((parent_path, name)) = split_parent_name(path) else {
        return false;
    };
    let Some(parent) = resolve_path(fs, parent_path) else {
        return false;
    };
    let Some(inode_number) = remove_dir_entry(parent, name, KIND_FILE) else {
        return false;
    };
    let Some(inode) = read_inode(fs, inode_number) else {
        return false;
    };
    if inode.kind != KIND_FILE {
        return false;
    }
    release_inode(fs, inode)
}

fn remove_dir_entry(dir: Inode, name: &[u8], expected_kind: u16) -> Option<u32> {
    if dir.kind != KIND_DIR || dir.size > block::SECTOR_SIZE * 16 {
        return None;
    }
    read_extent(
        dir.extent_start,
        dir.size,
        ptr::addr_of_mut!(DIR_BUFFER).cast(),
        block::SECTOR_SIZE * 16,
    )?;
    unsafe {
        let buffer = &mut *ptr::addr_of_mut!(DIR_BUFFER);
        let mut offset = 0usize;
        while offset + 8 <= dir.size {
            let inode = read_u32(buffer, offset)?;
            let name_len = read_u16(buffer, offset + 4)? as usize;
            let kind = read_u16(buffer, offset + 6)?;
            let next = align_up(offset + 8 + name_len, 4);
            if next > dir.size {
                return None;
            }
            if kind == expected_kind && buffer.get(offset + 8..offset + 8 + name_len) == Some(name)
            {
                write_u16(buffer, offset + 6, 0);
                if write_extent(dir.extent_start, dir.size, &buffer[..dir.size]) {
                    return Some(inode);
                }
                return None;
            }
            offset = next;
        }
    }
    None
}

fn allocate_inode(mut fs: Superblock) -> Option<(Superblock, u32)> {
    for inode_number in 1..=fs.inode_count {
        let Some(inode) = read_inode(fs, inode_number) else {
            continue;
        };
        if inode.kind == 0 {
            return Some((fs, inode_number));
        }
    }

    let max_inode = fs.inode_table_blocks * block::SECTOR_SIZE as u32 / INODE_SIZE as u32;
    if fs.inode_count >= max_inode || fs.inode_count >= MAX_INODES {
        return None;
    }
    fs.inode_count += 1;
    if write_mount(fs) {
        Some((fs, fs.inode_count))
    } else {
        None
    }
}

fn release_inode(fs: Superblock, inode: Inode) -> bool {
    write_inode(
        fs,
        Inode {
            number: inode.number,
            kind: 0,
            size: 0,
            extent_start: inode.extent_start,
            extent_blocks: inode.extent_blocks,
        },
    )
}

fn write_empty_dir(extent_start: u32, self_inode: u32, parent_inode: u32) -> Option<usize> {
    let mut data = [0u8; block::SECTOR_SIZE];
    let mut offset = 0usize;
    offset += write_dir_record(&mut data, offset, align_up(9, 4), b".", self_inode, KIND_DIR);
    offset += write_dir_record(
        &mut data,
        offset,
        align_up(10, 4),
        b"..",
        parent_inode,
        KIND_DIR,
    );
    if write_extent(extent_start, offset, &data[..offset]) {
        Some(offset)
    } else {
        None
    }
}

fn write_dir_record(
    buffer: &mut [u8],
    offset: usize,
    record_len: usize,
    name: &[u8],
    inode: u32,
    kind: u16,
) -> usize {
    write_u32(buffer, offset, inode);
    write_u16(buffer, offset + 4, name.len() as u16);
    write_u16(buffer, offset + 6, kind);
    buffer[offset + 8..offset + 8 + name.len()].copy_from_slice(name);
    for byte in &mut buffer[offset + 8 + name.len()..offset + record_len] {
        *byte = 0;
    }
    record_len
}

fn dir_is_empty(dir: Inode) -> bool {
    if dir.kind != KIND_DIR || dir.size > block::SECTOR_SIZE * 16 {
        return false;
    }
    if read_extent(
        dir.extent_start,
        dir.size,
        ptr::addr_of_mut!(DIR_BUFFER).cast(),
        block::SECTOR_SIZE * 16,
    )
    .is_none()
    {
        return false;
    }
    unsafe {
        let buffer = &*ptr::addr_of!(DIR_BUFFER);
        let mut offset = 0usize;
        while offset + 8 <= dir.size {
            let name_len = read_u16(buffer, offset + 4).unwrap_or(0) as usize;
            let kind = read_u16(buffer, offset + 6).unwrap_or(0);
            let next = align_up(offset + 8 + name_len, 4);
            if next > dir.size {
                return false;
            }
            let name = buffer.get(offset + 8..offset + 8 + name_len).unwrap_or(b"");
            if kind != 0 && name != b"." && name != b".." {
                return false;
            }
            offset = next;
        }
    }
    true
}

fn path_is_descendant(path: &[u8], parent: &[u8]) -> bool {
    path.len() > parent.len()
        && path.starts_with(parent)
        && (parent == b"/" || path.get(parent.len()) == Some(&b'/'))
}

fn update_ram_paths(old_path: &[u8], new_path: &[u8]) {
    unsafe {
        let files = &mut *RAM_FILES.0.get();
        for file in files.iter_mut() {
            if !file.used {
                continue;
            }
            if file.path_len == old_path.len() && &file.path[..file.path_len] == old_path {
                if new_path.len() <= file.path.len() {
                    file.path[..new_path.len()].copy_from_slice(new_path);
                    file.path_len = new_path.len();
                }
            } else if file.path_len > old_path.len()
                && file.path.starts_with(old_path)
                && file.path.get(old_path.len()) == Some(&b'/')
            {
                let suffix_len = file.path_len - old_path.len();
                if new_path.len() + suffix_len <= file.path.len() {
                    let mut next = [0u8; 256];
                    next[..new_path.len()].copy_from_slice(new_path);
                    next[new_path.len()..new_path.len() + suffix_len]
                        .copy_from_slice(&file.path[old_path.len()..file.path_len]);
                    file.path[..new_path.len() + suffix_len]
                        .copy_from_slice(&next[..new_path.len() + suffix_len]);
                    file.path_len = new_path.len() + suffix_len;
                }
            }
        }
    }
}

fn clear_caches() {
    unsafe {
        FILE_CACHE_INODE = 0;
        SMALL_FILE_CACHE_INODE = 0;
    }
}

fn flush_writable_file(index: usize) -> bool {
    if index >= RAM_FILE_COUNT {
        return false;
    }
    let Some(mut fs) = mount() else {
        return false;
    };
    unsafe {
        let files = &mut *RAM_FILES.0.get();
        let file = &mut files[index];
        if !file.used || file.inode == 0 {
            return false;
        }
        let needed_blocks = align_up(file.len.max(1), block::SECTOR_SIZE) / block::SECTOR_SIZE;
        if needed_blocks > file.extent_blocks as usize {
            let Some((next_fs, extent_start)) = allocate_blocks(fs, needed_blocks as u32) else {
                return false;
            };
            fs = next_fs;
            file.extent_start = extent_start;
            file.extent_blocks = needed_blocks as u32;
        }
        if !write_extent(file.extent_start, file.len, &file.data[..file.len]) {
            return false;
        }
        let inode = Inode {
            number: file.inode,
            kind: KIND_FILE,
            size: file.len,
            extent_start: file.extent_start,
            extent_blocks: file.extent_blocks,
        };
        if !write_inode(fs, inode) {
            return false;
        }
        FILE_CACHE_INODE = 0;
        SMALL_FILE_CACHE_INODE = 0;
        file.dirty = false;
    }
    true
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
        let kind = read_u16(buffer, offset + 6).unwrap_or(0);
        if kind != 0 && buffer.get(offset + 8..offset + 8 + name_len) == Some(name) {
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
        extent_blocks: read_u32(&sector, offset + 20)?,
    })
}

fn write_inode(fs: Superblock, inode: Inode) -> bool {
    if inode.number == 0
        || inode.number > fs.inode_count
        || inode.number > fs.inode_table_blocks * block::SECTOR_SIZE as u32 / INODE_SIZE as u32
    {
        return false;
    }
    let byte_offset = fs.inode_table_start as usize * fs.block_size as usize
        + (inode.number as usize - 1) * INODE_SIZE;
    let lba = byte_offset / block::SECTOR_SIZE;
    let offset = byte_offset % block::SECTOR_SIZE;
    if offset + INODE_SIZE > block::SECTOR_SIZE {
        return false;
    }
    let mut sector = [0u8; block::SECTOR_SIZE];
    if !block::read_sector(lba as u32, &mut sector) {
        return false;
    }
    for byte in &mut sector[offset..offset + INODE_SIZE] {
        *byte = 0;
    }
    write_u16(&mut sector, offset, inode.kind);
    write_u16(
        &mut sector,
        offset + 2,
        if inode.kind == 0 {
            0
        } else if inode.kind == KIND_DIR {
            0o040555
        } else {
            0o100755
        },
    );
    write_u32(&mut sector, offset + 4, 1);
    write_u64(&mut sector, offset + 8, inode.size as u64);
    write_u32(&mut sector, offset + 16, inode.extent_start);
    write_u32(&mut sector, offset + 20, inode.extent_blocks);
    block::write_sector(lba as u32, &sector)
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

fn write_extent(start_block: u32, size: usize, data: &[u8]) -> bool {
    if size > data.len() {
        return false;
    }
    let sectors = align_up(size.max(1), block::SECTOR_SIZE) / block::SECTOR_SIZE;
    let mut written = 0usize;
    while written < sectors * block::SECTOR_SIZE {
        let chunk_sectors = (sectors - (written / block::SECTOR_SIZE)).min(128);
        let chunk_bytes = chunk_sectors * block::SECTOR_SIZE;
        unsafe {
            let buffer = core::slice::from_raw_parts_mut(
                ptr::addr_of_mut!(EXTENT_BUFFER).cast(),
                chunk_bytes,
            );
            for byte in buffer.iter_mut() {
                *byte = 0;
            }
            let count = size.saturating_sub(written).min(chunk_bytes);
            if count > 0 {
                buffer[..count].copy_from_slice(&data[written..written + count]);
            }
            if !block::write_sectors(
                start_block + (written / block::SECTOR_SIZE) as u32,
                chunk_sectors,
                buffer,
            ) {
                return false;
            }
        }
        written += chunk_bytes;
    }
    true
}

fn allocate_blocks(mut fs: Superblock, blocks: u32) -> Option<(Superblock, u32)> {
    if blocks == 0 {
        return Some((fs, 0));
    }
    for inode_number in 1..=fs.inode_count {
        let Some(inode) = read_inode(fs, inode_number) else {
            continue;
        };
        if inode.kind != 0
            || inode.extent_start < fs.data_start
            || inode.extent_blocks < blocks
            || inode.extent_blocks == 0
        {
            continue;
        }
        let start = inode.extent_start;
        let remaining = inode.extent_blocks - blocks;
        let free_inode = Inode {
            number: inode.number,
            kind: 0,
            size: 0,
            extent_start: if remaining == 0 {
                0
            } else {
                inode.extent_start + blocks
            },
            extent_blocks: remaining,
        };
        if !write_inode(fs, free_inode) {
            return None;
        }
        return Some((fs, start));
    }
    let start = fs.next_free_block;
    let next = start.checked_add(blocks)?;
    if fs.total_blocks != 0 && next > fs.total_blocks {
        return None;
    }
    fs.next_free_block = next;
    if write_mount(fs) {
        Some((fs, start))
    } else {
        None
    }
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

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    let raw = value.to_le_bytes();
    for (index, byte) in raw.iter().enumerate() {
        bytes[offset + index] = *byte;
    }
}
