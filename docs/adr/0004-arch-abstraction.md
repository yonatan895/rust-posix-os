# ADR-0004: Architecture Abstraction Boundary

- Status: Proposed
- Date: 2026-08-15
- Updated: 2026-08-15
- Issue: #52

## Context

According to **ADR-0001**, `ostd` is the Trusted Core Base (TCB) and the only layer allowed to interact directly with hardware. However, the current implementation of `ostd` is heavily hardcoded for the x86-64 architecture, not just in instruction selection, but in fundamental data structures and APIs. 

Adding a second architecture (e.g., `aarch64` or `riscv64`) without an abstraction layer would require rewriting shared modules in place. Specifically:

* **`ostd::mm` (vmm):** Hardcodes x86-64 4-level paging (PML4/PDPT/PD/PT index decomposition), specific bit positions (`PAGE_PRESENT`, `PAGE_WRITABLE`, `PAGE_USER`, `PAGE_NX`), `invlpg` TLB invalidation, CR3 register manipulation for address-space activation, and the HHDM-offset model for physical-to-virtual translation.
* **`ostd::task`:** Hardcodes the x86-64 context model, including the `TrapFrame` register layout, naked-asm `voluntary_task_switch`, the `syscall`/`sysretq` register contract (rcx=RIP, r11=RFLAGS), `iretq` resumption, and the `hlt` instruction in the idle loop.
* **`ostd::irq`:** Currently coupled to legacy PC hardware (8259 PIC + 8254 PIT) rather than pure architecture primitives. `aarch64` requires GIC + ARM generic timer; `riscv64` requires PLIC/ACLINT + SBI timer.
* **`ostd::arch`:** Exists as a flat x86-64 module (GDT, TSS, IDT, syscall MSRs) with no per-arch namespacing.

Meanwhile, architecture-neutral components like `ostd::sync::SpinLock`, the `UserPtr`/`UserSlice` models, and the `services` layer prove that a clean boundary is possible and effective.

## Decision

We will introduce a strict architecture abstraction boundary within `ostd` to support multiple ISAs (x86-64, aarch64, riscv64) using `cfg(target_arch)`-gated modules with a uniform API surface. We will *not* use trait objects (`dyn Trait`) or dynamic dispatch for this boundary to avoid performance penalties in the TCB.

### 1. Namespace Refactoring
* Current `ostd::arch` will be moved to `ostd::arch::x86_64`.
* PC-hardware specific bits will be extracted from `ostd::irq` into `ostd::arch::x86_64::{pic, pit}`.
* `ostd::irq` will retain only the architecture-neutral interface.

### 2. Memory Management (`ostd::mm`)
We will define an architecture-neutral surface for page table manipulation:
* **API:** `map_page`, `unmap_page`, `set_page_flags`, `translate`, `activate`, and `tlb_flush(addr)`.
* **Types:** A `PageFlags` newtype (`PRESENT`, `WRITABLE`, `USER`, `NO_EXEC`) mapped per-arch, and an `AddressSpace` handle (representing CR3 on x86, TTBR0_EL1 on ARM, `satp` on RISC-V).
* The VMA layer (`Vec<Vma>`) will remain strictly architecture-neutral.

### 3. Task Management (`ostd::task`)
* **API:** Per-architecture `TrapFrame` (or `ExceptionFrame`) type, `init_user_kernel_stack`, `init_kernel_task_stack`, `init_fork_child_stack`, the naked-asm voluntary switch, syscall/exception entry contracts, and the idle body (`hlt` / `wfi` / SBI `wfi`).
* **Invariant (ADR-0003):** The unified `TrapFrame`-at-`saved_kernel_rsp` invariant **must** hold on every architecture. This invariant is what makes the scheduler portable.

### 4. Interrupts and Timers (`ostd::irq`)
* **API:** `init_timer(hz)`, `ack_timer`, `ticks`, `enable`, `disable`, `mask`, and `EOI`.
* **Delivery Mechanisms:** PIT IRQ0 (x86), ARM generic timer IRQ (ARM), RISC-V timer via SBI `set_timer` (RISC-V).
* **Masking Discipline (ADR-0002):** The `spinlocks + IRQ-masking` discipline ports directly. `cli`/`sti` (x86), `msr daifclr/daifset` (ARM), and `csrw sie` (RISC-V) will be abstracted as `irq_save` / `irq_restore` returning an opaque flags token. We will *not* leak RFLAGS-shaped semantics into the portable API.

### 5. Syscall ABI
We will retain the existing Linux-x86_64 syscall numbers defined in `libs/posix-abi` for **all** architectures. Because this kernel defines its own ABI, there is no reason to adopt upstream per-arch Linux numbering, which differs wildly between aarch64, riscv64, and x86-64.

## Consequences

### Positive
* **Multi-Architecture Support:** The kernel will be able to compile and run on `aarch64` and `riscv64` natively, provided the bootloader (Limine) supports the target.
* **Cleaner Boundaries:** The TCB becomes strictly isolated by concerns (architecture vs. hardware vs. logic).
* **Scheduler Portability:** The `services/` and scheduler code will be strictly forbidden from referencing arch-specific symbols (e.g., verified by `grep -r "x86_64\|cr3\|iretq\|sysret" kernel/src/services/`).

### Negative / Risks
* **Refactoring Overhead:** We must perform a series of zero-functional-change refactors to extract these boundaries without breaking the currently working x86-64 build.
* **Guesswork Risk:** An abstraction designed against only one architecture is guesswork. The API cannot be considered "finished" until it is exercised by a second architecture.

### Execution Discipline
In accordance with the `AGENTS.md` workflow rule 6, namespace moves and abstraction extraction must be zero-functional-change refactors. The x86-64 QEMU smoke test must stay green through every step. 

Implementation will follow a strict 6-step PR sequence:
1. Namespace move (zero functional change).
2. `mm` abstraction.
3. `task` abstraction.
4. `irq`/`timer` abstraction.
5. `aarch64` skeleton port (compile-only initially, stubbing non-critical paths with `unimplemented!`).
6. `riscv64` skeleton port.

Skeleton ports will not be started before steps 1–4 land.
