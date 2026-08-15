//! Zero-flicker line editor with history traversal, autocompletion,
//! command navigation (Home/End/Word/Arrows), clipboard kill-ring, and bracketed paste.

use crate::completion::handle_tab_completion;
use crate::history::*;
use crate::line_draw::LineBuffer;

/// Capacity in bytes of the shell kill-ring buffer.
pub const KILL_RING_SIZE: usize = 1024;

/// In-memory terminal clipboard and kill-ring buffer.
#[derive(Clone)]
pub struct KillRing {
    /// Raw byte buffer containing killed or copied text.
    pub buf: [u8; KILL_RING_SIZE],
    /// Number of valid bytes in `buf`.
    pub len: usize,
}

impl KillRing {
    /// Creates an empty kill-ring instance.
    pub const fn new() -> Self {
        Self {
            buf: [0; KILL_RING_SIZE],
            len: 0,
        }
    }

    /// Saves a slice of bytes into the kill-ring buffer.
    pub fn save(&mut self, src: &[u8]) {
        let count = src.len().min(KILL_RING_SIZE);
        self.buf[..count].copy_from_slice(&src[..count]);
        self.len = count;
    }

    /// Returns a slice over the current kill-ring buffer contents.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

// SAFETY: Single-threaded REPL execution in Ring 3 interactive shell daemon.
/// Global shell kill-ring clipboard instance.
pub static mut KILL_RING: KillRing = KillRing::new();

/// Moves cursor backward by one word.
pub fn word_left(buf: &[u8], mut cursor_pos: usize) -> usize {
    while cursor_pos > 0 && (buf[cursor_pos - 1] == b' ' || buf[cursor_pos - 1] == b'\t') {
        cursor_pos -= 1;
    }
    while cursor_pos > 0 && buf[cursor_pos - 1] != b' ' && buf[cursor_pos - 1] != b'\t' {
        cursor_pos -= 1;
    }
    cursor_pos
}

/// Moves cursor forward by one word.
pub fn word_right(buf: &[u8], len: usize, mut cursor_pos: usize) -> usize {
    while cursor_pos < len && buf[cursor_pos] != b' ' && buf[cursor_pos] != b'\t' {
        cursor_pos += 1;
    }
    while cursor_pos < len && (buf[cursor_pos] == b' ' || buf[cursor_pos] == b'\t') {
        cursor_pos += 1;
    }
    cursor_pos
}

/// Splices `data` into `buf` at `cursor_pos` safely without out-of-bounds writes.
/// Returns the number of bytes successfully inserted.
pub fn splice_insert(
    buf: &mut [u8],
    len: &mut usize,
    cursor_pos: &mut usize,
    data: &[u8],
) -> usize {
    if buf.is_empty() || *len >= buf.len() - 1 {
        return 0;
    }
    let capacity_left = (buf.len() - 1).saturating_sub(*len);
    let insert_count = capacity_left.min(data.len());
    if insert_count == 0 {
        return 0;
    }
    // Shift tail elements to the right by insert_count
    for i in (*cursor_pos..*len).rev() {
        buf[i + insert_count] = buf[i];
    }
    // Copy data into the gap
    for (i, &b) in data[..insert_count].iter().enumerate() {
        let mut byte = b;
        if byte == b'\r' || byte == b'\n' {
            byte = b' ';
        }
        buf[*cursor_pos + i] = byte;
    }
    *cursor_pos += insert_count;
    *len += insert_count;
    buf[*len] = 0;
    insert_count
}

/// RFC 4648 standard Base64 encoding table.
pub const B64_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes `src` bytes into RFC 4648 Base64 in `dst`.
/// Returns the number of encoded bytes written into `dst`.
pub fn base64_encode(src: &[u8], dst: &mut [u8]) -> usize {
    let mut out_len = 0;
    let mut i = 0;
    while i < src.len() {
        if out_len + 4 > dst.len() {
            break;
        }
        let rem = src.len() - i;
        if rem >= 3 {
            let b0 = src[i];
            let b1 = src[i + 1];
            let b2 = src[i + 2];
            dst[out_len] = B64_CHARS[(b0 >> 2) as usize];
            dst[out_len + 1] = B64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize];
            dst[out_len + 2] = B64_CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize];
            dst[out_len + 3] = B64_CHARS[(b2 & 0x3f) as usize];
            out_len += 4;
            i += 3;
        } else if rem == 2 {
            let b0 = src[i];
            let b1 = src[i + 1];
            dst[out_len] = B64_CHARS[(b0 >> 2) as usize];
            dst[out_len + 1] = B64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize];
            dst[out_len + 2] = B64_CHARS[((b1 & 0x0f) << 2) as usize];
            dst[out_len + 3] = b'=';
            out_len += 4;
            i += 2;
        } else {
            let b0 = src[i];
            dst[out_len] = B64_CHARS[(b0 >> 2) as usize];
            dst[out_len + 1] = B64_CHARS[((b0 & 0x03) << 4) as usize];
            dst[out_len + 2] = b'=';
            dst[out_len + 3] = b'=';
            out_len += 4;
            i += 1;
        }
    }
    out_len
}

/// Emits an ANSI OSC 52 clipboard copy sequence (`\x1b]52;c;<base64>\x07`) in a single syscall
/// to copy data to the host OS clipboard.
pub fn osc52_copy(data: &[u8]) {
    if data.is_empty() {
        return;
    }
    // Maximum base64 payload for 1024 bytes is 1368 bytes + 8 header/footer bytes = 1376 bytes.
    let mut out = [0u8; 1536];
    let header = b"\x1b]52;c;";
    out[..header.len()].copy_from_slice(header);
    let mut total = header.len();

    let encoded_len = base64_encode(data, &mut out[total..total + 1400]);
    total += encoded_len;

    out[total] = 0x07; // BEL terminator
    total += 1;

    // SAFETY: Flushes complete OSC 52 escape sequence in a single write syscall to stdout fd 1.
    unsafe {
        libc::write(1, out.as_ptr(), total);
    }
}

/// Kills (cuts) text from `cursor_pos` to the end of the line into `kill_ring` and syncs with host clipboard.
pub fn kill_to_end(buf: &mut [u8], len: &mut usize, cursor_pos: usize, kill_ring: &mut KillRing) {
    if cursor_pos < *len {
        kill_ring.save(&buf[cursor_pos..*len]);
        osc52_copy(kill_ring.as_bytes());
        *len = cursor_pos;
        buf[*len] = 0;
    }
}

/// Kills (cuts) text from the start of the line up to `cursor_pos` into `kill_ring` and syncs with host clipboard.
pub fn kill_to_start(
    buf: &mut [u8],
    len: &mut usize,
    cursor_pos: &mut usize,
    kill_ring: &mut KillRing,
) {
    if *cursor_pos > 0 {
        kill_ring.save(&buf[..*cursor_pos]);
        osc52_copy(kill_ring.as_bytes());
        for i in *cursor_pos..*len {
            buf[i - *cursor_pos] = buf[i];
        }
        *len -= *cursor_pos;
        *cursor_pos = 0;
        buf[*len] = 0;
    } else if *len > 0 {
        kill_ring.save(&buf[..*len]);
        osc52_copy(kill_ring.as_bytes());
        *len = 0;
        buf[0] = 0;
    }
}

/// Kills (cuts) the previous word before `cursor_pos` into `kill_ring` and syncs with host clipboard.
pub fn kill_word_backward(
    buf: &mut [u8],
    len: &mut usize,
    cursor_pos: &mut usize,
    kill_ring: &mut KillRing,
) {
    if *cursor_pos > 0 {
        let word_start = word_left(buf, *cursor_pos);
        let count = *cursor_pos - word_start;
        kill_ring.save(&buf[word_start..*cursor_pos]);
        osc52_copy(kill_ring.as_bytes());
        for i in *cursor_pos..*len {
            buf[i - count] = buf[i];
        }
        *len -= count;
        *cursor_pos = word_start;
        buf[*len] = 0;
    }
}

/// Yanks (pastes) the contents of `kill_ring` into `buf` at `cursor_pos`.
pub fn yank(
    buf: &mut [u8],
    len: &mut usize,
    cursor_pos: &mut usize,
    kill_ring: &KillRing,
) -> usize {
    splice_insert(buf, len, cursor_pos, kill_ring.as_bytes())
}

/// Deletes the character preceding `cursor_pos` (Backspace).
pub fn backspace(buf: &mut [u8], len: &mut usize, cursor_pos: &mut usize) -> bool {
    if *cursor_pos > 0 {
        for i in *cursor_pos..*len {
            buf[i - 1] = buf[i];
        }
        *cursor_pos -= 1;
        *len -= 1;
        buf[*len] = 0;
        true
    } else {
        false
    }
}

/// Deletes the character at `cursor_pos` (Delete key / Ctrl+D).
pub fn delete_char(buf: &mut [u8], len: &mut usize, cursor_pos: usize) -> bool {
    if cursor_pos < *len {
        for i in cursor_pos..*len - 1 {
            buf[i] = buf[i + 1];
        }
        *len -= 1;
        buf[*len] = 0;
        true
    } else {
        false
    }
}

/// Checks whether a command string matches any element in the `known_commands` list.
pub fn is_known_command(cmd: &str, known_commands: &[&str]) -> bool {
    for &k in known_commands {
        if cmd == k {
            return true;
        }
    }
    false
}

/// Repaints the entire prompt and editable command line with syntax highlighting.
///
/// # Safety
///
/// `cwd` must point to a valid null-terminated C-string.
pub unsafe fn repaint_prompt_line(
    cwd: *const u8,
    buf: &[u8],
    len: usize,
    cursor_pos: usize,
    known_commands: &[&str],
) {
    // SAFETY: Repainting prompt line via single-write LineBuffer syscall.
    unsafe {
        crate::line_draw::paint_prompt(cwd, buf, len, cursor_pos, |cmd| {
            is_known_command(cmd, known_commands)
        });
    }
}

/// Clears any completion menu line below the prompt and refreshes prompt rendering.
///
/// # Safety
///
/// `cwd` must point to a valid null-terminated C-string.
pub unsafe fn clear_menu_line(
    cwd: *const u8,
    buf: &[u8],
    len: usize,
    cursor_pos: usize,
    known_commands: &[&str],
) {
    let mut scratch = [0u8; 64];
    let mut out = LineBuffer::new(&mut scratch);
    out.push_str("\n\r\x1b[K\x1b[A");
    out.flush();
    // SAFETY: Redrawing prompt after clearing menu line.
    unsafe {
        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
    }
}

/// Parsed ANSI escape sequence action.
enum EscSeq {
    /// Up arrow key.
    Up,
    /// Down arrow key.
    Down,
    /// Right arrow key.
    Right,
    /// Left arrow key.
    Left,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Delete key.
    Delete,
    /// Jump one word left (Ctrl+Left / Alt+b).
    WordLeft,
    /// Jump one word right (Ctrl+Right / Alt+f).
    WordRight,
    /// Start of bracketed paste sequence (`\x1b[200~`).
    BracketedPasteStart,
    /// Unrecognized or incomplete escape sequence.
    None,
}

/// Reads and parses an ANSI escape sequence from terminal standard input.
///
/// # Safety
///
/// Standard input must be open and readable in raw mode.
unsafe fn parse_escape_sequence() -> EscSeq {
    // SAFETY: Reading subsequent bytes of ANSI escape sequence from stdin.
    let b2 = unsafe { libc::getchar() };
    if b2 < 0 {
        return EscSeq::None;
    }
    match b2 as u8 {
        b'[' => {
            // SAFETY: Reading parameter/action byte from stdin.
            let b3 = unsafe { libc::getchar() };
            if b3 < 0 {
                return EscSeq::None;
            }
            match b3 as u8 {
                b'A' => EscSeq::Up,
                b'B' => EscSeq::Down,
                b'C' => EscSeq::Right,
                b'D' => EscSeq::Left,
                b'H' => EscSeq::Home,
                b'F' => EscSeq::End,
                b'1' => {
                    // SAFETY: Reading subsequent byte in multi-byte escape sequence.
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::Home
                    } else if b4 == b';' as i32 {
                        // SAFETY: Reading modifier and key code.
                        let b5 = unsafe { libc::getchar() };
                        let b6 = unsafe { libc::getchar() };
                        if b5 == b'5' as i32 {
                            if b6 == b'C' as i32 {
                                EscSeq::WordRight
                            } else if b6 == b'D' as i32 {
                                EscSeq::WordLeft
                            } else {
                                EscSeq::None
                            }
                        } else {
                            EscSeq::None
                        }
                    } else {
                        EscSeq::None
                    }
                }
                b'2' => {
                    // SAFETY: Checking for bracketed paste start sequence `\x1b[200~`.
                    let b4 = unsafe { libc::getchar() };
                    let b5 = unsafe { libc::getchar() };
                    let b6 = unsafe { libc::getchar() };
                    if b4 == b'0' as i32 && b5 == b'0' as i32 && b6 == b'~' as i32 {
                        EscSeq::BracketedPasteStart
                    } else {
                        EscSeq::None
                    }
                }
                b'3' => {
                    // SAFETY: Reading tilde terminator for Delete key `\x1b[3~`.
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::Delete
                    } else {
                        EscSeq::None
                    }
                }
                b'4' => {
                    // SAFETY: Reading tilde terminator for End key `\x1b[4~`.
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::End
                    } else {
                        EscSeq::None
                    }
                }
                b'7' => {
                    // SAFETY: Reading tilde terminator for Home key `\x1b[7~`.
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::Home
                    } else {
                        EscSeq::None
                    }
                }
                b'8' => {
                    // SAFETY: Reading tilde terminator for End key `\x1b[8~`.
                    let b4 = unsafe { libc::getchar() };
                    if b4 == b'~' as i32 {
                        EscSeq::End
                    } else {
                        EscSeq::None
                    }
                }
                _ => EscSeq::None,
            }
        }
        b'O' => {
            // SAFETY: Reading application mode key character.
            let b3 = unsafe { libc::getchar() };
            if b3 < 0 {
                return EscSeq::None;
            }
            match b3 as u8 {
                b'H' => EscSeq::Home,
                b'F' => EscSeq::End,
                b'A' => EscSeq::Up,
                b'B' => EscSeq::Down,
                b'C' => EscSeq::Right,
                b'D' => EscSeq::Left,
                _ => EscSeq::None,
            }
        }
        b'b' => EscSeq::WordLeft,
        b'f' => EscSeq::WordRight,
        _ => EscSeq::None,
    }
}

/// Reads pasted characters from bracketed paste mode (`\x1b[200~` ... `\x1b[201~`).
/// Grammar: reads stream until trailer `ESC [ 2 0 1 ~`, with partial-match byte replay.
///
/// # Safety
///
/// Terminal stdin must be readable in raw mode.
unsafe fn read_bracketed_paste(buf: &mut [u8], len: &mut usize, cursor_pos: &mut usize) {
    let mut paste_buf = [0u8; 1024];
    let mut paste_len = 0;
    let mut state = 0;

    loop {
        // SAFETY: Reading character from terminal stdin.
        let b = unsafe { libc::getchar() };
        if b < 0 {
            break;
        }
        let ch = b as u8;

        match state {
            0 => {
                if ch == 0x1b {
                    state = 1;
                } else if paste_len < paste_buf.len() {
                    paste_buf[paste_len] = ch;
                    paste_len += 1;
                }
            }
            1 => {
                if ch == b'[' {
                    state = 2;
                } else {
                    if paste_len + 1 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = ch;
                        paste_len += 2;
                    }
                    state = 0;
                }
            }
            2 => {
                if ch == b'2' {
                    state = 3;
                } else {
                    if paste_len + 2 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = b'[';
                        paste_buf[paste_len + 2] = ch;
                        paste_len += 3;
                    }
                    state = 0;
                }
            }
            3 => {
                if ch == b'0' {
                    state = 4;
                } else {
                    if paste_len + 3 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = b'[';
                        paste_buf[paste_len + 2] = b'2';
                        paste_buf[paste_len + 3] = ch;
                        paste_len += 4;
                    }
                    state = 0;
                }
            }
            4 => {
                if ch == b'1' {
                    state = 5;
                } else {
                    if paste_len + 4 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = b'[';
                        paste_buf[paste_len + 2] = b'2';
                        paste_buf[paste_len + 3] = b'0';
                        paste_buf[paste_len + 4] = ch;
                        paste_len += 5;
                    }
                    state = 0;
                }
            }
            5 => {
                if ch == b'~' {
                    break;
                } else {
                    if paste_len + 5 < paste_buf.len() {
                        paste_buf[paste_len] = 0x1b;
                        paste_buf[paste_len + 1] = b'[';
                        paste_buf[paste_len + 2] = b'2';
                        paste_buf[paste_len + 3] = b'0';
                        paste_buf[paste_len + 4] = b'1';
                        paste_buf[paste_len + 5] = ch;
                        paste_len += 6;
                    }
                    state = 0;
                }
            }
            _ => state = 0,
        }
    }

    if paste_len > 0 {
        splice_insert(buf, len, cursor_pos, &paste_buf[..paste_len]);
    }
}

/// Reads an interactive input line with full history navigation, editing shortcuts, and tab completion.
///
/// Returns the length of the command line entered in bytes.
///
/// # Safety
///
/// `cwd` must point to a valid null-terminated C-string. Standard I/O must be configured for raw mode.
pub unsafe fn read_line_with_history(
    cwd: *const u8,
    buf: &mut [u8],
    known_commands: &[&str],
) -> usize {
    let mut len = 0;
    let mut cursor_pos = 0;
    let mut history_cursor = 0;
    let mut draft_buf = [0u8; MAX_CMD_LEN];
    let mut draft_len = 0;

    // Enable bracketed paste mode in compatible terminals
    let mut init_scratch = [0u8; 16];
    let mut init_out = LineBuffer::new(&mut init_scratch);
    init_out.push_str("\x1b[?2004h");
    init_out.flush();

    // SAFETY: Repaint initial empty prompt line.
    unsafe {
        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
    }

    loop {
        // SAFETY: Reading next character from terminal stdin.
        let b = unsafe { libc::getchar() };
        if b < 0 {
            continue;
        }
        let ch = b as u8;

        if ch == b'\n' || ch == b'\r' {
            let mut scratch = [0u8; 16];
            let mut out = LineBuffer::new(&mut scratch);
            // Disable bracketed paste mode on return
            out.push_str("\x1b[?2004l\n\r");
            out.flush();
            break;
        } else if ch == 0x01 {
            // Ctrl+A -> Move cursor to start of line
            cursor_pos = 0;
            // SAFETY: Repainting prompt with cursor at start of line.
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x05 {
            // Ctrl+E -> Move cursor to end of line
            cursor_pos = len;
            // SAFETY: Repainting prompt with cursor at end of line.
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x02 {
            // Ctrl+B -> Move cursor backward
            if cursor_pos > 0 {
                cursor_pos -= 1;
                // SAFETY: Repainting prompt after cursor move left.
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x06 {
            // Ctrl+F -> Move cursor forward
            if cursor_pos < len {
                cursor_pos += 1;
                // SAFETY: Repainting prompt after cursor move right.
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x0b {
            // Ctrl+K -> Kill from cursor to end of line
            // SAFETY: Single-threaded REPL execution in Ring 3 interactive shell daemon.
            unsafe {
                let kr = &raw mut KILL_RING;
                kill_to_end(buf, &mut len, cursor_pos, &mut *kr);
            }
            if history_cursor == 0 {
                draft_len = len.min(MAX_CMD_LEN);
                draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
            }
            // SAFETY: Repainting prompt after killing text to end of line.
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x15 {
            // Ctrl+U -> Kill from cursor to start of line (or whole line if cursor is at start)
            // SAFETY: Single-threaded REPL execution in Ring 3 interactive shell daemon.
            unsafe {
                let kr = &raw mut KILL_RING;
                kill_to_start(buf, &mut len, &mut cursor_pos, &mut *kr);
            }
            if history_cursor == 0 {
                draft_len = len.min(MAX_CMD_LEN);
                draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
            }
            // SAFETY: Repainting prompt after killing text to start of line.
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x17 {
            // Ctrl+W -> Kill previous word
            // SAFETY: Single-threaded REPL execution in Ring 3 interactive shell daemon.
            unsafe {
                let kr = &raw mut KILL_RING;
                kill_word_backward(buf, &mut len, &mut cursor_pos, &mut *kr);
            }
            if history_cursor == 0 {
                draft_len = len.min(MAX_CMD_LEN);
                draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
            }
            // SAFETY: Repainting prompt after killing previous word.
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x19 {
            // Ctrl+Y -> Yank (paste from kill ring)
            // SAFETY: Single-threaded REPL execution in Ring 3 interactive shell daemon.
            unsafe {
                let kr = &raw const KILL_RING;
                yank(buf, &mut len, &mut cursor_pos, &*kr);
            }
            if history_cursor == 0 {
                draft_len = len.min(MAX_CMD_LEN);
                draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
            }
            // SAFETY: Repainting prompt after yanking from kill ring.
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x0c {
            // Ctrl+L -> Clear screen and repaint prompt
            let mut scratch = [0u8; 16];
            let mut out = LineBuffer::new(&mut scratch);
            out.push_str("\x1b[2J\x1b[H");
            out.flush();
            // SAFETY: Repainting prompt after screen clear.
            unsafe {
                repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
            }
        } else if ch == 0x04 {
            // Ctrl+D -> Delete under cursor or EOF if line empty
            if len == 0 {
                let mut scratch = [0u8; 16];
                let mut out = LineBuffer::new(&mut scratch);
                out.push_str("\x1b[?2004l\n\r");
                out.flush();
                return 0;
            } else if delete_char(buf, &mut len, cursor_pos) {
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                // SAFETY: Repainting prompt after delete character.
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == b'\t' {
            // SAFETY: Interactive tab completion.
            unsafe {
                handle_tab_completion(
                    cwd,
                    buf,
                    &mut len,
                    known_commands,
                    |c, b, l| repaint_prompt_line(c, b, l, l, known_commands),
                    |c, b, l| clear_menu_line(c, b, l, l, known_commands),
                );
                cursor_pos = len;
            }
        } else if ch == 0x7f || ch == 0x08 {
            // Backspace -> delete character before cursor
            if backspace(buf, &mut len, &mut cursor_pos) {
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                // SAFETY: Repainting prompt after backspace.
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        } else if ch == 0x1b {
            // SAFETY: Parsing ANSI escape sequence.
            match unsafe { parse_escape_sequence() } {
                EscSeq::Up => {
                    if history_cursor == 0 {
                        draft_len = len.min(MAX_CMD_LEN);
                        draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                    }
                    // SAFETY: Traversing previous history entry.
                    unsafe {
                        history_prev(&mut history_cursor, buf, &mut len);
                        cursor_pos = len;
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::Down => {
                    // SAFETY: Traversing next history entry.
                    unsafe {
                        history_next(&mut history_cursor, buf, &mut len, &draft_buf, draft_len);
                        cursor_pos = len;
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::Left => {
                    if cursor_pos > 0 {
                        cursor_pos -= 1;
                        // SAFETY: Repainting prompt after moving cursor left.
                        unsafe {
                            repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                        }
                    }
                }
                EscSeq::Right => {
                    if cursor_pos < len {
                        cursor_pos += 1;
                        // SAFETY: Repainting prompt after moving cursor right.
                        unsafe {
                            repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                        }
                    }
                }
                EscSeq::Home => {
                    cursor_pos = 0;
                    // SAFETY: Repainting prompt at start of line.
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::End => {
                    cursor_pos = len;
                    // SAFETY: Repainting prompt at end of line.
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::WordLeft => {
                    cursor_pos = word_left(buf, cursor_pos);
                    // SAFETY: Repainting prompt after word left jump.
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::WordRight => {
                    cursor_pos = word_right(buf, len, cursor_pos);
                    // SAFETY: Repainting prompt after word right jump.
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::Delete => {
                    if delete_char(buf, &mut len, cursor_pos) {
                        if history_cursor == 0 {
                            draft_len = len.min(MAX_CMD_LEN);
                            draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                        }
                        // SAFETY: Repainting prompt after deleting character under cursor.
                        unsafe {
                            repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                        }
                    }
                }
                EscSeq::BracketedPasteStart => {
                    // SAFETY: Reading bracketed paste sequence from terminal stdin.
                    unsafe {
                        read_bracketed_paste(buf, &mut len, &mut cursor_pos);
                    }
                    if history_cursor == 0 {
                        draft_len = len.min(MAX_CMD_LEN);
                        draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                    }
                    // SAFETY: Repainting prompt after bracketed paste.
                    unsafe {
                        repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                    }
                }
                EscSeq::None => {}
            }
        } else if ch >= 0x20 && ch < 0x7f {
            let ch_slice = [ch];
            if splice_insert(buf, &mut len, &mut cursor_pos, &ch_slice) > 0 {
                if history_cursor == 0 {
                    draft_len = len.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                // SAFETY: Repainting prompt after inserting printable character.
                unsafe {
                    repaint_prompt_line(cwd, buf, len, cursor_pos, known_commands);
                }
            }
        }
    }
    buf[len] = 0;
    if len > 0 {
        // SAFETY: Storing non-empty command into in-memory history ring buffer.
        unsafe {
            history_add(&buf[..len], len);
        }
    }
    len
}
