use crate::{pci, serial};

const VIRTIO_VENDOR_ID: u16 = 0x1af4;

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
    }
}
