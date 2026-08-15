//! Standalone Limine Boot Protocol v8 Specification Definitions.

use core::cell::UnsafeCell;

/// 64-bit magic numbers common to all Limine boot protocol requests.
pub const LIMINE_COMMON_MAGIC: [u64; 2] = [0xc7b1dd30df4c8b88, 0x0a82e883a194f07b];

/// Request identifier tag for the Higher-Half Direct Map (HHDM) feature.
pub const LIMINE_HHDM_REQUEST: [u64; 4] = [
    LIMINE_COMMON_MAGIC[0],
    LIMINE_COMMON_MAGIC[1],
    0x48dcf1cb8ad2b852,
    0x63984e959a98244b,
];

/// Request identifier tag for the physical memory map feature.
pub const LIMINE_MEMMAP_REQUEST: [u64; 4] = [
    LIMINE_COMMON_MAGIC[0],
    LIMINE_COMMON_MAGIC[1],
    0x67cf3d9d378a806f,
    0xe304acdfc50c3c62,
];

/// Request identifier tag for the graphical framebuffer feature.
pub const LIMINE_FRAMEBUFFER_REQUEST: [u64; 4] = [
    LIMINE_COMMON_MAGIC[0],
    LIMINE_COMMON_MAGIC[1],
    0x9d5827dcd881dd75,
    0xa3148604f6fab11b,
];

/// Request identifier tag for the bootloader module list feature.
pub const LIMINE_MODULE_REQUEST: [u64; 4] = [
    LIMINE_COMMON_MAGIC[0],
    LIMINE_COMMON_MAGIC[1],
    0x3e7e279702be32af,
    0xca1c4f3bd1280cee,
];

/// Marker struct placed at the start of the `.requests_start` linker section.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineRequestsStartMarker {
    /// 4-element magic ID array identifying the start of Limine requests.
    pub id: [u64; 4],
}
// SAFETY: LimineRequestsStartMarker contains only immutable magic numbers and is safe to share across threads.
unsafe impl Sync for LimineRequestsStartMarker {}
// SAFETY: LimineRequestsStartMarker is a Plain Old Data marker struct safe to transfer across threads.
unsafe impl Send for LimineRequestsStartMarker {}

/// Marker struct placed at the end of the `.requests_end` linker section.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineRequestsEndMarker {
    /// 2-element magic ID array identifying the end of Limine requests.
    pub id: [u64; 2],
}
// SAFETY: LimineRequestsEndMarker contains only immutable magic numbers and is safe to share across threads.
unsafe impl Sync for LimineRequestsEndMarker {}
// SAFETY: LimineRequestsEndMarker is a Plain Old Data marker struct safe to transfer across threads.
unsafe impl Send for LimineRequestsEndMarker {}

/// Base protocol revision declaration for the Limine bootloader.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineBaseRevision {
    /// Magic tag array identifying the base revision request.
    pub id: [u64; 2],
    /// Target protocol revision version.
    pub revision: u64,
}
// SAFETY: LimineBaseRevision contains only immutable numeric fields and is safe to share across threads.
unsafe impl Sync for LimineBaseRevision {}
// SAFETY: LimineBaseRevision is a Plain Old Data struct safe to transfer across threads.
unsafe impl Send for LimineBaseRevision {}

/// Response structure returned by Limine for the HHDM request.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineHhdmResponse {
    /// Response version.
    pub revision: u64,
    /// Virtual address base offset of the higher-half direct map.
    pub offset: u64,
}

/// Request structure asking Limine to map physical memory to the higher-half.
#[repr(C, align(8))]
pub struct LimineHhdmRequest {
    /// Magic request identifier tag.
    pub id: [u64; 4],
    /// Request revision number.
    pub revision: u64,
    /// Pointer filled by Limine with the response structure.
    pub response: UnsafeCell<*mut LimineHhdmResponse>,
}
// SAFETY: LimineHhdmRequest response pointer is initialized by the bootloader during early boot and read-only afterwards; UnsafeCell synchronization is managed internally.
unsafe impl Sync for LimineHhdmRequest {}
// SAFETY: LimineHhdmRequest contains Plain Old Data and raw pointer fields safe to transfer across threads.
unsafe impl Send for LimineHhdmRequest {}

/// Memory map entry type: Usable conventional RAM.
pub const LIMINE_MEMMAP_USABLE: u64 = 0;
/// Memory map entry type: Reserved physical memory.
pub const LIMINE_MEMMAP_RESERVED: u64 = 1;
/// Memory map entry type: ACPI reclaimable memory.
pub const LIMINE_MEMMAP_ACPI_RECLAIMABLE: u64 = 2;
/// Memory map entry type: ACPI non-volatile storage.
pub const LIMINE_MEMMAP_ACPI_NVS: u64 = 3;
/// Memory map entry type: Defective / unusable memory.
pub const LIMINE_MEMMAP_BAD_MEMORY: u64 = 4;
/// Memory map entry type: Bootloader reclaimable memory.
pub const LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE: u64 = 5;
/// Memory map entry type: Kernel image and boot modules.
pub const LIMINE_MEMMAP_KERNEL_AND_MODULES: u64 = 6;
/// Memory map entry type: Linear video framebuffer memory.
pub const LIMINE_MEMMAP_FRAMEBUFFER: u64 = 7;

/// Single memory map range entry provided by Limine.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineMemmapEntry {
    /// Base physical address of the memory region.
    pub base: u64,
    /// Length in bytes of the memory region.
    pub length: u64,
    /// Memory region classification type (e.g. [`LIMINE_MEMMAP_USABLE`]).
    pub typ: u64,
}

/// Response structure containing the physical memory map.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineMemmapResponse {
    /// Response revision.
    pub revision: u64,
    /// Number of memory map entries.
    pub entry_count: u64,
    /// Pointer to an array of pointers to [`LimineMemmapEntry`].
    pub entries: *mut *mut LimineMemmapEntry,
}

/// Request structure querying the system physical memory map.
#[repr(C, align(8))]
pub struct LimineMemmapRequest {
    /// Magic request identifier tag.
    pub id: [u64; 4],
    /// Request revision.
    pub revision: u64,
    /// Pointer filled by Limine with the response.
    pub response: UnsafeCell<*mut LimineMemmapResponse>,
}
// SAFETY: LimineMemmapRequest response pointer is initialized by the bootloader during early boot and read-only afterwards; UnsafeCell synchronization is managed internally.
unsafe impl Sync for LimineMemmapRequest {}
// SAFETY: LimineMemmapRequest contains Plain Old Data and raw pointer fields safe to transfer across threads.
unsafe impl Send for LimineMemmapRequest {}

/// Linear graphical framebuffer descriptor.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineFramebuffer {
    /// Virtual address pointer to the video framebuffer.
    pub address: *mut u8,
    /// Horizontal resolution in pixels.
    pub width: u64,
    /// Vertical resolution in pixels.
    pub height: u64,
    /// Number of bytes per scanline.
    pub pitch: u64,
    /// Bits per pixel.
    pub bpp: u16,
    /// Color model format identifier.
    pub memory_model: u8,
    /// Red color channel bit depth.
    pub red_mask_size: u8,
    /// Red color channel bit position shift.
    pub red_mask_shift: u8,
    /// Green color channel bit depth.
    pub green_mask_size: u8,
    /// Green color channel bit position shift.
    pub green_mask_shift: u8,
    /// Blue color channel bit depth.
    pub blue_mask_size: u8,
    /// Blue color channel bit position shift.
    pub blue_mask_shift: u8,
}

/// Response containing detected graphical framebuffers.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineFramebufferResponse {
    /// Response revision.
    pub revision: u64,
    /// Number of available framebuffers.
    pub framebuffer_count: u64,
    /// Pointer to array of framebuffer pointers.
    pub framebuffers: *mut *mut LimineFramebuffer,
}

/// Request querying graphical framebuffer devices.
#[repr(C, align(8))]
pub struct LimineFramebufferRequest {
    /// Magic request identifier tag.
    pub id: [u64; 4],
    /// Request revision.
    pub revision: u64,
    /// Pointer filled by Limine with response.
    pub response: UnsafeCell<*mut LimineFramebufferResponse>,
}
// SAFETY: LimineFramebufferRequest response pointer is initialized by the bootloader during early boot and read-only afterwards; UnsafeCell synchronization is managed internally.
unsafe impl Sync for LimineFramebufferRequest {}
// SAFETY: LimineFramebufferRequest contains Plain Old Data and raw pointer fields safe to transfer across threads.
unsafe impl Send for LimineFramebufferRequest {}

/// Bootloader-loaded file or module descriptor.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineFile {
    /// Structure revision.
    pub revision: u64,
    /// Linear virtual address of the loaded file payload.
    pub address: *mut u8,
    /// Size in bytes of the file.
    pub size: u64,
    /// NUL-terminated path string.
    pub path: *const u8,
    /// NUL-terminated command line string.
    pub cmdline: *const u8,
    /// Storage media type identifier.
    pub media_type: u32,
    /// Reserved field.
    pub unused: u32,
    /// TFTP IP address if loaded via network.
    pub tftp_ip: u32,
    /// TFTP port if loaded via network.
    pub tftp_port: u32,
    /// 1-based partition index on disk.
    pub partition_index: u32,
    /// MBR disk identifier.
    pub mbr_disk_id: u32,
    /// GPT disk UUID.
    pub gpt_disk_uuid: [u8; 16],
    /// GPT partition UUID.
    pub gpt_part_uuid: [u8; 16],
    /// General partition UUID.
    pub part_uuid: [u8; 16],
}

/// Response structure containing loaded boot modules.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy)]
pub struct LimineModuleResponse {
    /// Response revision.
    pub revision: u64,
    /// Number of loaded modules.
    pub module_count: u64,
    /// Pointer to array of file pointers.
    pub modules: *mut *mut LimineFile,
}

/// Request querying bootloader modules.
#[repr(C, align(8))]
pub struct LimineModuleRequest {
    /// Magic request identifier tag.
    pub id: [u64; 4],
    /// Request revision.
    pub revision: u64,
    /// Pointer filled by Limine with response.
    pub response: UnsafeCell<*mut LimineModuleResponse>,
}
// SAFETY: LimineModuleRequest response pointer is initialized by the bootloader during early boot and read-only afterwards; UnsafeCell synchronization is managed internally.
unsafe impl Sync for LimineModuleRequest {}
// SAFETY: LimineModuleRequest contains Plain Old Data and raw pointer fields safe to transfer across threads.
unsafe impl Send for LimineModuleRequest {}

/// Static marker for start of Limine requests section.
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

/// Static request declaring Limine base protocol revision.
#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: LimineBaseRevision = LimineBaseRevision {
    id: [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc],
    revision: 3,
};

/// Static request querying higher-half direct mapping (HHDM).
#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: LimineHhdmRequest = LimineHhdmRequest {
    id: LIMINE_HHDM_REQUEST,
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

/// Static request querying physical memory map.
#[used]
#[unsafe(link_section = ".requests")]
static MEMMAP_REQUEST: LimineMemmapRequest = LimineMemmapRequest {
    id: LIMINE_MEMMAP_REQUEST,
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

/// Static request querying graphical framebuffer.
#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: LimineFramebufferRequest = LimineFramebufferRequest {
    id: LIMINE_FRAMEBUFFER_REQUEST,
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

/// Static request querying loaded boot modules.
#[used]
#[unsafe(link_section = ".requests")]
static MODULE_REQUEST: LimineModuleRequest = LimineModuleRequest {
    id: LIMINE_MODULE_REQUEST,
    revision: 0,
    response: UnsafeCell::new(core::ptr::null_mut()),
};

/// Static marker for end of Limine requests section.
#[used]
#[unsafe(link_section = ".requests_end")]
static REQ_END: LimineRequestsEndMarker = LimineRequestsEndMarker {
    id: [0xadc0e0531bb10d03, 0x9572709f31764c62],
};

/// Get the higher-half direct map (HHDM) virtual offset provided by Limine.
///
/// # Panics
///
/// Panics if the Limine bootloader did not supply an HHDM response.
pub fn hhdm_offset() -> usize {
    // SAFETY: Reading UnsafeCell response pointer written by Limine bootloader before kernel execution.
    let resp = unsafe { *HHDM_REQUEST.response.get() };
    if resp.is_null() {
        panic!("Limine HHDM response missing");
    }
    // SAFETY: resp is verified non-null and points to a valid LimineHhdmResponse initialized by Limine.
    unsafe { (*resp).offset as usize }
}

/// Get the physical memory map response pointer.
///
/// # Panics
///
/// Panics if the Limine bootloader did not supply a memory map response.
pub(crate) fn memmap_response() -> *mut LimineMemmapResponse {
    // SAFETY: Reading UnsafeCell response pointer written by Limine bootloader before kernel execution.
    let resp = unsafe { *MEMMAP_REQUEST.response.get() };
    if resp.is_null() {
        panic!("Limine memory map response missing");
    }
    resp
}

/// Get the bootloader module response pointer.
pub(crate) fn module_response() -> *mut LimineModuleResponse {
    // SAFETY: Reading UnsafeCell response pointer written by Limine bootloader before kernel execution.
    unsafe { *MODULE_REQUEST.response.get() }
}

/// Initialize the framebuffer driver if a display device is reported by Limine.
pub fn init_framebuffer() {
    // SAFETY: Reading UnsafeCell response pointer written by Limine bootloader before kernel execution.
    let resp = unsafe { *FRAMEBUFFER_REQUEST.response.get() };
    if resp.is_null() {
        return;
    }
    // SAFETY: resp is verified non-null and points to a valid LimineFramebufferResponse initialized by Limine.
    let count = unsafe { (*resp).framebuffer_count };
    if count > 0 {
        // SAFETY: (*resp).framebuffers points to an array containing at least count valid LimineFramebuffer pointers.
        let fb = unsafe { **(*resp).framebuffers };
        // SAFETY: fb describes a valid physical/virtual framebuffer buffer initialized by the bootloader.
        unsafe { crate::ostd::drivers::framebuffer::fb_init(fb) };
        log::info!(
            "[OSTD] Framebuffer initialized ({}x{} @ {}bpp).",
            fb.width,
            fb.height,
            fb.bpp
        );
    }
}
