//! Fuzzy command and path autocompletion engine with interactive arrow-key menus.

use crate::line_draw::LineBuffer;
use libc::*;
use posix_abi::*;

/// Computes a fuzzy match score for `target` against `pattern`.
///
/// Returns `Some(score)` if all characters in `pattern` appear sequentially in `target`,
/// rewarding boundary prefixes and consecutive matches, or `None` if unmatched.
pub fn fuzzy_score(pattern: &str, target: &str) -> Option<i32> {
    let p_bytes = pattern.as_bytes();
    let t_bytes = target.as_bytes();
    if p_bytes.is_empty() {
        return Some(0);
    }
    if p_bytes.len() > t_bytes.len() {
        return None;
    }
    let mut p_idx = 0;
    let mut score = 0;
    let mut prev_matched_idx = -1i32;
    for (t_idx, &t_byte) in t_bytes.iter().enumerate() {
        if p_idx < p_bytes.len()
            && p_bytes[p_idx].to_ascii_lowercase() == t_byte.to_ascii_lowercase()
        {
            score += 10;
            if t_idx == 0
                || t_bytes[t_idx - 1] == b'/'
                || t_bytes[t_idx - 1] == b'-'
                || t_bytes[t_idx - 1] == b'_'
            {
                score += 15;
            }
            if prev_matched_idx >= 0 && (t_idx as i32 == prev_matched_idx + 1) {
                score += 20;
            }
            if p_bytes[p_idx] == t_byte {
                score += 5;
            }
            prev_matched_idx = t_idx as i32;
            p_idx += 1;
        }
    }
    if p_idx == p_bytes.len() {
        score -= (t_bytes.len() - p_bytes.len()) as i32;
        Some(score)
    } else {
        None
    }
}

/// Represents an autocompletion match candidate for command or path expansion.
#[derive(Clone, Copy)]
pub struct MatchCandidate {
    /// Byte buffer storing the candidate name.
    pub name: [u8; 64],
    /// Number of valid bytes in `name`.
    pub len: usize,
    /// Fuzzy relevance ranking score.
    pub score: i32,
    /// Indicates whether the candidate is a directory.
    pub is_dir: bool,
}

impl Default for MatchCandidate {
    fn default() -> Self {
        Self {
            name: [0; 64],
            len: 0,
            score: 0,
            is_dir: false,
        }
    }
}

/// Renders a horizontal completion menu row to the terminal, highlighting the active selection.
///
/// # Safety
///
/// Standard output must be writable and configured for terminal escape sequences.
pub unsafe fn render_menu_row(matches: &[MatchCandidate], count: usize, selected: usize) {
    let mut scratch = [0u8; 1024];
    let mut out = LineBuffer::new(&mut scratch);
    out.push_str("\n\r\x1b[K");
    for (idx, m) in matches[..count].iter().enumerate() {
        if idx > 0 {
            out.push_str("  ");
        }
        if idx == selected {
            out.push_str("\x1b[7;32m[");
        } else {
            out.push_str(" ");
        }
        for b in &m.name[..m.len] {
            out.push_byte(*b);
        }
        if m.is_dir {
            out.push_byte(b'/');
        }
        if idx == selected {
            out.push_str("]\x1b[0m");
        } else {
            out.push_str(" ");
        }
    }
    out.flush();
}

/// Displays an interactive completion menu permitting arrow-key or Tab selection of matches.
///
/// # Safety
///
/// `cwd` must point to a valid null-terminated C-string. Standard I/O must be configured for raw mode.
pub unsafe fn show_completion_menu(
    cwd: *const u8,
    buf: &mut [u8],
    len: &mut usize,
    replace_start: usize,
    matches: &mut [MatchCandidate],
    count: usize,
    repaint_fn: &impl Fn(*const u8, &[u8], usize),
    clear_menu_fn: impl Fn(*const u8, &[u8], usize),
) {
    let mut sel = 0;
    unsafe {
        render_menu_row(matches, count, sel);
        out_cursor_up_to_prompt(cwd, buf, *len, repaint_fn);
    }

    loop {
        let b = unsafe { libc::getchar() };
        if b < 0 {
            continue;
        }
        let ch = b as u8;
        if ch == b'\t' || ch == 0x06 {
            sel = (sel + 1) % count;
            unsafe {
                render_menu_row(matches, count, sel);
                out_cursor_up_to_prompt(cwd, buf, *len, repaint_fn);
            }
        } else if ch == 0x1b {
            let b2 = unsafe { libc::getchar() };
            let b3 = unsafe { libc::getchar() };
            if b2 == b'[' as i32 {
                match b3 as u8 {
                    b'C' => {
                        sel = (sel + 1) % count;
                        unsafe {
                            render_menu_row(matches, count, sel);
                            out_cursor_up_to_prompt(cwd, buf, *len, repaint_fn);
                        }
                    }
                    b'D' => {
                        sel = if sel == 0 { count - 1 } else { sel - 1 };
                        unsafe {
                            render_menu_row(matches, count, sel);
                            out_cursor_up_to_prompt(cwd, buf, *len, repaint_fn);
                        }
                    }
                    _ => {}
                }
            }
        } else if ch == b'\n' || ch == b'\r' || ch == b' ' {
            let m = &matches[sel];
            let ins_len = m.len;
            if replace_start + ins_len + 1 < buf.len() {
                buf[replace_start..replace_start + ins_len].copy_from_slice(&m.name[..ins_len]);
                let mut end_pos = replace_start + ins_len;
                if m.is_dir {
                    buf[end_pos] = b'/';
                    end_pos += 1;
                } else if ch == b' ' {
                    buf[end_pos] = b' ';
                    end_pos += 1;
                }
                buf[end_pos] = 0;
                *len = end_pos;
            }
            clear_menu_fn(cwd, buf, *len);
            break;
        } else {
            clear_menu_fn(cwd, buf, *len);
            break;
        }
    }
}

/// Moves the cursor up one line and triggers a redraw of the prompt and line buffer.
///
/// # Safety
///
/// `cwd` must point to a valid null-terminated C-string.
unsafe fn out_cursor_up_to_prompt(
    cwd: *const u8,
    buf: &[u8],
    len: usize,
    repaint_fn: &impl Fn(*const u8, &[u8], usize),
) {
    let mut scratch = [0u8; 32];
    let mut out = LineBuffer::new(&mut scratch);
    out.push_str("\x1b[A");
    out.flush();
    repaint_fn(cwd, buf, len);
}

/// Handles Tab key presses by resolving matching commands or filesystem paths and autocompleting.
///
/// # Safety
///
/// `cwd` must point to a valid null-terminated C-string. Terminal must be in raw mode.
pub unsafe fn handle_tab_completion(
    cwd: *const u8,
    buf: &mut [u8],
    len: &mut usize,
    known_commands: &[&str],
    repaint_fn: impl Fn(*const u8, &[u8], usize),
    clear_menu_fn: impl Fn(*const u8, &[u8], usize),
) {
    let mut start = 0;
    while start < *len && (buf[start] == b' ' || buf[start] == b'\t') {
        start += 1;
    }
    let mut is_command_name = true;
    for i in start..*len {
        if buf[i] == b' ' || buf[i] == b'\t' {
            is_command_name = false;
            break;
        }
    }
    let mut matches: [MatchCandidate; 24] = [MatchCandidate::default(); 24];
    let mut match_count = 0;
    let replace_start: usize;
    if is_command_name {
        replace_start = start;
        let prefix = match core::str::from_utf8(&buf[start..*len]) {
            Ok(s) => s,
            Err(_) => return,
        };
        for &cmd in known_commands.iter() {
            if let Some(sc) = fuzzy_score(prefix, cmd) {
                if match_count < 24 {
                    let bytes = cmd.as_bytes();
                    let item_len = bytes.len().min(63);
                    matches[match_count].name[..item_len].copy_from_slice(&bytes[..item_len]);
                    matches[match_count].len = item_len;
                    matches[match_count].score = sc;
                    match_count += 1;
                }
            }
        }
    } else {
        let mut last_space = *len;
        while last_space > 0 && buf[last_space - 1] != b' ' && buf[last_space - 1] != b'\t' {
            last_space -= 1;
        }
        replace_start = last_space;
        let arg_prefix = match core::str::from_utf8(&buf[last_space..*len]) {
            Ok(s) => s,
            Err(_) => return,
        };
        let fd = unsafe { open(b".\0".as_ptr(), O_RDONLY | O_DIRECTORY, 0) };
        if fd >= 0 {
            let mut dir_buf = [0u8; 4096];
            let n = unsafe {
                syscall::syscall3(
                    SYS_GETDENTS64,
                    fd as usize,
                    dir_buf.as_mut_ptr() as usize,
                    dir_buf.len(),
                ) as isize
            };
            unsafe { close(fd) };
            if n > 0 {
                let mut offset = 0;
                while offset < n as usize && match_count < 24 {
                    let dirent = unsafe { &*(dir_buf.as_ptr().add(offset) as *const Dirent64) };
                    let mut name_len = 0;
                    while name_len < dirent.d_name.len() && dirent.d_name[name_len] != 0 {
                        name_len += 1;
                    }
                    if name_len > 0 {
                        let name_bytes = &dirent.d_name[..name_len];
                        if let Ok(name_str) = core::str::from_utf8(name_bytes) {
                            if !name_str.starts_with('.') || arg_prefix.starts_with('.') {
                                if let Some(sc) = fuzzy_score(arg_prefix, name_str) {
                                    let item_len = name_len.min(63);
                                    matches[match_count].name[..item_len]
                                        .copy_from_slice(&name_bytes[..item_len]);
                                    matches[match_count].len = item_len;
                                    matches[match_count].score = sc;
                                    matches[match_count].is_dir = dirent.d_type == DT_DIR;
                                    match_count += 1;
                                }
                            }
                        }
                    }
                    offset += core::mem::size_of::<Dirent64>();
                }
            }
        }
    }

    if match_count == 0 {
        return;
    }

    for i in 0..match_count {
        for j in (i + 1)..match_count {
            if matches[j].score > matches[i].score {
                matches.swap(i, j);
            }
        }
    }

    if match_count == 1 {
        let m = &matches[0];
        let ins_len = m.len;
        if replace_start + ins_len + 1 < buf.len() {
            buf[replace_start..replace_start + ins_len].copy_from_slice(&m.name[..ins_len]);
            let mut end_pos = replace_start + ins_len;
            if m.is_dir {
                buf[end_pos] = b'/';
                end_pos += 1;
            } else {
                buf[end_pos] = b' ';
                end_pos += 1;
            }
            buf[end_pos] = 0;
            *len = end_pos;
        }
        repaint_fn(cwd, buf, *len);
    } else {
        unsafe {
            show_completion_menu(
                cwd,
                buf,
                len,
                replace_start,
                &mut matches,
                match_count,
                &repaint_fn,
                clear_menu_fn,
            );
        }
    }
}
