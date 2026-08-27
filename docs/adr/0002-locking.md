# ADR-0002: Locking Discipline, IRQ Safety, and Lock Ordering

- Status: Proposed
- Date: 2026-08-14
- Updated: 2026-08-14
- Issue: #24

## Context

ADR-0001 established the TCB boundary but deliberately left locking undecided. Current state on `main`:

- One global lock guards the process table: `PROCESS_TABLE` in `kernel/src/services/process/mod.rs`.
- `get_current_process()` locks the whole table just to clone an `Arc`, at the top of nearly every syscall handler — unrelated syscalls serialize.
- `services/monitor.rs` also locks `PROCESS_TABLE`, adding background contention.
- VFS uses per-inode `SpinLock`s (`RamFsDir.entries/subdirs`), and ADR-0001's forward list already records: "VFS must take cwd/creds as arguments (no `Process` lock from VFS)" — lock ordering is already biting on one CPU.
- A design review flagged the `ostd/sync.rs` acquire loop as possibly running with interrupts unmasked, while the xtask suite contains an "OSTD IRQ-Safe SpinLock & RFLAGS Save/Restore Test". The contradiction must be resolved: a same-core IRQ handler re-entering a held lock is a permanent deadlock.

## Decision

### D1. Lock hierarchy

Locks are only ever acquired in this order:

    PROCESS_TABLE → Scheduler → IPC (SignalManager) → VFS mount/table → individual Inode → device locks

A lock may never be acquired while holding a lock from a later tier. New locks are slotted into this hierarchy by amending this ADR in the same PR that introduces the lock.

### D2. Rules

- L1. No user-memory access while holding any spinlock. Copy-in before acquiring, copy-out after releasing. Extends ADR-0001 R2.
- L2. No blocking or sleeping while holding a spinlock. Critical sections are short and bounded.
- L3. No calling into VFS while holding a `Process` lock. cwd/creds are passed as arguments (ADR-0001 forward list).
- L4. Locks are acquired in D1 hierarchy order only.
- L5. IRQ context may acquire only Scheduler-tier locks and below — never `PROCESS_TABLE`. Interrupt handlers learn the current task from a per-CPU current pointer (introduced with preemption, see ADR-0003), not from the process table.
- L6. Multi-inode VFS operations (cross-directory `rename`): Parent directory locks are acquired in ascending pointer address order (`addr(A) < addr(B)`). Target directory emptiness is evaluated lock-free via an `AtomicUsize` entry counter (`RamFsDir.entry_count`) to avoid taking a 3rd lock and prevent AB-BA deadlock against concurrent operations on the target directory.

### D3. Interrupt discipline: the IRQ-safety question, answered

All `ostd` spinlocks are IRQ-safe by construction. Evidence on `main`:

- `kernel/src/ostd/sync.rs` module documentation: "SpinLock masks CPU interrupts on acquire and restores the previous RFLAGS state on drop, preventing deadlocks when acquiring locks held across ISR contexts."
- `SpinLock::lock()` SAFETY comment: "Read RFLAGS and disable interrupts before acquiring the spinlock."
- `Drop for SpinLockGuard` releases the lock with `Ordering::Release`, then `restore_rflags(self.rflags)` — `spin_lock_irqsave` / `spin_unlock_irqrestore` semantics.
- Coverage: "OSTD IRQ-Safe SpinLock & RFLAGS Save/Restore Test" in `tools/xtask/src/test.rs`.

The design-review flag is stale relative to current `main` (the IRQ-safety fix landed via the `fix/spinlock-irq-mmap-fork` line of work).

Standing invariant: **interrupts are masked before the lock is observed as acquired.** There is no window in which the lock is held with `RFLAGS.IF=1`; therefore a same-core IRQ handler attempting to acquire a held lock cannot occur, and the classic self-deadlock is excluded by construction. Any change to the acquire/drop ordering in `sync.rs` must update this ADR and the named test in the same PR.

### D4. SMP stance

Spinlocks stay. When application processors exist:

- The Scheduler ready queue becomes per-CPU.
- `PROCESS_TABLE` and the VFS mount table stay global (IRQ-safe per D3).
- The current-task pointer is per-CPU by definition.
- Device locks stay global, one per device.

No code changes now; this names the split so ADR-0003 and future SMP work do not re-derive it.

## Known contention (accepted, not fixed here)

`get_current_process()` locks `PROCESS_TABLE` to clone an `Arc` at the top of nearly every syscall; `services/monitor.rs` adds background contention. Accepted short-term. Named future direction: per-CPU current pointer plus `Arc` clone without taking the table lock; the table lock is then used only for insert/remove/lookup-by-pid. That change is a separate PR and must update this ADR.

## Consequences

- wait4 blocking, blocking pipes, and all future SMP work build on L1–L5.
- ADR-0003 (task model and preemption) depends on D3 and L5: the timer tick handler will take the Scheduler lock from IRQ context.
- `AGENTS.md` gains a one-line pointer: locking rules live in ADR-0002.
- No functional code changes.

## References

- `docs/adr/0001-tcb-boundary.md` (R2, R4, forward list)
- `kernel/src/ostd/sync.rs`
- `kernel/src/services/process/mod.rs`
- `kernel/src/services/monitor.rs`
- `tools/xtask/src/test.rs`
