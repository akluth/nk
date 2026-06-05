use crate::arch;

const CONFIG_ADDRESS: u16 = 0xcf8;
const CONFIG_DATA: u16 = 0xcfc;
const INVALID_VENDOR: u16 = 0xffff;

#[derive(Clone, Copy)]
pub struct Device {
    pub bus: u8,
    pub slot: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
}

pub trait Visitor {
    fn visit(&mut self, device: Device);
}

pub fn scan<V: Visitor>(visitor: &mut V) {
    for bus in 0..=255 {
        for slot in 0..32 {
            scan_function(visitor, bus, slot, 0);

            let header_type = read_u8(bus, slot, 0, 0x0e);
            if header_type & 0x80 != 0 {
                for function in 1..8 {
                    scan_function(visitor, bus, slot, function);
                }
            }
        }
    }
}

fn scan_function<V: Visitor>(visitor: &mut V, bus: u8, slot: u8, function: u8) {
    let vendor_id = read_u16(bus, slot, function, 0x00);
    if vendor_id == INVALID_VENDOR {
        return;
    }

    visitor.visit(Device {
        bus,
        slot,
        function,
        vendor_id,
        device_id: read_u16(bus, slot, function, 0x02),
        class: read_u8(bus, slot, function, 0x0b),
        subclass: read_u8(bus, slot, function, 0x0a),
    });
}

fn read_u8(bus: u8, slot: u8, function: u8, offset: u8) -> u8 {
    let value = read_u32(bus, slot, function, offset & !0x03);
    (value >> ((offset & 0x03) * 8)) as u8
}

fn read_u16(bus: u8, slot: u8, function: u8, offset: u8) -> u16 {
    let value = read_u32(bus, slot, function, offset & !0x03);
    (value >> ((offset & 0x02) * 8)) as u16
}

fn read_u32(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    let address = 0x8000_0000
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xfc);

    unsafe {
        arch::outl(CONFIG_ADDRESS, address);
        arch::inl(CONFIG_DATA)
    }
}
