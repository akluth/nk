use core::arch::asm;

use crate::serial;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_DATA_SELECTOR: u16 = 0x1b;
pub const USER_CODE_SELECTOR: u16 = 0x23;
const TSS_SELECTOR: u16 = 0x28;

#[repr(C, packed)]
struct GdtPointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct Tss {
    reserved0: u32,
    rsp: [u64; 3],
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    io_map_base: u16,
}

static mut TSS: Tss = Tss {
    reserved0: 0,
    rsp: [0; 3],
    reserved1: 0,
    ist: [0; 7],
    reserved2: 0,
    reserved3: 0,
    io_map_base: core::mem::size_of::<Tss>() as u16,
};

#[repr(align(16))]
struct Stack {
    bytes: [u8; 16 * 1024],
}

static mut KERNEL_STACK: Stack = Stack {
    bytes: [0; 16 * 1024],
};
static mut IST_STACK: Stack = Stack {
    bytes: [0; 16 * 1024],
};
static mut GDT: [u64; 7] = [0; 7];

pub fn init() {
    unsafe {
        let kernel_stack_top = stack_top(core::ptr::addr_of!(KERNEL_STACK));
        let ist_stack_top = stack_top(core::ptr::addr_of!(IST_STACK));
        TSS.rsp[0] = kernel_stack_top;
        TSS.ist[0] = ist_stack_top;

        GDT[0] = 0;
        GDT[1] = code_descriptor(0);
        GDT[2] = data_descriptor(0);
        GDT[3] = data_descriptor(3);
        GDT[4] = code_descriptor(3);
        set_tss_descriptor(
            5,
            core::ptr::addr_of!(TSS) as u64,
            core::mem::size_of::<Tss>() as u32 - 1,
        );

        let pointer = GdtPointer {
            limit: (core::mem::size_of::<[u64; 7]>() - 1) as u16,
            base: core::ptr::addr_of!(GDT) as u64,
        };

        asm!("lgdt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
        reload_segments();
        asm!("ltr ax", in("ax") TSS_SELECTOR, options(nostack, preserves_flags));
    }

    serial::write_line("nk: gdt/tss ready");
}

fn code_descriptor(dpl: u64) -> u64 {
    (1 << 43) | (1 << 44) | (dpl << 45) | (1 << 47) | (1 << 53)
}

fn data_descriptor(dpl: u64) -> u64 {
    (1 << 41) | (1 << 44) | (dpl << 45) | (1 << 47)
}

unsafe fn set_tss_descriptor(index: usize, base: u64, limit: u32) {
    GDT[index] = (limit as u64 & 0xffff)
        | ((base & 0x00ff_ffff) << 16)
        | (0x89 << 40)
        | (((limit as u64 >> 16) & 0x0f) << 48)
        | (((base >> 24) & 0xff) << 56);
    GDT[index + 1] = base >> 32;
}

unsafe fn reload_segments() {
    asm!(
        "push {code}",
        "lea rax, [rip + 2f]",
        "push rax",
        "retfq",
        "2:",
        "mov ax, {data}",
        "mov ds, ax",
        "mov es, ax",
        "mov ss, ax",
        "mov fs, ax",
        "mov gs, ax",
        code = const KERNEL_CODE_SELECTOR as u64,
        data = const KERNEL_DATA_SELECTOR,
        out("rax") _,
        options(preserves_flags)
    );
}

unsafe fn stack_top(stack: *const Stack) -> u64 {
    let bytes = core::ptr::addr_of!((*stack).bytes);
    bytes as u64 + core::mem::size_of::<Stack>() as u64
}

pub fn user_selectors() -> (u16, u16) {
    (USER_CODE_SELECTOR, USER_DATA_SELECTOR)
}

pub fn kernel_stack_top() -> u64 {
    unsafe { stack_top(core::ptr::addr_of!(KERNEL_STACK)) }
}
