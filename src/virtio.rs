use core::sync::atomic::{compiler_fence, Ordering};

use crate::{arch, memory, pci, serial};

const VIRTIO_VENDOR_ID: u16 = 0x1af4;
const VIRTIO_PCI_CAP_ID: u8 = 0x09;

const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

const QUEUE_SIZE: u16 = 256;
const SECTOR_SIZE: usize = 512;

const LEGACY_DEVICE_FEATURES: u16 = 0;
const LEGACY_GUEST_FEATURES: u16 = 4;
const LEGACY_QUEUE_PFN: u16 = 8;
const LEGACY_QUEUE_NUM: u16 = 12;
const LEGACY_QUEUE_SEL: u16 = 14;
const LEGACY_QUEUE_NOTIFY: u16 = 16;
const LEGACY_DEVICE_STATUS: u16 = 18;
const LEGACY_ISR_STATUS: u16 = 19;

const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;

const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;

const VIRTIO_BLK_T_IN: u32 = 0;

pub fn init() {
    let mut visitor = VirtioVisitor { devices: 0 };
    pci::scan(&mut visitor);

    serial::write_str("nk: virtio scan complete, devices=");
    serial::write_dec_u8(visitor.devices);
    serial::write_line("");
}

pub fn block_ready() -> bool {
    unsafe { BLOCK.ready }
}

pub fn read_block_sectors(lba: u32, sectors: usize, out: &mut [u8]) -> bool {
    if sectors == 0 || out.len() < sectors * SECTOR_SIZE {
        return false;
    }
    unsafe { BLOCK.read(lba as u64, sectors, out) }
}

struct VirtioVisitor {
    devices: u8,
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C, align(2))]
struct AvailableRing {
    flags: u16,
    idx: u16,
    ring: [u16; QUEUE_SIZE as usize],
}

#[repr(C, align(4))]
#[derive(Clone, Copy)]
struct UsedElem {
    id: u32,
    len: u32,
}

#[repr(C, align(4))]
struct UsedRing {
    flags: u16,
    idx: u16,
    ring: [UsedElem; QUEUE_SIZE as usize],
}

#[repr(align(4096))]
struct QueueMemory {
    descriptors: [Descriptor; QUEUE_SIZE as usize],
    available: AvailableRing,
    used: UsedRing,
}

static mut QUEUES: [QueueMemory; 2] = [QueueMemory::new(), QueueMemory::new()];
static mut BLOCK_QUEUE: LegacyQueue = LegacyQueue::new();
static mut BLOCK_DMA: BlockDma = BlockDma::new();
static mut BLOCK: LegacyBlock = LegacyBlock::empty();

impl QueueMemory {
    const fn new() -> Self {
        Self {
            descriptors: [Descriptor {
                addr: 0,
                len: 0,
                flags: 0,
                next: 0,
            }; QUEUE_SIZE as usize],
            available: AvailableRing {
                flags: 0,
                idx: 0,
                ring: [0; QUEUE_SIZE as usize],
            },
            used: UsedRing {
                flags: 0,
                idx: 0,
                ring: [UsedElem { id: 0, len: 0 }; QUEUE_SIZE as usize],
            },
        }
    }
}

impl pci::Visitor for VirtioVisitor {
    fn visit(&mut self, device: pci::Device) {
        if device.vendor_id != VIRTIO_VENDOR_ID {
            return;
        }

        self.devices = self.devices.saturating_add(1);
        serial::write_str("nk: virtio pci device ");
        serial::write_dec_u8(device.bus);
        serial::write_str(":");
        serial::write_dec_u8(device.slot);
        serial::write_str(".");
        serial::write_dec_u8(device.function);
        serial::write_str(" id=");
        serial::write_hex_u16(device.device_id);
        serial::write_str(" class=");
        serial::write_hex_u16(((device.class as u16) << 8) | device.subclass as u16);
        serial::write_line("");

        classify_device(device.device_id);
        inspect_caps(device);
        try_init_legacy_block(device);
        prepare_queue((self.devices - 1) as usize);
    }
}

fn classify_device(device_id: u16) {
    match device_id {
        0x1001 | 0x1042 => serial::write_line("nk: virtio block skeleton ready"),
        0x1052 => serial::write_line("nk: virtio input skeleton ready"),
        _ => serial::write_line("nk: virtio generic skeleton ready"),
    }
}

#[repr(C, align(4096))]
struct LegacyQueue {
    bytes: [u8; 16384],
}

impl LegacyQueue {
    const fn new() -> Self {
        Self { bytes: [0; 16384] }
    }

    fn clear(&mut self) {
        self.bytes = [0; 16384];
    }

    fn descriptors(&mut self) -> *mut Descriptor {
        self.bytes.as_mut_ptr().cast()
    }

    fn available(&mut self) -> *mut AvailableRing {
        unsafe { self.bytes.as_mut_ptr().add(16 * QUEUE_SIZE as usize).cast() }
    }

    fn used(&mut self) -> *mut UsedRing {
        unsafe { self.bytes.as_mut_ptr().add(8192).cast() }
    }
}

#[repr(C, align(16))]
struct BlockRequest {
    request_type: u32,
    reserved: u32,
    sector: u64,
}

struct LegacyBlock {
    io_base: u16,
    queue_phys: u64,
    ready: bool,
    last_used: u16,
    request: BlockRequest,
    status: u8,
    logged_failure: bool,
}

#[repr(C, align(4096))]
struct BlockDma {
    bytes: [u8; 128 * SECTOR_SIZE],
}

impl BlockDma {
    const fn new() -> Self {
        Self {
            bytes: [0; 128 * SECTOR_SIZE],
        }
    }
}

impl LegacyBlock {
    const fn empty() -> Self {
        Self {
            io_base: 0,
            queue_phys: 0,
            ready: false,
            last_used: 0,
            request: BlockRequest {
                request_type: 0,
                reserved: 0,
                sector: 0,
            },
            status: 0xff,
            logged_failure: false,
        }
    }

    unsafe fn read(&mut self, lba: u64, sectors: usize, out: &mut [u8]) -> bool {
        if !self.ready || sectors > 128 {
            return false;
        }
        let byte_len = sectors * SECTOR_SIZE;
        let Some(request_phys) = memory::kernel_virt_to_phys((&self.request as *const _) as u64)
        else {
            return false;
        };
        let Some(status_phys) = memory::kernel_virt_to_phys((&self.status as *const _) as u64)
        else {
            return false;
        };
        let dma = &mut BLOCK_DMA;
        let Some(out_phys) = memory::kernel_virt_to_phys(dma.bytes.as_ptr() as u64) else {
            return false;
        };

        self.request.request_type = VIRTIO_BLK_T_IN;
        self.request.reserved = 0;
        self.request.sector = lba;
        self.status = 0xff;

        let queue = &mut BLOCK_QUEUE;
        let desc = queue.descriptors();
        core::ptr::write_volatile(desc.add(0), Descriptor {
            addr: request_phys,
            len: core::mem::size_of::<BlockRequest>() as u32,
            flags: DESC_F_NEXT,
            next: 1,
        });
        core::ptr::write_volatile(desc.add(1), Descriptor {
            addr: out_phys,
            len: byte_len as u32,
            flags: DESC_F_NEXT | DESC_F_WRITE,
            next: 2,
        });
        core::ptr::write_volatile(desc.add(2), Descriptor {
            addr: status_phys,
            len: 1,
            flags: DESC_F_WRITE,
            next: 0,
        });

        let avail = queue.available();
        let avail_idx = core::ptr::read_volatile(core::ptr::addr_of!((*avail).idx));
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*avail).ring[(avail_idx as usize) % QUEUE_SIZE as usize]),
            0,
        );
        compiler_fence(Ordering::SeqCst);
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*avail).idx),
            avail_idx.wrapping_add(1),
        );
        compiler_fence(Ordering::SeqCst);
        arch::outw(self.io_base + LEGACY_QUEUE_NOTIFY, 0);

        let used = queue.used();
        for _ in 0..100_000 {
            let used_idx = core::ptr::read_volatile(core::ptr::addr_of!((*used).idx));
            if used_idx != self.last_used {
                self.last_used = used_idx;
                let _ = arch::inb(self.io_base + LEGACY_ISR_STATUS);
                if self.status == 0 {
                    out[..byte_len].copy_from_slice(&dma.bytes[..byte_len]);
                    return true;
                }
                return false;
            }
        }
        if !self.logged_failure {
            self.logged_failure = true;
            serial::write_str("nk: virtio read timeout qpf=");
            serial::write_hex_u64(self.queue_phys);
            serial::write_str(" avail=");
            serial::write_hex_u16(core::ptr::read_volatile(core::ptr::addr_of!((*avail).idx)));
            serial::write_str(" used=");
            serial::write_hex_u16(core::ptr::read_volatile(core::ptr::addr_of!((*used).idx)));
            serial::write_str(" req_status=");
            serial::write_hex_u16(self.status as u16);
            serial::write_str(" dev_status=");
            serial::write_hex_u16(arch::inb(self.io_base + LEGACY_DEVICE_STATUS) as u16);
            serial::write_line("");
        }
        false
    }
}

fn try_init_legacy_block(device: pci::Device) {
    if unsafe { BLOCK.ready } || device.device_id != 0x1001 {
        return;
    }
    let bar = pci::read_bar(device, 0);
    if !bar.is_io || bar.base == 0 {
        return;
    }

    unsafe {
        let io_base = bar.base as u16;
        pci::write_u16(device.bus, device.slot, device.function, 0x04, 0x0005);
        arch::outb(io_base + LEGACY_DEVICE_STATUS, 0);
        arch::outb(
            io_base + LEGACY_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER,
        );
        let _device_features = arch::inl(io_base + LEGACY_DEVICE_FEATURES);
        arch::outl(io_base + LEGACY_GUEST_FEATURES, 0);
        arch::outb(
            io_base + LEGACY_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
        );
        arch::outw(io_base + LEGACY_QUEUE_SEL, 0);
        let queue_size = arch::inw(io_base + LEGACY_QUEUE_NUM);
        if queue_size < QUEUE_SIZE {
            serial::write_line("nk: virtio legacy block queue too small");
            return;
        }
        BLOCK_QUEUE.clear();
        let Some(queue_phys) = memory::kernel_virt_to_phys(BLOCK_QUEUE.bytes.as_ptr() as u64)
        else {
            serial::write_line("nk: virtio legacy block queue phys missing");
            return;
        };
        arch::outl(io_base + LEGACY_QUEUE_PFN, 0);
        arch::outl(io_base + LEGACY_QUEUE_PFN, (queue_phys >> 12) as u32);
        arch::outb(
            io_base + LEGACY_DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );
        BLOCK.io_base = io_base;
        BLOCK.queue_phys = queue_phys;
        BLOCK.ready = true;
        BLOCK.last_used = 0;
        serial::write_str("nk: virtio legacy block ready io=");
        serial::write_hex_u16(io_base);
        serial::write_str(" q=");
        serial::write_hex_u16(queue_size);
        serial::write_line("");
    }
}

fn inspect_caps(device: pci::Device) {
    pci::visit_capabilities(device, |cap| {
        if cap.id != VIRTIO_PCI_CAP_ID {
            return;
        }

        let cfg_type = pci::read_cap_u8(device, cap.offset, 3);
        let bar = pci::read_cap_u8(device, cap.offset, 4);
        let offset = pci::read_cap_u32(device, cap.offset, 8);
        let bar_info = pci::read_bar(device, bar);

        serial::write_str("nk: virtio cap ");
        serial::write_dec_u8(cfg_type);
        serial::write_str(" bar=");
        serial::write_dec_u8(bar);
        serial::write_str(" base=");
        serial::write_hex_u16((bar_info.base & 0xffff) as u16);
        serial::write_str(if bar_info.is_io { " io" } else { " mmio" });
        let _raw = bar_info.raw;
        serial::write_str(" off=");
        serial::write_hex_u16((offset & 0xffff) as u16);
        match cfg_type {
            VIRTIO_PCI_CAP_COMMON_CFG => serial::write_line(" common"),
            VIRTIO_PCI_CAP_NOTIFY_CFG => serial::write_line(" notify"),
            VIRTIO_PCI_CAP_ISR_CFG => serial::write_line(" isr"),
            VIRTIO_PCI_CAP_DEVICE_CFG => serial::write_line(" device"),
            _ => serial::write_line(" other"),
        }
    });
}

fn prepare_queue(index: usize) {
    if index >= 2 {
        return;
    }

    unsafe {
        let queue = &mut QUEUES[index];
        queue.available.idx = 0;
        queue.used.idx = 0;
        queue.descriptors[0].len = 0;
        serial::write_str("nk: virtio queue memory ready q=");
        serial::write_dec_u8(index as u8);
        serial::write_str(" desc=");
        serial::write_hex_u16((queue.descriptors.as_ptr() as u64 & 0xffff) as u16);
        serial::write_line("");
    }
}
