//! Standalone Limine Boot Protocol v8 Specification Definitions.

use core::cell::UnsafeCell;

pub const LIMINE_COMMON_MAGIC: [u64; 2] = [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b];

pub const LIMINE_HHDM_REQUEST: [u64; 4] = [
    LIMINE_COMMON_MAGIC[0],
    LIMINE_COMMON_MAGIC[1],
    0x48dcf1cb8ad2b852,
    0x63984e959a98244b,
];

pub const LIMINE_MEMMAP_REQUEST: [u64; 4] = [
    LIMINE_COMMON_MAGIC[0],
    LIMINE_COMMON_MAGIC[1],
    0x67cf3d9d378a806f,
    0xe304acdfc50c3c62,
];

pub const LIMINE_FRAMEBUFFER_REQUEST: [u64; 4] = [
    LIMINE_COMMON_MAGIC[0],
    LIMINE_COMMON_MAGIC[1],
    0x9d5827dcd881dd75,
    0xa3148604f6fab11b,
];

pub const LIMINE_MODULE_REQUEST: [u64; 4] = [
    LIMINE_COMMON_MAGIC[0],
    LIMINE_COMMON_MAGIC[1],
    0x3e7e279702be32af,
    0xca1c4f3bd1280cee,
];

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineRequestsStartMarker {
    pub id: [u64; 4],
}
unsafe impl Sync for LimineRequestsStartMarker {}
unsafe impl Send for LimineRequestsStartMarker {}

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineRequestsEndMarker {
    pub id: [u64; 2],
}
unsafe impl Sync for LimineRequestsEndMarker {}
unsafe impl Send for LimineRequestsEndMarker {}

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineBaseRevision {
    pub id: [u64; 2],
    pub revision: u64,
}
unsafe impl Sync for LimineBaseRevision {}
unsafe impl Send for LimineBaseRevision {}

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineHhdmResponse {
    pub revision: u64,
    pub offset: u64,
}

#[repr(C, align(8))]
pub struct LimineHhdmRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: UnsafeCell<*mut LimineHhdmResponse>,
}
unsafe impl Sync for LimineHhdmRequest {}
unsafe impl Send for LimineHhdmRequest {}

pub const LIMINE_MEMMAP_USABLE: u64 = 0;
pub const LIMINE_MEMMAP_RESERVED: u64 = 1;
pub const LIMINE_MEMMAP_ACPI_RECLAIMABLE: u64 = 2;
pub const LIMINE_MEMMAP_ACPI_NVS: u64 = 3;
pub const LIMINE_MEMMAP_BAD_MEMORY: u64 = 4;
pub const LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE: u64 = 5;
pub const LIMINE_MEMMAP_KERNEL_AND_MODULES: u64 = 6;
pub const LIMINE_MEMMAP_FRAMEBUFFER: u64 = 7;

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineMemmapEntry {
    pub base: u64,
    pub length: u64,
    pub typ: u64,
}

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineMemmapResponse {
    pub revision: u64,
    pub entry_count: u64,
    pub entries: *mut *mut LimineMemmapEntry,
}

#[repr(C, align(8))]
pub struct LimineMemmapRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: UnsafeCell<*mut LimineMemmapResponse>,
}
unsafe impl Sync for LimineMemmapRequest {}
unsafe impl Send for LimineMemmapRequest {}

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineFramebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
}

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineFramebufferResponse {
    pub revision: u64,
    pub framebuffer_count: u64,
    pub framebuffers: *mut *mut LimineFramebuffer,
}

#[repr(C, align(8))]
pub struct LimineFramebufferRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: UnsafeCell<*mut LimineFramebufferResponse>,
}
unsafe impl Sync for LimineFramebufferRequest {}
unsafe impl Send for LimineFramebufferRequest {}

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineFile {
    pub revision: u64,
    pub address: *mut u8,
    pub size: u64,
    pub path: *const u8,
    pub cmdline: *const u8,
    pub media_type: u32,
    pub unused: u32,
    pub tftp_ip: u32,
    pub tftp_port: u32,
    pub partition_index: u32,
    pub mbr_disk_id: u32,
    pub gpt_disk_uuid: [u8; 16],
    pub gpt_part_uuid: [u8; 16],
    pub part_uuid: [u8; 16],
}

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineModuleResponse {
    pub revision: u64,
    pub module_count: u64,
    pub modules: *mut *mut LimineFile,
}

#[repr(C, align(8))]
pub struct LimineModuleRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: UnsafeCell<*mut LimineModuleResponse>,
}
unsafe impl Sync for LimineModuleRequest {}
unsafe impl Send for LimineModuleRequest {}

#[used]
#[unsafe(link_section = ".requests_start")]
static REQ_START: LimineRequestsStartMarker = LimineRequestsStartMarker {
    id: [
        0xf6b8f4b39de7d1ae,
        0xfab91a6940fcb9cf,
        0x785c6ed015d3e316,
        0x181e920a7852b9d9,
    ],
};

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: LimineBaseRevision = LimineBaseRevision {
    id: [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc],
    revision: 3,
};

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: LimineHhdmRequest = LimineHhdmRequest {
    id: LIMINE_HHDM_REQUEST,
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".requests")]
static MEMMAP_REQUEST: LimineMemmapRequest = LimineMemmapRequest {
    id: LIMINE_MEMMAP_REQUEST,
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: LimineFramebufferRequest = LimineFramebufferRequest {
    id: LIMINE_FRAMEBUFFER_REQUEST,
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".requests")]
static MODULE_REQUEST: LimineModuleRequest = LimineModuleRequest {
    id: LIMINE_MODULE_REQUEST,
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

#[used]
#[unsafe(link_section = ".requests_end")]
static REQ_END: LimineRequestsEndMarker = LimineRequestsEndMarker {
    id: [0xadc0e0531bb10d03, 0x9572709f31764c62],
};

/// Get the higher-half direct map (HHDM) virtual offset provided by Limine.
pub fn hhdm_offset() -> usize {
    let resp = unsafe { *HHDM_REQUEST.response.get() };
    if resp.is_null() {
        panic!("Limine HHDM response missing");
    }
    unsafe { (*resp).offset as usize }
}

/// Get the physical memory map response pointer.
pub(crate) fn memmap_response() -> *mut LimineMemmapResponse {
    let resp = unsafe { *MEMMAP_REQUEST.response.get() };
    if resp.is_null() {
        panic!("Limine memory map response missing");
    }
    resp
}

/// Get the bootloader module response pointer.
pub(crate) fn module_response() -> *mut LimineModuleResponse {
    unsafe { *MODULE_REQUEST.response.get() }
}

/// Initialize the framebuffer driver if a display device is reported by Limine.
pub fn init_framebuffer() {
    let resp = unsafe { *FRAMEBUFFER_REQUEST.response.get() };
    if !resp.is_null() && unsafe { (*resp).framebuffer_count } > 0 {
        let fb = unsafe { **(*resp).framebuffers };
        // SAFETY: `fb` is provided and validated by the Limine bootloader payload.
        unsafe { crate::ostd::drivers::framebuffer::fb_init(fb) };
        log::info!(
            "[OSTD] Framebuffer initialized ({}x{} @ {}bpp).",
            fb.width,
            fb.height,
            fb.bpp
        );
    }
}
