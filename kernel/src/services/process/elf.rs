//! ELF64 Executable Loader - De-privileged Safe Service.

use crate::ostd::mm::{PAGE_PRESENT, PAGE_USER, PAGE_WRITABLE, PAGE_NX, VmSpace};

pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const ELF_CLASS_64: u8 = 2;
pub const ELF_DATA_2LSB: u8 = 1;
pub const EM_X86_64: u16 = 0x3E;

pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

pub const USER_STACK_TOP: usize = 0x0000_7FFF_FFFF_0000;
pub const USER_STACK_SIZE: usize = 128 * 1024; // 128 KiB User Stack

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

pub struct LoadedElf {
    pub entry_point: usize,
    pub user_stack_top: usize,
}

pub fn load_elf(elf_bytes: &[u8], vm_space: &mut VmSpace) -> Result<LoadedElf, &'static str> {
    if elf_bytes.len() < core::mem::size_of::<Elf64Header>() {
        return Err("ELF file too small for header");
    }

    let header = unsafe { &*(elf_bytes.as_ptr() as *const Elf64Header) };

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

    for i in 0..phnum {
        let offset = phoff + i * phentsize;
        if offset + core::mem::size_of::<Elf64Phdr>() > elf_bytes.len() {
            return Err("Program header out of bounds");
        }
        let phdr = unsafe { &*(elf_bytes.as_ptr().add(offset) as *const Elf64Phdr) };

        if phdr.p_type == PT_LOAD {
            let mut flags = PAGE_PRESENT | PAGE_USER;
            if phdr.p_flags & PF_W != 0 {
                flags |= PAGE_WRITABLE;
            }
            if phdr.p_flags & PF_X == 0 {
                flags |= PAGE_NX;
            }

            let vaddr = phdr.p_vaddr as usize;
            let memsz = phdr.p_memsz as usize;
            let filesz = phdr.p_filesz as usize;
            let file_offset = phdr.p_offset as usize;

            if file_offset + filesz > elf_bytes.len() {
                return Err("Segment data out of bounds in ELF file");
            }

            // Map segment pages
            vm_space.alloc_and_map_range(vaddr, memsz, flags)?;

            // Copy file data into the newly mapped segment
            if filesz > 0 {
                let segment_data = &elf_bytes[file_offset..file_offset + filesz];
                vm_space.write_bytes_to_space(vaddr, segment_data)?;
            }
        }
    }

    // Allocate user stack
    let stack_bottom = USER_STACK_TOP - USER_STACK_SIZE;
    vm_space.alloc_and_map_range(stack_bottom, USER_STACK_SIZE, PAGE_PRESENT | PAGE_USER | PAGE_WRITABLE)?;

    Ok(LoadedElf {
        entry_point: header.e_entry as usize,
        user_stack_top: USER_STACK_TOP,
    })
}
