//! Single-syscall prompt painter.
//!
//! Flicker comes from writing one frame (spaces) then overwriting it. The
//! rule is: never send something you will immediately replace. One write
//! of CR + text + EL (erase to end of line) is enough.

use posix_abi::STDOUT_FILENO;

pub struct LineBuffer<'a> {
    pub buf: &'a mut [u8],
    pub len: usize,
}

impl<'a> LineBuffer<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    pub fn push_byte(&mut self, b: u8) {
        if self.len < self.buf.len() {
            self.buf[self.len] = b;
            self.len += 1;
        }
    }

    pub fn push_num(&mut self, mut num: usize) {
        if num == 0 {
            self.push_byte(b'0');
            return;
        }
        let mut digits = [0u8; 10];
        let mut count = 0;
        while num > 0 {
            digits[count] = b'0' + (num % 10) as u8;
            num /= 10;
            count += 1;
        }
        while count > 0 {
            count -= 1;
            self.push_byte(digits[count]);
        }
    }

    pub fn push_str(&mut self, s: &str) {
        for b in s.bytes() {
            self.push_byte(b);
        }
    }

    pub fn push_cstr(&mut self, s: *const u8) {
        if s.is_null() {
            return;
        }
        let mut ptr = s;
        unsafe {
            while *ptr != 0 {
                self.push_byte(*ptr);
                ptr = ptr.add(1);
            }
        }
    }

    pub fn flush(&self) {
        if self.len > 0 {
            unsafe {
                libc::write(STDOUT_FILENO, self.buf.as_ptr(), self.len);
            }
        }
    }
}

pub unsafe fn cmd_span(buf: &[u8], len: usize) -> (usize, usize) {
    let mut start = 0;
    while start < len && (buf[start] == b' ' || buf[start] == b'\t') {
        start += 1;
    }
    let mut end = start;
    while end < len
        && buf[end] != b' '
        && buf[end] != b'\t'
        && buf[end] != b'|'
        && buf[end] != b'>'
        && buf[end] != b'<'
    {
        end += 1;
    }
    (start, end)
}

pub unsafe fn paint_prompt(
    cwd: *const u8,
    buf: &[u8],
    len: usize,
    cursor_pos: usize,
    is_known: impl Fn(&str) -> bool,
) {
    let mut scratch = [0u8; 1024];
    let mut out = LineBuffer::new(&mut scratch);
    // Hide cursor so it does not flash at column 0 during the rewrite.
    out.push_str("\x1b[?25l\rposix-os:");
    out.push_cstr(cwd);
    out.push_str("# ");

    let (start, cmd_end) = unsafe { cmd_span(buf, len) };
    for i in 0..start {
        out.push_byte(buf[i]);
    }
    if start < len {
        let valid = core::str::from_utf8(&buf[start..cmd_end])
            .map(|s| is_known(s))
            .unwrap_or(false);
        out.push_str(if valid { "\x1b[32m" } else { "\x1b[31m" });
        for &b in &buf[start..cmd_end] {
            out.push_byte(b);
        }
        out.push_str("\x1b[0m");
        for i in cmd_end..len {
            out.push_byte(buf[i]);
        }
    }
    // Erase leftover glyphs from a longer previous line.
    out.push_str("\x1b[K");

    // Reposition cursor to cursor_pos if cursor is not at the end of the line
    if cursor_pos < len {
        let back = len - cursor_pos;
        out.push_str("\x1b[");
        out.push_num(back);
        out.push_byte(b'D');
    }

    // Unhide cursor at target position
    out.push_str("\x1b[?25h");
    out.flush();
}
