//! Built-in commands and utility routines for the interactive POSIX shell.

use libc::*;
use posix_abi::*;

/// Buffer holding the previous working directory for `cd -`.
pub static mut OLDPWD_BUF: [u8; 128] = [0u8; 128];
/// Indicates whether `OLDPWD_BUF` contains a valid previous working directory.
pub static mut HAS_OLDPWD: bool = false;

/// Formats and outputs an errno error message to standard output.
///
/// # Safety
///
/// `action` and `target` must point to valid null-terminated C-strings.
pub unsafe fn print_error(action: *const u8, target: *const u8, err: i32) {
    let err_code = if err < 0 { -err } else { err };
    let msg = match err_code {
        ENOENT => b"No such file or directory\0".as_ptr(),
        EACCES => b"Permission denied\0".as_ptr(),
        EEXIST => b"File or directory exists\0".as_ptr(),
        ENOTDIR => b"Not a directory\0".as_ptr(),
        EISDIR => b"Is a directory\0".as_ptr(),
        EBADF => b"Bad file descriptor\0".as_ptr(),
        EINVAL => b"Invalid argument\0".as_ptr(),
        ENOMEM => b"Cannot allocate memory\0".as_ptr(),
        ENOSYS => b"Function not implemented\0".as_ptr(),
        _ => b"Operation failed\0".as_ptr(),
    };
    // SAFETY: Formats and prints errno diagnostic message to stdout.
    unsafe {
        printf(
            b"%s: '%s': %s (errno: %d)\n\0".as_ptr(),
            action,
            target,
            msg,
            err_code,
        );
    }
}

/// Returns a pointer to the final filename component of a path.
///
/// # Safety
///
/// `path` must be a valid pointer to a null-terminated C-string or null.
pub unsafe fn get_basename(path: *const u8) -> *const u8 {
    if path.is_null() {
        return b"\0".as_ptr();
    }
    let mut last = path;
    let mut ptr = path;
    // SAFETY: Traverses null-terminated C string to locate last '/' component.
    unsafe {
        while *ptr != 0 {
            if *ptr == b'/' && *ptr.add(1) != 0 {
                last = ptr.add(1);
            }
            ptr = ptr.add(1);
        }
    }
    last
}

/// Combines a parent directory and entry name into `out` buffer.
///
/// # Safety
///
/// `dir` and `name` must be valid null-terminated strings. `out` must have at least 256 bytes.
pub unsafe fn join_path(dir: *const u8, name: *const u8, out: &mut [u8; 256]) -> *const u8 {
    // SAFETY: Copies dir, optional slash, and name into out buffer safely within 256 bytes.
    unsafe {
        let dlen = strlen(dir);
        let nlen = strlen(name);
        let slash = if dlen > 0 && *dir.add(dlen - 1) != b'/' { 1 } else { 0 };
        if dlen + slash + nlen < 255 {
            core::ptr::copy_nonoverlapping(dir, out.as_mut_ptr(), dlen);
            if slash == 1 {
                out[dlen] = b'/';
            }
            core::ptr::copy_nonoverlapping(name, out.as_mut_ptr().add(dlen + slash), nlen);
            out[dlen + slash + nlen] = 0;
            out.as_ptr()
        } else {
            b"\0".as_ptr()
        }
    }
}

/// Iterates over directory entries in `dir` (skipping `.` and `..`) and invokes `cb` on each entry name.
///
/// # Safety
///
/// `dir` must be a valid null-terminated directory path.
pub unsafe fn walk_dir<F: FnMut(*const u8)>(dir: *const u8, mut cb: F) {
    // SAFETY: Opens directory and reads entries via SYS_GETDENTS64.
    unsafe {
        let fd = open(dir, O_RDONLY | O_DIRECTORY, 0);
        if fd < 0 {
            return;
        }
        let mut buf = [0u8; 4096];
        loop {
            let n = syscall::syscall3(SYS_GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize;
            if n <= 0 {
                break;
            }
            let mut offset = 0;
            while offset < n as usize {
                let dirent = &*(buf.as_ptr().add(offset) as *const Dirent64);
                let name = dirent.d_name.as_ptr();
                if strcmp(name, b".\0".as_ptr()) != 0 && strcmp(name, b"..\0".as_ptr()) != 0 {
                    cb(name);
                }
                offset += core::mem::size_of::<Dirent64>();
            }
        }
        close(fd);
    }
}

/// Changes the current working directory, supporting `~`, `-`, and relative/absolute paths.
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated strings or null. Mutates `OLDPWD_BUF`.
pub unsafe fn handle_cd(argc: usize, argv: &[*const u8; 16]) {
    let mut current_cwd = [0u8; 128];
    // SAFETY: Gets current working directory into buffer.
    unsafe { getcwd(current_cwd.as_mut_ptr(), current_cwd.len()) };
    let is_dash = argc > 1 && !argv[1].is_null() && unsafe { strcmp(argv[1], b"-\0".as_ptr()) } == 0;
    let target = if argc == 1 || argv[1].is_null() || unsafe { strcmp(argv[1], b"~\0".as_ptr()) } == 0 {
        b"/\0".as_ptr()
    } else if is_dash {
        if !unsafe { HAS_OLDPWD } {
            // SAFETY: Prints error message to stdout.
            unsafe { puts(b"cd: OLDPWD not set\0".as_ptr()) };
            return;
        }
        core::ptr::addr_of!(OLDPWD_BUF) as *const u8
    } else {
        argv[1]
    };
    // SAFETY: Invokes chdir syscall with target directory path.
    let res = unsafe { chdir(target) };
    if res < 0 {
        // SAFETY: Prints error message to stdout.
        unsafe { print_error(b"cd\0".as_ptr(), target, res) };
    } else {
        // SAFETY: Updates OLDPWD_BUF static buffer.
        unsafe {
            let cur_len = strlen(current_cwd.as_ptr()).min(127);
            core::ptr::copy_nonoverlapping(current_cwd.as_ptr(), core::ptr::addr_of_mut!(OLDPWD_BUF) as *mut u8, cur_len);
            OLDPWD_BUF[cur_len] = 0;
            HAS_OLDPWD = true;
            if is_dash {
                let mut new_cwd = [0u8; 128];
                getcwd(new_cwd.as_mut_ptr(), new_cwd.len());
                puts(new_cwd.as_ptr());
            }
        }
    }
}

/// Lists files and directories with optional `-a`, `-l`, and `-h` flags.
///
/// # Safety
///
/// `argv` elements up to `argc` must point to valid null-terminated C-strings or be null.
pub unsafe fn handle_ls(argc: usize, argv: &[*const u8; 16]) {
    let mut show_all = false;
    let mut long_format = false;
    let mut human = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        // SAFETY: Parses flag characters.
        if unsafe { *arg == b'-' && *arg.add(1) != 0 } {
            unsafe {
                let mut ptr = arg.add(1);
                while *ptr != 0 {
                    match *ptr {
                        b'a' => show_all = true,
                        b'l' => long_format = true,
                        b'h' => human = true,
                        _ => {}
                    }
                    ptr = ptr.add(1);
                }
            }
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }

    if path_count == 0 {
        // SAFETY: Lists current directory.
        unsafe { list_directory_advanced(b".\0".as_ptr(), show_all, long_format, human) };
    } else {
        for p in 0..path_count {
            if path_count > 1 {
                // SAFETY: Prints directory header.
                unsafe { printf(b"%s:\n\0".as_ptr(), paths[p]) };
            }
            // SAFETY: Lists directory entries.
            unsafe { list_directory_advanced(paths[p], show_all, long_format, human) };
            if path_count > 1 && p < path_count - 1 {
                // SAFETY: Prints separating newline.
                unsafe { putchar(b'\n' as i32) };
            }
        }
    }
}

/// Reads and displays directory entries with formatting and metadata inspection.
///
/// # Safety
///
/// `path` must be a valid pointer to a null-terminated C-string.
pub unsafe fn list_directory_advanced(path: *const u8, show_all: bool, long_format: bool, human: bool) {
    // SAFETY: Opens directory path.
    let fd = unsafe { open(path, O_RDONLY | O_DIRECTORY, 0) };
    if fd < 0 {
        let mut st = Stat::default();
        // SAFETY: Handles regular file argument.
        if unsafe { stat(path, &mut st) } == 0 {
            if long_format {
                unsafe { printf(b"%c%s  %6d  %s\n\0".as_ptr(), if (st.st_mode & S_IFDIR) != 0 { b'd' } else { b'-' } as i32, b"rw-r--r--\0".as_ptr(), st.st_size as i32, path) };
            } else {
                unsafe { printf(b"  %s\n\0".as_ptr(), path) };
            }
            return;
        }
        // SAFETY: Prints error message on failure.
        unsafe { print_error(b"ls\0".as_ptr(), path, fd) };
        return;
    }
    let mut buf = [0u8; 4096];
    loop {
        // SAFETY: Reads directory entries via SYS_GETDENTS64.
        let n = unsafe { syscall::syscall3(SYS_GETDENTS64, fd as usize, buf.as_mut_ptr() as usize, buf.len()) as isize };
        if n <= 0 { break; }
        let mut offset = 0;
        while offset < n as usize {
            // SAFETY: Reads Dirent64 entry.
            let dirent = unsafe { &*(buf.as_ptr().add(offset) as *const Dirent64) };
            let name_ptr = dirent.d_name.as_ptr();
            let is_dot = unsafe { !show_all && *name_ptr == b'.' };
            if !is_dot {
                let suffix = if dirent.d_type == DT_DIR { b"/\0".as_ptr() } else { b"\0".as_ptr() };
                if long_format {
                    let mut fullpath = [0u8; 256];
                    let subpath = unsafe { join_path(path, name_ptr, &mut fullpath) };
                    let mut st = Stat::default();
                    if unsafe { stat(subpath, &mut st) } == 0 {
                        let type_char = if dirent.d_type == DT_DIR { b'd' } else { b'-' };
                        let mode_str = if dirent.d_type == DT_DIR { b"rwxr-xr-x\0".as_ptr() } else { b"rw-r--r--\0".as_ptr() };
                        if human && st.st_size >= 1024 {
                            unsafe { printf(b"%c%s  %4dK  %s%s\n\0".as_ptr(), type_char as i32, mode_str, ((st.st_size + 1023) / 1024) as i32, name_ptr, suffix) };
                        } else {
                            unsafe { printf(b"%c%s  %6d  %s%s\n\0".as_ptr(), type_char as i32, mode_str, st.st_size as i32, name_ptr, suffix) };
                        }
                    } else {
                        unsafe { printf(b"  %s%s\n\0".as_ptr(), name_ptr, suffix) };
                    }
                } else {
                    unsafe { printf(b"  %s%s\n\0".as_ptr(), name_ptr, suffix) };
                }
            }
            offset += core::mem::size_of::<Dirent64>();
        }
    }
    // SAFETY: Closes directory file descriptor.
    unsafe { close(fd) };
}

/// Creates files or updates access timestamps with support for `-c` (`--no-create`).
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn handle_touch(argc: usize, argv: &[*const u8; 16]) {
    let mut no_create = false;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if unsafe { strcmp(arg, b"-c\0".as_ptr()) == 0 || strcmp(arg, b"--no-create\0".as_ptr()) == 0 } {
            no_create = true;
        } else {
            let mut st = Stat::default();
            let exists = unsafe { stat(arg, &mut st) == 0 };
            if !exists && !no_create {
                // SAFETY: Creates new empty file.
                let fd = unsafe { open(arg, O_CREAT | O_WRONLY | O_TRUNC, 0o644) };
                if fd >= 0 { unsafe { close(fd) }; } else { unsafe { print_error(b"touch\0".as_ptr(), arg, fd) }; }
            }
        }
    }
}

/// Creates directories with support for `-p` (`--parents`).
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn handle_mkdir(argc: usize, argv: &[*const u8; 16]) {
    let mut parents = false;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if unsafe { strcmp(arg, b"-p\0".as_ptr()) == 0 || strcmp(arg, b"--parents\0".as_ptr()) == 0 } {
            parents = true;
        } else if parents {
            let mut sub = [0u8; 256];
            let len = unsafe { strlen(arg).min(255) };
            unsafe { core::ptr::copy_nonoverlapping(arg, sub.as_mut_ptr(), len) };
            for j in 1..=len {
                if j == len || sub[j] == b'/' {
                    let old = sub[j];
                    sub[j] = 0;
                    let _ = unsafe { mkdir(sub.as_ptr(), 0o755) };
                    sub[j] = old;
                }
            }
        } else {
            let res = unsafe { mkdir(arg, 0o755) };
            if res < 0 { unsafe { print_error(b"mkdir\0".as_ptr(), arg, res) }; }
        }
    }
}

/// Unlinks a file or recursively removes directory trees.
///
/// # Safety
///
/// `path` must be a valid pointer to a null-terminated C-string.
pub unsafe fn remove_path(path: *const u8, recursive: bool, force: bool) {
    if recursive {
        let mut subpath = [0u8; 256];
        // SAFETY: Walks directory entries recursively.
        unsafe {
            walk_dir(path, |name| {
                let full = join_path(path, name, &mut subpath);
                remove_path(full, true, force);
            });
        }
    }
    // SAFETY: Issues unlink syscall to delete file or directory.
    let res = unsafe { unlink(path) };
    if res < 0 && !force {
        unsafe { print_error(b"rm\0".as_ptr(), path, res) };
    }
}

/// Removes files or directory trees with support for `-r`/`-R` and `-f`.
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn handle_rm(argc: usize, argv: &[*const u8; 16]) {
    let mut recursive = false;
    let mut force = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if unsafe { *arg == b'-' && *arg.add(1) != 0 } {
            unsafe {
                let mut ptr = arg.add(1);
                while *ptr != 0 {
                    match *ptr {
                        b'r' | b'R' => recursive = true,
                        b'f' => force = true,
                        _ => {}
                    }
                    ptr = ptr.add(1);
                }
            }
        } else if count < 8 {
            paths[count] = arg;
            count += 1;
        }
    }
    if count == 0 && !force {
        unsafe { puts(b"rm: missing operand\0".as_ptr()) };
        return;
    }
    for p in 0..count {
        unsafe { remove_path(paths[p], recursive, force) };
    }
}

/// Copies a single file from source to destination path.
///
/// # Safety
///
/// `src` and `dest` must be valid pointers to null-terminated C-strings.
pub unsafe fn copy_file(src: *const u8, dest: *const u8, force: bool) -> i32 {
    let in_fd = unsafe { open(src, O_RDONLY, 0) };
    if in_fd < 0 {
        if !force { unsafe { print_error(b"cp\0".as_ptr(), src, in_fd) }; }
        return in_fd;
    }
    let out_fd = unsafe { open(dest, O_WRONLY | O_CREAT | O_TRUNC, 0o644) };
    if out_fd < 0 {
        unsafe {
            close(in_fd);
            if !force { print_error(b"cp\0".as_ptr(), dest, out_fd); }
        }
        return out_fd;
    }
    let mut buf = [0u8; 1024];
    loop {
        let n = unsafe { read(in_fd, buf.as_mut_ptr(), buf.len()) };
        if n <= 0 { break; }
        unsafe { write(out_fd, buf.as_ptr(), n as usize) };
    }
    unsafe {
        close(in_fd);
        close(out_fd);
    }
    0
}

/// Copies a file or recursively copies a directory tree from source to destination.
///
/// # Safety
///
/// `src` and `dest` must be valid pointers to null-terminated C-strings.
pub unsafe fn copy_path(src: *const u8, dest: *const u8, recursive: bool, force: bool) -> i32 {
    let mut st = Stat::default();
    let res = unsafe { stat(src, &mut st) };
    if res != 0 {
        if !force { unsafe { print_error(b"cp\0".as_ptr(), src, res) }; }
        return res;
    }
    if (st.st_mode & S_IFDIR) != 0 {
        if !recursive {
            unsafe { printf(b"cp: -r not specified; omitting directory '%s'\n\0".as_ptr(), src) };
            return -EISDIR;
        }
        let _ = unsafe { mkdir(dest, 0o755) };
        let mut sub_src = [0u8; 256];
        let mut sub_dest = [0u8; 256];
        unsafe {
            walk_dir(src, |name| {
                let s = join_path(src, name, &mut sub_src);
                let d = join_path(dest, name, &mut sub_dest);
                copy_path(s, d, true, force);
            });
        }
        0
    } else {
        unsafe { copy_file(src, dest, force) }
    }
}

/// Copies files and directories with support for `-r`/`-R` and `-f`.
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn handle_cp(argc: usize, argv: &[*const u8; 16]) {
    let mut recursive = false;
    let mut force = false;
    let mut operands: [*const u8; 16] = [core::ptr::null(); 16];
    let mut count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if unsafe { *arg == b'-' && *arg.add(1) != 0 } {
            unsafe {
                let mut ptr = arg.add(1);
                while *ptr != 0 {
                    match *ptr {
                        b'r' | b'R' => recursive = true,
                        b'f' => force = true,
                        _ => {}
                    }
                    ptr = ptr.add(1);
                }
            }
        } else if count < 16 {
            operands[count] = arg;
            count += 1;
        }
    }
    if count < 2 {
        unsafe { puts(b"cp: missing file operand\0".as_ptr()) };
        return;
    }
    let dest = operands[count - 1];
    let mut dest_st = Stat::default();
    let dest_is_dir = unsafe { stat(dest, &mut dest_st) == 0 && (dest_st.st_mode & S_IFDIR) != 0 };

    if count > 2 && !dest_is_dir {
        unsafe { printf(b"cp: target '%s' is not a directory\n\0".as_ptr(), dest) };
        return;
    }
    if count == 2 && !dest_is_dir {
        unsafe { copy_path(operands[0], dest, recursive, force) };
    } else {
        let mut target_buf = [0u8; 256];
        for i in 0..count - 1 {
            let base = unsafe { get_basename(operands[i]) };
            let full = unsafe { join_path(dest, base, &mut target_buf) };
            unsafe { copy_path(operands[i], full, recursive, force) };
        }
    }
}

/// Moves or renames a path, falling back to copy and delete if `rename` fails.
///
/// # Safety
///
/// `src` and `dest` must be valid pointers to null-terminated C-strings.
pub unsafe fn move_path(src: *const u8, dest: *const u8, force: bool) -> i32 {
    let res = unsafe { rename(src, dest) };
    if res == 0 { return 0; }
    let cp_res = unsafe { copy_path(src, dest, true, force) };
    if cp_res == 0 {
        unsafe { remove_path(src, true, true) };
        0
    } else {
        if !force { unsafe { print_error(b"mv\0".as_ptr(), src, res) }; }
        res
    }
}

/// Moves or renames files and directories with support for `-f`.
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn handle_mv(argc: usize, argv: &[*const u8; 16]) {
    let mut force = false;
    let mut operands: [*const u8; 16] = [core::ptr::null(); 16];
    let mut count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() { continue; }
        if unsafe { *arg == b'-' && *arg.add(1) != 0 } {
            unsafe {
                let mut ptr = arg.add(1);
                while *ptr != 0 {
                    if *ptr == b'f' { force = true; }
                    ptr = ptr.add(1);
                }
            }
        } else if count < 16 {
            operands[count] = arg;
            count += 1;
        }
    }
    if count < 2 {
        unsafe { puts(b"mv: missing file operand\0".as_ptr()) };
        return;
    }
    let dest = operands[count - 1];
    let mut dest_st = Stat::default();
    let dest_is_dir = unsafe { stat(dest, &mut dest_st) == 0 && (dest_st.st_mode & S_IFDIR) != 0 };

    if count > 2 && !dest_is_dir {
        unsafe { printf(b"mv: target '%s' is not a directory\n\0".as_ptr(), dest) };
        return;
    }
    if count == 2 && !dest_is_dir {
        unsafe { move_path(operands[0], dest, force) };
    } else {
        let mut target_buf = [0u8; 256];
        for i in 0..count - 1 {
            let base = unsafe { get_basename(operands[i]) };
            let full = unsafe { join_path(dest, base, &mut target_buf) };
            unsafe { move_path(operands[i], full, force) };
        }
    }
}

/// Concatenates and prints file contents or stdin with support for line numbering (`-n`).
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn handle_cat(argc: usize, argv: &[*const u8; 16]) {
    let mut number_lines = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() {
            continue;
        }
        // SAFETY: Checks flag arguments.
        if unsafe { strcmp(arg, b"-n\0".as_ptr()) == 0 || strcmp(arg, b"--number\0".as_ptr()) == 0 }
        {
            number_lines = true;
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 {
        // SAFETY: Displays stdin stream.
        unsafe { display_stream(STDIN_FILENO, number_lines) };
    } else {
        for p in 0..path_count {
            // SAFETY: Displays file stream.
            unsafe { display_file(paths[p], number_lines) };
        }
    }
}

/// Streams content from a file descriptor to stdout with optional line numbering.
///
/// # Safety
///
/// `fd` must be a valid, readable file descriptor.
pub unsafe fn display_stream(fd: i32, number_lines: bool) {
    let mut buf = [0u8; 512];
    let mut line_num = 1;
    let mut at_line_start = true;
    loop {
        // SAFETY: Reads from stream file descriptor into stack buffer.
        let n = unsafe { read(fd, buf.as_mut_ptr(), buf.len()) };
        if n <= 0 {
            break;
        }
        if number_lines {
            for i in 0..n as usize {
                if at_line_start {
                    // SAFETY: Prints line number prefix.
                    unsafe { printf(b"%6d  \0".as_ptr(), line_num) };
                    line_num += 1;
                    at_line_start = false;
                }
                // SAFETY: Writes character to stdout.
                unsafe { putchar(buf[i] as i32) };
                if buf[i] == b'\n' {
                    at_line_start = true;
                }
            }
        } else {
            // SAFETY: Writes raw buffer chunk to stdout.
            unsafe { write(STDOUT_FILENO, buf.as_ptr(), n as usize) };
        }
    }
}

/// Opens and streams a file's content to stdout with optional line numbering.
///
/// # Safety
///
/// `path` must be a valid pointer to a null-terminated C-string.
pub unsafe fn display_file(path: *const u8, number_lines: bool) {
    // SAFETY: Opens file path for reading.
    let fd = unsafe { open(path, O_RDONLY, 0) };
    if fd < 0 {
        // SAFETY: Prints error message to stdout.
        unsafe { print_error(b"cat\0".as_ptr(), path, fd) };
        return;
    }
    // SAFETY: Displays stream and closes fd.
    unsafe {
        display_stream(fd, number_lines);
        close(fd);
    }
}

/// Prints command arguments separated by spaces, with support for `-n` to omit trailing newline.
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn handle_echo(argc: usize, argv: &[*const u8; 16]) {
    let mut no_newline = false;
    let mut start_idx = 1;
    if argc > 1 && !argv[1].is_null() && unsafe { strcmp(argv[1], b"-n\0".as_ptr()) } == 0 {
        no_newline = true;
        start_idx = 2;
    }
    for i in start_idx..argc {
        let arg = argv[i];
        if !arg.is_null() {
            if i > start_idx {
                // SAFETY: Writes separating space.
                unsafe { putchar(b' ' as i32) };
            }
            // SAFETY: Measures and writes argument string to stdout.
            unsafe {
                let len = strlen(arg);
                write(STDOUT_FILENO, arg, len);
            }
        }
    }
    if !no_newline {
        // SAFETY: Writes trailing newline to stdout.
        unsafe { putchar(b'\n' as i32) };
    }
}

/// Fetches kernel telemetry via `SYS_SYSINFO` and prints uptime, process count, and memory usage.
///
/// # Safety
///
/// Standard output file descriptor must be open and writable.
pub unsafe fn display_system_monitor() {
    let mut info = Sysinfo::default();
    // SAFETY: Issues SYS_SYSINFO syscall to query kernel telemetry.
    if unsafe { syscall::syscall1(SYS_SYSINFO, &mut info as *mut _ as usize) } == 0 {
        let total_mb = info.totalram / (1024 * 1024);
        let free_mb = info.freeram / (1024 * 1024);
        let used_mb = total_mb.saturating_sub(free_mb);
        let heap_kb = info.bufferram / 1024;
        let heap_used_kb = info.sharedram / 1024;
        let uptime_s = info.uptime;

        // SAFETY: Prints telemetry dashboard to stdout.
        unsafe {
            puts(
                b"\x1b[1;36m==================== RUST POSIX OS MONITOR ====================\x1b[0m\0"
                    .as_ptr(),
            );
            printf(b"  Uptime:       %d seconds\n\0".as_ptr(), uptime_s as i32);
            printf(b"  Processes:    %d active\n\0".as_ptr(), info.procs as i32);
            printf(
                b"  Memory Total: %d MiB | Used: %d MiB | Free: %d MiB\n\0".as_ptr(),
                total_mb as i32,
                used_mb as i32,
                free_mb as i32,
            );
            printf(
                b"  Kernel Heap:  %d KiB | Used: %d KiB\n\0".as_ptr(),
                heap_kb as i32,
                heap_used_kb as i32,
            );
            puts(b"  Scheduler:    Lock-free Preemptive Multitasking\n\x1b[1;36m===============================================================\x1b[0m\0".as_ptr());
        }
    } else {
        // SAFETY: Prints failure message to stdout.
        unsafe { puts(b"monitor: failed to query telemetry\0".as_ptr()) };
    }
}

/// Simulates kernel async I/O and event loop handling using `epoll` and anonymous pipes.
///
/// # Safety
///
/// Requires kernel support for epoll and pipe syscalls. Standard output must be writable.
pub unsafe fn run_async_demo() {
    // SAFETY: Creates epoll and pipe instances, sends message, and waits for event.
    unsafe {
        puts(b"Running Kernel Async/Epoll Event-Loop Simulation...\0".as_ptr());
        let epfd = epoll_create1(0);
        if epfd < 0 {
            puts(b"async-demo: epoll_create1 failed\0".as_ptr());
            return;
        }

        let mut pipefds = [0i32; 2];
        if pipe(&mut pipefds as *mut [i32; 2]) < 0 {
            puts(b"async-demo: pipe failed\0".as_ptr());
            close(epfd);
            return;
        }

        let mut ev = EpollEvent {
            events: EPOLLIN,
            data: 0x42,
        };
        epoll_ctl(epfd, EPOLL_CTL_ADD, pipefds[0], &mut ev);

        let msg = b"Async POSIX Micro-Task Message\n";
        write(pipefds[1], msg.as_ptr(), msg.len());

        let mut events = [EpollEvent::default(); 4];
        let ready = epoll_wait(epfd, events.as_mut_ptr(), 4, 100);
        if ready > 0 {
            printf(
                b"async-demo: epoll woke up! Ready events: %d (tag=0x%x)\n\0".as_ptr(),
                ready,
                events[0].data as i32,
            );
            display_stream(pipefds[0], false);
        }

        close(pipefds[0]);
        close(pipefds[1]);
        close(epfd);
        puts(b"Async demo completed successfully.\0".as_ptr());
    }
}

/// Triggers a system state audit snapshot via `SYS_AUDIT_SNAPSHOT`.
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null.
pub unsafe fn handle_snapshot_command(argc: usize, argv: &[*const u8; 16]) {
    let label = if argc > 1 && !argv[1].is_null() {
        argv[1]
    } else {
        b"user_manual\0".as_ptr()
    };
    // SAFETY: Issues SYS_AUDIT_SNAPSHOT syscall with snapshot label string pointer.
    let snap_id = unsafe { syscall::syscall2(SYS_AUDIT_SNAPSHOT, label as usize, 0) as isize };
    if snap_id > 0 {
        // SAFETY: Prints snapshot creation confirmation.
        unsafe {
            printf(
                b"System State Snapshot #%d Created successfully (label: %s)\n\0".as_ptr(),
                snap_id as i32,
                label,
            );
        }
    } else {
        // SAFETY: Prints error diagnostics.
        unsafe {
            printf(
                b"snapshot: failed to capture system snapshot (errno: %d)\n\0".as_ptr(),
                -snap_id as i32,
            );
        }
    }
}

/// Prints current real and effective user and group IDs (`uid`, `gid`, `euid`, `egid`).
///
/// # Safety
///
/// Standard output file descriptor must be open and writable.
pub unsafe fn handle_id() {
    // SAFETY: Queries uid, gid, euid, egid via syscall wrappers.
    let uid = unsafe { getuid() };
    let gid = unsafe { getgid() };
    let euid = unsafe { geteuid() };
    let egid = unsafe { getegid() };
    // SAFETY: Formats user identity strings to stdout.
    unsafe {
        if uid == 0 {
            printf(b"uid=%d(root) gid=%d(root)\0".as_ptr(), uid, gid);
        } else {
            printf(b"uid=%d gid=%d\0".as_ptr(), uid, gid);
        }
        if euid != uid || egid != gid {
            printf(b" euid=%d egid=%d\0".as_ptr(), euid, egid);
        }
        puts(b"\0".as_ptr());
    }
}

/// Manages the shell clipboard and kill-ring, synchronizing with host clipboard via OSC 52.
///
/// # Safety
///
/// `argv` pointers up to `argc` must be valid null-terminated C-strings or null. Mutates kill-ring global state.
pub unsafe fn handle_clip(argc: usize, argv: &[*const u8; 16]) {
    if argc > 1 && !argv[1].is_null() {
        // SAFETY: Checking for -h / --help or -p / --paste flags.
        let is_help = unsafe {
            strcmp(argv[1], b"-h\0".as_ptr()) == 0 || strcmp(argv[1], b"--help\0".as_ptr()) == 0
        };
        if is_help {
            // SAFETY: Prints usage instructions.
            unsafe {
                puts(b"Usage: clip <text> | <cmd> | clip | clip -p\nSyncs text into in-memory ring and emits ANSI OSC 52 to host clipboard.\0".as_ptr())
            };
            return;
        }
        let is_paste = unsafe {
            strcmp(argv[1], b"-p\0".as_ptr()) == 0 || strcmp(argv[1], b"--paste\0".as_ptr()) == 0
        };
        if is_paste {
            // SAFETY: Read active in-memory clipboard buffer in single-threaded shell runtime.
            let bytes = unsafe {
                let kr = &raw const crate::editor::KILL_RING;
                (*kr).as_bytes()
            };
            if bytes.is_empty() {
                // SAFETY: Prints empty clipboard notice.
                unsafe { puts(b"Clipboard is empty.\0".as_ptr()) };
            } else {
                // SAFETY: Writes clipboard content to stdout.
                unsafe {
                    write(STDOUT_FILENO, bytes.as_ptr(), bytes.len());
                    puts(b"\0".as_ptr());
                }
            }
            return;
        }

        let mut buf = [0u8; 1024];
        let mut offset = 0;
        for i in 1..argc {
            let arg = argv[i];
            if arg.is_null() {
                break;
            }
            // SAFETY: Reading valid nul-terminated command line argument string.
            let len = unsafe { strlen(arg) };
            if offset + len + 1 < buf.len() {
                if offset > 0 {
                    buf[offset] = b' ';
                    offset += 1;
                }
                // SAFETY: Copying valid bytes from command argument into stack buffer.
                unsafe {
                    buf[offset..offset + len]
                        .copy_from_slice(core::slice::from_raw_parts(arg, len));
                }
                offset += len;
            }
        }
        // SAFETY: Single-threaded shell mutation of global kill-ring buffer and OSC 52 sync.
        unsafe {
            let kr = &raw mut crate::editor::KILL_RING;
            (*kr).save(&buf[..offset]);
            crate::editor::osc52_copy(&buf[..offset]);
            puts(b"Copied to clipboard.\0".as_ptr());
        }
    } else {
        let mut buf = [0u8; 1024];
        let mut total = 0;
        loop {
            // SAFETY: Reading piped stream from stdin until EOF or capacity.
            let n = unsafe { read(STDIN_FILENO, buf.as_mut_ptr().add(total), buf.len() - total) };
            if n <= 0 {
                break;
            }
            total += n as usize;
            if total >= buf.len() {
                break;
            }
        }
        if total > 0 {
            // SAFETY: Single-threaded shell mutation of global kill-ring buffer and OSC 52 sync.
            unsafe {
                let kr = &raw mut crate::editor::KILL_RING;
                (*kr).save(&buf[..total]);
                crate::editor::osc52_copy(&buf[..total]);
                printf(b"Copied %d bytes to clipboard.\n\0".as_ptr(), total as i32);
            }
        } else {
            // SAFETY: Reading active in-memory kill-ring buffer for display in single-threaded shell context.
            let bytes = unsafe {
                let kr = &raw const crate::editor::KILL_RING;
                (*kr).as_bytes()
            };
            if bytes.is_empty() {
                // SAFETY: Prints empty clipboard message.
                unsafe {
                    puts(b"Clipboard is empty. Usage: clip <text> | <cmd> | clip\0".as_ptr())
                };
            } else {
                // SAFETY: Writes clipboard bytes to stdout.
                unsafe {
                    write(STDOUT_FILENO, bytes.as_ptr(), bytes.len());
                    puts(b"\0".as_ptr());
                }
            }
        }
    }
}
