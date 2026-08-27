# rust-posix-os

A 64-bit POSIX.1-oriented OS kernel written in Rust.
Framekernel design: a safe `services` layer over a minimal Trusted Computing Base (`ostd`).
Runs under QEMU + Limine (UEFI) on x86-64.

> **Syscall ABI:** Linux x86-64 syscall numbers; POSIX.1-2024 subset.
> **Not** a full Linux clone — see the [implemented syscall table](#implemented-syscalls) below.

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    userland                         │
│        init · shell · coreutils                     │
├─────────────────────────────────────────────────────┤
│                  libs/libc                          │
│        syscall stubs (read, write, fork, …)         │
├─────────────────────────────────────────────────────┤
│                libs/posix-abi                       │
│        syscall numbers, errno, repr(C) types        │
├─────────────────────────────────────────────────────┤
│             kernel/src/services        ← safe Rust  │
│   #![deny(unsafe_code)]                             │
│   vfs · process · scheduler · ipc · posix/*         │
├─────────────────────────────────────────────────────┤
│              kernel/src/ostd           ←  TCB only  │
│   arch · mm (pmm/vmm/heap/user) · irq               │
│   task · sync · limine · drivers                    │
└─────────────────────────────────────────────────────┘
```

**The TCB boundary is the central invariant.**
`ostd` is the only layer allowed to contain `unsafe` code, touch hardware registers (CR3, MSRs, RFLAGS), or dereference raw user pointers.
`services` is pure safe Rust — `#![deny(unsafe_code)]` — and enforces POSIX policy above the `ostd` API.
This boundary is enforced in CI by [`tcb.yml`](.github/workflows/tcb.yml).
The rationale and rules live in [`docs/adr/0001-tcb-boundary.md`](docs/adr/0001-tcb-boundary.md).

---

## Boot Flow

```
1. UEFI firmware loads Limine bootloader
2. Limine maps the kernel ELF, sets up HHDM, passes boot info
3. ostd::limine  →  parse memory map, init PMM (physical frame allocator)
4. ostd::mm      →  init VMM (kernel page tables), heap allocator
5. ostd::arch    →  load GDT/TSS, init IDT, wire syscall MSRs, start PIT timer
6. services::vfs →  mount ramfs (/), devfs (/dev), procfs (/proc); unpack tar initrd
7. services::process → create PID 0 (idle), PID 1 (init)
8. ostd::task    →  enter_user_mode → ring-3 init process
9. init          →  execve shell; reaps zombies in a waitpid loop
10. shell        →  interactive prompt; forks/execs coreutils builtins
```

---

## Syscall Dispatch Path

```
user syscall instruction
  → ostd::arch::x86_64::syscall  (naked asm trampoline; saves registers)
  → with_syscall_regs()          (safe wrapper, constructs SyscallRegisters)
  → services::posix::dispatch_syscall(rax, regs)
  → sys_read / sys_write / sys_fork / …
  → check_and_deliver_signals()  (on return to ring-3: deliver pending signals)
  → sysretq
```

Raw `*const u8` / `*mut u8` arguments from the trampoline die at the dispatcher boundary.
All user-memory access goes through `ostd::mm::{UserPtr<T>, UserSlice}` — never a raw dereference in `services`.

---

## Implemented Syscalls

All syscall numbers follow the Linux x86-64 ABI for compatibility with musl/glibc-built toolchains.

| # | Syscall | Status |
|---|---------|--------|
| 0 | `read` | ✅ |
| 1 | `write` | ✅ |
| 2 | `open` | ✅ |
| 3 | `close` | ✅ |
| 4 | `stat` | ✅ |
| 5 | `fstat` | ✅ |
| 8 | `lseek` | ✅ |
| 9 | `mmap` | ✅ bump allocator; no VMA rollback yet (see [#90](https://github.com/yonatan895/rust-posix-os/issues/90)) |
| 10 | `mprotect` | ✅ |
| 11 | `munmap` | ✅ |
| 12 | `brk` | ✅ |
| 13 | `rt_sigaction` | ✅ |
| 14 | `rt_sigprocmask` | ✅ |
| 15 | `rt_sigreturn` | ✅ |
| 16 | `ioctl` | ✅ partial (tty/serial) |
| 22 | `pipe` | ✅ blocking |
| 32 | `dup` | ✅ |
| 33 | `dup2` | ✅ |
| 35 | `nanosleep` | ✅ |
| 39 | `getpid` | ✅ |
| 57 | `fork` | ✅ eager clone (no COW yet — [#72](https://github.com/yonatan895/rust-posix-os/issues/72)) |
| 59 | `execve` | ✅ SysV stack (argc/argv/envp/auxv) |
| 60 | `exit` | ✅ |
| 61 | `wait4` | ✅ WNOHANG; blocking wait |
| 62 | `kill` | ✅ signals 1–31 |
| 63 | `uname` | ✅ |
| 79 | `getcwd` | ✅ |
| 80 | `chdir` | ✅ |
| 82 | `rename` | ✅ atomic with cycle prevention |
| 83 | `mkdir` | ✅ |
| 84 | `rmdir` | ✅ |
| 87 | `unlink` | ✅ |
| 95 | `umask` | ✅ |
| 99 | `sysinfo` | ✅ |
| 102 | `getuid` | ✅ |
| 104 | `getgid` | ✅ |
| 105 | `setuid` | ✅ POSIX.1-2017 with saved-uid |
| 106 | `setgid` | ✅ POSIX.1-2017 with saved-gid |
| 107 | `geteuid` | ✅ |
| 108 | `getegid` | ✅ |
| 110 | `getppid` | ✅ |
| 115 | `seteuid` | ✅ |
| 116 | `setegid` | ✅ |
| 117 | `setresuid` | ✅ |
| 118 | `getresuid` | ✅ |
| 119 | `setresgid` | ✅ |
| 120 | `getresgid` | ✅ |
| 213 | `epoll_create` | ✅ |
| 217 | `getdents64` | ✅ |
| 228 | `clock_gettime` | ✅ |
| 232 | `epoll_wait` | ✅ |
| 233 | `epoll_ctl` | ✅ |
| 291 | `epoll_create1` | ✅ |
| 293 | `pipe2` | ✅ |
| 301 | `audit_log` | ✅ kernel audit subsystem |
| 302 | `audit_snapshot` | ✅ |

**Not yet implemented:** `fork`/COW page tables, VMA list (`munmap` validation), persistent filesystem, SMP, APIC timer, networking.

---

## VFS Layout

At boot the following mounts are live:

```
/           ramfs       (read-write in-memory root)
/dev        devfs       (null, zero, tty, urandom)
/proc       procfs      (uptime, meminfo, self/*, [pid]/*)
initrd      tar         (packed at build time; unpacked into /)
```

Pipes and epoll instances are anonymous inodes — they live in the fd table, not the directory tree.

---

## Key Design Documents

| Document | Covers |
|---|---|
| [`AGENTS.md`](AGENTS.md) | Contribution rules, PR bar, bug patterns, checklist |
| [`docs/adr/0001-tcb-boundary.md`](docs/adr/0001-tcb-boundary.md) | TCB split: what is and isn't allowed in `ostd` vs `services` |
| [`docs/adr/0002-locking.md`](docs/adr/0002-locking.md) | Lock hierarchy, IRQ discipline, no-user-memory-under-spinlock |
| [`docs/adr/0003-task-model.md`](docs/adr/0003-task-model.md) | 1:1 process model, kernel stack layout, preemptive scheduling |
| [`docs/adr/0004-arch-abstraction.md`](docs/adr/0004-arch-abstraction.md) | x86-64 abstraction boundary; aarch64/riscv64 readiness |

---

## Build & Run

**Prerequisites:** Rust nightly, `qemu-system-x86_64`, `ovmf` (UEFI firmware).

```bash
# Install Rust nightly with required components
rustup toolchain install nightly
rustup component add rust-src

# Build the kernel + UEFI boot image
cargo xtask build

# Boot in QEMU (opens a serial console)
cargo xtask run

# Run the host-side test suite
cargo xtask test
```

The serial console is the primary output channel. The shell banner (`Rust POSIX Shell`) indicates a successful boot.

---

## How to Add a Syscall

1. **Add the number** to `libs/posix-abi/src/syscalls.rs` (`SYS_FOO: usize = N`).
2. **Add the C stub** to `libs/libc/src/` (a thin `unsafe extern` wrapper calling the asm trampoline).
3. **Implement `sys_foo()`** in `kernel/src/services/posix/` — safe Rust only, no `unsafe`.
4. **Wire the dispatcher** in `kernel/src/services/posix/mod.rs` (`dispatch_syscall` match arm).
5. **Write a test:** either a `#[cfg(test)]` unit test in `posix-abi`, or a QEMU serial-assertion test in `tools/xtask/src/test.rs`.
6. **Check the PR bar** in [`AGENTS.md`](AGENTS.md): one concern, named invariant, both valid and invalid paths tested.

See [`docs/adr/0001-tcb-boundary.md`](docs/adr/0001-tcb-boundary.md) for the full list of rules governing the TCB/services split.

---

## Repository Layout

```
kernel/src/
  ostd/         # TCB: arch, mm, irq, task, sync, limine, drivers
  services/     # Safe POSIX: vfs, process, scheduler, ipc, posix/*
libs/
  libc/         # Userland C-compatible syscall wrappers
  posix-abi/    # Syscall numbers, errno codes, repr(C) types
userland/
  init/         # PID 1: supervisor, waitpid reaper
  shell/        # Interactive POSIX shell
  coreutils/    # echo, ls, cat, cp, mv, rm, …
tools/
  xtask/        # Build system: build, run, test, bench
docs/adr/       # Architecture Decision Records (0001–0004)
.github/
  workflows/    # ci-cd, quality, tcb, publish-nightly
```

---

## License

MIT or Apache-2.0.
