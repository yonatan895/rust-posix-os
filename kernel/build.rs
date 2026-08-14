fn main() {
    println!("cargo:rerun-if-changed=linker.ld");
    println!("cargo:rustc-link-arg=-Tkernel/linker.ld");
    println!("cargo:rustc-link-arg=-no-pie");
    println!("cargo:rustc-link-arg=-zmax-page-size=0x1000");
}
