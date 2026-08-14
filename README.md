# rust-posix-os

A 64-bit POSIX.1-oriented OS in Rust: framekernel, libc, init, shell, coreutils.
Runs under QEMU + Limine (UEFI).

## Status

Working: boot, PMM/VMM/heap, IRQ-safe spinlocks, VFS (ramfs/devfs/procfs/tar initrd),
POSIX syscalls (fs, process lifecycle except `fork`, mmap bump, epoll, signals stub,
audit), SysV `execve` stack (argc/argv/envp/auxv), ring-3 init → shell.

Not done: real `fork`/COW (`sys_fork` is `-ENOSYS`), VMA list, VFS lock inversion,
preemptive timer, SMP.

Architecture and agent rules: [`AGENTS.md`](AGENTS.md). TCB decision: [`docs/adr/0001-tcb-boundary.md`](docs/adr/0001-tcb-boundary.md).

```
userland → libc → posix-abi
kernel/services  (safe POSIX; #![deny(unsafe_code)])
kernel/ostd      (TCB: arch, mm, irq, limine, drivers)
```

## Build

Requires Rust nightly and `qemu-system-x86_64`.

```bash
cargo xtask build
cargo xtask run
cargo xtask test
```

`xtask test` must only list checks that execute. Do not add always-`[PASS]` names.

## License

MIT or Apache-2.0.
