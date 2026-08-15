//! ELF64 Executable Loader & System V AMD64 ABI User Stack Setup.

use crate::ostd::mm::{PageFlags, VmSpace, read_pod};
use posix_abi::*;

/// Expected 4-byte ELF identification magic (`\x7fELF`).
pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
/// ELF identification byte indicating 64-bit architecture.
pub const ELF_CLASS_64: u8 = 2;
/// ELF identification byte indicating 2's complement little-endian data.
pub const ELF_DATA_2LSB: u8 = 1;
/// ELF machine architecture identifier for x86-64 (AMD64).
pub const EM_X86_64: u16 = 0x3E;

/// Program header type for loadable segment.
pub const PT_LOAD: u32 = 1;
/// Segment permission flag: executable.
pub const PF_X: u32 = 1;
/// Segment permission flag: writable.
pub const PF_W: u32 = 2;
/// Segment permission flag: readable.
pub const PF_R: u32 = 4;

/// Highest virtual address boundary for user-space stack allocation.
pub const USER_STACK_TOP: usize = 0x0000_7FFF_FFFF_0000;
/// Default user stack allocation size (128 KiB).
pub const USER_STACK_SIZE: usize = 128 * 1024; // 128 KiB User Stack

/// Standard 64-bit ELF file header representation.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    /// Magic number and architecture/endianness identification bytes.
    pub e_ident: [u8; 16],
    /// Object file type (e.g., ET_EXEC, ET_DYN).
    pub e_type: u16,
    /// Target machine instruction set architecture.
    pub e_machine: u16,
    /// Object file format version.
    pub e_version: u32,
    /// Virtual entry point address to transfer control to.
    pub e_entry: u64,
    /// Byte offset to the program header table.
    pub e_phoff: u64,
    /// Byte offset to the section header table.
    pub e_shoff: u64,
    /// Processor-specific flags.
    pub e_flags: u32,
    /// ELF header size in bytes.
    pub e_ehsize: u16,
    /// Size of a program header table entry in bytes.
    pub e_phentsize: u16,
    /// Number of entries in the program header table.
    pub e_phnum: u16,
    /// Size of a section header table entry in bytes.
    pub e_shentsize: u16,
    /// Number of entries in the section header table.
    pub e_shnum: u16,
    /// Section header index of the section name string table.
    pub e_shstrndx: u16,
}

/// Standard 64-bit ELF program header entry.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
    /// Segment type (e.g., PT_LOAD).
    pub p_type: u32,
    /// Segment flags and access permissions (PF_R, PF_W, PF_X).
    pub p_flags: u32,
    /// Offset of the segment in the file image.
    pub p_offset: u64,
    /// Virtual address of the segment in memory.
    pub p_vaddr: u64,
    /// Physical address of the segment (reserved/unused).
    pub p_paddr: u64,
    /// Size of the segment in the file image.
    pub p_filesz: u64,
    /// Size of the segment in memory (zero-filled remainder).
    pub p_memsz: u64,
    /// Required segment alignment boundary.
    pub p_align: u64,
}

/// Metadata describing the loaded ELF executable memory layout.
pub struct LoadedElf {
    /// Initial instruction pointer entry point.
    pub entry_point: usize,
    /// Initial user stack pointer (RSP) positioned after argv/envp/auxv setup.
    pub user_stack_top: usize,
}

/// Parses an ELF64 binary image, maps loadable segments into `vm_space`, and prepares the user stack.
pub fn load_elf(
    elf_bytes: &[u8],
    vm_space: &mut VmSpace,
    argv: &[&str],
    envp: &[&str],
) -> Result<LoadedElf, &'static str> {
    let header: Elf64Header = read_pod(elf_bytes, 0).ok_or("ELF file too small for header")?;

    if header.e_ident[0..4] != ELF_MAGIC {
        return Err("Invalid ELF magic");
    }
    if header.e_ident[4] != ELF_CLASS_64 {
        return Err("Not a 64-bit ELF");
    }
    if header.e_ident[5] != ELF_DATA_2LSB {
        return Err("Not a little-endian ELF");
    }
    if header.e_machine != EM_X86_64 {
        return Err("Not an x86_64 ELF");
    }

    let phoff = header.e_phoff as usize;
    let phentsize = header.e_phentsize as usize;
    let phnum = header.e_phnum as usize;
    let mut phdr_vaddr = 0usize;

    for i in 0..phnum {
        let offset = phoff + i * phentsize;
        let phdr: Elf64Phdr = read_pod(elf_bytes, offset).ok_or("Program header out of bounds")?;

        if phdr.p_type == PT_LOAD {
            let flags = PageFlags {
                present: true,
                writable: phdr.p_flags & PF_W != 0,
                user: true,
                no_exec: phdr.p_flags & PF_X == 0,
            };

            let vaddr = phdr.p_vaddr as usize;
            let memsz = phdr.p_memsz as usize;
            let filesz = phdr.p_filesz as usize;
            let file_offset = phdr.p_offset as usize;

            if file_offset + filesz > elf_bytes.len() {
                return Err("Segment data out of bounds in ELF file");
            }

            vm_space.alloc_and_map_range(vaddr, memsz, flags)?;

            if filesz > 0 {
                let segment_data = &elf_bytes[file_offset..file_offset + filesz];
                vm_space.write_bytes_to_space(vaddr, segment_data)?;
            }

            if phoff >= file_offset && phoff < file_offset + filesz && phdr_vaddr == 0 {
                phdr_vaddr = vaddr + (phoff - file_offset);
            }
        }
    }

    let stack_bottom = USER_STACK_TOP - USER_STACK_SIZE;
    vm_space.alloc_and_map_range(stack_bottom, USER_STACK_SIZE, PageFlags::user_data())?;

    let initial_rsp = setup_user_stack(
        vm_space,
        header.e_entry as usize,
        phdr_vaddr,
        phentsize,
        phnum,
        argv,
        envp,
    )?;

    Ok(LoadedElf {
        entry_point: header.e_entry as usize,
        user_stack_top: initial_rsp,
    })
}

/// Sets up the initial user stack below `USER_STACK_TOP` in conformance with the
/// System V AMD64 ABI:
///
///   High Address
///   +---------------------------------------+
///   | Environment strings (envp[0..M])      |
///   | Argument strings (argv[0..N])         |
///   +---------------------------------------+
///   | 16-byte stack alignment padding       |
///   +---------------------------------------+
///   | Auxiliary Vector (AT_NULL, ..., AT_*) |
///   | NULL (envp terminator)                |
///   | envp[M-1..0] pointers                 |
///   | NULL (argv terminator)                |
///   | argv[N-1..0] pointers                 |
///   | argc (u64)                            | <-- rsp (16-byte aligned)
///   +---------------------------------------+
///   Low Address
pub fn setup_user_stack(
    vm_space: &mut VmSpace,
    entry_point: usize,
    phdr_vaddr: usize,
    phentsize: usize,
    phnum: usize,
    argv: &[&str],
    envp: &[&str],
) -> Result<usize, &'static str> {
    let mut sp = USER_STACK_TOP;
    let stack_bottom = USER_STACK_TOP - USER_STACK_SIZE;

    // 1. Write environment strings to the stack
    let mut envp_ptrs = alloc::vec::Vec::new();
    for env in envp.iter() {
        let bytes = env.as_bytes();
        if sp < stack_bottom + bytes.len() + 1 {
            return Err("Environment list exceeds user stack limit (E2BIG)");
        }
        sp -= bytes.len() + 1;
        vm_space.write_bytes_to_space(sp, bytes)?;
        vm_space.write_bytes_to_space(sp + bytes.len(), &[0u8])?;
        envp_ptrs.push(sp);
    }

    // 2. Write argument strings to the stack
    let mut argv_ptrs = alloc::vec::Vec::new();
    for arg in argv.iter() {
        let bytes = arg.as_bytes();
        if sp < stack_bottom + bytes.len() + 1 {
            return Err("Argument list exceeds user stack limit (E2BIG)");
        }
        sp -= bytes.len() + 1;
        vm_space.write_bytes_to_space(sp, bytes)?;
        vm_space.write_bytes_to_space(sp + bytes.len(), &[0u8])?;
        argv_ptrs.push(sp);
    }

    // 3. Align sp to 8-byte pointer boundary
    sp &= !0x7;

    // 4. Auxiliary vector definition
    let auxv: [(u64, u64); 6] = [
        (AT_PAGESZ, 4096),
        (AT_ENTRY, entry_point as u64),
        (AT_PHDR, phdr_vaddr as u64),
        (AT_PHENT, phentsize as u64),
        (AT_PHNUM, phnum as u64),
        (AT_NULL, 0),
    ];

    // Calculate total pointer/auxv words:
    // argc (1) + argv_ptrs (N) + NULL (1) + envp_ptrs (M) + NULL (1) + auxv (6 * 2)
    let total_words = 1 + (argv_ptrs.len() + 1) + (envp_ptrs.len() + 1) + (auxv.len() * 2);
    let total_bytes = total_words * 8;

    // Align final stack pointer so that rsp % 16 == 0
    let target_sp = sp
        .checked_sub(total_bytes)
        .ok_or("Stack pointer underflow")?;
    let aligned_sp = target_sp & !0xF;
    if aligned_sp < stack_bottom {
        return Err("User stack frame exceeds user stack limit (E2BIG)");
    }
    let padding = target_sp - aligned_sp;
    sp -= padding;

    // 5. Write auxiliary vector entries (in reverse order for downwards growth)
    for &(key, val) in auxv.iter().rev() {
        sp -= 8;
        vm_space.write_bytes_to_space(sp, &val.to_ne_bytes())?;
        sp -= 8;
        vm_space.write_bytes_to_space(sp, &key.to_ne_bytes())?;
    }

    // 6. Write envp pointers
    sp -= 8;
    vm_space.write_bytes_to_space(sp, &0u64.to_ne_bytes())?; // envp NULL terminator
    for &env_ptr in envp_ptrs.iter().rev() {
        sp -= 8;
        vm_space.write_bytes_to_space(sp, &(env_ptr as u64).to_ne_bytes())?;
    }

    // 7. Write argv pointers
    sp -= 8;
    vm_space.write_bytes_to_space(sp, &0u64.to_ne_bytes())?; // argv NULL terminator
    for &arg_ptr in argv_ptrs.iter().rev() {
        sp -= 8;
        vm_space.write_bytes_to_space(sp, &(arg_ptr as u64).to_ne_bytes())?;
    }

    // 8. Write argc
    sp -= 8;
    let argc = argv.len() as u64;
    vm_space.write_bytes_to_space(sp, &argc.to_ne_bytes())?;

    if sp != aligned_sp {
        return Err("Stack alignment computation mismatch");
    }

    Ok(sp)
}
