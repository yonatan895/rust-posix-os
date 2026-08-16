//! Copy-on-Write frame reference counting and the anonymous zero page.
//!
//! # Invariant
//!
//! Every frame allocated via [`pmm::alloc_frame`] carries an *implicit* reference
//! count of 1. [`cow_inc_ref`] adds a sharer (e.g. during `clone_cow`). [`cow_dec_ref`]
//! decrements and only calls [`pmm::free_frame`] when the last owner is done.
//!
//! The zero page is a permanently-live shared read-only frame mapped for all
//! lazily-allocated anonymous pages until a first write promotes them to a private copy.

use super::pmm::{PAGE_SIZE, alloc_frame, free_frame};
use super::vmm::zero_phys_frame;
use crate::ostd::sync::SpinLock;
use alloc::collections::BTreeMap;

/// Per-physical-frame reference count table (page-aligned frame address → count).
/// Frames not present here are implicitly at refcount 1 (exclusively owned).
static FRAME_REFS: SpinLock<BTreeMap<usize, u32>> = SpinLock::new(BTreeMap::new());

/// Physical address of the globally shared anonymous zero page (lazily initialised).
static ZERO_PAGE_PHYS: SpinLock<Option<usize>> = SpinLock::new(None);

/// Sentinel refcount used for the zero page so it is never freed.
const SENTINEL: u32 = u32::MAX;

/// Returns the physical address of the shared anonymous zero page, allocating it on first call.
///
/// The page is zeroed, mapped read-only, and its refcount is set to [`SENTINEL`] so it
/// can never be freed by [`cow_dec_ref`].
pub fn zero_page_phys() -> usize {
    let mut guard = ZERO_PAGE_PHYS.lock();
    if let Some(phys) = *guard {
        return phys;
    }
    let phys = alloc_frame().expect("OOM allocating CoW zero page");
    zero_phys_frame(phys);
    FRAME_REFS.lock().insert(phys, SENTINEL);
    *guard = Some(phys);
    phys
}

/// Increments the share count of `phys`.
///
/// If the frame is not yet tracked it is assumed exclusively owned (implicit refcount 1)
/// and the stored count becomes 2 after this call.
pub fn cow_inc_ref(phys: usize) {
    let phys = phys & !(PAGE_SIZE - 1);
    let mut refs = FRAME_REFS.lock();
    let count = refs.entry(phys).or_insert(1);
    *count = count.saturating_add(1);
}

/// Decrements the share count of `phys`.
///
/// Frees the frame via [`pmm::free_frame`] and returns `true` when the refcount
/// reaches zero. Returns `false` for the zero page (sentinel) and shared frames.
/// Untracked frames (implicit refcount 1) are freed immediately.
pub fn cow_dec_ref(phys: usize) -> bool {
    let phys = phys & !(PAGE_SIZE - 1);
    let mut refs = FRAME_REFS.lock();
    match refs.get_mut(&phys) {
        // Zero page or other permanent sentinel — never freed.
        Some(count) if *count == SENTINEL => false,
        // Last owner: remove entry and release the frame.
        Some(count) if *count <= 1 => {
            refs.remove(&phys);
            free_frame(phys);
            true
        }
        // Shared: just decrement.
        Some(count) => {
            *count -= 1;
            false
        }
        // Untracked implicit-refcount-1 frame: free directly.
        None => {
            free_frame(phys);
            true
        }
    }
}

/// Returns the current reference count of `phys` (1 if untracked).
pub fn cow_ref_count(phys: usize) -> u32 {
    let phys = phys & !(PAGE_SIZE - 1);
    FRAME_REFS.lock().get(&phys).copied().unwrap_or(1)
}
