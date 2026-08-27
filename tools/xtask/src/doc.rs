//! Automated documentation coverage verification command (`cargo xtask doc`).

use std::fs;
use std::path::Path;
use std::process::Command;

/// Runs documentation builds across all workspace crates ensuring 100% doc coverage without warnings.
///
/// Builds a unified documentation tree with a central index portal linking all 7 workspace crates:
/// `kernel`, `posix_abi`, `libc`, `shell`, `coreutils`, `init`, and `xtask`.
///
/// If `open` is true, opens the generated unified documentation index in the default web browser.
pub fn run_doc(open: bool) {
    println!("[xtask] Verifying documentation coverage (-D missing-docs -D warnings)...");

    // 1. Target Crates: posix-abi, mm-core, libc, kernel, init, shell, coreutils
    println!(
        "[xtask] Building unified docs for target crates (posix-abi, mm-core, libc, kernel, init, shell, coreutils)..."
    );
    let status = Command::new("cargo")
        .args([
            "doc",
            "--no-deps",
            "-p",
            "posix-abi",
            "-p",
            "mm-core",
            "-p",
            "libc",
            "-p",
            "kernel",
            "-p",
            "init",
            "-p",
            "shell",
            "-p",
            "coreutils",
            "--target",
            "x86_64-unknown-none",
            "-Zbuild-std=core,compiler_builtins,alloc",
            "-Zbuild-std-features=compiler-builtins-mem",
        ])
        .env("RUSTDOCFLAGS", "-D missing-docs -D warnings")
        .env(
            "RUSTFLAGS",
            "-C relocation-model=static -C code-model=kernel",
        )
        .status()
        .expect("Failed to execute cargo doc on target crates");
    if !status.success() {
        eprintln!("[xtask] Documentation build failed for target crates.");
        std::process::exit(1);
    }

    // 2. Host Crates: xtask
    println!("[xtask] Building docs for host tooling (xtask)...");
    let status = Command::new("cargo")
        .args(["doc", "--no-deps", "-p", "xtask"])
        .env("RUSTDOCFLAGS", "-D missing-docs -D warnings")
        .status()
        .expect("Failed to execute cargo doc on xtask");
    if !status.success() {
        eprintln!("[xtask] Documentation build failed for xtask.");
        std::process::exit(1);
    }

    // Copy xtask docs into target doc tree if available
    let xtask_doc_src = Path::new("target/doc/xtask");
    let xtask_doc_dst = Path::new("target/x86_64-unknown-none/doc/xtask");
    if xtask_doc_src.exists() {
        let _ = copy_dir_all(xtask_doc_src, xtask_doc_dst);
    }

    // 3. Generate Central Workspace Documentation Hub Landing Page
    generate_doc_portal();

    println!("[xtask] 100% Documentation coverage verified successfully across all crates!");

    let portal_path = Path::new("target/x86_64-unknown-none/doc/index.html");
    println!(
        "[xtask] Complete unified HTML documentation is available at: {}",
        portal_path.display()
    );

    if open && portal_path.exists() {
        println!(
            "[xtask] Opening {} in your default web browser...",
            portal_path.display()
        );
        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("cmd")
                .args(["/C", "start", "", portal_path.to_str().unwrap()])
                .spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg(portal_path).spawn();
        }
        #[cfg(target_os = "linux")]
        {
            let _ = Command::new("xdg-open").arg(portal_path).spawn();
        }
    }
}

/// Recursively copies a directory tree.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), dest_path)?;
        }
    }
    Ok(())
}

/// Generates a central documentation portal HTML index page linking all workspace crates.
fn generate_doc_portal() {
    let portal_html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Rust POSIX OS - Workspace Documentation Hub</title>
    <style>
        :root {
            --bg-primary: #14161b;
            --bg-card: #1e222b;
            --bg-card-hover: #262c38;
            --border: #2e3644;
            --text-primary: #e6edf3;
            --text-secondary: #8b949e;
            --accent: #58a6ff;
            --accent-glow: rgba(88, 166, 255, 0.15);
            --tag-kernel: #f85149;
            --tag-abi: #d29922;
            --tag-libc: #3fb950;
            --tag-userland: #a371f7;
            --tag-tools: #58a6ff;
        }
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            background-color: var(--bg-primary);
            color: var(--text-primary);
            line-height: 1.6;
            padding: 2.5rem 1.5rem;
        }
        .container {
            max-width: 1080px;
            margin: 0 auto;
        }
        header {
            margin-bottom: 2.5rem;
            border-bottom: 1px solid var(--border);
            padding-bottom: 1.5rem;
        }
        h1 {
            font-size: 2.25rem;
            font-weight: 700;
            color: #ffffff;
            margin-bottom: 0.5rem;
            display: flex;
            align-items: center;
            gap: 0.75rem;
        }
        .subtitle {
            font-size: 1.1rem;
            color: var(--text-secondary);
        }
        .grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
            gap: 1.5rem;
            margin-bottom: 2.5rem;
        }
        .card {
            background-color: var(--bg-card);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 1.5rem;
            text-decoration: none;
            color: inherit;
            transition: all 0.2s ease-in-out;
            display: flex;
            flex-direction: column;
            justify-content: space-between;
        }
        .card:hover {
            background-color: var(--bg-card-hover);
            border-color: var(--accent);
            transform: translateY(-2px);
            box-shadow: 0 8px 24px var(--accent-glow);
        }
        .card-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 0.75rem;
        }
        .card-title {
            font-size: 1.25rem;
            font-weight: 600;
            color: #ffffff;
        }
        .badge {
            font-size: 0.75rem;
            font-weight: 600;
            padding: 0.25rem 0.6rem;
            border-radius: 12px;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }
        .badge-kernel { background: rgba(248, 81, 73, 0.15); color: var(--tag-kernel); border: 1px solid rgba(248, 81, 73, 0.3); }
        .badge-abi { background: rgba(210, 153, 34, 0.15); color: var(--tag-abi); border: 1px solid rgba(210, 153, 34, 0.3); }
        .badge-libc { background: rgba(63, 185, 80, 0.15); color: var(--tag-libc); border: 1px solid rgba(63, 185, 80, 0.3); }
        .badge-userland { background: rgba(163, 113, 247, 0.15); color: var(--tag-userland); border: 1px solid rgba(163, 113, 247, 0.3); }
        .badge-tools { background: rgba(88, 166, 255, 0.15); color: var(--tag-tools); border: 1px solid rgba(88, 166, 255, 0.3); }
        .card-desc {
            color: var(--text-secondary);
            font-size: 0.95rem;
            margin-bottom: 1.25rem;
            flex-grow: 1;
        }
        .card-footer {
            font-size: 0.85rem;
            color: var(--accent);
            font-weight: 500;
            display: flex;
            align-items: center;
            gap: 0.3rem;
        }
        footer {
            text-align: center;
            font-size: 0.9rem;
            color: var(--text-secondary);
            border-top: 1px solid var(--border);
            padding-top: 1.5rem;
        }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <h1>🦀 Rust POSIX OS Documentation Hub</h1>
            <p class="subtitle">Complete, 100% covered technical documentation for all workspace crates and subsystems.</p>
        </header>

        <div class="grid">
            <a href="kernel/index.html" class="card">
                <div>
                    <div class="card-header">
                        <span class="card-title">kernel</span>
                        <span class="badge badge-kernel">Core OS / TCB</span>
                    </div>
                    <p class="card-desc">Privileged OS Framework (ostd) with architecture abstraction (x86_64, aarch64, riscv64), PMM, VMM, and de-privileged safe POSIX services (VFS, processes, IPC, signals, scheduler).</p>
                </div>
                <div class="card-footer">View kernel docs &rarr;</div>
            </a>

            <a href="posix_abi/index.html" class="card">
                <div>
                    <div class="card-header">
                        <span class="card-title">posix-abi</span>
                        <span class="badge badge-abi">Architecture ABI</span>
                    </div>
                    <p class="card-desc">Portable POSIX ABI definitions, system call numbers, standard structures (Stat, Timespec, Sysinfo, Termios, Dirent64), bitflags, and errno error codes.</p>
                </div>
                <div class="card-footer">View posix-abi docs &rarr;</div>
            </a>

            <a href="mm_core/index.html" class="card">
                <div>
                    <div class="card-header">
                        <span class="card-title">mm-core</span>
                        <span class="badge badge-abi">Memory Core</span>
                    </div>
                    <p class="card-desc">Pure, host-testable memory allocation and virtual memory mapping state machine with FrameAllocator/PageMapper abstractions and atomic rollback.</p>
                </div>
                <div class="card-footer">View mm-core docs &rarr;</div>
            </a>

            <a href="libc/index.html" class="card">
                <div>
                    <div class="card-header">
                        <span class="card-title">libc</span>
                        <span class="badge badge-libc">Standard Library</span>
                    </div>
                    <p class="card-desc">Freestanding C-compatible standard library with small-object slab/mmap allocator, POSIX system call bindings, stdio, string manipulation, and signal handlers.</p>
                </div>
                <div class="card-footer">View libc docs &rarr;</div>
            </a>

            <a href="shell/index.html" class="card">
                <div>
                    <div class="card-header">
                        <span class="card-title">shell</span>
                        <span class="badge badge-userland">Userland</span>
                    </div>
                    <p class="card-desc">Interactive POSIX shell with multi-stage pipelines, I/O redirection, ANSI escape sequences, bracketed paste, kill ring, line history, and fuzzy tab completion.</p>
                </div>
                <div class="card-footer">View shell docs &rarr;</div>
            </a>

            <a href="coreutils/index.html" class="card">
                <div>
                    <div class="card-header">
                        <span class="card-title">coreutils</span>
                        <span class="badge badge-userland">Userland</span>
                    </div>
                    <p class="card-desc">Multi-call userland binary containing standard POSIX core utilities: ls, cat, echo, uname, pwd, touch, mkdir, rm, cp, and mv.</p>
                </div>
                <div class="card-footer">View coreutils docs &rarr;</div>
            </a>

            <a href="init/index.html" class="card">
                <div>
                    <div class="card-header">
                        <span class="card-title">init</span>
                        <span class="badge badge-userland">Userland</span>
                    </div>
                    <p class="card-desc">Process ID 1 bootstrap environment, system initialization, initramfs mounting, in-guest test harness, and CPU TSC microbenchmark runner.</p>
                </div>
                <div class="card-footer">View init docs &rarr;</div>
            </a>

            <a href="xtask/index.html" class="card">
                <div>
                    <div class="card-header">
                        <span class="card-title">xtask</span>
                        <span class="badge badge-tools">Automation</span>
                    </div>
                    <p class="card-desc">Workspace task runner providing build automation, QEMU UEFI boot staging, initramfs packaging, syscall dispatcher benchmarking, and 20 automated domain test suites.</p>
                </div>
                <div class="card-footer">View xtask docs &rarr;</div>
            </a>
        </div>

        <footer>
            Rust POSIX OS &bull; Built with Rust nightly &bull; 100% Documentation Coverage Verified
        </footer>
    </div>
</body>
</html>
"#;

    let target_doc_dir = Path::new("target/x86_64-unknown-none/doc");
    if target_doc_dir.exists() {
        let _ = fs::write(target_doc_dir.join("index.html"), portal_html);
    }
    let host_doc_dir = Path::new("target/doc");
    if host_doc_dir.exists() {
        let _ = fs::write(host_doc_dir.join("index.html"), portal_html);
    }
}
