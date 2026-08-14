# Rust POSIX OS

A 64-bit POSIX.1-2024-oriented operating system written in Rust: kernel, libc, init, and an interactive shell.

## Architecture

- **Target**: x86_64 bare metal, UEFI boot via Limine
- **Memory**: bitmap PMM, 4-level paging with HHDM, 16 MiB kernel heap
- **Syscalls**: `syscall` / `sysret` (LSTAR / STAR / FMASK)
- **VFS**: ramfs, devfs, procfs, pipes, epoll
- **Userland**: PID 1 init, POSIX-style shell with history, completion, and pipelines

## Build and run

Requires Rust nightly and `qemu-system-x86_64`.

```bash
cargo xtask build
cargo xtask run
cargo xtask test
```

## Layout

```
kernel/              # OSTD, memory, VFS, syscalls
libs/libc/           # freestanding POSIX C ABI
libs/posix-abi/      # shared syscall numbers and types
userland/init/       # PID 1
userland/shell/      # interactive shell
userland/coreutils/  # extra userland binary
tools/xtask/         # image, QEMU, and CI helpers
```

## License

MIT or Apache-2.0.
