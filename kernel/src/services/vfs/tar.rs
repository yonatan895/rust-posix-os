//! Initramfs TAR Archive Parser.

use crate::ostd::mm::read_pod;
use crate::services::vfs::ramfs::{RamFsDir, RamFsFile};
use alloc::sync::Arc;
use alloc::vec::Vec;

/// POSIX ustar / tar archive 512-byte header record.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TarHeader {
    /// File path name.
    name: [u8; 100],
    /// File permission mode in octal ASCII.
    mode: [u8; 8],
    /// Owner user ID in octal ASCII.
    uid: [u8; 8],
    /// Owner group ID in octal ASCII.
    gid: [u8; 8],
    /// File size in octal ASCII.
    size: [u8; 12],
    /// Modification time in octal ASCII.
    mtime: [u8; 12],
    /// Header checksum in octal ASCII.
    chksum: [u8; 8],
    /// Type flag character ('0' regular, '5' directory).
    typeflag: u8,
    /// Target name for symbolic links.
    linkname: [u8; 100],
    /// UStar indicator magic string.
    magic: [u8; 6],
    /// UStar version string.
    version: [u8; 2],
    /// Owner user name.
    uname: [u8; 32],
    /// Owner group name.
    gname: [u8; 32],
    /// Major device number.
    devmajor: [u8; 8],
    /// Minor device number.
    devminor: [u8; 8],
    /// Pathname prefix.
    prefix: [u8; 155],
    /// Header alignment padding.
    padding: [u8; 12],
}

/// Parses an ASCII octal number string from a tar header field.
fn parse_octal(bytes: &[u8]) -> usize {
    let mut val = 0;
    for &b in bytes {
        if (b'0'..=b'7').contains(&b) {
            val = (val << 3) | (b - b'0') as usize;
        } else if b == 0 || b == b' ' {
            break;
        }
    }
    val
}

/// Unpacks a ustar formatted initramfs byte buffer into `root_dir`.
pub fn unpack_tar_archive(
    tar_data: &[u8],
    root_dir: &Arc<RamFsDir>,
) -> Result<usize, &'static str> {
    let mut offset = 0;
    let mut files_unpacked = 0;

    while offset + 512 <= tar_data.len() {
        let header_slice = &tar_data[offset..offset + 512];
        if header_slice.iter().all(|&b| b == 0) {
            break;
        }

        let header: TarHeader = read_pod(header_slice, 0).ok_or("Tar header truncated")?;
        let name = header.name;
        let name_len = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        let path =
            core::str::from_utf8(&name[..name_len]).map_err(|_| "Invalid UTF-8 filename in tar")?;
        let size = parse_octal(&header.size);
        let typeflag = header.typeflag;

        offset += 512;
        if offset + size > tar_data.len() {
            return Err("Tar file entry payload truncated");
        }

        let is_dir = typeflag == b'5' || path.ends_with('/');
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

        let data_blocks = size.div_ceil(512);
        offset += data_blocks * 512;
    }

    Ok(files_unpacked)
}
