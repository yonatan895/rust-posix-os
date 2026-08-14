# AGENTS.md

Rules for humans and agents. Safety, then correctness, then performance, then DX.
Inspired by [TigerStyle](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md).
Zero technical debt: do it right the first time. An hour of design beats a week in production.

## Workflow

1. Branch from `main`. Never from a feature branch unless the PR says it is stacked.
2. One concern per PR. Names: `fix/…`, `feat/…`, `refactor/…`.
3. PR to `main`. Draft until tests are green. Description says *why*.
4. Do not rewrite a file you have not read in full. If the body is unavailable, stop and ask.
5. Do not invent ABI numbers, syscall tables, or dispatcher arms. `libs/posix-abi` is the source of truth.

## 1. Separation of concerns

Dependencies only point down.

| Layer | Owns | Must not |
|---|---|---|
| `userland/*` | programs | kernel internals |
| `libs/libc` | C wrappers over syscalls | page tables, process table |
| `libs/posix-abi` | numbers, errno, `repr(C)` types | implementation |
| `kernel/src/services` | POSIX policy, VFS, process, scheduler | `unsafe`, CR3, Limine, raw user pointers |
| `kernel/src/ostd` | TCB: paging, copyin/out, IRQ, boot, arch | POSIX errno, VFS policy |

ADR-0001 (enforce, do not restatedocument):
- R1. `unsafe` only in `ostd/`. Gate: `.github/workflows/tcb.yml`. After migration: `#[deny(unsafe_code)]` on `services`.
- R2. User memory only via `ostd::mm::user`. Errno mapping only in `services/posix/user_access.rs`.
- R3. Boot protocol only in `ostd::limine`.
- R4. Address-space / context switch only in `ostd`.
- R5. Every `unsafe` block has `// SAFETY:` stating why it is sound.

A change that needs two layers is two PRs (ostd primitive first).

## 2. Abstractions

Make illegal states unrepresentable. Wait for a third call site before generalizing.

- User pointers: `UserPtr<T>` / `UserSlice`. Raw `*const` dies at the dispatcher.
- Address spaces: `VmSpace` owns mappings. mmap state is per-process, never a process-global bump.
- Errors: `UserAccessError` in ostd; errno only in posix-abi + `user_access`.
- Put a limit on everything (paths, I/O size, fd table, string scans). Fail fast.
- Assertions catch programmer errors (crash). Operating errors return errno. Test the valid *and* invalid spaces.

## 3. Prune before you add

Before writing code, delete or shrink:
- Duplicate helpers — one module, many callers.
- Globals that should be fields.
- Comments that restate the code. Say why.
- Features that lie (no “preemptive” banner without a timer).
- Dead `unsafe` a safe ostd API already covers.

`-ENOSYS` is better than a silent broken `fork`. If the diff grew after “cleanup,” you added, not pruned.

## 4. Tests

No feature lands without both:
- Unit tests on host-buildable logic (`posix-abi`, ELF/tar/path parsers). Valid and invalid inputs.
- Integration: `cargo xtask test` / QEMU smoke for any syscall or boot path.

## 5. Always branch from main, PR to main

```
git fetch origin && git checkout main && git pull --ff-only
git checkout -b fix/short-name
# …smallest change that preserves an invariant…
git push -u origin HEAD
# open PR → main
```

## Architecture

```
userland (init, shell, coreutils)
  → libc (syscall stubs)
    → posix-abi (numbers, types, errno)
kernel
  services/   safe POSIX: vfs, process, scheduler, ipc, posix/*
  ostd/       TCB: arch, mm (pmm/vmm/heap/user), irq, task, sync, limine, drivers
```

Boot: Limine → ostd init → services init → first user process.
Syscall: `ostd::arch::syscall` → posix dispatcher → `sys_*` → ostd primitives.

Do these before new features:
1. Finish ADR-0001 (remaining `unsafe` in services).
2. Honest `fork` or `-ENOSYS`; per-process VMAs.
3. `SpinLock` must mask interrupts.
4. One task model — processes *or* async — written down in `ostd/task`.
5. `mm_init` must not assume contiguous heap frames.

## Checklist

- [ ] Branched from latest `main`; PR targets `main`
- [ ] Read every file you will edit
- [ ] Named the layer and the invariant
- [ ] Deleted something before adding something
- [ ] Unit + integration tests included
- [ ] `unsafe` still only in `ostd/`
- [ ] PR body says why, not just what
