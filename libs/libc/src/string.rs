//! Standard C String and Memory Routines (extern "C").

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut len = 0;
    while unsafe { *s.add(len) } != 0 {
        len += 1;
    }
    len
}

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for i in 0..n {
        unsafe {
            *dest.add(i) = *src.add(i);
        }
    }
    dest
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    for i in 0..n {
        unsafe {
            *s.add(i) = c as u8;
        }
    }
    s
}

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
