//! POSIX-layer adapter over `ostd::mm::user`.
//!
//! `ostd` owns the hardware contract (page-table walk, range checks) and
//! must not depend on POSIX errno numbers. This module is the *only* place
//! in `services/` that translates a [`UserAccessError`] into an errno, and
//! the only place that copies a NUL-terminated user path into a kernel
//! buffer.

use posix_abi::*;
use crate::ostd::mm::{copy_cstr_from_user, UserAccessError, USER_STR_MAX};

pub fn map_user_error(err: UserAccessError) -> i32 {
    match err {
        UserAccessError::TooLong => ENAMETOOLONG,
        _ => EFAULT,
    }
}

/// Copy a NUL-terminated user path into `kbuf` and validate UTF-8.
pub fn copy_user_path(
    path_ptr: *const u8,
    kbuf: &mut [u8; USER_STR_MAX],
) -> Result<&str, i32> {
    let len = copy_cstr_from_user(path_ptr as usize, kbuf).map_err(map_user_error)?;
    core::str::from_utf8(&kbuf[..len]).map_err(|_| EINVAL)
}

/// Like [`copy_user_path`], but a null pointer is a valid empty string.
pub fn copy_optional_user_str(
    ptr: *const u8,
    kbuf: &mut [u8; USER_STR_MAX],
) -> Result<&str, i32> {
    if ptr.is_null() {
        Ok("")
    } else {
        copy_user_path(ptr, kbuf)
    }
}

/// Safely copies an array of user string pointers (terminated by a NULL pointer)
/// into a Vec of owned `String`s.
pub fn copy_user_str_array(
    arr_ptr: *const *const u8,
    max_count: usize,
) -> Result<alloc::vec::Vec<alloc::string::String>, i32> {
    if arr_ptr.is_null() {
        return Ok(alloc::vec::Vec::new());
    }

    let mut result = alloc::vec::Vec::new();
    let mut curr_ptr = arr_ptr as usize;
    let mut terminated = false;

    for _ in 0..max_count {
        let uptr = crate::ostd::mm::UserPtr::<usize>::from_raw(curr_ptr).map_err(map_user_error)?;
        let str_addr = uptr.read().map_err(map_user_error)?;
        if str_addr == 0 {
            terminated = true;
            break;
        }

        let mut kbuf = [0u8; USER_STR_MAX];
        let len = copy_cstr_from_user(str_addr, &mut kbuf).map_err(map_user_error)?;
        let s = core::str::from_utf8(&kbuf[..len]).map_err(|_| EINVAL)?;
        result.push(alloc::string::ToString::to_string(s));

        curr_ptr = curr_ptr.checked_add(core::mem::size_of::<usize>()).ok_or(EFAULT)?;
    }

    if !terminated {
        return Err(E2BIG);
    }

    Ok(result)
}
