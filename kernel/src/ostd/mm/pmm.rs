//! Physical Memory Manager (PMM) - Fast Bitmap Frame Allocator.

use crate::ostd::limine::{LIMINE_MEMMAP_USABLE, LimineMemmapResponse};
use crate::ostd::sync::SpinLock;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const PAGE_SIZE: usize = 4096;

pub struct PhysicalMemoryManager {
    bitmap: *mut u8,
    total_frames: usize,
    free_frames: usize,
    last_frame: usize,
}

unsafe impl Send for PhysicalMemoryManager {}
unsafe impl Sync for PhysicalMemoryManager {}

static PMM: SpinLock<Option<PhysicalMemoryManager>> = SpinLock::new(None);
static TOTAL_MEMORY_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Initializes the physical memory manager bitmap allocator from the bootloader memory map.
///
/// # Safety
///
/// `memmap_response` must be a valid pointer provided by the bootloader, and `hhdm_offset`
/// must be the valid higher-half direct mapping virtual offset.
pub unsafe fn pmm_init(memmap_response: *mut LimineMemmapResponse, hhdm_offset: usize) {
    if memmap_response.is_null() {
        return;
    }
    let (count, entries) = unsafe {
        (
            (*memmap_response).entry_count as usize,
            (*memmap_response).entries,
        )
    };

    let mut highest_addr: u64 = 0;
    let mut total_bytes: usize = 0;

    for i in 0..count {
        let entry = unsafe { **entries.add(i) };
        if entry.typ == LIMINE_MEMMAP_USABLE {
            total_bytes += entry.length as usize;
            let top = entry.base + entry.length;
            if top > highest_addr {
                highest_addr = top;
            }
        }
    }

    TOTAL_MEMORY_BYTES.store(total_bytes, Ordering::Relaxed);
    let total_frames = (highest_addr as usize).div_ceil(PAGE_SIZE);
    let bitmap_size = total_frames.div_ceil(8);

    // Find a usable region large enough for the bitmap
    let mut bitmap_addr: u64 = 0;
    for i in 0..count {
        let entry = unsafe { **entries.add(i) };
        if entry.typ == LIMINE_MEMMAP_USABLE && (entry.length as usize) >= bitmap_size {
            bitmap_addr = entry.base;
            break;
        }
    }

    let bitmap_ptr = (bitmap_addr as usize + hhdm_offset) as *mut u8;
    // Initially mark everything as allocated (1 = used)
    unsafe {
        core::ptr::write_bytes(bitmap_ptr, 0xFF, bitmap_size);
    }

    let mut manager = PhysicalMemoryManager {
        bitmap: bitmap_ptr,
        total_frames,
        free_frames: 0,
        last_frame: 1,
    };

    // Mark usable areas as free (0 = free)
    for i in 0..count {
        let entry = unsafe { **entries.add(i) };
        if entry.typ == LIMINE_MEMMAP_USABLE {
            let start_frame = (entry.base as usize) / PAGE_SIZE;
            let frame_count = (entry.length as usize) / PAGE_SIZE;
            for f in start_frame..(start_frame + frame_count) {
                manager.set_bit(f, false);
                manager.free_frames += 1;
            }
        }
    }

    // Mark frame 0 and the bitmap itself as used
    manager.set_bit(0, true);
    let bitmap_start_frame = (bitmap_addr as usize) / PAGE_SIZE;
    let bitmap_frame_count = bitmap_size.div_ceil(PAGE_SIZE);
    for f in bitmap_start_frame..(bitmap_start_frame + bitmap_frame_count) {
        manager.set_bit(f, true);
        manager.free_frames = manager.free_frames.saturating_sub(1);
    }

    log::info!(
        "[PMM] Bitmap initialized: {} total frames, {} free frames.",
        total_frames,
        manager.free_frames
    );
    *PMM.lock() = Some(manager);
}

impl PhysicalMemoryManager {
    #[inline(always)]
    fn set_bit(&mut self, frame_idx: usize, used: bool) {
        if frame_idx >= self.total_frames {
            return;
        }
        let byte_idx = frame_idx / 8;
        let bit_idx = frame_idx % 8;
        unsafe {
            let ptr = self.bitmap.add(byte_idx);
            if used {
                *ptr |= 1 << bit_idx;
            } else {
                *ptr &= !(1 << bit_idx);
            }
        }
    }

    #[inline(always)]
    fn test_bit(&self, frame_idx: usize) -> bool {
        if frame_idx >= self.total_frames {
            return true;
        }
        let byte_idx = frame_idx / 8;
        let bit_idx = frame_idx % 8;
        unsafe { (*self.bitmap.add(byte_idx) & (1 << bit_idx)) != 0 }
    }

    pub fn alloc_frame(&mut self) -> Option<usize> {
        let start = self.last_frame;
        for f in start..self.total_frames {
            if !self.test_bit(f) {
                self.set_bit(f, true);
                self.free_frames = self.free_frames.saturating_sub(1);
                self.last_frame = f + 1;
                return Some(f * PAGE_SIZE);
            }
        }
        for f in 1..start {
            if !self.test_bit(f) {
                self.set_bit(f, true);
                self.free_frames = self.free_frames.saturating_sub(1);
                self.last_frame = f + 1;
                return Some(f * PAGE_SIZE);
            }
        }
        None
    }

    pub fn alloc_contiguous_frames(&mut self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        if count == 1 {
            return self.alloc_frame();
        }
        let mut run_start = 1;
        let mut run_len = 0;
        for f in 1..self.total_frames {
            if !self.test_bit(f) {
                if run_len == 0 {
                    run_start = f;
                }
                run_len += 1;
                if run_len == count {
                    for idx in run_start..(run_start + count) {
                        self.set_bit(idx, true);
                    }
                    self.free_frames = self.free_frames.saturating_sub(count);
                    return Some(run_start * PAGE_SIZE);
                }
            } else {
                run_len = 0;
            }
        }
        None
    }

    pub fn free_frame(&mut self, phys_addr: usize) {
        let frame_idx = phys_addr / PAGE_SIZE;
        if frame_idx < self.total_frames && self.test_bit(frame_idx) {
            self.set_bit(frame_idx, false);
            self.free_frames += 1;
            if frame_idx < self.last_frame {
                self.last_frame = frame_idx;
            }
        }
    }
}

pub fn alloc_frame() -> Option<usize> {
    PMM.lock().as_mut().and_then(|pmm| pmm.alloc_frame())
}

pub fn alloc_contiguous_frames(count: usize) -> Option<usize> {
    PMM.lock()
        .as_mut()
        .and_then(|pmm| pmm.alloc_contiguous_frames(count))
}

pub fn free_frame(phys_addr: usize) {
    if let Some(pmm) = PMM.lock().as_mut() {
        pmm.free_frame(phys_addr);
    }
}

pub fn get_pmm_stats() -> (usize, usize) {
    if let Some(ref pmm) = *PMM.lock() {
        (pmm.total_frames, pmm.free_frames)
    } else {
        (0, 0)
    }
}
