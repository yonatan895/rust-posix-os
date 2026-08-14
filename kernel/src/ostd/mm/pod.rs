//! Bounds-checked POD reads from kernel byte slices (ADR-0001).
//!
//! ELF/tar parsers in services must not cast `&[u8]` to a header pointer.
//! This is the only place that may do that transmute, and only after a
//! bounds check.

/// Copy a `Copy` value out of `bytes` at `offset`.
/// Returns `None` if the range does not fit.
pub fn read_pod<T: Copy>(bytes: &[u8], offset: usize) -> Option<T> {
    let size = core::mem::size_of::<T>();
    let end = offset.checked_add(size)?;
    let slice = bytes.get(offset..end)?;
    // SAFETY: `slice` is exactly `size_of::<T>()` bytes from a live kernel
    // buffer. `T: Copy` so we do not drop an existing value. `read_unaligned`
    // is required: ELF headers are naturally aligned in the file but tar
    // headers are packed, and the caller may pass any offset.
    Some(unsafe { core::ptr::read_unaligned(slice.as_ptr() as *const T) })
}
