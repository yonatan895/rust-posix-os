//! POSIX shell pipeline (`|`) parsing and I/O redirection (`>`, `>>`, `<`) execution engine.

use libc::*;
use posix_abi::*;

/// Splits a mutable line buffer in-place into null-terminated token arguments in `argv`.
///
/// Returns the number of tokens parsed.
///
/// # Safety
///
/// `line` must point to a valid mutable null-terminated byte slice.
pub unsafe fn tokenize_line(line: *mut u8, argv: &mut [*const u8; 16]) -> usize {
    let mut argc = 0;
    let mut ptr = line;
    for slot in argv.iter_mut() {
        *slot = core::ptr::null();
    }
    if ptr.is_null() {
        return 0;
    }
    // SAFETY: Caller guarantees `line` points to a valid mutable null-terminated C string.
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

/// Specifications for standard input and output redirection targets.
#[derive(Default)]
pub struct Redirection {
    /// File path for `<` input redirection, or null if stdin is inherited.
    pub stdin_file: *const u8,
    /// File path for `>` or `>>` output redirection, or null if stdout is inherited.
    pub stdout_file: *const u8,
    /// Whether output redirection is in append mode (`>>`).
    pub stdout_append: bool,
}

/// Represents an individual command execution stage within a pipeline.
pub struct Stage {
    /// Argument vector for the stage command.
    pub argv: [*const u8; 16],
    /// Number of valid arguments in `argv`.
    pub argc: usize,
    /// I/O redirection settings for this stage.
    pub redir: Redirection,
}

/// Parses a command segment into arguments and input/output redirection directives.
///
/// # Safety
///
/// `stage_str` must be a valid mutable pointer to a null-terminated C-string.
pub unsafe fn parse_stage(stage_str: *mut u8) -> Stage {
    let mut raw_argv: [*const u8; 16] = [core::ptr::null(); 16];
    // SAFETY: Tokenizes input string into argument tokens.
    let raw_argc = unsafe { tokenize_line(stage_str, &mut raw_argv) };
    let mut stage = Stage {
        argv: [core::ptr::null(); 16],
        argc: 0,
        redir: Redirection::default(),
    };
    let mut i = 0;
    while i < raw_argc {
        let token = raw_argv[i];
        if token.is_null() {
            i += 1;
            continue;
        }
        // SAFETY: Reads character bytes from token pointer.
        let (b0, b1) = unsafe {
            let b0 = *token;
            let b1 = if b0 != 0 { *token.add(1) } else { 0 };
            (b0, b1)
        };
        if b0 == b'>' && b1 == b'>' {
            // SAFETY: Checks byte after >>.
            if unsafe { *token.add(2) } != 0 {
                // SAFETY: Offsets pointer by 2 bytes.
                stage.redir.stdout_file = unsafe { token.add(2) };
            } else if i + 1 < raw_argc {
                stage.redir.stdout_file = raw_argv[i + 1];
                i += 1;
            }
            stage.redir.stdout_append = true;
        } else if b0 == b'>' {
            // SAFETY: Checks byte after >.
            if unsafe { *token.add(1) } != 0 {
                // SAFETY: Offsets pointer by 1 byte.
                stage.redir.stdout_file = unsafe { token.add(1) };
            } else if i + 1 < raw_argc {
                stage.redir.stdout_file = raw_argv[i + 1];
                i += 1;
            }
            stage.redir.stdout_append = false;
        } else if b0 == b'<' {
            // SAFETY: Checks byte after <.
            if unsafe { *token.add(1) } != 0 {
                // SAFETY: Offsets pointer by 1 byte.
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

/// Parses and executes a pipeline containing multiple stages linked by pipes (`|`) and redirections.
///
/// # Safety
///
/// `line` must point to a valid mutable null-terminated C-string.
pub unsafe fn execute_pipeline_line(
    line: *mut u8,
    execute_cmd_fn: impl Fn(usize, &[*const u8; 16]),
) {
    let mut stages_str: [*mut u8; 8] = [core::ptr::null_mut(); 8];
    let mut num_stages = 0;
    let mut ptr = line;
    // SAFETY: Parses pipeline tokens separated by '|'.
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
        // SAFETY: Parses single pipeline stage.
        let stage = unsafe { parse_stage(stages_str[0]) };
        if stage.argc == 0 {
            return;
        }
        // SAFETY: Duplicates original stdin and stdout descriptors.
        let orig_in = unsafe { dup(STDIN_FILENO) };
        let orig_out = unsafe { dup(STDOUT_FILENO) };
        let mut ok = true;
        if !stage.redir.stdin_file.is_null() {
            // SAFETY: Opens redirected input file.
            let fd = unsafe { open(stage.redir.stdin_file, O_RDONLY, 0) };
            if fd >= 0 {
                // SAFETY: Duplicates input file onto STDIN_FILENO and closes raw fd.
                unsafe {
                    dup2(fd, STDIN_FILENO);
                    close(fd);
                }
            } else {
                // SAFETY: Outputs error diagnostic message.
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
            // SAFETY: Opens redirected output file.
            let fd = unsafe { open(stage.redir.stdout_file, flags, 0o644) };
            if fd >= 0 {
                // SAFETY: Duplicates output file onto STDOUT_FILENO and closes raw fd.
                unsafe {
                    dup2(fd, STDOUT_FILENO);
                    close(fd);
                }
            } else {
                // SAFETY: Outputs error diagnostic message.
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
        // SAFETY: Restores original stdin and stdout descriptors.
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
        // SAFETY: Creates anonymous pipes between adjacent pipeline stages.
        if unsafe { pipe(&mut pipes[i] as *mut [i32; 2]) } < 0 {
            // SAFETY: Reports pipe failure to stdout.
            unsafe { puts(b"shell: pipe creation failed\0".as_ptr()) };
            return;
        }
    }

    // SAFETY: Duplicates original stdin and stdout descriptors before pipeline execution.
    let orig_in = unsafe { dup(STDIN_FILENO) };
    let orig_out = unsafe { dup(STDOUT_FILENO) };

    for i in 0..num_stages {
        // SAFETY: Parses stage command string.
        let stage = unsafe { parse_stage(stages_str[i]) };
        if stage.argc == 0 {
            continue;
        }
        if i > 0 {
            // SAFETY: Connects stdin of current stage to read end of previous pipe.
            unsafe { dup2(pipes[i - 1][0], STDIN_FILENO) };
        } else if !stage.redir.stdin_file.is_null() {
            // SAFETY: Opens redirected input file for first stage.
            let fd = unsafe { open(stage.redir.stdin_file, O_RDONLY, 0) };
            if fd >= 0 {
                // SAFETY: Duplicates input file onto STDIN_FILENO and closes fd.
                unsafe {
                    dup2(fd, STDIN_FILENO);
                    close(fd);
                }
            }
        }
        if i < num_stages - 1 {
            // SAFETY: Connects stdout of current stage to write end of pipe.
            unsafe { dup2(pipes[i][1], STDOUT_FILENO) };
        } else if !stage.redir.stdout_file.is_null() {
            let flags = O_WRONLY
                | O_CREAT
                | if stage.redir.stdout_append {
                    O_APPEND
                } else {
                    O_TRUNC
                };
            // SAFETY: Opens redirected output file for final stage.
            let fd = unsafe { open(stage.redir.stdout_file, flags, 0o644) };
            if fd >= 0 {
                // SAFETY: Duplicates output file onto STDOUT_FILENO and closes fd.
                unsafe {
                    dup2(fd, STDOUT_FILENO);
                    close(fd);
                }
            }
        }
        for p in 0..(num_stages - 1) {
            // SAFETY: Closes pipe descriptors in child process context.
            unsafe {
                close(pipes[p][0]);
                close(pipes[p][1]);
            }
        }
        execute_cmd_fn(stage.argc, &stage.argv);
        // SAFETY: Restores original stdin/stdout file descriptors.
        unsafe {
            dup2(orig_in, STDIN_FILENO);
            dup2(orig_out, STDOUT_FILENO);
        }
    }
    // SAFETY: Closes saved original file descriptors.
    unsafe {
        close(orig_in);
        close(orig_out);
    }
}
