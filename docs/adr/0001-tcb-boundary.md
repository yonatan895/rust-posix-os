# ADR-0001: The TCB Boundary (ostd vs. services)

- Status: Accepted
- Date: 2026-08-14
- Context: foundation review, finding #1

## Context

The kernel declares an Asterinas-style framekernel split: `ostd/` is the
privileged Trusted Computing Base (the only module allowed `unsafe`), and
`services/` is "De-Privileged OS Services — 100% Safe Rust". As of
`8af4155` that invariant is documentation, not mechanism: `unsafe` appears
in at least 10 files under `services/` (user-pointer derefs, `mov cr3`
inline asm, raw ELF/tar header casts, Limine boot-protocol derefs). Every
planned feature (signals, threads, real fork, more syscalls) would multiply
that surface.

## Decision

Rules, in force immediately:

- R1. `unsafe` code lives only in `kernel/src/ostd/`. Nothing else in the
  kernel may contain `unsafe { }`, `unsafe fn`, `unsafe impl`, or
  `unsafe extern`.
- R2. User memory is touched only through `ostd::mm::user`
  (`UserPtr`, `UserSlice`, `copy_cstr_from_user`). Syscall handlers never
  see raw pointers; the dispatcher converts registers into validated types.
- R3. The boot protocol (Limine requests/responses, module discovery) is
  read only inside `ostd::limine`, which hands safe references upward.
- R4. Address-space switches (`mov cr3`) and context switches live only in
  `ostd` (`VmSpace::activate`, `ostd::task`).
- R5. Every `unsafe` block in `ostd` carries a `// SAFETY:` comment stating
  why it is sound.

## Enforcement

- CI gate `.github/workflows/tcb.yml` greps `kernel/src/services/` for
  unsafe usage and fails the build. It is RED until the migration below is
  complete — that is the point of the gate.
- After migration: add `#[deny(unsafe_code)]` to `mod services;` in
  `kernel/src/main.rs` for compiler-level enforcement (kept out of this
  commit because it does not compile until the sites below are converted).
- `kernel/src/ostd/mm/user.rs` lands in this commit; wire it with
  `pub mod user;` in `kernel/src/ostd/mm/mod.rs`.

## Migration checklist

| File | Current unsafe | Move to |
|---|---|---|
| services/mod.rs | Limine module response derefs | ostd::limine: safe `modules()` accessor (R3) |
| services/posix/fs.rs | `slice::from_raw_parts(_mut)` on user bufs | `UserSlice::copy_from_user` / `copy_to_user` |
| services/posix/mem.rs | `vm.map_page(...)`, `write_bytes` via HHDM | safe `VmSpace::map_page` + `ostd::mm::zero_frame` |
| services/posix/mod.rs | `&mut *regs`, `mov cr3` asm | typed regs from `ostd::arch::syscall`; `VmSpace::activate()` |
| services/posix/epoll.rs | `*event_ptr`, `from_raw_parts_mut` | `UserPtr::<EpollEvent>::read`, `UserSlice::copy_to_user` |
| services/posix/audit.rs | user string derefs | `copy_cstr_from_user` |
| services/posix/system.rs | `*buf = uts`, `*info = si` | `UserPtr::write` |
| services/posix/process.rs | execve path scan, `*status_ptr` | `copy_cstr_from_user`, `UserPtr::write` |
| services/process/elf.rs | `&*(bytes as *const Elf64Header)` | bounds-checked `read_pod` helper in ostd |
| services/vfs/tar.rs | `&*(slice as *const TarHeader)` | same `read_pod` helper |

## Consequences

- The unsafe surface shrinks to one audited module; everything planned
  (threads, signals, COW fork) builds on validated primitives.
- `ostd::mm::user` validates against the current CR3 and assumes a
  single-CPU kernel; SMP/threading must revisit validate-then-copy
  (see module docs).
- userland (`libs/libc`, `userland/*`) is unaffected: `unsafe` is
  legitimate there (syscall asm, `_start`).

## References

- Asterinas OSTD (framekernel model this project follows)
- Review thread: foundation findings #1 and #2
