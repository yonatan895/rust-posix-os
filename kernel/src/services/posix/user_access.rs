//! POSIX-layer adapter over `ostd::mm::user`.
//!
//! `ostd` owns the hardware contract (page-table walk, range checks) and
//! must not depend on POSIX errno numbers. This module is the *only* place
//! in `services/` that translates a [`UserAccessError`] into an errno, and
//! the only place that copies a NUL-terminated user path into a kernel
//! buffer. Syscall handlers (`fs`, `system`, later `process`/`audit`/`epoll`)
//! call these helpers; they never mention `UserAccessError` themselves.
//!
//! This is the services-side half of ADR-0001 R2. The ostd half is
//! `ostd::mm::user`. Keep both thin.

use posix_abi::*;
use crate::ostd::mm::{copy_cstr_from_user, UserAccessError, USER_STR_MAX};

/// Map an ostd user-access failure to a POSIX errno.
///
/// Every variant except [`UserAccessError::TooLong`] is `EFAULT`
/// (bad/unmapped/out-of-range/not-writable pointer). `TooLong` is the
/// one case POSIX distinguishes: a name that exceeded `PATH_MAX`.
pub fn map_user_error(err: UserAccessError) -> i32 {
    match err {
        UserAccessError::TooLong => ENAMETOOLONG,
        _ => EFAULT,
    }
}

/// Copy a NUL-terminated user path into `kbuf` and validate UTF-8.
///
/// Returns the path as a `&str` borrowing `kbuf`. Failures:
///   * `EFAULT`       — null / unmapped / out of user range
///   * `ENAMETOOLONG` — no NUL within `USER_STR_MAX` bytes
///   * `EINVAL`       — bytes are not valid UTF-8
pub fn copy_user_path<'a>(
    path_ptr: *const u8,
    kbuf: &'a mut [u8; USER_STR_MAX],
) -> Result<&'a str, i32> {
    let len = copy_cstr_from_user(path_ptr as usize, kbuf).map_err(map_user_error)?;
    core::str::from_utf8(&kbuf[..len]).map_err(|_| EINVAL)
}
