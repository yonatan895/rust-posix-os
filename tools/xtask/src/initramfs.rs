//! POSIX tarball packaging for userland initramfs images.

use crate::build::strip_binary;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Assembles stripped userland binaries into an `initramfs.tar` archive for the kernel.
pub fn create_initramfs() {
    println!("[xtask] Packaging initramfs.tar archive...");
    let target_dir = Path::new("target/x86_64-unknown-none/debug");
    let initramfs_path = target_dir.join("initramfs.tar");
    let mut tar_file = File::create(&initramfs_path).expect("Failed to create initramfs.tar");
    pack_bin(&mut tar_file, &target_dir.join("init"), "bin/init");
    pack_bin(&mut tar_file, &target_dir.join("shell"), "bin/sh");
    pack_bin(
        &mut tar_file,
        &target_dir.join("coreutils"),
        "bin/coreutils",
    );
    let motd = b"Welcome to Rust POSIX OS\nPOSIX.1-2024 Compliant Framekernel\n\n";
    write_tar_entry(&mut tar_file, "etc/motd", motd, false);
    let zero = [0u8; 512];
    tar_file.write_all(&zero).unwrap();
    tar_file.write_all(&zero).unwrap();
    println!("[xtask] Successfully created {}", initramfs_path.display());
}

/// Strips an executable binary and records it as an entry in the tar archive.
pub fn pack_bin(tar: &mut File, src: &Path, dest: &str) {
    strip_binary(src);
    match fs::read(src) {
        Ok(data) => {
            if data.len() > 512 * 1024 {
                eprintln!(
                    "[xtask] warning: {} is {} bytes after strip; unpack may stress the kernel heap",
                    dest,
                    data.len()
                );
            }
            write_tar_entry(tar, dest, &data, false);
            println!("[xtask]   + Packed /{} ({} bytes)", dest, data.len());
        }
        Err(e) => eprintln!("[xtask] warning: skip {}: {}", src.display(), e),
    }
}

/// Writes a standard POSIX ustar tar header and aligned data blocks to the output stream.
pub fn write_tar_entry<W: Write>(writer: &mut W, name: &str, data: &[u8], is_dir: bool) {
    let mut header = [0u8; 512];
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(99);
    header[..name_len].copy_from_slice(&name_bytes[..name_len]);
    header[100..108].copy_from_slice(b"0000755\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size_str = format!("{:011o}\0", data.len());
    header[124..136].copy_from_slice(size_str.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[156] = if is_dir { b'5' } else { b'0' };
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[148..156].fill(b' ');
    let chksum: u32 = header.iter().map(|&b| b as u32).sum();
    let chksum_str = format!("{:06o}\0 ", chksum);
    header[148..156].copy_from_slice(chksum_str.as_bytes());
    writer.write_all(&header).unwrap();
    if !data.is_empty() {
        writer.write_all(data).unwrap();
        let rem = data.len() % 512;
        if rem != 0 {
            let padding = [0u8; 512];
            writer.write_all(&padding[..512 - rem]).unwrap();
        }
    }
}
