use crate::framebuffer::Framebuffer;

#[derive(Clone, Copy)]
pub struct KernelAddress {
    pub physical_base: u64,
    pub virtual_base: u64,
}

#[repr(C)]
struct FramebufferRequest {
    id: [u64; 4],
    revision: u64,
    response: *const FramebufferResponse,
}

#[repr(C)]
struct KernelAddressRequest {
    id: [u64; 4],
    revision: u64,
    response: *const KernelAddressResponse,
}

#[repr(C)]
struct HhdmRequest {
    id: [u64; 4],
    revision: u64,
    response: *const HhdmResponse,
}

#[repr(C)]
struct MemmapRequest {
    id: [u64; 4],
    revision: u64,
    response: *const MemmapResponse,
}

#[repr(C)]
struct KernelAddressResponse {
    revision: u64,
    physical_base: u64,
    virtual_base: u64,
}

#[repr(C)]
struct HhdmResponse {
    revision: u64,
    offset: u64,
}

#[repr(C)]
struct MemmapResponse {
    revision: u64,
    entry_count: u64,
    entries: *const *const LimineMemmapEntry,
}

#[repr(C)]
struct LimineMemmapEntry {
    base: u64,
    length: u64,
    kind: u64,
}

#[derive(Clone, Copy)]
pub struct MemoryRegion {
    pub base: u64,
    pub length: u64,
    pub kind: u64,
}

#[repr(C)]
struct FramebufferResponse {
    revision: u64,
    framebuffer_count: u64,
    framebuffers: *const *const LimineFramebuffer,
}

#[repr(C)]
struct LimineFramebuffer {
    address: *mut u8,
    width: u64,
    height: u64,
    pitch: u64,
    bpp: u16,
    memory_model: u8,
    red_mask_size: u8,
    red_mask_shift: u8,
    green_mask_size: u8,
    green_mask_shift: u8,
    blue_mask_size: u8,
    blue_mask_shift: u8,
    unused: [u8; 7],
    edid_size: u64,
    edid: *const u8,
    mode_count: u64,
    modes: *const u8,
}

#[used]
#[link_section = ".limine_requests_start"]
static LIMINE_REQUESTS_START: [u64; 4] = [
    0xf6b8f4b39de7d1ae,
    0xfab91a6940fcb9cf,
    0x785c6ed015d3e316,
    0x181e920a7852b9d9,
];

#[used]
#[link_section = ".limine_requests"]
static LIMINE_BASE_REVISION: [u64; 3] = [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, 0];

#[used]
#[link_section = ".limine_requests"]
static mut FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest {
    id: [
        0xc7b1dd30df4c8b88,
        0x0a82e883a194f07b,
        0x9d5827dcd881dd75,
        0xa3148604f6fab11b,
    ],
    revision: 0,
    response: core::ptr::null(),
};

#[used]
#[link_section = ".limine_requests"]
static mut KERNEL_ADDRESS_REQUEST: KernelAddressRequest = KernelAddressRequest {
    id: [
        0xc7b1dd30df4c8b88,
        0x0a82e883a194f07b,
        0x71ba76863cc55f63,
        0xb2644a48c516a487,
    ],
    revision: 0,
    response: core::ptr::null(),
};

#[used]
#[link_section = ".limine_requests"]
static mut HHDM_REQUEST: HhdmRequest = HhdmRequest {
    id: [
        0xc7b1dd30df4c8b88,
        0x0a82e883a194f07b,
        0x48dcf1cb8ad2b852,
        0x63984e959a98244b,
    ],
    revision: 0,
    response: core::ptr::null(),
};

#[used]
#[link_section = ".limine_requests"]
static mut MEMMAP_REQUEST: MemmapRequest = MemmapRequest {
    id: [
        0xc7b1dd30df4c8b88,
        0x0a82e883a194f07b,
        0x67cf3d9d378a806f,
        0xe304acdfc50c3c62,
    ],
    revision: 0,
    response: core::ptr::null(),
};

#[used]
#[link_section = ".limine_requests_end"]
static LIMINE_REQUESTS_END: [u64; 2] = [0xadc0e0531bb10d03, 0x9572709f31764c62];

pub fn framebuffer() -> Option<Framebuffer> {
    unsafe {
        let response = core::ptr::addr_of!(FRAMEBUFFER_REQUEST)
            .as_ref()?
            .response
            .as_ref()?;
        if response.framebuffer_count == 0 {
            return None;
        }

        let raw = *response.framebuffers;
        let fb = raw.as_ref()?;
        Some(Framebuffer::new(
            fb.address,
            fb.width as usize,
            fb.height as usize,
            fb.pitch as usize,
            fb.bpp as usize,
        ))
    }
}

pub fn kernel_address() -> Option<KernelAddress> {
    unsafe {
        let response = core::ptr::addr_of!(KERNEL_ADDRESS_REQUEST)
            .as_ref()?
            .response
            .as_ref()?;

        Some(KernelAddress {
            physical_base: response.physical_base,
            virtual_base: response.virtual_base,
        })
    }
}

pub fn hhdm_offset() -> Option<u64> {
    unsafe {
        Some(
            core::ptr::addr_of!(HHDM_REQUEST)
                .as_ref()?
                .response
                .as_ref()?
                .offset,
        )
    }
}

pub fn memory_region_count() -> usize {
    unsafe {
        core::ptr::addr_of!(MEMMAP_REQUEST)
            .as_ref()
            .and_then(|request| request.response.as_ref())
            .map(|response| response.entry_count as usize)
            .unwrap_or(0)
    }
}

pub fn memory_region(index: usize) -> Option<MemoryRegion> {
    unsafe {
        let response = core::ptr::addr_of!(MEMMAP_REQUEST)
            .as_ref()?
            .response
            .as_ref()?;
        if index >= response.entry_count as usize {
            return None;
        }
        let raw = *response.entries.add(index);
        let entry = raw.as_ref()?;
        Some(MemoryRegion {
            base: entry.base,
            length: entry.length,
            kind: entry.kind,
        })
    }
}
