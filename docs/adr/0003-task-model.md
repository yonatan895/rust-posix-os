# ADR-0003: Task Model, Saved Context, and Preemption Design

- Status: Proposed
- Date: 2026-08-14
- Updated: 2026-08-15
- Issue: #25

## Context

ADR-0001 established the TCB boundary and noted that the task model should be documented in `ostd/task`. Current state on `main`:

- `ostd::task::switch_context` in `kernel/src/ostd/task/mod.rs` contains a complete naked-assembly callee-saved context switch routine with zero call sites.
- `kernel/src/services/scheduler/mod.rs` is a `SpinLock`-wrapped `VecDeque<i32>` FIFO (`pick_next` = `pop_front`), but its module doc claimed "Preemptive / MLFQ Scheduler" — a lying banner per `AGENTS.md`.
- Boot transitions to Ring 3 once via `ostd::task::enter_user_mode(entry, stack, pml4)`. The kernel main thread then halts forever (`loop { arch::hlt(); }`). There is no active scheduling loop and no second process to switch to.
- An async executor exists (`ostd::task::executor`) whose run loop executes exactly once during boot for `services/monitor.rs`.

## Decision

### D1. 1:1 Task-to-Process Model

A schedulable task is exactly a POSIX `Process` (`pid: i32`), 1:1.
- There are no kernel threads or multi-threading inside a single process at this stage.
- Scheduling decisions operate on `pid: i32` values queued in `Scheduler` and mapped to `Process` descriptors in `PROCESS_TABLE`.
- Multithreading primitives (`clone(CLONE_THREAD)` / `pthreads`) are deferred until SMP and thread-group primitives are designed.

### D2. Saved Context and Kernel Stack Discipline

Each process owns a dedicated, page-aligned kernel stack allocated during process creation.

1. **Unified Context Switch Representation (`TrapFrame` at `saved_kernel_rsp`)**:
   - Both involuntary IRQ preemption and voluntary blocking/yielding use the **identical** 160-byte `TrapFrame` layout at `saved_kernel_rsp` and the **identical** `pop 15 GPRs; iretq` resumption path.
   - For Ring 3 user tasks, `CS = USER_CODE_SEL (0x23)` and `SS = USER_DATA_SEL (0x1b)`.
   - For Ring 0 kernel tasks (including PID 0 idle task and voluntarily blocked kernel syscall contexts), `CS = KERNEL_CODE_SEL (0x08)` and `SS = KERNEL_DATA_SEL (0x10)`.

2. **Involuntary Context Switch (IRQ Preemption)**:
   - When a hardware timer interrupt fires, the CPU automatically pushes the hardware interrupt frame `[SS, RSP, RFLAGS, CS, RIP]` onto the active task's kernel stack.
   - The interrupt entry stub pushes the remaining general-purpose registers (`rax`, `rbx`, `rcx`, `rdx`, `rsi`, `rdi`, `rbp`, `r8`..`r15`), forming a complete `TrapFrame`.
   - The process control block records `saved_kernel_rsp: AtomicUsize`, pointing to the top of this saved frame.

3. **Voluntary Context Switch (Yield / Block)**:
   - When a task blocks (e.g., waiting for pipe I/O, child exit in `wait4`, or sleep) or yields, `ostd::task::voluntary_task_switch` pushes a synthetic kernel-mode `TrapFrame` onto the outgoing stack, writes `rsp` directly into `prev_proc.saved_kernel_rsp`, sets `rsp = next_proc.saved_kernel_rsp`, and resumes the incoming task via `pop 15 GPRs; iretq`.

4. **TSS & Per-CPU Kernel Stack Pointer Invariant**:
   - On **every** context switch, `TSS.rsp0` and `BSP_PER_CPU.kernel_rsp` **must be updated** to the top of the incoming task's kernel stack (`set_kernel_stack(next_kernel_stack_top)`).
   - This guarantees that subsequent user-to-kernel transitions (via `syscall` or interrupts) will always use the active task's dedicated kernel stack.

### D3. Address-Space Switching Policy

- When switching execution between tasks with distinct memory spaces:
  - If `next_proc.vm_space.pml4_phys != current_pml4`, the kernel executes `write_cr3(next_proc.vm_space.pml4_phys)` via `VmSpace::activate` (ADR-0001 R4).
  - Higher-half kernel mappings ($\ge \text{0xFFFF\_8000\_0000\_0000}$) are identical across all `VmSpace` instances, ensuring continuous execution of kernel code across CR3 switches.
  - PCID (Process-Context Identifiers) support is designated as future work to avoid global TLB invalidations upon CR3 reloads.

### D4. Preemption Design & Timer Interrupt Flow

- **Timer Source**: LAPIC Timer (with PIT Channel 0 at 100 Hz / 10 ms interval as fallback) mapped to IDT interrupt vector `0x20`.
- **Preemption Flow**:
  1. Timer interrupt fires while running userland or interruptible kernel code.
  2. CPU pushes `InterruptFrame`; ISR pushes remaining GPRs onto the active kernel stack.
  3. The timer ISR calls the safe scheduler: `SCHEDULER.lock().pick_next()` (satisfying ADR-0002 L5).
  4. If the selected PID differs from the running PID:
     - Save current stack pointer in `prev_proc.saved_kernel_rsp`.
     - Update per-CPU current task pointer.
     - Switch address space via `write_cr3(next_proc.vm_space.pml4_phys)`.
     - Update `TSS.rsp0` and `BSP_PER_CPU.kernel_rsp` to `next_proc.kernel_stack_top`.
     - Set CPU stack pointer `RSP` to `next_proc.saved_kernel_rsp`.
  5. Pop GPRs and execute `iretq`, resuming the new task in userland.

### D5. Async Executor Boundary (removed — Issue #33)

The `ostd::task::executor` cooperative future runtime was **removed** (2026-08-15, Issue #33).
It ran exactly once during boot for a single `monitor` task that drained its fixed five
iterations before control reached ring 3, so it was never a live runtime; `/proc` computes
its metrics on read, so the task provided no value.

- POSIX processes (Ring 3 preemptive tasks) are managed strictly by the `Process` and
  `Scheduler` subsystems.
- Reintroduction requires a concrete second async consumer and an executor driven
  continuously from the idle task loop with a waker on timer ticks. A single boot-drained
  task does not justify a runtime.

## Consequences

- Unblocks implementation of timer interrupt handling and scheduler integration ("feat(sched): wire timer IRQ to the scheduler").
- Unblocks asynchronous signal delivery and process preemption ("feat(ipc): end-to-end signal delivery").
- Eliminates inaccurate doc comments regarding MLFQ in `services/scheduler`.
- Maintains ADR-0001 TCB boundary (all assembly and CR3/TSS manipulation remains in `ostd/task` and `ostd/arch`).

## References

- `docs/adr/0001-tcb-boundary.md` (R4 address-space switch, TCB invariants)
- `docs/adr/0002-locking.md` (L5 IRQ lock tier, D3 IRQ safety)
- `kernel/src/ostd/task/mod.rs`
- `kernel/src/services/scheduler/mod.rs`
- `kernel/src/services/process/mod.rs`
