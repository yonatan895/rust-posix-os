//! Standard Input/Output Library (stdio.h).

use posix_abi::*;
use crate::unistd::{read, write};
use crate::string::strlen;

#[no_mangle]
pub unsafe extern "C" fn putchar(c: i32) -> i32 {
    let byte = c as u8;
    let ret = write(STDOUT_FILENO, &byte as *const u8, 1);
    if ret == 1 { c } else { -1 }
}

#[no_mangle]
pub unsafe extern "C" fn puts(s: *const u8) -> i32 {
    let len = strlen(s);
    write(STDOUT_FILENO, s, len);
    let newline = b'\n';
    write(STDOUT_FILENO, &newline as *const u8, 1);
    0
}

#[no_mangle]
pub unsafe extern "C" fn getchar() -> i32 {
    let mut buf = 0u8;
    let n = read(STDIN_FILENO, &mut buf as *mut u8, 1);
    if n == 1 {
        buf as i32
    } else {
        -1
    }
}

#[no_mangle]
pub unsafe extern "C" fn rename(oldpath: *const u8, newpath: *const u8) -> i32 {
    crate::syscall::syscall2(SYS_RENAME, oldpath as usize, newpath as usize) as i32
}

pub struct FormatBuffer<'a> {
    pub buf: &'a mut [u8],
    pub written: usize,
}

impl<'a> FormatBuffer<'a> {
    pub fn push(&mut self, b: u8) {
        if self.written < self.buf.len() {
            self.buf[self.written] = b;
        }
        self.written += 1;
    }

    pub fn write_str(&mut self, s: &str) {
        for b in s.bytes() {
            self.push(b);
        }
    }

    pub fn write_num(&mut self, num: i64, base: u8, is_signed: bool) {
        self.write_num_padded(num, base, is_signed, 0);
    }

    pub fn write_num_padded(&mut self, mut num: i64, base: u8, is_signed: bool, width: usize) {
        let is_neg = is_signed && num < 0;
        if is_neg {
            num = -num;
        }
        let mut unum = num as u64;
        let mut digits = [0u8; 32];
        let mut count = 0;
        loop {
            let d = (unum % base as u64) as u8;
            digits[count] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
            count += 1;
            unum /= base as u64;
            if unum == 0 {
                break;
            }
        }
        let total_len = count + if is_neg { 1 } else { 0 };
        if width > total_len {
            for _ in 0..(width - total_len) {
                self.push(b' ');
            }
        }
        if is_neg {
            self.push(b'-');
        }
        while count > 0 {
            count -= 1;
            self.push(digits[count]);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn snprintf(
    str_ptr: *mut u8,
    size: usize,
    format: *const u8,
    mut args: ...
) -> i32 {
    if size == 0 {
        return 0;
    }
    let slice = core::slice::from_raw_parts_mut(str_ptr, size);
    let mut fmt_buf = FormatBuffer {
        buf: slice,
        written: 0,
    };

    let mut i = 0;
    while *format.add(i) != 0 {
        let b = *format.add(i);
        if b == b'%' {
            i += 1;
            let mut width = 0;
            while *format.add(i) >= b'0' && *format.add(i) <= b'9' {
                width = width * 10 + (*format.add(i) - b'0') as usize;
                i += 1;
            }
            let spec = *format.add(i);
            match spec {
                b'd' | b'i' => {
                    let val: i32 = args.next_arg::<i32>();
                    fmt_buf.write_num_padded(val as i64, 10, true, width);
                }
                b'u' => {
                    let val: u32 = args.next_arg::<u32>();
                    fmt_buf.write_num_padded(val as i64, 10, false, width);
                }
                b'x' => {
                    let val: u32 = args.next_arg::<u32>();
                    fmt_buf.write_num_padded(val as i64, 16, false, width);
                }
                b'p' => {
                    let val: usize = args.next_arg::<usize>();
                    fmt_buf.write_str("0x");
                    fmt_buf.write_num_padded(val as i64, 16, false, width);
                }
                b's' => {
                    let val: *const u8 = args.next_arg::<*const u8>();
                    if val.is_null() {
                        fmt_buf.write_str("(null)");
                    } else {
                        let len = strlen(val);
                        for j in 0..len {
                            fmt_buf.push(*val.add(j));
                        }
                    }
                }
                b'c' => {
                    let val: i32 = args.next_arg::<i32>();
                    fmt_buf.push(val as u8);
                }
                b'%' => {
                    fmt_buf.push(b'%');
                }
                _ => {
                    fmt_buf.push(b'%');
                    fmt_buf.push(spec);
                }
            }
        } else {
            fmt_buf.push(b);
        }
        i += 1;
    }

    let written = fmt_buf.written;
    if written < size {
        *str_ptr.add(written) = 0;
    } else {
        *str_ptr.add(size - 1) = 0;
    }
    written as i32
}

#[no_mangle]
pub unsafe extern "C" fn printf(format: *const u8, mut args: ...) -> i32 {
    let mut buf = [0u8; 1024];
    let mut fmt_buf = FormatBuffer {
        buf: &mut buf,
        written: 0,
    };

    let mut i = 0;
    while *format.add(i) != 0 {
        let b = *format.add(i);
        if b == b'%' {
            i += 1;
            let mut width = 0;
            while *format.add(i) >= b'0' && *format.add(i) <= b'9' {
                width = width * 10 + (*format.add(i) - b'0') as usize;
                i += 1;
            }
            let spec = *format.add(i);
            match spec {
                b'd' | b'i' => {
                    let val: i32 = args.next_arg::<i32>();
                    fmt_buf.write_num_padded(val as i64, 10, true, width);
                }
                b'u' => {
                    let val: u32 = args.next_arg::<u32>();
                    fmt_buf.write_num_padded(val as i64, 10, false, width);
                }
                b'x' => {
                    let val: u32 = args.next_arg::<u32>();
                    fmt_buf.write_num_padded(val as i64, 16, false, width);
                }
                b'p' => {
                    let val: usize = args.next_arg::<usize>();
                    fmt_buf.write_str("0x");
                    fmt_buf.write_num_padded(val as i64, 16, false, width);
                }
                b's' => {
                    let val: *const u8 = args.next_arg::<*const u8>();
                    if val.is_null() {
                        fmt_buf.write_str("(null)");
                    } else {
                        let len = strlen(val);
                        for j in 0..len {
                            fmt_buf.push(*val.add(j));
                        }
                    }
                }
                b'c' => {
                    let val: i32 = args.next_arg::<i32>();
                    fmt_buf.push(val as u8);
                }
                b'%' => {
                    fmt_buf.push(b'%');
                }
                _ => {
                    fmt_buf.push(b'%');
                    fmt_buf.push(spec);
                }
            }
        } else {
            fmt_buf.push(b);
        }
        i += 1;
    }

    let written = fmt_buf.written;
    let actual_len = if written < buf.len() { written } else { buf.len() - 1 };
    write(STDOUT_FILENO, buf.as_ptr(), actual_len);
    written as i32
}
