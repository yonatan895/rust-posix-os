//! Global Descriptor Table (GDT) and Task State Segment (TSS) for x86_64.

use core::arch::asm;
use core::mem::size_of;

/// Kernel 64-bit code segment selector (Ring 0, GDT index 1).
pub const KERNEL_CODE_SEL: u16 = 0x08;
/// Kernel data segment selector (Ring 0, GDT index 2).
pub const KERNEL_DATA_SEL: u16 = 0x10;
/// User data segment selector (Ring 3, GDT index 3).
pub const USER_DATA_SEL: u16 = 0x18 | 3;
/// User 64-bit code segment selector (Ring 3, GDT index 4).
pub const USER_CODE_SEL: u16 = 0x20 | 3;
/// Task State Segment (TSS) selector (GDT index 5, 16-byte descriptor).
pub const TSS_SEL: u16 = 0x28;

/// x86_64 Task State Segment (TSS) structure.
#[repr(C, packed)]
pub struct TSS {
    /// Reserved; must be zero.
    pub reserved0: u32,
    /// Stack pointer for privilege level 0 (Ring 0 kernel stack).
    pub rsp0: u64,
    /// Stack pointer for privilege level 1 (unused).
    pub rsp1: u64,
    /// Stack pointer for privilege level 2 (unused).
    pub rsp2: u64,
    /// Reserved; must be zero.
    pub reserved1: u64,
    /// Interrupt Stack Table entry 1.
    pub ist1: u64,
    /// Interrupt Stack Table entry 2.
    pub ist2: u64,
    /// Interrupt Stack Table entry 3.
    pub ist3: u64,
    /// Interrupt Stack Table entry 4.
    pub ist4: u64,
    /// Interrupt Stack Table entry 5.
    pub ist5: u64,
    /// Interrupt Stack Table entry 6.
    pub ist6: u64,
    /// Interrupt Stack Table entry 7.
    pub ist7: u64,
    /// Reserved; must be zero.
    pub reserved2: u64,
    /// Reserved; must be zero.
    pub reserved3: u16,
    /// 16-bit offset to the I/O permission bitmap from the TSS base.
    pub iomap_base: u16,
}

impl TSS {
    /// Creates a zero-initialized [`TSS`] with I/O bitmap base pointing past the structure.
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

/// x86_64 Global Descriptor Table Register (GDTR) descriptor.
#[repr(C, packed)]
pub struct GdtDescriptor {
    /// Table limit (table size in bytes minus 1).
    pub limit: u16,
    /// Linear base address of the GDT.
    pub base: u64,
}

/// Fixed-size x86_64 Global Descriptor Table storing kernel/user segments and the 16-byte TSS descriptor.
#[repr(C, align(16))]
pub struct Gdt {
    /// Raw 64-bit segment descriptor slots (null, kernel code/data, user data/code, 16-byte TSS).
    pub entries: [u64; 7],
}

use core::cell::SyncUnsafeCell;

/// Global static Task State Segment for the BSP.
static GLOBAL_TSS: SyncUnsafeCell<TSS> = SyncUnsafeCell::new(TSS::new());
/// Global static Global Descriptor Table for the BSP.
static GLOBAL_GDT: SyncUnsafeCell<Gdt> = SyncUnsafeCell::new(Gdt { entries: [0; 7] });

/// Initializes the GDT and loads the 64-bit Task State Segment (TSS).
///
/// # Safety
///
/// Must be called during single-threaded boot initialization with a valid kernel stack address.
pub unsafe fn gdt_init(kernel_stack_top: u64) {
    let tss_ptr = GLOBAL_TSS.get();
    let gdt_ptr = GLOBAL_GDT.get();

    // SAFETY: Writing kernel_stack_top to TSS during single-threaded boot. GLOBAL_TSS is statically allocated, aligned, and valid.
    unsafe {
        (*tss_ptr).rsp0 = kernel_stack_top;
    }

    let tss_base = tss_ptr as u64;
    let tss_limit = (size_of::<TSS>() - 1) as u64;

    // SAFETY: Populating 64-bit GDT entries into statically allocated GLOBAL_GDT during single-threaded boot before segment registers are reloaded.
    unsafe {
        // Entry 0: Null Descriptor
        (*gdt_ptr).entries[0] = 0;
        // Entry 1: 0x08 - Kernel Code 64-bit (Ring 0, Exec/Read)
        (*gdt_ptr).entries[1] = 0x00AF9A000000FFFF;
        // Entry 2: 0x10 - Kernel Data (Ring 0, Read/Write)
        (*gdt_ptr).entries[2] = 0x00CF92000000FFFF;
        // Entry 3: 0x18 - User Data (Ring 3, Read/Write)
        (*gdt_ptr).entries[3] = 0x00CFF2000000FFFF;
        // Entry 4: 0x20 - User Code 64-bit (Ring 3, Exec/Read)
        (*gdt_ptr).entries[4] = 0x00AFF8000000FFFF;

        // Entries 5-6: 0x28 - TSS Descriptor (16 bytes in 64-bit mode)
        let mut tss_low: u64 = tss_limit & 0xFFFF;
        tss_low |= (tss_base & 0xFFFFFF) << 16;
        tss_low |= 0x89u64 << 40; // Present, 64-bit TSS (Available)
        tss_low |= ((tss_limit >> 16) & 0xF) << 48;
        tss_low |= ((tss_base >> 24) & 0xFF) << 56;

        let tss_high: u64 = tss_base >> 32;

        (*gdt_ptr).entries[5] = tss_low;
        (*gdt_ptr).entries[6] = tss_high;
    }

    let descriptor = GdtDescriptor {
        limit: (size_of::<Gdt>() - 1) as u16,
        base: gdt_ptr as u64,
    };

    // SAFETY: Loading GDT descriptor with lgdt, reloading CS via far return to KERNEL_CODE_SEL (0x08), loading data segments with KERNEL_DATA_SEL (0x10), and loading Task Register with TSS_SEL (0x28).
    unsafe {
        asm!("lgdt [{}]", in(reg) &descriptor, options(nostack));

        // Reload segment registers with newly initialized GDT selectors
        asm!(
            "push {kcs}",
            "lea {tmp}, [2f + rip]",
            "push {tmp}",
            "retfq",
            "2:",
            "mov {tmp:x}, {kds}",
            "mov ds, {tmp:x}",
            "mov es, {tmp:x}",
            "mov ss, {tmp:x}",
            "mov fs, {tmp:x}",
            "mov gs, {tmp:x}",
            kcs = const KERNEL_CODE_SEL,
            kds = const KERNEL_DATA_SEL,
            tmp = out(reg) _,
            options(preserves_flags)
        );

        // Load Task Register (LTR) with TSS selector 0x28
        asm!("ltr ax", in("ax") TSS_SEL, options(nomem, nostack, preserves_flags));
    }
}

/// Updates the TSS.rsp0 privilege level 0 stack pointer on task switch.
///
/// # Safety
///
/// `stack_top` must be a valid, mapped kernel stack memory address.
pub unsafe fn set_kernel_stack(stack_top: u64) {
    // SAFETY: Updating TSS.rsp0 via GLOBAL_TSS raw pointer for user-to-kernel interrupt and privilege level switches. Caller guarantees stack_top is a valid mapped kernel stack.
    unsafe {
        (*GLOBAL_TSS.get()).rsp0 = stack_top;
    }
    // SAFETY: Updating per-CPU kernel stack pointer used on fast syscall entry. Caller guarantees stack_top is a valid mapped kernel stack.
    unsafe {
        super::syscall::set_syscall_kernel_stack(stack_top);
    }
}
