//! POSIX Shell Pipeline (|) & I/O Redirection (>, >>, <) Execution.

use libc::*;
use posix_abi::*;

pub unsafe fn tokenize_line(line: *mut u8, argv: &mut [*const u8; 16]) -> usize {
    let mut argc = 0;
    let mut ptr = line;
    for slot in argv.iter_mut() {
        *slot = core::ptr::null();
    }
    if ptr.is_null() {
        return 0;
    }
    unsafe {
        while *ptr != 0 && argc < 16 {
            while *ptr != 0 && (*ptr == b' ' || *ptr == b'\t' || *ptr == b'\r' || *ptr == b'\n') {
                *ptr = 0;
                ptr = ptr.add(1);
            }
            if *ptr == 0 {
                break;
            }
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
    }
    argc
}

#[derive(Default)]
pub struct Redirection {
    pub stdin_file: *const u8,
    pub stdout_file: *const u8,
    pub stdout_append: bool,
}

pub struct Stage {
    pub argv: [*const u8; 16],
    pub argc: usize,
    pub redir: Redirection,
}

pub unsafe fn parse_stage(stage_str: *mut u8) -> Stage {
    let mut raw_argv: [*const u8; 16] = [core::ptr::null(); 16];
    let raw_argc = unsafe { tokenize_line(stage_str, &mut raw_argv) };
    let mut stage = Stage {
        argv: [core::ptr::null(); 16],
        argc: 0,
        redir: Redirection::default(),
    };
    let mut i = 0;
    while i < raw_argc {
        let token = raw_argv[i];
        let (b0, b1) = unsafe {
            let b0 = *token;
            let b1 = if b0 != 0 { *token.add(1) } else { 0 };
            (b0, b1)
        };
        if b0 == b'>' && b1 == b'>' {
            if unsafe { *token.add(2) } != 0 {
                stage.redir.stdout_file = unsafe { token.add(2) };
            } else if i + 1 < raw_argc {
                stage.redir.stdout_file = raw_argv[i + 1];
                i += 1;
            }
            stage.redir.stdout_append = true;
        } else if b0 == b'>' {
            if unsafe { *token.add(1) } != 0 {
                stage.redir.stdout_file = unsafe { token.add(1) };
            } else if i + 1 < raw_argc {
                stage.redir.stdout_file = raw_argv[i + 1];
                i += 1;
            }
            stage.redir.stdout_append = false;
        } else if b0 == b'<' {
            if unsafe { *token.add(1) } != 0 {
                stage.redir.stdin_file = unsafe { token.add(1) };
            } else if i + 1 < raw_argc {
                stage.redir.stdin_file = raw_argv[i + 1];
                i += 1;
            }
        } else if stage.argc < 16 {
            stage.argv[stage.argc] = token;
            stage.argc += 1;
        }
        i += 1;
    }
    stage
}

pub unsafe fn execute_pipeline_line(
    line: *mut u8,
    execute_cmd_fn: impl Fn(usize, &[*const u8; 16]),
) {
    let mut stages_str: [*mut u8; 8] = [core::ptr::null_mut(); 8];
    let mut num_stages = 0;
    let mut ptr = line;
    unsafe {
        while *ptr != 0 && num_stages < 8 {
            while *ptr == b' ' || *ptr == b'\t' {
                ptr = ptr.add(1);
            }
            if *ptr == 0 {
                break;
            }
            stages_str[num_stages] = ptr;
            num_stages += 1;
            while *ptr != 0 && *ptr != b'|' {
                ptr = ptr.add(1);
            }
            if *ptr == b'|' {
                *ptr = 0;
                ptr = ptr.add(1);
            }
        }
    }
    if num_stages == 0 {
        return;
    }

    if num_stages == 1 {
        let stage = unsafe { parse_stage(stages_str[0]) };
        if stage.argc == 0 {
            return;
        }
        let orig_in = unsafe { dup(STDIN_FILENO) };
        let orig_out = unsafe { dup(STDOUT_FILENO) };
        let mut ok = true;
        if !stage.redir.stdin_file.is_null() {
            let fd = unsafe { open(stage.redir.stdin_file, O_RDONLY, 0) };
            if fd >= 0 {
                unsafe {
                    dup2(fd, STDIN_FILENO);
                    close(fd);
                }
            } else {
                unsafe {
                    printf(
                        b"shell: cannot open '%s' for input\n\0".as_ptr(),
                        stage.redir.stdin_file,
                    );
                }
                ok = false;
            }
        }
        if ok && !stage.redir.stdout_file.is_null() {
            let flags = O_WRONLY
                | O_CREAT
                | if stage.redir.stdout_append {
                    O_APPEND
                } else {
                    O_TRUNC
                };
            let fd = unsafe { open(stage.redir.stdout_file, flags, 0o644) };
            if fd >= 0 {
                unsafe {
                    dup2(fd, STDOUT_FILENO);
                    close(fd);
                }
            } else {
                unsafe {
                    printf(
                        b"shell: cannot open '%s' for output\n\0".as_ptr(),
                        stage.redir.stdout_file,
                    );
                }
                ok = false;
            }
        }
        if ok {
            execute_cmd_fn(stage.argc, &stage.argv);
        }
        unsafe {
            dup2(orig_in, STDIN_FILENO);
            close(orig_in);
            dup2(orig_out, STDOUT_FILENO);
            close(orig_out);
        }
        return;
    }

    let mut pipes: [[i32; 2]; 7] = [[-1, -1]; 7];
    for i in 0..(num_stages - 1) {
        if unsafe { pipe(&mut pipes[i] as *mut [i32; 2]) } < 0 {
            unsafe { puts(b"shell: pipe creation failed\0".as_ptr()) };
            return;
        }
    }

    let orig_in = unsafe { dup(STDIN_FILENO) };
    let orig_out = unsafe { dup(STDOUT_FILENO) };

    for i in 0..num_stages {
        let stage = unsafe { parse_stage(stages_str[i]) };
        if stage.argc == 0 {
            continue;
        }
        if i > 0 {
            unsafe { dup2(pipes[i - 1][0], STDIN_FILENO) };
        } else if !stage.redir.stdin_file.is_null() {
            let fd = unsafe { open(stage.redir.stdin_file, O_RDONLY, 0) };
            if fd >= 0 {
                unsafe {
                    dup2(fd, STDIN_FILENO);
                    close(fd);
                }
            }
        }
        if i < num_stages - 1 {
            unsafe { dup2(pipes[i][1], STDOUT_FILENO) };
        } else if !stage.redir.stdout_file.is_null() {
            let flags = O_WRONLY
                | O_CREAT
                | if stage.redir.stdout_append {
                    O_APPEND
                } else {
                    O_TRUNC
                };
            let fd = unsafe { open(stage.redir.stdout_file, flags, 0o644) };
            if fd >= 0 {
                unsafe {
                    dup2(fd, STDOUT_FILENO);
                    close(fd);
                }
            }
        }
        for p in 0..(num_stages - 1) {
            unsafe {
                close(pipes[p][0]);
                close(pipes[p][1]);
            }
        }
        execute_cmd_fn(stage.argc, &stage.argv);
        unsafe {
            dup2(orig_in, STDIN_FILENO);
            dup2(orig_out, STDOUT_FILENO);
        }
    }
    unsafe {
        close(orig_in);
        close(orig_out);
    }
}
