# AGENTS.md

Rules for humans and agents. Safety, then correctness, then performance, then DX.
Inspired by [TigerStyle](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md).
Zero technical debt. An hour of design beats a week in production.

## Workflow

1. Branch from `main`. Never from a feature branch unless the PR says it is stacked.
2. One concern per PR. Names: `fix/…`, `feat/…`, `refactor/…`.
3. PR to `main`. Draft until tests are green. Description says *why* and what is *not* done.
4. Do not rewrite a file you have not read in full. If the body is unavailable, stop and ask.
5. Do not invent ABI numbers, syscall tables, or dispatcher arms. `libs/posix-abi` is the source of truth.
6. Do not reconstruct a working protocol from memory. Move or wrap; keep every field (syscall rax writeback, execve rcx/rsp/cr3, Limine requests).


## PR / commit bar

One concern. One invariant. Diff matches title and body.

Do not:
- Bundle TCB/kernel work with userland, or two ADRs, unless the second is a one-line call-site update.
- Add a string to `tools/xtask/src/test.rs` that always prints `[PASS]`. A test runs code or it is not a test.
- Claim an invariant is done if only half exists (`mmap tracking` ≠ bump pointer).
- Open a `test(...)` PR whose diff changes runtime behavior. If the diff changes a validator or a syscall, the type is `fix`/`feat` and the test is evidence, not the change.
- Put `#[no_mangle] extern "C"` in `ostd` so `deny(unsafe_code)` holds. That ABI is safe. `ostd` must not name `services`.
- Export raw TCB pointers (`*mut Limine*`, CR3, page tables). New ostd items are private/`pub(crate)` unless the PR names the export and why.
- Invent boot values (HHDM offset, memmap). Missing response → panic/halt.
- Read an ABI you do not produce (user `argc`/`argv` before exec writes a SysV frame).
- Collapse errnos (`E2BIG` → `ENOEXEC`). Map each failure to its POSIX code.
- Leave process image state across `exec` (`mmap_next_vaddr`, later ASLR). Reset or document.

Every user-controlled length has a cap. Hitting the cap without a terminator is `E2BIG`/`ENAMETOOLONG`, not silent truncate.

Commit subject: `type(scope): fact` — what/why, not a brochure.

## 1. Separation of concerns

Dependencies only point down.

| Layer | Owns | Must not |
|---|---|---|
| `userland/*` | programs | kernel internals |
| `libs/libc` | C wrappers over syscalls | page tables, process table |
| `libs/posix-abi` | numbers, errno, `repr(C)` types | implementation |
| `kernel/src/services` | POSIX policy, VFS, process, scheduler | `unsafe`, CR3, Limine, raw user pointers |
| `kernel/src/ostd` | TCB: paging, copyin/out, IRQ, boot, arch | POSIX policy, `services::` |

ADR-0001:
- R1. `unsafe` only in `ostd/`. Gate: `.github/workflows/tcb.yml`. `#![deny(unsafe_code)]` on `services`.
- R2. User memory only via `ostd::mm::user`. Errno mapping only in `services/posix/user_access.rs`.
- R3. Boot protocol only in `ostd::limine`. Services take `BootBlob`, never `Limine*`.
- R4. Address-space / context switch only in `ostd`.
- R5. Every `unsafe` block has a `// SAFETY:` stating why it is sound — the invariant, not the action. `// SAFETY: Initialize GDT` is a restatement; `// SAFETY: single-threaded boot, no concurrent access` is a rationale. Applies in userland too.
- R6. A change that touches scheduler, context switch, IDT, or return-to-user paths is not mergeable on host-side tests alone. The QEMU smoke job must run and pass; do not merge on a skipped QEMU job.

ADR-0002: locking rules (hierarchy, IRQ discipline, no user memory under spinlock) live in docs/adr/0002-locking.md.
ADR-0003: task model (1:1 process mapping, kernel stack & TSS discipline, preemption) lives in docs/adr/0003-task-model.md.

A change that needs two layers is two PRs (ostd primitive first).

## 2. Abstractions

Make illegal states unrepresentable. Wait for a third call site before generalizing.

- User pointers: `UserPtr<T>` / `UserSlice`. Raw `*const` dies at the dispatcher.
- Address spaces: `VmSpace` owns mappings (`Vec<Vma>`). mmap bump is not tracking.
- Errors: `UserAccessError` in ostd; errno only in posix-abi + `user_access`.
- `exec` resets per-image fields (`mmap_next_vaddr`, …). `fork` copies VMAs + those fields, or stays `-ENOSYS`.
- Put a limit on everything. Fail fast. Assertions = programmer error (crash). Operating errors = errno.
- Check every arithmetic step on user-controlled sizes, not just the last: `checked_add(addr, pages * PAGE_SIZE)` is hollow when the multiply wraps first. `checked_mul` feeds `checked_add`.

## 3. Prune before you add

Delete or shrink first: duplicate helpers, globals that should be fields, lying banners, dead `unsafe` a safe ostd API already covers.
`-ENOSYS` beats a silent broken `fork`. If the diff grew after “cleanup,” you added, not pruned.

## 4. Tests

- `cargo xtask test` only lists checks that execute. Prefer `#[cfg(test)]` on host-buildable crates (`posix-abi`, stack-layout helpers).
- QEMU smoke must assert a serial string the change actually prints.
- No feature lands without a real check of the valid *and* invalid path (null, unmapped, too long, missing terminator).
- A simulation test must include the boundary input that motivated the change (`usize::MAX` length, straddle into an unmapped page, cap ± 1). Asserting on literals the test just formatted is a tautology — it cannot fail.
- Benchmarks: measure the real path (in-guest syscall, real IRQ) or label the output `simulation`/`dispatch`. Never print `ns/syscall` for a host function call.

## 5. Always branch from main, PR to main

```
git fetch origin && git checkout main && git pull --ff-only
git checkout -b fix/short-name
git push -u origin HEAD
# open PR → main
```

## Bug patterns seen in review (do not reintroduce)

- P1. Calling up the lock hierarchy from a lower tier (e.g. PROCESS_TABLE under an Inode lock). Smell: `wake_*` or `get_current_process()` called with any lock held.
- P2. Saving resumption state into a stack local, written back to the PCB after resume. Circular: resume needs the PCB value first. Store into the PCB directly.
- P3. `if state == X { state = Y; }` followed by an unconditional queue push. The push belongs inside the guard.
- P4. User-stack frame protocols where the return path pops bytes the restore code doesn't account for (ret pops the restorer → frame base is rsp-8, not rsp).
- P5. Waking a blocked task without the woken syscall checking why it woke (EINTR). Every wake must be matched by a pending-condition check at the resume site.
- P6. Cleanup walking a metadata list (VMAs) instead of ground truth (page tables).
- P7. `checked_add` over an unchecked multiply (`vaddr.checked_add(pages * PAGE_SIZE)`): the multiply wraps before the add is checked, and the guard passes. Check every step; the test vector is `usize::MAX`.
- P8. Self-simulation tests and benchmarks: the mock asserts on its own literals and stays green when the real code regresses. Mirror tests must encode the motivating boundary; prefer exercising the real helper.
- P9. CI treated as config, not code: a workflow change merged with its own job red, or `github.event` data interpolated into a `run:` shell (injection). The changed workflow must be green on the PR that changes it.

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
Syscall: `ostd::arch::syscall` trampoline → `with_syscall_regs` → `services::posix::dispatch_syscall` → `sys_*`.
The `#[no_mangle] extern "C"` stub lives in **services** (safe). Raw `*mut` deref lives in ostd.

Done since this list was written: `VmSpace` + VMA tracking, `sys_fork`, preemptive round-robin scheduling, signals, EFAULT user-pointer hardening, userland panic → fd 2, `ci:full` escape hatch, in-guest syscall bench.

Still open (order):
1. VFS: pass cwd/creds in; never lock `Process` from VFS (IRQ-off hang).
2. mmap failure path: a mid-loop mapping failure leaves the bump advanced and partial mappings behind. Roll back or document.
3. coreutils argv (userland PR).
4. Userland networking.
5. SMP.
6. exec-time ASLR.

## Checklist

- [ ] Branched from latest `main`; PR targets `main`
- [ ] One concern; title/body match the diff
- [ ] Title names the change (`type(scope): fact`); body links the issue; nothing auto-generated
- [ ] Read every file you will edit; did not drop an existing contract
- [ ] Named the layer and the invariant; named what is *not* done
- [ ] Deleted something before adding something
- [ ] Real test (not an xtask `[PASS]` string); QEMU string if user-visible
- [ ] Tests/benchmarks execute the real path or are labeled simulation; every assertion can fail; the motivating boundary value is a test case
- [ ] `unsafe` only in `ostd/`; `ostd` does not import `services`
- [ ] Every `// SAFETY:` states the invariant that makes it sound (kernel and userland), not the action it performs
- [ ] Size arithmetic on user input: every step checked (`checked_mul` before `checked_add`); `usize::MAX` and cap ± 1 tested
- [ ] Caps + fail-closed on user lengths and boot responses
- [ ] Workflow changes: the changed workflow is green on this PR; no `github.event` interpolation into `run:` shell
- [ ] Lock discipline: for every `.lock()` you wrote or moved, listed what is already held and named the tier; acquisition follows ADR-0002 D1 order
- [ ] State machines: every state transition is guarded (never overwrite terminal states like Zombie); the guard and ALL its side effects (requeue, wake, notify) live in the same conditional block
- [ ] Blocking paths use mark → register → re-check → sleep; every wake site has a corresponding blocked-state producer (grepped for both halves)
- [ ] Save/restore and frame protocols: traced the full round trip in BOTH directions with exact arithmetic (rsp deltas, popped slots, offsets); resumption state read by another path is stored in shared state BEFORE the switch point, never in a stack local written back after
- [ ] Implicit state at every context-switch boundary: enumerated RFLAGS, CR3, TLB, held locks; documented who restores each and when
- [ ] Resource lifecycle: cleanup driven by ground truth (page tables, allocator bookkeeping), not auxiliary metadata (VMA lists, caches); every alloc has a verified free; partial-construction failure paths traced
- [ ] POSIX semantics: checked the man page for every branch (error codes, default signal actions, edge cases); cited the section in a comment
- [ ] No silent contract violations: a deferred path returns an honest -ENOSYS or the PR is retitled to its actual scope; a blocking fd never returns -EAGAIN
- [ ] Tests: spec/mock tests mirror the real control flow (loops, re-checks, retries); a behavior change is a SEMANTIC test change, not a label rename; switching / return-to-user / IDT changes have a green QEMU run, not a skipped one
