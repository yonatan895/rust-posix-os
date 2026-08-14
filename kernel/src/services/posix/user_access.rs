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
pub fn copy_user_path<'a>(
    path_ptr: *const u8,
    kbuf: &'a mut [u8; USER_STR_MAX],
) -> Result<&'a str, i32> {
    let len = copy_cstr_from_user(path_ptr as usize, kbuf).map_err(map_user_error)?;
    core::str::from_utf8(&kbuf[..len]).map_err(|_| EINVAL)
}

/// Like [`copy_user_path`], but a null pointer is a valid empty string.
pub fn copy_optional_user_str<'a>(
    ptr: *const u8,
    kbuf: &'a mut [u8; USER_STR_MAX],
) -> Result<&'a str, i32> {
    if ptr.is_null() {
        Ok("")
    } else {
        copy_user_path(ptr, kbuf)
    }
}
