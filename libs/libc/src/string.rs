//! Standard C String and Memory Routines (extern "C").

/// Computes the length of a null-terminated string.
///
/// # Safety
///
/// `s` must point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut len = 0;
    while unsafe { *s.add(len) } != 0 {
        len += 1;
    }
    len
}

/// Compares two null-terminated strings lexicographically.
///
/// Returns an integer less than, equal to, or greater than zero.
///
/// # Safety
///
/// `s1` and `s2` must point to valid null-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcmp(s1: *const u8, s2: *const u8) -> i32 {
    let mut i = 0;
    loop {
        let (c1, c2) = unsafe { (*s1.add(i), *s2.add(i)) };
        if c1 != c2 {
            return c1 as i32 - c2 as i32;
        }
        if c1 == 0 {
            return 0;
        }
        i += 1;
    }
}

/// Compares at most `n` bytes of two strings lexicographically.
///
/// # Safety
///
/// `s1` and `s2` must point to readable memory buffers of at least `n` bytes or be null-terminated earlier.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let (c1, c2) = unsafe { (*s1.add(i), *s2.add(i)) };
        if c1 != c2 {
            return c1 as i32 - c2 as i32;
        }
        if c1 == 0 {
            return 0;
        }
    }
    0
}

/// Copies the string pointed to by `src` (including null byte) into `dest`.
///
/// Returns the pointer `dest`.
///
/// # Safety
///
/// `dest` buffer must be large enough to contain `src` including null terminator.
/// Memory areas must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcpy(dest: *mut u8, src: *const u8) -> *mut u8 {
    let mut i = 0;
    loop {
        let c = unsafe { *src.add(i) };
        unsafe {
            *dest.add(i) = c;
        }
        if c == 0 {
            break;
        }
        i += 1;
    }
    dest
}

/// Copies up to `n` bytes from string `src` to `dest`, padding with null bytes if needed.
///
/// Returns the pointer `dest`.
///
/// # Safety
///
/// `dest` buffer must have space for at least `n` bytes. Memory regions must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n && unsafe { *src.add(i) } != 0 {
        unsafe {
            *dest.add(i) = *src.add(i);
        }
        i += 1;
    }
    while i < n {
        unsafe {
            *dest.add(i) = 0;
        }
        i += 1;
    }
    dest
}

/// Copies `n` bytes from memory area `src` to memory area `dest`.
///
/// Returns the pointer `dest`.
///
/// # Safety
///
/// Both `dest` and `src` must be valid for reads/writes of `n` bytes.
/// Memory areas must not overlap (use [`memmove`] if overlap is possible).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        unsafe {
            *dest.add(i) = *src.add(i);
        }
    }
    dest
}

/// Fills the first `n` bytes of memory area `s` with byte `c`.
///
/// Returns the pointer `s`.
///
/// # Safety
///
/// `s` must be valid for writes of `n` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        unsafe {
            *s.add(i) = c as u8;
        }
    }
    s
}

/// Compares the first `n` bytes of two memory areas `s1` and `s2`.
///
/// Returns an integer less than, equal to, or greater than zero.
///
/// # Safety
///
/// `s1` and `s2` must be readable for at least `n` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let (c1, c2) = unsafe { (*s1.add(i), *s2.add(i)) };
        if c1 != c2 {
            return c1 as i32 - c2 as i32;
        }
    }
    0
}

/// Copies `n` bytes between potentially overlapping memory areas `src` and `dest`.
///
/// Returns the pointer `dest`.
///
/// # Safety
///
/// `dest` and `src` must be valid for `n` bytes. Correctly handles overlapping buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dest as usize <= src as usize {
        for i in 0..n {
            unsafe {
                *dest.add(i) = *src.add(i);
            }
        }
    } else {
        for i in (0..n).rev() {
            unsafe {
                *dest.add(i) = *src.add(i);
            }
        }
    }
    dest
}

/// Locates the first occurrence of character `c` in string `s`.
///
/// Returns a pointer to the matched character, or null if not found.
///
/// # Safety
///
/// `s` must point to a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strchr(s: *const u8, c: i32) -> *const u8 {
    let target = c as u8;
    let mut ptr = s;
    loop {
        if unsafe { *ptr } == target {
            return ptr;
        }
        if unsafe { *ptr } == 0 {
            return core::ptr::null();
        }
        ptr = unsafe { ptr.add(1) };
    }
}
