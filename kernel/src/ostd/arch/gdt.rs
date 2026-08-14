//! Global Descriptor Table (GDT) and Task State Segment (TSS).

use core::arch::asm;
use core::mem::size_of;

pub const KERNEL_CODE_SEL: u16 = 0x08;
pub const KERNEL_DATA_SEL: u16 = 0x10;
pub const USER_DATA_SEL: u16 = 0x18 | 3;
pub const USER_CODE_SEL: u16 = 0x20 | 3;
pub const TSS_SEL: u16 = 0x28;

#[repr(C, packed)]
pub struct TSS {
    pub reserved0: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    pub reserved1: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    pub reserved2: u64,
    pub reserved3: u16,
    pub iomap_base: u16,
}

impl TSS {
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist1: 0,
            ist2: 0,
            ist3: 0,
            ist4: 0,
            ist5: 0,
            ist6: 0,
            ist7: 0,
            reserved2: 0,
            reserved3: 0,
            iomap_base: size_of::<TSS>() as u16,
        }
    }
}

impl Default for TSS {
    fn default() -> Self {
        Self::new()
    }
}

#[repr(C, packed)]
pub struct GdtDescriptor {
    pub limit: u16,
    pub base: u64,
}

#[repr(C, align(16))]
pub struct Gdt {
    pub entries: [u64; 7],
}

static mut GLOBAL_TSS: TSS = TSS::new();
static mut GLOBAL_GDT: Gdt = Gdt { entries: [0; 7] };

/// Initializes the GDT and loads the 64-bit Task State Segment (TSS).
///
/// # Safety
///
/// Must be called during single-threaded boot initialization with a valid kernel stack address.
pub unsafe fn gdt_init(kernel_stack_top: u64) {
    GLOBAL_TSS.rsp0 = kernel_stack_top;

    let tss_base = &raw const GLOBAL_TSS as u64;
    let tss_limit = (size_of::<TSS>() - 1) as u64;

    // Entry 0: Null Descriptor
    GLOBAL_GDT.entries[0] = 0;
    // Entry 1: 0x08 - Kernel Code 64-bit (Ring 0, Exec/Read)
    GLOBAL_GDT.entries[1] = 0x00AF9A000000FFFF;
    // Entry 2: 0x10 - Kernel Data (Ring 0, Read/Write)
    GLOBAL_GDT.entries[2] = 0x00CF92000000FFFF;
    // Entry 3: 0x18 - User Data (Ring 3, Read/Write)
    GLOBAL_GDT.entries[3] = 0x00CFF2000000FFFF;
    // Entry 4: 0x20 - User Code 64-bit (Ring 3, Exec/Read)
    GLOBAL_GDT.entries[4] = 0x00AFFA000000FFFF;
    // Entry 5 & 6: 0x28 - 64-bit TSS Descriptor (16 bytes)
    GLOBAL_GDT.entries[5] = (tss_limit & 0xFFFF)
        | ((tss_base & 0xFFFF) << 16)
        | (((tss_base >> 16) & 0xFF) << 32)
        | (0x89 << 40) // Present, 64-bit TSS (Type 9)
        | (((tss_limit >> 16) & 0x0F) << 48)
        | (((tss_base >> 24) & 0xFF) << 56);
    GLOBAL_GDT.entries[6] = tss_base >> 32;

    let descriptor = GdtDescriptor {
        limit: (size_of::<Gdt>() - 1) as u16,
        base: &raw const GLOBAL_GDT as u64,
    };

    asm!(
        "lgdt [{}]",
        "push {kcs}",
        "lea {tmp}, [2f + rip]",
        "push {tmp}",
        "retfq",
        "2:",
        "mov ax, {kds}",
        "mov ds, ax",
        "mov es, ax",
        "mov ss, ax",
        "mov fs, ax",
        "mov gs, ax",
        "ltr {tss_sel:x}",
        in(reg) &descriptor,
        kcs = const KERNEL_CODE_SEL,
        kds = const KERNEL_DATA_SEL,
        tss_sel = in(reg) TSS_SEL,
        tmp = out(reg) _,
        options(nostack)
    );
}

/// Sets the privilege level 0 stack pointer (RSP0) in the active TSS.
///
/// # Safety
///
/// `stack_top` must be a valid, mapped kernel stack memory address.
pub unsafe fn set_kernel_stack(stack_top: u64) {
    GLOBAL_TSS.rsp0 = stack_top;
}
