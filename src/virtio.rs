use crate::{pci, serial};

const VIRTIO_VENDOR_ID: u16 = 0x1af4;
const VIRTIO_PCI_CAP_ID: u8 = 0x09;

const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

const QUEUE_SIZE: u16 = 8;

pub fn init() {
    let mut visitor = VirtioVisitor { devices: 0 };
    pci::scan(&mut visitor);

    serial::write_str("nk: virtio scan complete, devices=");
    serial::write_dec_u8(visitor.devices);
    serial::write_line("");
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

static mut QUEUES: [QueueMemory; 2] = [
    QueueMemory::new(),
    QueueMemory::new(),
];

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
