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

ADR-0001:
- R1. `unsafe` only in `ostd/`. Gate: `.github/workflows/tcb.yml`. `#![deny(unsafe_code)]` on `services`.
- R2. User memory only via `ostd::mm::user`. Errno mapping only in `services/posix/user_access.rs`.
- R3. Boot protocol only in `ostd::limine`. Services take `BootBlob`, never `Limine*`.
- R4. Address-space / context switch only in `ostd`.
- R5. Every `unsafe` block has `// SAFETY:` stating why it is sound.
- R6. A change that touches scheduler, context switch, IDT, or return-to-user paths is not mergeable on host-side tests alone. Mark the PR ready so the QEMU smoke job runs; do not merge on a skipped QEMU job.

ADR-0002: locking rules (hierarchy, IRQ discipline, no user memory under spinlock) live in docs/adr/0002-locking.md.
ADR-0003: task model (1:1 process mapping, kernel stack & TSS discipline, preemption) lives in docs/adr/0003-task-model.md.

A change that needs two layers is two PRs (ostd primitive first).

## Pre-submit checklist (every PR, no exceptions)

- C1. Lock discipline: for every `.lock()` you wrote or moved, list what is already held at that call site and name the tier. Acquisition must follow ADR-0002 D1 order. If you can't name the tier, don't write the lock.
- C2. State machines: every state transition is guarded (never overwrite terminal states like Zombie). The guard and ALL its side effects (requeue, wake, notify) live in the same conditional block.
- C3. Blocking paths use mark → register → re-check → sleep. Every wake site has a corresponding blocked-state producer. Grep for both halves before submitting.
- C4. Save/restore and frame protocols: trace the full round trip in BOTH directions with exact arithmetic (rsp deltas, popped slots, offsets). Write the trace as a comment. If resumption state is read by another path, it must be stored in shared state BEFORE the switch point, never in a stack local written back after.
- C5. Implicit state at every context-switch boundary: enumerate RFLAGS, CR3, TLB, held locks. Document who restores each, and when.
- C6. Resource lifecycle: cleanup is driven by ground truth (page tables, allocator bookkeeping), not auxiliary metadata (VMA lists, caches). Every alloc has a verified free; partial-construction failure paths are traced.
- C7. POSIX semantics: check the man page for every branch — error codes, default signal actions, edge cases (munmap of unmapped = 0, mprotect gap = ENOMEM, stop vs terminate). Cite the section in a comment.
- C8. No silent contract violations: a deferred path returns an honest -ENOSYS or the PR is retitled to its actual scope. A blocking fd must never return -EAGAIN.
- C9. Tests: spec/mock tests must mirror the real control flow (loops, re-checks, retries) or they're documentation, not tests. A behavior change requires a SEMANTIC test change, not a label rename. Switching / return-to-user / IDT changes require a green QEMU run, not a skipped one.
- C10. The commit message and PR description must describe what the diff actually does. Re-read the diff before pushing. Code is the source of truth.

## Bug patterns seen in review (do not reintroduce)

- P1. Calling up the lock hierarchy from a lower tier (e.g. PROCESS_TABLE under an Inode lock). Smell: `wake_*` or `get_current_process()` called with any lock held.
- P2. Saving resumption state into a stack local, written back to the PCB after resume. Circular: resume needs the PCB value first. Store into the PCB directly.
- P3. `if state == X { state = Y; }` followed by an unconditional queue push. The push belongs inside the guard.
- P4. User-stack frame protocols where the return path pops bytes the restore code doesn't account for (ret pops the restorer → frame base is rsp-8, not rsp).
- P5. Waking a blocked task without the woken syscall checking why it woke (EINTR). Every wake must be matched by a pending-condition check at the resume site.
- P6. Cleanup walking a metadata list (VMAs) instead of ground truth (page tables).

## Do

- Put `#[no_mangle] extern "C"` in `ostd` so `deny(unsafe_code)` holds. That ABI is safe. `ostd` must not name `services`.
- Export raw TCB pointers (`*mut Limine*`, CR3, page tables). New ostd items are private/`pub(crate)` unless the PR names the export and why.

## Do not

- Invent boot values (HHDM offset, memmap). Missing response → panic/halt.
- Read an ABI you do not produce (user `argc`/`argv` before exec writes a SysV frame).
- Collapse errnos (`E2BIG` → `ENOEXEC`). Map each failure to its POSIX code.
- Leave process image state across `exec` (`mmap_next_vaddr`, later ASLR). Reset or document.

## Architecture map

Syscall: `ostd::arch::syscall` trampoline → `with_syscall_regs` → `services::posix::dispatch_syscall` → `sys_*`.
The `#[no_mangle] extern "C"` stub lives in **services** (safe). Raw `*mut` deref lives in ostd.

Still open (order):
1. VFS: pass cwd/creds in; never lock `Process` from VFS (IRQ-off hang).
2. `VmSpace`: `Vec<Vma>`; mmap/munmap/exit/fork.
3. `sys_fork` only after (2).
4. SysV stack is done; coreutils argv is a separate userland PR.