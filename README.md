# Rust POSIX Operating System (Framekernel Model)

An advanced, IEEE POSIX.1-2024 conformant 64-bit operating system kernel, C library (`libc`), init daemon, and interactive shell developed from scratch in pure safe/unsafe Rust.

---

## 🌟 Key Architecture & Features

### 1. Core Kernel & OSTD Services
- **Target**: `x86_64` Bare-Metal with UEFI Boot (Limine Boot Protocol v8).
- **Physical Memory Manager (PMM)**: Bitmap-based 4KiB page frame allocation.
- **Virtual Memory Manager (VMM)**: 4-level paging (PML4, PDPT, PD, PT) with Higher-Half Direct Mapping (HHDM).
- **Global Kernel Heap**: 16MiB dynamic memory allocator backed by spinlocks.
- **Syscall Dispatcher**: Fast system call interface via `syscall` / `sysret` MSRs (`LSTAR`, `STAR`, `FMASK`).
- **Async Runtime**: Custom async executor, wakers, and futures multiplexing kernel tasks.

### 2. POSIX Subsystems & File Systems
- **VFS Layer**: Inode abstraction with full support for directories, regular files, pipes, and device nodes.
- **RamFS & DevFS**: In-memory root hierarchy and device nodes (`/dev/console`, `/dev/null`, `/dev/zero`).
- **ProcFS Telemetry**: Real-time system monitoring via `/proc/meminfo`, `/proc/processes`, `/proc/stat`, `/proc/uptime`, `/proc/audit_journal`, and `/proc/snapshots`.
- **Epoll Multiplexing**: Non-blocking I/O event multiplexer (`epoll_create1`, `epoll_ctl`, `epoll_wait`).
- **Pipelining & Redirections**: Full shell support for chained pipelines (`|`) and file redirections (`>`, `>>`, `<`).

### 3. Security Audit & System Snapshots
- **Audit Event Journal**: Append-only security logging subsystem with timestamps and process telemetry.
- **State Snapshots**: Kernel snapshot engine to capture and inspect system state checkpoints (`snapshot create <label>`, `snapshot list`).

### 4. Interactive Userspace Shell & CLI
- **Flicker-Free Line Editor**: In-place single-syscall delta renderer with zero screen tearing or jitter.
- **Real-Time Dynamic Syntax Highlighting**: Valid commands highlighted in **ANSI Green** (`\x1b[32m`), invalid/unknown commands in **ANSI Red** (`\x1b[31m`).
- **Interactive Fuzzy Tab Menu**:
  - Subsequence-based fuzzy matching.
  - Arrow key navigation (`Left`/`Right`/`Up`/`Down`) across candidate choices.
  - Active item highlighted in inverted bold green (`\x1b[7;1;32m [ > cmd < ] \x1b[0m`).
- **1000-Command In-Memory History**:
  - Ring buffer storing up to 1000 commands with consecutive deduplication.
  - `Up Arrow` / `Down Arrow` traversal with draft preservation and instant syntax highlighting.
- **Extended Command Parameters**:
  - `ls`: `-l` (long format), `-a` (show hidden files), `-la`, `-al`, `-h` (human-readable sizes).
  - `rm`: `-r` / `-R` (recursive directory deletion), `-f` (force ignore missing), `-rf`.
  - `cd`: `cd -` (jump to previous `$OLDPWD` and print path), `cd` / `cd ~` (jump to `/`), `cd ..`.
  - `touch`: `-c` / `--no-create`, multiple target files.
  - `mkdir`: `-p` / `--parents` (recursive creation), multiple directory targets.
  - `cat`: `-n` (1-based line numbering), multiple files, and stdin streams.
  - `echo`: `-n` (omit newline), `-e` (interpret `\n`, `\t`, `\r`, `\\`).

---

## 🚀 Building & Running

### Prerequisites
- **Rust Nightly**: `rustup default nightly`
- **QEMU**: `qemu-system-x86_64`

### Build Workspace
```bash
cargo xtask build
```

### Run in QEMU
```bash
cargo xtask run
```

### Run Automated Tests (13 Test Suites)
```bash
cargo xtask test
```

---

## 📁 Repository Structure
```
rust-posix-os/
├── kernel/              # Core OS kernel (OSTD, PMM, VMM, VFS, Syscalls, Async, Audit)
├── libs/
│   ├── libc/            # Standard C library implementation (POSIX compliant)
│   └── posix-abi/       # Shared system call numbers, constants, structures
├── userland/
│   ├── init/            # PID 1 Init Daemon
│   ├── shell/           # Interactive POSIX Shell with syntax highlighting & history
│   └── coreutils/       # Userland binaries & helper utilities
└── tools/
    └── xtask/           # Build automation, disk imaging, testing & QEMU runner
```

---

## 📜 License
Dual-licensed under MIT or Apache 2.0.
