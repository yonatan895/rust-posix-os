//! Built-in Commands for POSIX Interactive Shell.

use libc::*;
use posix_abi::*;

pub static mut OLDPWD_BUF: [u8; 128] = [0u8; 128];
pub static mut HAS_OLDPWD: bool = false;

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
    printf(
        b"%s: '%s': %s (errno: %d)\n\0".as_ptr(),
        action,
        target,
        msg,
        err_code,
    );
}

pub unsafe fn handle_cd(argc: usize, argv: &[*const u8; 16]) {
    let mut current_cwd = [0u8; 128];
    getcwd(current_cwd.as_mut_ptr(), current_cwd.len());
    let is_dash = argc > 1 && !argv[1].is_null() && strcmp(argv[1], b"-\0".as_ptr()) == 0;
    let target = if argc == 1 || argv[1].is_null() || strcmp(argv[1], b"~\0".as_ptr()) == 0 {
        b"/\0".as_ptr()
    } else if is_dash {
        if !HAS_OLDPWD {
            puts(b"cd: OLDPWD not set\0".as_ptr());
            return;
        }
        core::ptr::addr_of!(OLDPWD_BUF) as *const u8
    } else {
        argv[1]
    };
    let res = chdir(target);
    if res < 0 {
        print_error(b"cd\0".as_ptr(), target, res);
    } else {
        let cur_len = strlen(current_cwd.as_ptr()).min(127);
        core::ptr::copy_nonoverlapping(
            current_cwd.as_ptr(),
            core::ptr::addr_of_mut!(OLDPWD_BUF) as *mut u8,
            cur_len,
        );
        OLDPWD_BUF[cur_len] = 0;
        HAS_OLDPWD = true;
        if is_dash {
            let mut new_cwd = [0u8; 128];
            getcwd(new_cwd.as_mut_ptr(), new_cwd.len());
            puts(new_cwd.as_ptr());
        }
    }
}

pub unsafe fn handle_ls(argc: usize, argv: &[*const u8; 16]) {
    let mut show_all = false;
    let mut long_format = false;
    let mut human = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() {
            continue;
        }
        if *arg == b'-' && *arg.add(1) != 0 {
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
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }

    if path_count == 0 {
        list_directory_advanced(b".\0".as_ptr(), show_all, long_format, human);
    } else {
        for p in 0..path_count {
            if path_count > 1 {
                printf(b"%s:\n\0".as_ptr(), paths[p]);
            }
            list_directory_advanced(paths[p], show_all, long_format, human);
            if path_count > 1 && p < path_count - 1 {
                putchar(b'\n' as i32);
            }
        }
    }
}

pub unsafe fn list_directory_advanced(
    path: *const u8,
    show_all: bool,
    long_format: bool,
    human: bool,
) {
    let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
    if fd < 0 {
        let mut st = Stat::default();
        if stat(path, &mut st) == 0 {
            if long_format {
                printf(
                    b"%c%s  %6d  %s\n\0".as_ptr(),
                    if (st.st_mode & S_IFDIR) != 0 {
                        b'd'
                    } else {
                        b'-'
                    } as i32,
                    b"rw-r--r--\0".as_ptr(),
                    st.st_size as i32,
                    path,
                );
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
        let n = syscall::syscall3(
            SYS_GETDENTS64,
            fd as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
        ) as isize;
        if n <= 0 {
            break;
        }
        let mut offset = 0;
        while offset < n as usize {
            let dirent = &*(buf.as_ptr().add(offset) as *const Dirent64);
            let name_ptr = dirent.d_name.as_ptr();
            if !show_all && *name_ptr == b'.' {
                offset += core::mem::size_of::<Dirent64>();
                continue;
            }
            let suffix = if dirent.d_type == DT_DIR {
                b"/\0".as_ptr()
            } else {
                b"\0".as_ptr()
            };
            if long_format {
                let mut fullpath = [0u8; 256];
                let path_len = strlen(path);
                let name_len = strlen(name_ptr);
                let need_slash = if path_len > 0 && *path.add(path_len - 1) != b'/' {
                    1
                } else {
                    0
                };
                if path_len + need_slash + name_len < 255 {
                    core::ptr::copy_nonoverlapping(path, fullpath.as_mut_ptr(), path_len);
                    if need_slash == 1 {
                        fullpath[path_len] = b'/';
                    }
                    core::ptr::copy_nonoverlapping(
                        name_ptr,
                        fullpath.as_mut_ptr().add(path_len + need_slash),
                        name_len,
                    );
                    fullpath[path_len + need_slash + name_len] = 0;
                    let mut st = Stat::default();
                    if stat(fullpath.as_ptr(), &mut st) == 0 {
                        let type_char = if dirent.d_type == DT_DIR { b'd' } else { b'-' };
                        let mode_str = if dirent.d_type == DT_DIR {
                            b"rwxr-xr-x\0".as_ptr()
                        } else {
                            b"rw-r--r--\0".as_ptr()
                        };
                        if human && st.st_size >= 1024 {
                            printf(
                                b"%c%s  %4dK  %s%s\n\0".as_ptr(),
                                type_char as i32,
                                mode_str,
                                ((st.st_size + 1023) / 1024) as i32,
                                name_ptr,
                                suffix,
                            );
                        } else {
                            printf(
                                b"%c%s  %6d  %s%s\n\0".as_ptr(),
                                type_char as i32,
                                mode_str,
                                st.st_size as i32,
                                name_ptr,
                                suffix,
                            );
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

pub unsafe fn handle_touch(argc: usize, argv: &[*const u8; 16]) {
    let mut no_create = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() {
            continue;
        }
        if strcmp(arg, b"-c\0".as_ptr()) == 0 || strcmp(arg, b"--no-create\0".as_ptr()) == 0 {
            no_create = true;
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }

    if path_count == 0 {
        puts(b"touch: missing file operand\0".as_ptr());
        return;
    }

    for p in 0..path_count {
        let flags = if no_create { O_RDWR } else { O_RDWR | O_CREAT };
        let fd = open(paths[p], flags, 0o644);
        if fd >= 0 {
            close(fd);
        } else if !no_create {
            print_error(b"touch\0".as_ptr(), paths[p], fd);
        }
    }
}

pub unsafe fn handle_mkdir(argc: usize, argv: &[*const u8; 16]) {
    let mut create_parents = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() {
            continue;
        }
        if strcmp(arg, b"-p\0".as_ptr()) == 0 || strcmp(arg, b"--parents\0".as_ptr()) == 0 {
            create_parents = true;
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 {
        puts(b"mkdir: missing operand\0".as_ptr());
        return;
    }
    for p in 0..path_count {
        if create_parents {
            mkdir_p(paths[p]);
        } else {
            let res = mkdir(paths[p], 0o755);
            if res < 0 {
                print_error(b"mkdir: cannot create directory\0".as_ptr(), paths[p], res);
            }
        }
    }
}

pub unsafe fn mkdir_p(path: *const u8) {
    let len = strlen(path);
    let mut subpath = [0u8; 256];
    if len >= 255 {
        return;
    }
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

pub unsafe fn handle_rm(argc: usize, argv: &[*const u8; 16]) {
    let mut recursive = false;
    let mut force = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() {
            continue;
        }
        if *arg == b'-' && *arg.add(1) != 0 {
            let mut ptr = arg.add(1);
            while *ptr != 0 {
                match *ptr {
                    b'r' | b'R' => recursive = true,
                    b'f' => force = true,
                    _ => {}
                }
                ptr = ptr.add(1);
            }
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 {
        if !force {
            puts(b"rm: missing operand\0".as_ptr());
        }
        return;
    }
    for p in 0..path_count {
        remove_path(paths[p], recursive, force);
    }
}

pub unsafe fn remove_path(path: *const u8, recursive: bool, force: bool) {
    if recursive {
        let fd = open(path, O_RDONLY | O_DIRECTORY, 0);
        if fd >= 0 {
            let mut buf = [0u8; 4096];
            loop {
                let n = syscall::syscall3(
                    SYS_GETDENTS64,
                    fd as usize,
                    buf.as_mut_ptr() as usize,
                    buf.len(),
                ) as isize;
                if n <= 0 {
                    break;
                }
                let mut offset = 0;
                while offset < n as usize {
                    let dirent = &*(buf.as_ptr().add(offset) as *const Dirent64);
                    let name_ptr = dirent.d_name.as_ptr();
                    if strcmp(name_ptr, b".\0".as_ptr()) != 0
                        && strcmp(name_ptr, b"..\0".as_ptr()) != 0
                    {
                        let mut subpath = [0u8; 256];
                        let path_len = strlen(path);
                        let name_len = strlen(name_ptr);
                        let need_slash = if path_len > 0 && *path.add(path_len - 1) != b'/' {
                            1
                        } else {
                            0
                        };
                        if path_len + need_slash + name_len < 255 {
                            core::ptr::copy_nonoverlapping(path, subpath.as_mut_ptr(), path_len);
                            if need_slash == 1 {
                                subpath[path_len] = b'/';
                            }
                            core::ptr::copy_nonoverlapping(
                                name_ptr,
                                subpath.as_mut_ptr().add(path_len + need_slash),
                                name_len,
                            );
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
    if res < 0 && !force {
        print_error(b"rm\0".as_ptr(), path, res);
    }
}

pub unsafe fn get_basename(path: *const u8) -> *const u8 {
    if path.is_null() {
        return b"\0".as_ptr();
    }

    let mut last_slash = path;
    let mut ptr = path;
    while *ptr != 0 {
        if *ptr == b'/' && *ptr.add(1) != 0 {
            last_slash = ptr.add(1);
        }
        ptr = ptr.add(1);
    }
    last_slash
}

pub unsafe fn copy_file(src: *const u8, dest: *const u8, force: bool) -> i32 {
    let in_fd = open(src, O_RDONLY, 0);
    if in_fd < 0 {
        if !force {
            print_error(b"cp\0".as_ptr(), src, in_fd);
        }
        return in_fd;
    }
    let out_fd = open(dest, O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    if out_fd < 0 {
        close(in_fd);
        if !force {
            print_error(b"cp\0".as_ptr(), dest, out_fd);
        }
        return out_fd;
    }
    let mut buf = [0u8; 1024];
    loop {
        let n = read(in_fd, buf.as_mut_ptr(), buf.len());
        if n <= 0 {
            break;
        }
        write(out_fd, buf.as_ptr(), n as usize);
    }
    close(in_fd);
    close(out_fd);
    0
}

pub unsafe fn copy_path(src: *const u8, dest: *const u8, recursive: bool, force: bool) -> i32 {
    let mut st = Stat::default();
    let res = stat(src, &mut st);
    if res != 0 {
        if !force {
            print_error(b"cp\0".as_ptr(), src, res);
        }
        return res;
    }
    if (st.st_mode & S_IFDIR) != 0 {
        if !recursive {
            printf(
                b"cp: -r not specified; omitting directory '%s'\n\0".as_ptr(),
                src,
            );
            return -EISDIR;
        }
        let _ = mkdir(dest, 0o755);
        let fd = open(src, O_RDONLY | O_DIRECTORY, 0);
        if fd >= 0 {
            let mut buf = [0u8; 4096];
            loop {
                let n = syscall::syscall3(
                    SYS_GETDENTS64,
                    fd as usize,
                    buf.as_mut_ptr() as usize,
                    buf.len(),
                ) as isize;
                if n <= 0 {
                    break;
                }
                let mut offset = 0;
                while offset < n as usize {
                    let dirent = &*(buf.as_ptr().add(offset) as *const Dirent64);
                    let name_ptr = dirent.d_name.as_ptr();
                    if strcmp(name_ptr, b".\0".as_ptr()) != 0
                        && strcmp(name_ptr, b"..\0".as_ptr()) != 0
                    {
                        let mut sub_src = [0u8; 256];
                        let mut sub_dest = [0u8; 256];
                        let src_len = strlen(src);
                        let dest_len = strlen(dest);
                        let name_len = strlen(name_ptr);
                        let src_slash = if src_len > 0 && *src.add(src_len - 1) != b'/' {
                            1
                        } else {
                            0
                        };
                        let dest_slash = if dest_len > 0 && *dest.add(dest_len - 1) != b'/' {
                            1
                        } else {
                            0
                        };

                        if src_len + src_slash + name_len < 255
                            && dest_len + dest_slash + name_len < 255
                        {
                            core::ptr::copy_nonoverlapping(src, sub_src.as_mut_ptr(), src_len);
                            if src_slash == 1 {
                                sub_src[src_len] = b'/';
                            }
                            core::ptr::copy_nonoverlapping(
                                name_ptr,
                                sub_src.as_mut_ptr().add(src_len + src_slash),
                                name_len,
                            );
                            sub_src[src_len + src_slash + name_len] = 0;

                            core::ptr::copy_nonoverlapping(dest, sub_dest.as_mut_ptr(), dest_len);
                            if dest_slash == 1 {
                                sub_dest[dest_len] = b'/';
                            }
                            core::ptr::copy_nonoverlapping(
                                name_ptr,
                                sub_dest.as_mut_ptr().add(dest_len + dest_slash),
                                name_len,
                            );
                            sub_dest[dest_len + dest_slash + name_len] = 0;

                            copy_path(sub_src.as_ptr(), sub_dest.as_ptr(), true, force);
                        }
                    }
                    offset += core::mem::size_of::<Dirent64>();
                }
            }
            close(fd);
        }
        0
    } else {
        copy_file(src, dest, force)
    }
}

pub unsafe fn handle_cp(argc: usize, argv: &[*const u8; 16]) {
    let mut recursive = false;
    let mut force = false;
    let mut operands: [*const u8; 16] = [core::ptr::null(); 16];
    let mut operand_count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() {
            continue;
        }
        if *arg == b'-' && *arg.add(1) != 0 {
            let mut ptr = arg.add(1);
            while *ptr != 0 {
                match *ptr {
                    b'r' | b'R' => recursive = true,
                    b'f' => force = true,
                    _ => {}
                }
                ptr = ptr.add(1);
            }
        } else if operand_count < 16 {
            operands[operand_count] = arg;
            operand_count += 1;
        }
    }

    if operand_count == 0 {
        puts(b"cp: missing file operand\0".as_ptr());
        return;
    }
    if operand_count == 1 {
        printf(
            b"cp: missing destination file operand after '%s'\n\0".as_ptr(),
            operands[0],
        );
        return;
    }

    let dest = operands[operand_count - 1];
    let mut dest_st = Stat::default();
    let dest_is_dir = stat(dest, &mut dest_st) == 0 && (dest_st.st_mode & S_IFDIR) != 0;

    if operand_count > 2 && !dest_is_dir {
        printf(b"cp: target '%s' is not a directory\n\0".as_ptr(), dest);
        return;
    }

    if operand_count == 2 && !dest_is_dir {
        copy_path(operands[0], dest, recursive, force);
    } else {
        for i in 0..operand_count - 1 {
            let src = operands[i];
            let base = get_basename(src);
            let mut full_target = [0u8; 256];
            let dest_len = strlen(dest);
            let base_len = strlen(base);
            let need_slash = if dest_len > 0 && *dest.add(dest_len - 1) != b'/' {
                1
            } else {
                0
            };

            if dest_len + need_slash + base_len < 255 {
                core::ptr::copy_nonoverlapping(dest, full_target.as_mut_ptr(), dest_len);
                if need_slash == 1 {
                    full_target[dest_len] = b'/';
                }
                core::ptr::copy_nonoverlapping(
                    base,
                    full_target.as_mut_ptr().add(dest_len + need_slash),
                    base_len,
                );
                full_target[dest_len + need_slash + base_len] = 0;
                copy_path(src, full_target.as_ptr(), recursive, force);
            }
        }
    }
}

pub unsafe fn move_path(src: *const u8, dest: *const u8, force: bool) -> i32 {
    let res = rename(src, dest);
    if res == 0 {
        return 0;
    }
    let cp_res = copy_path(src, dest, true, force);
    if cp_res == 0 {
        remove_path(src, true, true);
        0
    } else {
        if !force {
            print_error(b"mv\0".as_ptr(), src, res);
        }
        res
    }
}

pub unsafe fn handle_mv(argc: usize, argv: &[*const u8; 16]) {
    let mut force = false;
    let mut operands: [*const u8; 16] = [core::ptr::null(); 16];
    let mut operand_count = 0;

    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() {
            continue;
        }
        if *arg == b'-' && *arg.add(1) != 0 {
            let mut ptr = arg.add(1);
            while *ptr != 0 {
                match *ptr {
                    b'f' => force = true,
                    _ => {}
                }
                ptr = ptr.add(1);
            }
        } else if operand_count < 16 {
            operands[operand_count] = arg;
            operand_count += 1;
        }
    }

    if operand_count == 0 {
        puts(b"mv: missing file operand\0".as_ptr());
        return;
    }
    if operand_count == 1 {
        printf(
            b"mv: missing destination file operand after '%s'\n\0".as_ptr(),
            operands[0],
        );
        return;
    }

    let dest = operands[operand_count - 1];
    let mut dest_st = Stat::default();
    let dest_is_dir = stat(dest, &mut dest_st) == 0 && (dest_st.st_mode & S_IFDIR) != 0;

    if operand_count > 2 && !dest_is_dir {
        printf(b"mv: target '%s' is not a directory\n\0".as_ptr(), dest);
        return;
    }

    if operand_count == 2 && !dest_is_dir {
        move_path(operands[0], dest, force);
    } else {
        for i in 0..operand_count - 1 {
            let src = operands[i];
            let base = get_basename(src);
            let mut full_target = [0u8; 256];
            let dest_len = strlen(dest);
            let base_len = strlen(base);
            let need_slash = if dest_len > 0 && *dest.add(dest_len - 1) != b'/' {
                1
            } else {
                0
            };

            if dest_len + need_slash + base_len < 255 {
                core::ptr::copy_nonoverlapping(dest, full_target.as_mut_ptr(), dest_len);
                if need_slash == 1 {
                    full_target[dest_len] = b'/';
                }
                core::ptr::copy_nonoverlapping(
                    base,
                    full_target.as_mut_ptr().add(dest_len + need_slash),
                    base_len,
                );
                full_target[dest_len + need_slash + base_len] = 0;
                move_path(src, full_target.as_ptr(), force);
            }
        }
    }
}

pub unsafe fn handle_cat(argc: usize, argv: &[*const u8; 16]) {
    let mut number_lines = false;
    let mut paths: [*const u8; 8] = [core::ptr::null(); 8];
    let mut path_count = 0;
    for i in 1..argc {
        let arg = argv[i];
        if arg.is_null() {
            continue;
        }
        if strcmp(arg, b"-n\0".as_ptr()) == 0 || strcmp(arg, b"--number\0".as_ptr()) == 0 {
            number_lines = true;
        } else if path_count < 8 {
            paths[path_count] = arg;
            path_count += 1;
        }
    }
    if path_count == 0 {
        display_stream(STDIN_FILENO, number_lines);
    } else {
        for p in 0..path_count {
            display_file(paths[p], number_lines);
        }
    }
}

pub unsafe fn display_stream(fd: i32, number_lines: bool) {
    let mut buf = [0u8; 512];
    let mut line_num = 1;
    let mut at_line_start = true;
    loop {
        let n = read(fd, buf.as_mut_ptr(), buf.len());
        if n <= 0 {
            break;
        }
        if number_lines {
            for i in 0..n as usize {
                if at_line_start {
                    printf(b"%6d  \0".as_ptr(), line_num);
                    line_num += 1;
                    at_line_start = false;
                }
                putchar(buf[i] as i32);
                if buf[i] == b'\n' {
                    at_line_start = true;
                }
            }
        } else {
            write(STDOUT_FILENO, buf.as_ptr(), n as usize);
        }
    }
}

pub unsafe fn display_file(path: *const u8, number_lines: bool) {
    let fd = open(path, O_RDONLY, 0);
    if fd < 0 {
        print_error(b"cat\0".as_ptr(), path, fd);
        return;
    }
    display_stream(fd, number_lines);
    close(fd);
}

pub unsafe fn handle_echo(argc: usize, argv: &[*const u8; 16]) {
    let mut no_newline = false;
    let mut start_idx = 1;
    if argc > 1 && !argv[1].is_null() && strcmp(argv[1], b"-n\0".as_ptr()) == 0 {
        no_newline = true;
        start_idx = 2;
    }
    for i in start_idx..argc {
        let arg = argv[i];
        if !arg.is_null() {
            if i > start_idx {
                putchar(b' ' as i32);
            }
            let len = strlen(arg);
            write(STDOUT_FILENO, arg, len);
        }
    }
    if !no_newline {
        putchar(b'\n' as i32);
    }
}

pub unsafe fn display_system_monitor() {
    let mut info = Sysinfo::default();
    if syscall::syscall1(SYS_SYSINFO, &mut info as *mut _ as usize) == 0 {
        let total_mb = info.totalram / (1024 * 1024);
        let free_mb = info.freeram / (1024 * 1024);
        let used_mb = total_mb.saturating_sub(free_mb);
        let heap_kb = info.bufferram / 1024;
        let heap_used_kb = info.sharedram / 1024;
        let uptime_s = info.uptime;

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
    } else {
        puts(b"monitor: failed to query telemetry\0".as_ptr());
    }
}

pub unsafe fn run_async_demo() {
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

pub unsafe fn handle_snapshot_command(argc: usize, argv: &[*const u8; 16]) {
    let label = if argc > 1 && !argv[1].is_null() {
        argv[1]
    } else {
        b"user_manual\0".as_ptr()
    };
    let snap_id = syscall::syscall2(SYS_AUDIT_SNAPSHOT, label as usize, 0) as isize;
    if snap_id > 0 {
        printf(
            b"System State Snapshot #%d Created successfully (label: %s)\n\0".as_ptr(),
            snap_id as i32,
            label,
        );
    } else {
        printf(
            b"snapshot: failed to capture system snapshot (errno: %d)\n\0".as_ptr(),
            -snap_id as i32,
        );
    }
}

pub unsafe fn handle_id() {
    let uid = getuid();
    let gid = getgid();
    let euid = geteuid();
    let egid = getegid();
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

pub unsafe fn handle_clip(argc: usize, argv: &[*const u8; 16]) {
    if argc > 1 && !argv[1].is_null() {
        // SAFETY: Checking for -h / --help or -p / --paste flags.
        if strcmp(argv[1], b"-h\0".as_ptr()) == 0 || strcmp(argv[1], b"--help\0".as_ptr()) == 0 {
            puts(b"Usage: clip <text> | <cmd> | clip | clip -p\nSyncs text into in-memory ring and emits ANSI OSC 52 to host clipboard.\0".as_ptr());
            return;
        }
        if strcmp(argv[1], b"-p\0".as_ptr()) == 0 || strcmp(argv[1], b"--paste\0".as_ptr()) == 0 {
            // SAFETY: Read active in-memory clipboard buffer.
            let kr = &raw const crate::editor::KILL_RING;
            let bytes = (*kr).as_bytes();
            if bytes.is_empty() {
                puts(b"Clipboard is empty.\0".as_ptr());
            } else {
                write(STDOUT_FILENO, bytes.as_ptr(), bytes.len());
                puts(b"\0".as_ptr());
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
            let len = strlen(arg);
            if offset + len + 1 < buf.len() {
                if offset > 0 {
                    buf[offset] = b' ';
                    offset += 1;
                }
                // SAFETY: Copying valid bytes from command argument into stack buffer.
                buf[offset..offset + len].copy_from_slice(core::slice::from_raw_parts(arg, len));
                offset += len;
            }
        }
        // SAFETY: Single-threaded shell mutation of global kill-ring buffer and OSC 52 sync.
        let kr = &raw mut crate::editor::KILL_RING;
        (*kr).save(&buf[..offset]);
        crate::editor::osc52_copy(&buf[..offset]);
        puts(b"Copied to clipboard.\0".as_ptr());
    } else {
        let mut buf = [0u8; 1024];
        let mut total = 0;
        loop {
            // SAFETY: Reading piped stream from stdin until EOF or capacity.
            let n = read(STDIN_FILENO, buf.as_mut_ptr().add(total), buf.len() - total);
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
            let kr = &raw mut crate::editor::KILL_RING;
            (*kr).save(&buf[..total]);
            crate::editor::osc52_copy(&buf[..total]);
            printf(b"Copied %d bytes to clipboard.\n\0".as_ptr(), total as i32);
        } else {
            // SAFETY: Reading active in-memory kill-ring buffer for display.
            let kr = &raw const crate::editor::KILL_RING;
            let bytes = (*kr).as_bytes();
            if bytes.is_empty() {
                puts(b"Clipboard is empty. Usage: clip <text> | <cmd> | clip\0".as_ptr());
            } else {
                write(STDOUT_FILENO, bytes.as_ptr(), bytes.len());
                puts(b"\0".as_ptr());
            }
        }
    }
}
