#![no_std]
#![no_main]

mod line_draw;

use core::panic::PanicInfo;
use libc::*;
use posix_abi::*;

static KNOWN_COMMANDS: [&str; 18] = [
    "help", "uname", "pwd", "cd", "ls", "cat", "touch", "mkdir", "rm",
    "ps", "top", "monitor", "journal", "snapshot", "echo", "async-demo",
    "clear", "exit",
];

static mut OLDPWD_BUF: [u8; 128] = [0u8; 128];
static mut HAS_OLDPWD: bool = false;

const MAX_HISTORY: usize = 1000;
const MAX_CMD_LEN: usize = 128;

#[derive(Clone, Copy)]
struct HistoryEntry {
    buf: [u8; MAX_CMD_LEN],
    len: usize,
}

impl Default for HistoryEntry {
    fn default() -> Self {
        Self { buf: [0; MAX_CMD_LEN], len: 0 }
    }
}

static mut HISTORY: [HistoryEntry; MAX_HISTORY] =
    [HistoryEntry { buf: [0; MAX_CMD_LEN], len: 0 }; MAX_HISTORY];
static mut HISTORY_COUNT: usize = 0;

use line_draw::LineBuffer;

unsafe fn history_add(cmd: &[u8], len: usize) {
    if len == 0 { return; }
    let mut non_ws = false;
    for i in 0..len {
        if cmd[i] != b' ' && cmd[i] != b'\t' && cmd[i] != b'\r' && cmd[i] != b'\n' {
            non_ws = true;
            break;
        }
    }
    if !non_ws { return; }
    if HISTORY_COUNT > 0 {
        let last_idx = (HISTORY_COUNT - 1) % MAX_HISTORY;
        let last = &HISTORY[last_idx];
        if last.len == len && &last.buf[..len] == &cmd[..len] { return; }
    }
    let idx = HISTORY_COUNT % MAX_HISTORY;
    let save_len = len.min(MAX_CMD_LEN - 1);
    HISTORY[idx].buf[..save_len].copy_from_slice(&cmd[..save_len]);
    HISTORY[idx].buf[save_len] = 0;
    HISTORY[idx].len = save_len;
    HISTORY_COUNT += 1;
}

unsafe fn history_get(history_index: usize) -> Option<(&'static [u8], usize)> {
    if HISTORY_COUNT == 0 || history_index >= HISTORY_COUNT { return None; }
    let earliest = if HISTORY_COUNT > MAX_HISTORY { HISTORY_COUNT - MAX_HISTORY } else { 0 };
    if history_index < earliest { return None; }
    let slot = history_index % MAX_HISTORY;
    Some((&HISTORY[slot].buf[..HISTORY[slot].len], HISTORY[slot].len))
}

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    puts(b"\n=======================================================\0".as_ptr());
    puts(b"  Rust POSIX Shell (v1.0.0-x86_64)                     \0".as_ptr());
    puts(b"  Type 'help' for a list of available commands.        \0".as_ptr());
    puts(b"=======================================================\n\0".as_ptr());

    let mut line_buf = [0u8; 256];
    let mut cwd_buf = [0u8; 128];
    loop {
        getcwd(cwd_buf.as_mut_ptr(), cwd_buf.len());
        let len = read_line(cwd_buf.as_ptr(), &mut line_buf);
        if len == 0 { continue; }
        execute_pipeline_line(line_buf.as_mut_ptr());
    }
}

unsafe fn is_known_command(cmd: &str) -> bool {
    for &k in KNOWN_COMMANDS.iter() {
        if k == cmd { return true; }
    }
    false
}

unsafe fn repaint_prompt_line(cwd: *const u8, buf: &[u8], len: usize) {
    line_draw::paint_prompt(cwd, buf, len, |s| is_known_command(s));
}

#[derive(Clone, Copy)]
struct MatchCandidate {
    name: [u8; 64],
    len: usize,
    score: i32,
    is_dir: bool,
}

impl Default for MatchCandidate {
    fn default() -> Self {
        Self { name: [0u8; 64], len: 0, score: 0, is_dir: false }
    }
}

fn fuzzy_score(pattern: &str, candidate: &str) -> Option<i32> {
    if pattern.is_empty() { return Some(100); }
    let p_bytes = pattern.as_bytes();
    let c_bytes = candidate.as_bytes();
    let mut p_idx = 0;
    let mut score = 0;
    let mut prev_matched_idx = 0;
    let mut consecutive = 0;
    for (c_idx, &cb) in c_bytes.iter().enumerate() {
        if p_idx < p_bytes.len() {
            let pb = p_bytes[p_idx];
            let pb_lower = if pb >= b'A' && pb <= b'Z' { pb + 32 } else { pb };
            let cb_lower = if cb >= b'A' && cb <= b'Z' { cb + 32 } else { cb };
            if pb_lower == cb_lower {
                score += 10;
                if c_idx == 0 { score += 40; }
                if p_idx > 0 && c_idx == prev_matched_idx + 1 {
                    consecutive += 1;
                    score += consecutive * 15;
                } else {
                    consecutive = 0;
                }
                prev_matched_idx = c_idx;
                p_idx += 1;
            }
        }
    }
    if p_idx == p_bytes.len() {
        let len_diff = candidate.len().saturating_sub(pattern.len()) as i32;
        score -= len_diff * 2;
        Some(score)
    } else {
        None
    }
}

unsafe fn render_menu_view(
    cwd: *const u8,
    buf: &[u8],
    len: usize,
    matches: &[MatchCandidate],
    match_count: usize,
    selected_idx: usize,
) {
    let mut scratch = [0u8; 1024];
    let mut out = LineBuffer::new(&mut scratch);
    out.push_str("\rposix-os:");
    out.push_cstr(cwd);
    out.push_str("# ");
    let mut start = 0;
    while start < len && (buf[start] == b' ' || buf[start] == b'\t') {
        out.push_byte(buf[start]);
        start += 1;
    }
    if start < len {
        let mut cmd_end = start;
        while cmd_end < len && buf[cmd_end] != b' ' && buf[cmd_end] != b'\t'
            && buf[cmd_end] != b'|' && buf[cmd_end] != b'>' && buf[cmd_end] != b'<' {
            cmd_end += 1;
        }
        let cmd_slice = &buf[start..cmd_end];
        let is_valid = core::str::from_utf8(cmd_slice).map(|s| is_known_command(s)).unwrap_or(false);
        out.push_str(if is_valid { "\x1b[32m" } else { "\x1b[31m" });
        for &b in cmd_slice { out.push_byte(b); }
        out.push_str("\x1b[0m");
        if cmd_end < len {
            for i in cmd_end..len { out.push_byte(buf[i]); }
        }
    }
    out.push_str("\n\r\x1b[K");
    for i in 0..match_count {
        let item = &matches[i];
        let name_slice = &item.name[..item.len];
        if i == selected_idx {
            out.push_str("\x1b[7;1;32m [ > ");
            for &b in name_slice { out.push_byte(b); }
            if item.is_dir { out.push_byte(b'/'); }
            out.push_str(" < ] \x1b[0m   ");
        } else {
            out.push_str("\x1b[36m   ");
            for &b in name_slice { out.push_byte(b); }
            if item.is_dir { out.push_byte(b'/'); }
            out.push_str("   \x1b[0m   ");
        }
    }
    out.push_str("\x1b[A\rposix-os:");
    out.push_cstr(cwd);
    out.push_str("# ");
    for i in 0..len { out.push_byte(buf[i]); }
    out.flush();
}

unsafe fn clear_menu_line(cwd: *const u8, buf: &[u8], len: usize) {
    let mut scratch = [0u8; 64];
    let mut out = LineBuffer::new(&mut scratch);
    out.push_str("\n\r\x1b[K\x1b[A");
    out.flush();
    repaint_prompt_line(cwd, buf, len);
}

unsafe fn handle_tab_completion(cwd: *const u8, buf: &mut [u8], len: &mut usize) {
    let mut start = 0;
    while start < *len && (buf[start] == b' ' || buf[start] == b'\t') { start += 1; }
    let mut is_command_name = true;
    for i in start..*len {
        if buf[i] == b' ' || buf[i] == b'\t' { is_command_name = false; break; }
    }
    let mut matches: [MatchCandidate; 18] = [MatchCandidate::default(); 18];
    let mut match_count = 0;
    let replace_start: usize;
    if is_command_name {
        replace_start = start;
        let prefix = match core::str::from_utf8(&buf[start..*len]) { Ok(s) => s, Err(_) => return };
        for &cmd in KNOWN_COMMANDS.iter() {
            if let Some(sc) = fuzzy_score(prefix, cmd) {
                if match_count < 18 {
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
        let arg_prefix = match core::str::from_utf8(&buf[last_space..*len]) { Ok(s) => s, Err(_) => return };
        let fd = open(b".\0".as_ptr(), O_RDONLY | O_DIRECTORY, 0);
        if fd >= 0 {
            let mut dir_buf = [0u8; 4096];
            let n = syscall::syscall3(SYS_GETDENTS64, fd as usize, dir_buf.as_mut_ptr() as usize, dir_buf.len()) as isize;
            close(fd);
            if n > 0 {
                let mut offset = 0;
                while offset < n as usize && match_count < 18 {
                    let dirent = &*(dir_buf.as_ptr().add(offset) as *const Dirent64);
                    let mut name_len = 0;
                    while name_len < dirent.d_name.len() && dirent.d_name[name_len] != 0 { name_len += 1; }
                    if name_len > 0 {
                        let name_bytes = &dirent.d_name[..name_len];
                        if let Ok(name_str) = core::str::from_utf8(name_bytes) {
                            if !name_str.starts_with('.') || arg_prefix.starts_with('.') {
                                if let Some(sc) = fuzzy_score(arg_prefix, name_str) {
                                    let item_len = name_len.min(63);
                                    matches[match_count].name[..item_len].copy_from_slice(&name_bytes[..item_len]);
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
    if match_count == 0 { return; }
    for i in 0..match_count {
        for j in (i + 1)..match_count {
            if matches[j].score > matches[i].score {
                let temp = matches[i];
                matches[i] = matches[j];
                matches[j] = temp;
            }
        }
    }
    if match_count == 1 {
        let item = &matches[0];
        let extra_char = if is_command_name || !item.is_dir { b' ' } else { b'/' };
        let new_len = replace_start + item.len + 1;
        if new_len < buf.len() {
            buf[replace_start..replace_start + item.len].copy_from_slice(&item.name[..item.len]);
            buf[replace_start + item.len] = extra_char;
            *len = new_len;
        }
        repaint_prompt_line(cwd, buf, *len);
        return;
    }
    let mut selected_idx = 0;
    render_menu_view(cwd, buf, *len, &matches, match_count, selected_idx);
    loop {
        let ch = getchar();
        if ch < 0 { usleep(1000); continue; }
        if ch == b'\n' as i32 || ch == b'\r' as i32 {
            let item = &matches[selected_idx];
            let extra_char = if is_command_name || !item.is_dir { b' ' } else { b'/' };
            let new_len = replace_start + item.len + 1;
            if new_len < buf.len() {
                buf[replace_start..replace_start + item.len].copy_from_slice(&item.name[..item.len]);
                buf[replace_start + item.len] = extra_char;
                *len = new_len;
            }
            clear_menu_line(cwd, buf, *len);
            break;
        }
        if ch == 0x09 {
            selected_idx = (selected_idx + 1) % match_count;
            render_menu_view(cwd, buf, *len, &matches, match_count, selected_idx);
            continue;
        }
        if ch == 0x1B {
            let c1 = getchar();
            if c1 == b'[' as i32 {
                let c2 = getchar();
                if c2 == b'D' as i32 || c2 == b'A' as i32 {
                    selected_idx = (selected_idx + match_count - 1) % match_count;
                    render_menu_view(cwd, buf, *len, &matches, match_count, selected_idx);
                    continue;
                } else if c2 == b'C' as i32 || c2 == b'B' as i32 {
                    selected_idx = (selected_idx + 1) % match_count;
                    render_menu_view(cwd, buf, *len, &matches, match_count, selected_idx);
                    continue;
                }
            }
            clear_menu_line(cwd, buf, *len);
            break;
        }
        if ch == 0x08 || ch == 0x7F {
            clear_menu_line(cwd, buf, *len);
            if *len > replace_start {
                *len -= 1;
                repaint_prompt_line(cwd, buf, *len);
            }
            break;
        }
        if (ch as u8) >= 0x20 {
            clear_menu_line(cwd, buf, *len);
            if *len < buf.len() - 1 {
                buf[*len] = ch as u8;
                *len += 1;
                repaint_prompt_line(cwd, buf, *len);
            }
            break;
        }
    }
}

unsafe fn read_line(cwd: *const u8, buf: &mut [u8]) -> usize {
    let mut idx = 0;
    repaint_prompt_line(cwd, buf, 0);
    let mut history_cursor = HISTORY_COUNT;
    let mut draft_buf = [0u8; 256];
    let mut draft_len = 0;
    let mut in_history = false;
    loop {
        let ch = getchar();
        if ch < 0 { usleep(1000); continue; }
        if ch == b'\n' as i32 || ch == b'\r' as i32 { break; }
        if ch == 0x08 || ch == 0x7F {
            if idx > 0 {
                idx -= 1;
                repaint_prompt_line(cwd, buf, idx);
            }
            continue;
        }
        if ch == 0x09 {
            handle_tab_completion(cwd, buf, &mut idx);
            continue;
        }
        if ch == 0x1B {
            let next = getchar();
            if next == b'[' as i32 {
                let code = getchar();
                if code == b'A' as i32 {
                    if HISTORY_COUNT > 0 {
                        if !in_history {
                            draft_buf[..idx].copy_from_slice(&buf[..idx]);
                            draft_len = idx;
                            in_history = true;
                        }
                        let earliest = if HISTORY_COUNT > MAX_HISTORY { HISTORY_COUNT - MAX_HISTORY } else { 0 };
                        if history_cursor > earliest {
                            history_cursor -= 1;
                            if let Some((h_cmd, h_len)) = history_get(history_cursor) {
                                let copy_len = h_len.min(buf.len() - 1);
                                buf[..copy_len].copy_from_slice(&h_cmd[..copy_len]);
                                idx = copy_len;
                                repaint_prompt_line(cwd, buf, idx);
                            }
                        }
                    }
                } else if code == b'B' as i32 {
                    if in_history {
                        if history_cursor + 1 < HISTORY_COUNT {
                            history_cursor += 1;
                            if let Some((h_cmd, h_len)) = history_get(history_cursor) {
                                let copy_len = h_len.min(buf.len() - 1);
                                buf[..copy_len].copy_from_slice(&h_cmd[..copy_len]);
                                idx = copy_len;
                                repaint_prompt_line(cwd, buf, idx);
                            }
                        } else if history_cursor + 1 == HISTORY_COUNT {
                            history_cursor = HISTORY_COUNT;
                            let copy_len = draft_len.min(buf.len() - 1);
                            buf[..copy_len].copy_from_slice(&draft_buf[..copy_len]);
                            idx = copy_len;
                            in_history = false;
                            repaint_prompt_line(cwd, buf, idx);
                        }
                    }
                } else {
                    loop {
                        let seq = getchar();
                        if seq < 0 || (seq >= 0x40 && seq <= 0x7E) || seq == b'\n' as i32 || seq == b'\r' as i32 {
                            break;
                        }
                    }
                }
            }
            continue;
        }
        if (ch as u8) < 0x20 { continue; }
        in_history = false;
        if idx < buf.len() - 1 {
            buf[idx] = ch as u8;
            idx += 1;
            repaint_prompt_line(cwd, buf, idx);
        }
    }
    putchar(b'\n' as i32);
    buf[idx] = 0;
    if idx > 0 { history_add(&buf[..idx], idx); }
    idx
}

unsafe fn tokenize_line(line: *mut u8, argv: &mut [*const u8; 16]) -> usize {
    let mut argc = 0;
    let mut ptr = line;
    for slot in argv.iter_mut() { *slot = core::ptr::null(); }
    while *ptr != 0 && argc < 16 {
        while *ptr != 0 && (*ptr == b' ' || *ptr == b'\t' || *ptr == b'\r' || *ptr == b'\n') {
            *ptr = 0;
            ptr = ptr.add(1);
        }
        if *ptr == 0 { break; }
        argv[argc] = ptr;
        argc += 1;
        while *ptr != 0 && *ptr != b' ' && *ptr != b'\t' && *ptr != b'\r' && *ptr != b'\n' {
            ptr = ptr.add(1);
        }
        if *ptr != 0 {
            *ptr = 0;
            ptr = ptr.add(1);
        }
    }
    argc
}

#[derive(Default)]
struct Redirection {
    stdin_file: *const u8,
    stdout_file: *const u8,
    stdout_append: bool,
}

struct Stage {
    argv: [*const u8; 16],
    argc: usize,
    redir: Redirection,
}

unsafe fn parse_stage(stage_str: *mut u8) -> Stage {
    let mut raw_argv: [*const u8; 16] = [core::ptr::null(); 16];
    let raw_argc = tokenize_line(stage_str, &mut raw_argv);
    let mut stage = Stage { argv: [core::ptr::null(); 16], argc: 0, redir: Redirection::default() };
    let mut i = 0;
    while i < raw_argc {
        let token = raw_argv[i];
        let b0 = *token;
        let b1 = if b0 != 0 { *token.add(1) } else { 0 };
        if b0 == b'>' && b1 == b'>' {
            if *token.add(2) != 0 { stage.redir.stdout_file = token.add(2); }
            else if i + 1 < raw_argc { stage.redir.stdout_file = raw_argv[i + 1]; i += 1; }
            stage.redir.stdout_append = true;
        } else if b0 == b'>' {
            if *token.add(1) != 0 { stage.redir.stdout_file = token.add(1); }
            else if i + 1 < raw_argc { stage.redir.stdout_file = raw_argv[i + 1]; i += 1; }
            stage.redir.stdout_append = false;
        } else if b0 == b'<' {
            if *token.add(1) != 0 { stage.redir.stdin_file = token.add(1); }
            else if i + 1 < raw_argc { stage.redir.stdin_file = raw_argv[i + 1]; i += 1; }
        } else if stage.argc < 16 {
            stage.argv[stage.argc] = token;
            stage.argc += 1;
        }
        i += 1;
    }
    stage
}

unsafe fn execute_pipeline_line(line: *mut u8) {
    let mut stages_str: [*mut u8; 8] = [core::ptr::null_mut(); 8];
    let mut num_stages = 0;
    let mut ptr = line;
    while *ptr != 0 && num_stages < 8 {
        while *ptr == b' ' || *ptr == b'\t' { ptr = ptr.add(1); }
        if *ptr == 0 { break; }
        stages_str[num_stages] = ptr;
        num_stages += 1;
        while *ptr != 0 && *ptr != b'|' { ptr = ptr.add(1); }
        if *ptr == b'|' { *ptr = 0; ptr = ptr.add(1); }
    }
    if num_stages == 0 { return; }
    if num_stages == 1 {
        let stage = parse_stage(stages_str[0]);
        if stage.argc == 0 { return; }
        let saved_in = dup(STDIN_FILENO);
        let saved_out = dup(STDOUT_FILENO);
        if !stage.redir.stdin_file.is_null() {
            let in_fd = open(stage.redir.stdin_file, O_RDONLY, 0);
            if in_fd >= 0 { dup2(in_fd, STDIN_FILENO); close(in_fd); }
            else { print_error(b"open\0".as_ptr(), stage.redir.stdin_file, in_fd); close(saved_in); close(saved_out); return; }
        }
        if !stage.redir.stdout_file.is_null() {
            let flags = O_WRONLY | O_CREAT | if stage.redir.stdout_append { O_APPEND } else { O_TRUNC };
            let out_fd = open(stage.redir.stdout_file, flags, 0o644);
            if out_fd >= 0 { dup2(out_fd, STDOUT_FILENO); close(out_fd); }
            else { print_error(b"open\0".as_ptr(), stage.redir.stdout_file, out_fd); close(saved_in); close(saved_out); return; }
        }
        execute_command(stage.argc, &stage.argv);
        dup2(saved_in, STDIN_FILENO); close(saved_in);
        dup2(saved_out, STDOUT_FILENO); close(saved_out);
    } else {
        let mut pipes = [[0i32; 2]; 7];
        for i in 0..num_stages - 1 { pipe(&mut pipes[i]); }
        for i in 0..num_stages {
            let stage = parse_stage(stages_str[i]);
            if stage.argc == 0 { continue; }
            let saved_in = dup(STDIN_FILENO);
            let saved_out = dup(STDOUT_FILENO);
            if i == 0 && !stage.redir.stdin_file.is_null() {
                let in_fd = open(stage.redir.stdin_file, O_RDONLY, 0);
                if in_fd >= 0 { dup2(in_fd, STDIN_FILENO); close(in_fd); }
            } else if i > 0 {
                dup2(pipes[i - 1][0], STDIN_FILENO);
            }
            if i == num_stages - 1 && !stage.redir.stdout_file.is_null() {
                let flags = O_WRONLY | O_CREAT | if stage.redir.stdout_append { O_APPEND } else { O_TRUNC };
                let out_fd = open(stage.redir.stdout_file, flags, 0o644);
                if out_fd >= 0 { dup2(out_fd, STDOUT_FILENO); close(out_fd); }
            } else if i < num_stages - 1 {
                dup2(pipes[i][1], STDOUT_FILENO);
            }
            execute_command(stage.argc, &stage.argv);
            if i < num_stages - 1 { close(pipes[i][1]); }
            if i > 0 { close(pipes[i - 1][0]); }
            dup2(saved_in, STDIN_FILENO); close(saved_in);
            dup2(saved_out, STDOUT_FILENO); close(saved_out);
        }
    }
}

unsafe fn print_error(action: *const u8, target: *const u8, err: i32) {
    let err_code = if err < 0 { -err } else { err };
    let msg = match err_code {
        ENOENT => b"No such file or directory\0".as_ptr(),
        EEXIST => b"File or directory already exists\0".as_ptr(),
        ENOTDIR => b"Not a directory\0".as_ptr(),
        EISDIR => b"Is a directory\0".as_ptr(),
        EACCES => b"Permission denied\0".as_ptr(),
        EBADF => b"Bad file descriptor\0".as_ptr(),
        EINVAL => b"Invalid argument\0".as_ptr(),
        ENOMEM => b"Cannot allocate memory\0".as_ptr(),
        ENOSYS => b"Function not implemented\0".as_ptr(),
        _ => b"Operation failed\0".as_ptr(),
    };
    printf(b"%s: '%s': %s (errno: %d)\n\0".as_ptr(), action, target, msg, err_code);
}

unsafe fn execute_command(argc: usize, argv: &[*const u8; 16]) {
    let cmd = argv[0];
    if strcmp(cmd, b"help\0".as_ptr()) == 0 {
        puts(b"Available POSIX Shell Commands:\n  help, uname, pwd, cd, ls, cat, touch, mkdir, rm,\n  ps, top, monitor, journal, snapshot, echo, async-demo, clear, exit\n\nPipeline: cmd1 | cmd2    Redirect: >, >>, <\0".as_ptr());
    } else if strcmp(cmd, b"uname\0".as_ptr()) == 0 {
        let mut uts = Utsname::default();
        syscall::syscall1(SYS_UNAME, &mut uts as *mut _ as usize);
        printf(b"%s %s %s %s\n\0".as_ptr(), uts.sysname.as_ptr(), uts.release.as_ptr(), uts.version.as_ptr(), uts.machine.as_ptr());
    } else if strcmp(cmd, b"pwd\0".as_ptr()) == 0 {
        let mut buf = [0u8; 128];
        getcwd(buf.as_mut_ptr(), buf.len());
        puts(buf.as_ptr());
    } else if strcmp(cmd, b"ps\0".as_ptr()) == 0 {
        display_file(b"/proc/processes\0".as_ptr(), false);
    } else if strcmp(cmd, b"top\0".as_ptr()) == 0 || strcmp(cmd, b"monitor\0".as_ptr()) == 0 {
        display_system_monitor();
    } else if strcmp(cmd, b"journal\0".as_ptr()) == 0 {
        display_file(b"/proc/audit_journal\0".as_ptr(), false);
    } else if strcmp(cmd, b"snapshot\0".as_ptr()) == 0 {
        handle_snapshot_command(argc, argv);
    } else if strcmp(cmd, b"cd\0".as_ptr()) == 0 {
        handle_cd(argc, argv);
    } else if strcmp(cmd, b"ls\0".as_ptr()) == 0 {
        handle_ls(argc, argv);
    } else if strcmp(cmd, b"cat\0".as_ptr()) == 0 {
        handle_cat(argc, argv);
    } else if strcmp(cmd, b"touch\0".as_ptr()) == 0 {
        handle_touch(argc, argv);
    } else if strcmp(cmd, b"mkdir\0".as_ptr()) == 0 {
        handle_mkdir(argc, argv);
    } else if strcmp(cmd, b"rm\0".as_ptr()) == 0 {
        handle_rm(argc, argv);
    } else if strcmp(cmd, b"echo\0".as_ptr()) == 0 {
        handle_echo(argc, argv);
    } else if strcmp(cmd, b"async-demo\0".as_ptr()) == 0 {
        run_async_demo();
    } else if strcmp(cmd, b"clear\0".as_ptr()) == 0 {
        puts(b"\x1b[2J\x1b[H\0".as_ptr());
    } else if strcmp(cmd, b"exit\0".as_ptr()) == 0 {
        puts(b"Exiting shell...\0".as_ptr());
        exit(0);
    } else {
        printf(b"shell: %s: command not found\n\0".as_ptr(), cmd);
    }
}

unsafe fn handle_cd(argc: usize, argv: &[*const u8; 16]) {
    let mut current_cwd = [0u8; 128];
    getcwd(current_cwd.as_mut_ptr(), current_cwd.len());
    let is_dash = argc > 1 && !argv[1].is_null() && strcmp(argv[1], b"-\0".as_ptr()) == 0;
    let target = if argc == 1 || argv[1].is_null() || strcmp(argv[1], b"~\0".as_ptr()) == 0 {
        b"/\0".as_ptr()
    } else if is_dash {
        if !HAS_OLDPWD { puts(b"cd: OLDPWD not set\0".as_ptr()); return; }
        core::ptr::addr_of!(OLDPWD_BUF) as *const u8
    } else {
        argv[1]
    };
    let res = chdir(target);
    if res < 0 {
        print_error(b"cd\0".as_ptr(), target, res);
    } else {
        let cur_len = strlen(current_cwd.as_ptr());
        if cur_len < 127 {
            let oldpwd_ptr = core::ptr::addr_of_mut!(OLDPWD_BUF) as *mut u8;
            core::ptr::copy_nonoverlapping(current_cwd.as_ptr(), oldpwd_ptr, cur_len);
            *oldpwd_ptr.add(cur_len) = 0;
            HAS_OLDPWD = true;
        }
        if is_dash {
            let mut new_cwd = [0u8; 128];
            getcwd(new_cwd.as_mut_ptr(), new_cwd.len());
            puts(new_cwd.as_ptr());
        }
    }
}

unsafe fn handle_ls(argc: usize, argv: &[*const u8; 16]) {
    let mut show_all = false;
    let mut long_format = false;
    let mut human_readable = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if *arg == b'-' && *arg.add(1) != 0 {
            let mut ptr = arg.add(1);
            while *ptr != 0 {
                match *ptr {
                    b'a' => show_all = true,
                    b'l' => long_format = true,
                    b'h' => human_readable = true,
                    _ => {}
                }
                ptr = ptr.add(1);
            }
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 { paths[0] = b".\0".as_ptr(); path_count = 1; }
    for p in 0..path_count {
        if path_count > 1 { printf(b"%s:\n\0".as_ptr(), paths[p]); }
        list_directory_advanced(paths[p], show_all, long_format, human_readable);
        if path_count > 1 && p + 1 < path_count { putchar(b'\n' as i32); }
    }
}

unsafe fn list_directory_advanced(path: *const u8, show_all: bool, long_format: bool, human: bool) {
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        let mut st = Stat::default();
        if stat(path, &mut st) == 0 {
            if long_format {
                printf(b"%c%s  %6d  %s\n\0".as_ptr(), if (st.st_mode & S_IFDIR) != 0 { b'd' } else { b'-' } as i32, b"rw-r--r--\0".as_ptr(), st.st_size as i32, path);
            } else {
                printf(b"  %s\n\0".as_ptr(), path);
            }
            return;
        }
        print_error(b"ls\0".as_ptr(), path, fd);
        return;
    }
    let mut buf = [0u8; 4096];
    loop {
        let n = syscall::syscall3(SYS_GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize;
        if n <= 0 { break; }
        let mut offset = 0;
        while offset < n as usize {
            let dirent = &*(buf.as_ptr().add(offset) as *const Dirent64);
            let name_ptr = dirent.d_name.as_ptr();
            if !show_all && *name_ptr == b'.' {
                offset += core::mem::size_of::<Dirent64>();
                continue;
            }
            let suffix = if dirent.d_type == DT_DIR { b"/\0".as_ptr() } else { b"\0".as_ptr() };
            if long_format {
                let mut fullpath = [0u8; 256];
                let path_len = strlen(path);
                let name_len = strlen(name_ptr);
                let need_slash = if path_len > 0 && *path.add(path_len - 1) != b'/' { 1 } else { 0 };
                if path_len + need_slash + name_len < 255 {
                    core::ptr::copy_nonoverlapping(path, fullpath.as_mut_ptr(), path_len);
                    if need_slash == 1 { fullpath[path_len] = b'/'; }
                    core::ptr::copy_nonoverlapping(name_ptr, fullpath.as_mut_ptr().add(path_len + need_slash), name_len);
                    fullpath[path_len + need_slash + name_len] = 0;
                    let mut st = Stat::default();
                    if stat(fullpath.as_ptr(), &mut st) == 0 {
                        let type_char = if dirent.d_type == DT_DIR { b'd' } else { b'-' };
                        let mode_str = if dirent.d_type == DT_DIR { b"rwxr-xr-x\0".as_ptr() } else { b"rw-r--r--\0".as_ptr() };
                        if human && st.st_size >= 1024 {
                            printf(b"%c%s  %4dK  %s%s\n\0".as_ptr(), type_char as i32, mode_str, ((st.st_size + 1023) / 1024) as i32, name_ptr, suffix);
                        } else {
                            printf(b"%c%s  %6d  %s%s\n\0".as_ptr(), type_char as i32, mode_str, st.st_size as i32, name_ptr, suffix);
                        }
                    } else {
                        printf(b"  %s%s\n\0".as_ptr(), name_ptr, suffix);
                    }
                }
            } else {
                printf(b"  %s%s\n\0".as_ptr(), name_ptr, suffix);
            }
            offset += core::mem::size_of::<Dirent64>();
        }
    }
    close(fd);
}

unsafe fn handle_touch(argc: usize, argv: &[*const u8; 16]) {
    let mut no_create = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if strcmp(arg, b"-c\0".as_ptr()) == 0 || strcmp(arg, b"--no-create\0".as_ptr()) == 0 {
            no_create = true;
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 { puts(b"touch: missing file operand\0".as_ptr()); return; }
    for p in 0..path_count {
        let path = paths[p];
        if no_create {
            let mut st = Stat::default();
            if stat(path, &mut st) == 0 {
                let fd = open(path, O_RDWR, 0);
                if fd >= 0 { close(fd); }
            }
        } else {
            let fd = open(path, O_RDWR | O_CREAT, 0o644);
            if fd >= 0 { close(fd); } else { print_error(b"touch: cannot create file\0".as_ptr(), path, fd); }
        }
    }
}

unsafe fn handle_mkdir(argc: usize, argv: &[*const u8; 16]) {
    let mut create_parents = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if strcmp(arg, b"-p\0".as_ptr()) == 0 || strcmp(arg, b"--parents\0".as_ptr()) == 0 {
            create_parents = true;
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 { puts(b"mkdir: missing operand\0".as_ptr()); return; }
    for p in 0..path_count {
        if create_parents { mkdir_p(paths[p]); }
        else {
            let res = mkdir(paths[p], 0o755);
            if res < 0 { print_error(b"mkdir: cannot create directory\0".as_ptr(), paths[p], res); }
        }
    }
}

unsafe fn mkdir_p(path: *const u8) {
    let len = strlen(path);
    let mut subpath = [0u8; 256];
    if len >= 255 { return; }
    for i in 0..len {
        let b = *path.add(i);
        subpath[i] = b;
        if (b == b'/' && i > 0) || i == len - 1 {
            subpath[i + 1] = 0;
            let res = mkdir(subpath.as_ptr(), 0o755);
            if res < 0 && res != -EEXIST {
                let mut st = Stat::default();
                if stat(subpath.as_ptr(), &mut st) != 0 || (st.st_mode & S_IFDIR == 0) {
                    print_error(b"mkdir -p\0".as_ptr(), subpath.as_ptr(), res);
                    return;
                }
            }
        }
    }
}

unsafe fn handle_rm(argc: usize, argv: &[*const u8; 16]) {
    let mut recursive = false;
    let mut force = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if *arg == b'-' && *arg.add(1) != 0 {
            let mut ptr = arg.add(1);
            while *ptr != 0 {
                match *ptr { b'r' | b'R' => recursive = true, b'f' => force = true, _ => {} }
                ptr = ptr.add(1);
            }
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 {
        if !force { puts(b"rm: missing operand\0".as_ptr()); }
        return;
    }
    for p in 0..path_count { remove_path(paths[p], recursive, force); }
}

unsafe fn remove_path(path: *const u8, recursive: bool, force: bool) {
    if recursive {
        let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
        if fd >= 0 {
            let mut buf = [0u8; 4096];
            loop {
                let n = syscall::syscall3(SYS_GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize;
                if n <= 0 { break; }
                let mut offset = 0;
                while offset < n as usize {
                    let dirent = &*(buf.as_ptr().add(offset) as *const Dirent64);
                    let name_ptr = dirent.d_name.as_ptr();
                    if strcmp(name_ptr, b".\0".as_ptr()) != 0 && strcmp(name_ptr, b"..\0".as_ptr()) != 0 {
                        let mut subpath = [0u8; 256];
                        let path_len = strlen(path);
                        let name_len = strlen(name_ptr);
                        let need_slash = if path_len > 0 && *path.add(path_len - 1) != b'/' { 1 } else { 0 };
                        if path_len + need_slash + name_len < 255 {
                            core::ptr::copy_nonoverlapping(path, subpath.as_mut_ptr(), path_len);
                            if need_slash == 1 { subpath[path_len] = b'/'; }
                            core::ptr::copy_nonoverlapping(name_ptr, subpath.as_mut_ptr().add(path_len + need_slash), name_len);
                            subpath[path_len + need_slash + name_len] = 0;
                            remove_path(subpath.as_ptr(), true, force);
                        }
                    }
                    offset += core::mem::size_of::<Dirent64>();
                }
            }
            close(fd);
        }
    }
    let res = unlink(path);
    if res < 0 && !force { print_error(b"rm\0".as_ptr(), path, res); }
}

unsafe fn handle_cat(argc: usize, argv: &[*const u8; 16]) {
    let mut number_lines = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if strcmp(arg, b"-n\0".as_ptr()) == 0 || strcmp(arg, b"--number\0".as_ptr()) == 0 {
            number_lines = true;
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 { display_stream(STDIN_FILENO, number_lines); }
    else { for p in 0..path_count { display_file(paths[p], number_lines); } }
}

unsafe fn display_stream(fd: i32, number_lines: bool) {
    let mut buf = [0u8; 512];
    let mut line_num = 1;
    let mut at_line_start = true;
    loop {
        let n = read(fd, buf.as_mut_ptr(), buf.len());
        if n <= 0 { break; }
        if number_lines {
            for i in 0..n as usize {
                let b = buf[i];
                if at_line_start { printf(b"     %d  \0".as_ptr(), line_num); line_num += 1; at_line_start = false; }
                putchar(b as i32);
                if b == b'\n' { at_line_start = true; }
            }
        } else {
            write(STDOUT_FILENO, buf.as_ptr(), n as usize);
        }
    }
}

unsafe fn display_file(path: *const u8, number_lines: bool) {
    let fd = open(path, O_RDONLY, 0);
    if fd < 0 { print_error(b"cat\0".as_ptr(), path, fd); return; }
    display_stream(fd, number_lines);
    close(fd);
}

unsafe fn handle_echo(argc: usize, argv: &[*const u8; 16]) {
    let mut no_newline = false;
    let mut interpret_escapes = false;
    let mut arg_start = 1;
    while arg_start < argc && !argv[arg_start].is_null() {
        let arg = argv[arg_start];
        if *arg == b'-' && *arg.add(1) != 0 {
            let mut ptr = arg.add(1);
            let mut is_flag = true;
            let mut cur_n = false;
            let mut cur_e = false;
            while *ptr != 0 {
                match *ptr { b'n' => cur_n = true, b'e' => cur_e = true, _ => { is_flag = false; break; } }
                ptr = ptr.add(1);
            }
            if is_flag && (cur_n || cur_e) {
                if cur_n { no_newline = true; }
                if cur_e { interpret_escapes = true; }
                arg_start += 1;
                continue;
            }
        }
        break;
    }
    for i in arg_start..argc {
        if !argv[i].is_null() {
            let text = argv[i];
            if interpret_escapes {
                let mut ptr = text;
                while *ptr != 0 {
                    if *ptr == b'\\' && *ptr.add(1) != 0 {
                        ptr = ptr.add(1);
                        match *ptr {
                            b'n' => { putchar(b'\n' as i32); }
                            b't' => { putchar(b'\t' as i32); }
                            b'r' => { putchar(b'\r' as i32); }
                            b'\\' => { putchar(b'\\' as i32); }
                            b'0' => { putchar(0); }
                            other => { putchar(b'\\' as i32); putchar(other as i32); }
                        }
                    } else {
                        putchar(*ptr as i32);
                    }
                    ptr = ptr.add(1);
                }
            } else {
                printf(b"%s\0".as_ptr(), text);
            }
            if i + 1 < argc { putchar(b' ' as i32); }
        }
    }
    if !no_newline { putchar(b'\n' as i32); }
}

unsafe fn handle_snapshot_command(argc: usize, argv: &[*const u8; 16]) {
    if argc == 1 || (argc > 1 && strcmp(argv[1], b"list\0".as_ptr()) == 0) {
        display_file(b"/proc/snapshots\0".as_ptr(), false);
    } else if argc >= 2 && strcmp(argv[1], b"create\0".as_ptr()) == 0 {
        let label = if argc > 2 && !argv[2].is_null() { argv[2] } else { b"manual_checkpoint\0".as_ptr() };
        let id = audit_snapshot(label, 0);
        if id > 0 { printf(b"[snapshot] Successfully created audit snapshot #%d ('%s')\n\0".as_ptr(), id as i32, label); }
        else { printf(b"[snapshot] Failed to create audit snapshot (error %d)\n\0".as_ptr(), id as i32); }
    } else {
        puts(b"Usage: snapshot [list] | snapshot create <label>\0".as_ptr());
    }
}

unsafe fn display_system_monitor() {
    let mut si = Sysinfo::default();
    if sysinfo(&mut si as *mut _) < 0 { puts(b"top: failed to retrieve system metrics\0".as_ptr()); return; }
    let total_mb = si.totalram / (1024 * 1024);
    let free_mb = si.freeram / (1024 * 1024);
    let used_mb = total_mb.saturating_sub(free_mb);
    let mem_pct = if total_mb > 0 { (used_mb * 100) / total_mb } else { 0 };
    printf(b"Uptime: %d ticks  Tasks: %d  RAM: %d%% %d/%d MB\n\0".as_ptr(), si.uptime as i32, si.procs as i32, mem_pct as i32, used_mb as i32, total_mb as i32);
    display_file(b"/proc/processes\0".as_ptr(), false);
}

unsafe fn run_async_demo() {
    puts(b"=== POSIX Epoll Demo ===".as_ptr());
    let epfd = epoll_create1(0);
    if epfd < 0 { print_error(b"epoll_create1\0".as_ptr(), b"epoll\0".as_ptr(), epfd); return; }
    let mut pipe1 = [0i32; 2];
    let mut pipe2 = [0i32; 2];
    pipe(&mut pipe1);
    pipe(&mut pipe2);
    let mut ev1 = EpollEvent { events: EPOLLIN, data: 1001 };
    let mut ev2 = EpollEvent { events: EPOLLIN, data: 1002 };
    epoll_ctl(epfd, EPOLL_CTL_ADD, pipe1[0], &mut ev1);
    epoll_ctl(epfd, EPOLL_CTL_ADD, pipe2[0], &mut ev2);
    let msg1 = b"Async payload from Channel 1\n\0";
    let msg2 = b"Async payload from Channel 2\n\0";
    write(pipe1[1], msg1.as_ptr(), msg1.len() - 1);
    write(pipe2[1], msg2.as_ptr(), msg2.len() - 1);
    let mut events = [EpollEvent::default(); 4];
    let nready = epoll_wait(epfd, events.as_mut_ptr(), events.len() as i32, 0);
    printf(b"epoll_wait returned %d event(s)\n\0".as_ptr(), nready);
    for i in 0..nready as usize {
        let ev = &events[i];
        let mut read_buf = [0u8; 128];
        let fd = if ev.data == 1001 { pipe1[0] } else { pipe2[0] };
        let nread = read(fd, read_buf.as_mut_ptr(), read_buf.len() - 1);
        if nread > 0 {
            read_buf[nread as usize] = 0;
            printf(b"  tag %d: %s\0".as_ptr(), ev.data as i32, read_buf.as_ptr());
        }
    }
    close(pipe1[0]); close(pipe1[1]); close(pipe2[0]); close(pipe2[1]); close(epfd);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe { exit(1) };
}
