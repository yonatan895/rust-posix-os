//! Zero-Flicker Line Editor with History Traversal & Autocompletion Integration.

use crate::completion::handle_tab_completion;
use crate::history::*;
use crate::line_draw::LineBuffer;

pub fn is_known_command(cmd: &str, known_commands: &[&str]) -> bool {
    for &k in known_commands {
        if cmd == k {
            return true;
        }
    }
    false
}

pub unsafe fn repaint_prompt_line(cwd: *const u8, buf: &[u8], len: usize, known_commands: &[&str]) {
    unsafe {
        crate::line_draw::paint_prompt(cwd, buf, len, |cmd| is_known_command(cmd, known_commands));
    }
}

pub unsafe fn clear_menu_line(cwd: *const u8, buf: &[u8], len: usize, known_commands: &[&str]) {
    let mut scratch = [0u8; 64];
    let mut out = LineBuffer::new(&mut scratch);
    out.push_str("\n\r\x1b[K\x1b[A");
    out.flush();
    unsafe {
        repaint_prompt_line(cwd, buf, len, known_commands);
    }
}

pub unsafe fn read_line_with_history(
    cwd: *const u8,
    buf: &mut [u8],
    known_commands: &[&str],
) -> usize {
    let mut idx = 0;
    let mut history_cursor = 0;
    let mut draft_buf = [0u8; MAX_CMD_LEN];
    let mut draft_len = 0;

    unsafe {
        repaint_prompt_line(cwd, buf, idx, known_commands);
    }

    loop {
        let b = unsafe { libc::getchar() };
        if b < 0 {
            continue;
        }
        let ch = b as u8;

        if ch == b'\n' || ch == b'\r' {
            let mut scratch = [0u8; 8];
            let mut out = LineBuffer::new(&mut scratch);
            out.push_str("\n\r");
            out.flush();
            break;
        } else if ch == b'\t' || ch == 0x06 {
            unsafe {
                handle_tab_completion(
                    cwd,
                    buf,
                    &mut idx,
                    known_commands,
                    |c, b, l| repaint_prompt_line(c, b, l, known_commands),
                    |c, b, l| clear_menu_line(c, b, l, known_commands),
                );
            }
        } else if ch == 0x7f || ch == 0x08 {
            if idx > 0 {
                idx -= 1;
                buf[idx] = 0;
                if history_cursor == 0 {
                    draft_len = idx.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, idx, known_commands);
                }
            }
        } else if ch == 0x1b {
            let b2 = unsafe { libc::getchar() };
            let b3 = unsafe { libc::getchar() };
            if b2 == b'[' as i32 {
                match b3 as u8 {
                    b'A' => {
                        if history_cursor == 0 {
                            draft_len = idx.min(MAX_CMD_LEN);
                            draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                        }
                        unsafe {
                            history_prev(&mut history_cursor, buf, &mut idx);
                            repaint_prompt_line(cwd, buf, idx, known_commands);
                        }
                    }
                    b'B' => unsafe {
                        history_next(&mut history_cursor, buf, &mut idx, &draft_buf, draft_len);
                        repaint_prompt_line(cwd, buf, idx, known_commands);
                    },
                    _ => {}
                }
            }
        } else if ch >= 0x20 && ch < 0x7f {
            if idx < buf.len() - 1 {
                buf[idx] = ch;
                idx += 1;
                buf[idx] = 0;
                if history_cursor == 0 {
                    draft_len = idx.min(MAX_CMD_LEN);
                    draft_buf[..draft_len].copy_from_slice(&buf[..draft_len]);
                }
                unsafe {
                    repaint_prompt_line(cwd, buf, idx, known_commands);
                }
            }
        }
    }
    buf[idx] = 0;
    if idx > 0 {
        unsafe {
            history_add(&buf[..idx], idx);
        }
    }
    idx
}
