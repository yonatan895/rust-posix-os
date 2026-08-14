//! Initramfs TAR Archive Parser.

use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::services::vfs::ramfs::{RamFsDir, RamFsFile};

#[repr(C, packed)]
struct TarHeader {
    name: [u8; 100],
    mode: [u8; 8],
    uid: [u8; 8],
    gid: [u8; 8],
    size: [u8; 12],
    mtime: [u8; 12],
    chksum: [u8; 8],
    typeflag: u8,
    linkname: [u8; 100],
    magic: [u8; 6],
    version: [u8; 2],
    uname: [u8; 32],
    gname: [u8; 32],
    devmajor: [u8; 8],
    devminor: [u8; 8],
    prefix: [u8; 155],
    padding: [u8; 12],
}

fn parse_octal(bytes: &[u8]) -> usize {
    let mut val = 0;
    for &b in bytes {
        if b >= b'0' && b <= b'7' {
            val = (val << 3) | (b - b'0') as usize;
        } else if b == 0 || b == b' ' {
            break;
        }
    }
    val
}

pub fn unpack_tar_archive(tar_data: &[u8], root_dir: &Arc<RamFsDir>) -> Result<usize, &'static str> {
    let mut offset = 0;
    let mut files_unpacked = 0;

    while offset + 512 <= tar_data.len() {
        let header_slice = &tar_data[offset..offset + 512];
        if header_slice.iter().all(|&b| b == 0) {
            break;
        }

        let header = unsafe { &*(header_slice.as_ptr() as *const TarHeader) };
        let name_len = header.name.iter().position(|&b| b == 0).unwrap_or(header.name.len());
        let path = core::str::from_utf8(&header.name[..name_len]).map_err(|_| "Invalid UTF-8 filename in tar")?;
        let size = parse_octal(&header.size);

        offset += 512;
        if offset + size > tar_data.len() {
            return Err("Tar file entry payload truncated");
        }

        let is_dir = header.typeflag == b'5' || path.ends_with('/');
        let trimmed_path = path.trim_matches('/');

        if !trimmed_path.is_empty() {
            let mut current = root_dir.clone();
            let components: Vec<&str> = trimmed_path.split('/').collect();

            for (i, component) in components.iter().enumerate() {
                let is_last = i == components.len() - 1;
                if is_last && !is_dir {
                    let file_data = tar_data[offset..offset + size].to_vec();
                    current.add_child(component, RamFsFile::new(file_data));
                    files_unpacked += 1;
                } else {
                    current = current.get_or_create_subdir(component);
                }
            }
        }

        // Advance past data blocks (512-byte aligned)
        let data_blocks = (size + 511) / 512;
        offset += data_blocks * 512;
    }

    Ok(files_unpacked)
}
