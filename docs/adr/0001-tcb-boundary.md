# ADR-0001: The TCB Boundary (ostd vs. services)

- Status: Accepted, implemented
- Date: 2026-08-14
- Updated: 2026-08-14

## Context

The kernel is an Asterinas-style framekernel: `ostd/` is the privileged TCB;
`services/` is de-privileged POSIX policy. The split was documentation until
unsafe user-pointer derefs, `mov cr3`, ELF/tar casts, and Limine walks lived
in `services/`.

## Decision

- R1. `unsafe` only in `kernel/src/ostd/`.
- R2. User memory only via `ostd::mm::user` (`UserPtr`, `UserSlice`,
  `copy_cstr_from_user`). Errno mapping only in `services/posix/user_access.rs`.
- R3. Boot protocol only in `ostd::limine`. Services take `BootBlob`, never
  `Limine*`.
- R4. Address-space / context switch only in `ostd` (`VmSpace::activate`).
- R5. Every `unsafe` block has `// SAFETY:`.
- R6. `ostd` must not import `services`. The `#[no_mangle] extern "C"`
  syscall stub is safe and lives in `services`. Raw `*mut` deref is
  `ostd::mm::with_syscall_regs`.

## Enforcement (current)

- `.github/workflows/tcb.yml` greps `kernel/src/services/` for `unsafe` usage.
  **Green** when services stay safe.
- `#![deny(unsafe_code)]` on `kernel/src/services/mod.rs` and
  `#[deny(unsafe_code)]` on `pub mod services` in `main.rs`.
- Limine request statics live in `ostd/limine.rs` (not `pub`). Query helpers
  are `pub(crate)` except `hhdm_offset`, `init_framebuffer`, `mm_init()`,
  `boot_modules()`.

## Migration checklist

| Site | Status |
|---|---|
| posix/fs.rs, system.rs | done — UserPtr / UserSlice / copy_user_path |
| posix/{process,audit,epoll}.rs | done |
| posix/mem.rs | done — safe `map_page` / `zero_phys_frame` |
| posix/mod.rs | done — `dispatch_syscall(&mut SyscallRegisters)`; rax writeback; execve rcx/rsp + `vm.activate()` |
| rust_syscall_dispatcher | done — safe no_mangle in services; deref in ostd |
| services/mod.rs Limine | done — `services_init(Vec<BootBlob>)` |
| process/elf.rs, vfs/tar.rs | done — `read_pod` |
| `#[deny(unsafe_code)]` | done |

## Implemented around the TCB (not this ADR, but current tree)

- IRQ-safe `SpinLock` (save RFLAGS, cli, restore on drop).
- Contiguous heap frames (`alloc_contiguous_frames`).
- Honest `sys_fork` → `-ENOSYS` until VMA + page-table clone/COW.
- Per-process `mmap_next_vaddr` (bump, not a VMA list). Reset on `exec`.
- SysV AMD64 user stack on execve (argc, argv, envp, auxv).
- Eradication of `static mut` globals in favor of `SyncUnsafeCell` and `AtomicU64` with CI grep enforcement.
- Per-operation `unsafe` scoping with required `// SAFETY:` rationale comments.

## Still open

- VFS must take cwd/creds as arguments (no `Process` lock from VFS).
- `VmSpace`: `Vec<Vma>`; then real `fork`.
- One task model documented in `ostd/task`.

## Consequences

New kernel features go through ostd primitives first, then a services PR.
Do not reconstruct the dispatcher or Limine requests from memory.
See `AGENTS.md` PR/commit bar.
