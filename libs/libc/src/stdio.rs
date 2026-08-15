//! Standard Input/Output Library (stdio.h).

use crate::string::strlen;
use crate::unistd::{read, write};
use posix_abi::*;

/// Writes a single byte/character to standard output.
///
/// Returns the written character cast to `i32`, or `-1` on error.
///
/// # Safety
///
/// Performs direct unbuffered write to [`STDOUT_FILENO`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putchar(c: i32) -> i32 {
    let byte = c as u8;
    // SAFETY: Invokes write syscall on standard output (STDOUT_FILENO) with a 1-byte local stack buffer.
    let ret = unsafe { write(STDOUT_FILENO, &byte as *const u8, 1) };
    if ret == 1 { c } else { -1 }
}

/// Writes a null-terminated string followed by a newline to standard output.
///
/// Returns `0` on success.
///
/// # Safety
///
/// `s` must be a valid pointer to a null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn puts(s: *const u8) -> i32 {
    // SAFETY: Caller guarantees `s` points to a valid null-terminated C string.
    let len = unsafe { strlen(s) };
    // SAFETY: Writes the string bytes followed by a newline byte to standard output.
    unsafe {
        write(STDOUT_FILENO, s, len);
        let newline = b'\n';
        write(STDOUT_FILENO, &newline as *const u8, 1);
    }
    0
}

/// Reads a single unsigned char from standard input.
///
/// Returns the character value cast to `i32`, or `-1` on end-of-file or error.
///
/// # Safety
///
/// Performs direct unbuffered read from [`STDIN_FILENO`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getchar() -> i32 {
    let mut buf = 0u8;
    // SAFETY: Invokes read syscall on standard input (STDIN_FILENO) to read a single byte into local stack variable.
    let n = unsafe { read(STDIN_FILENO, &mut buf as *mut u8, 1) };
    if n == 1 { buf as i32 } else { -1 }
}

/// Renames or moves a filesystem object from `oldpath` to `newpath`.
///
/// Returns `0` on success, or a negative error code on failure.
///
/// # Safety
///
/// `oldpath` and `newpath` must be valid pointers to null-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rename(oldpath: *const u8, newpath: *const u8) -> i32 {
    // SAFETY: Invokes SYS_RENAME syscall with pointers to null-terminated pathname strings.
    unsafe { crate::syscall::syscall2(SYS_RENAME, oldpath as usize, newpath as usize) as i32 }
}

/// In-memory formatting buffer accumulator for formatted string generation.
pub struct FormatBuffer<'a> {
    /// Underlying destination byte buffer.
    pub buf: &'a mut [u8],
    /// Total logical count of bytes emitted (can exceed `buf.len()`).
    pub written: usize,
}

impl<'a> FormatBuffer<'a> {
    /// Appends a single byte to the buffer if capacity remains.
    pub fn push(&mut self, b: u8) {
        if self.written < self.buf.len() {
            self.buf[self.written] = b;
        }
        self.written += 1;
    }

    /// Appends a string slice to the buffer.
    pub fn write_str(&mut self, s: &str) {
        for b in s.bytes() {
            self.push(b);
        }
    }

    /// Formats an integer in the given radix without field padding.
    pub fn write_num(&mut self, num: i64, base: u8, is_signed: bool) {
        self.write_num_padded(num, base, is_signed, 0);
    }

    /// Formats an integer in the given radix with optional minimum width padding.
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

/// Formats text according to format specifier into a fixed-size buffer.
///
/// Returns the number of characters that would have been written (excluding null terminator).
///
/// # Safety
///
/// `str_ptr` must point to writable memory of at least `size` bytes if `size > 0`.
/// `format` must point to a valid null-terminated C format string matching the passed variadic arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snprintf(
    str_ptr: *mut u8,
    size: usize,
    format: *const u8,
    mut args: ...
) -> i32 {
    if size == 0 {
        return 0;
    }
    // SAFETY: Caller guarantees `str_ptr` points to writable buffer of at least `size` bytes.
    let slice = unsafe { core::slice::from_raw_parts_mut(str_ptr, size) };
    let mut fmt_buf = FormatBuffer {
        buf: slice,
        written: 0,
    };

    let mut i = 0;
    // SAFETY: Caller guarantees `format` points to a valid null-terminated format string matching variadic `args`.
    unsafe {
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
    }

    let written = fmt_buf.written;
    // SAFETY: Writes null terminator within the bounded `size` buffer.
    unsafe {
        if written < size {
            *str_ptr.add(written) = 0;
        } else {
            *str_ptr.add(size - 1) = 0;
        }
    }
    written as i32
}

/// Formats text according to format specifier and writes it to standard output.
///
/// Returns the number of characters written.
///
/// # Safety
///
/// `format` must point to a valid null-terminated C format string matching the passed variadic arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn printf(format: *const u8, mut args: ...) -> i32 {
    let mut buf = [0u8; 1024];
    let mut fmt_buf = FormatBuffer {
        buf: &mut buf,
        written: 0,
    };

    let mut i = 0;
    // SAFETY: Caller guarantees `format` points to a valid null-terminated format string matching variadic `args`.
    unsafe {
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
    }

    let written = fmt_buf.written;
    let actual_len = if written < buf.len() {
        written
    } else {
        buf.len() - 1
    };
    // SAFETY: Writes formatted output bytes from stack buffer to stdout.
    unsafe {
        write(STDOUT_FILENO, buf.as_ptr(), actual_len);
    }
    written as i32
}

/// An unbuffered writer targeting a file descriptor, implementing `core::fmt::Write`.
pub struct FdWriter(
    /// Target file descriptor for raw write operations.
    pub i32,
);

impl FdWriter {
    /// Creates a new `FdWriter` for the given file descriptor.
    pub const fn new(fd: i32) -> Self {
        Self(fd)
    }
}

impl core::fmt::Write for FdWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let mut written = 0;
        while written < bytes.len() {
            // SAFETY: bytes.as_ptr().add(written) is within slice bounds; length is remaining bytes.
            let ret = unsafe { write(self.0, bytes.as_ptr().add(written), bytes.len() - written) };
            if ret <= 0 {
                return Err(core::fmt::Error);
            }
            written += ret as usize;
        }
        Ok(())
    }
}

/// Writes formatted `PanicInfo` to the specified file descriptor.
///
/// The write is intentionally best-effort: if the file descriptor is closed or unwritable,
/// the function returns without hanging or panicking recursively.
pub fn write_panic_info(fd: i32, prefix: &str, info: &core::panic::PanicInfo) {
    let mut writer = FdWriter::new(fd);
    // Best-effort write: ignore error if fd is not available to guarantee panic handler can exit.
    let _ = core::fmt::write(&mut writer, format_args!("{}: {}\n", prefix, info));
}
