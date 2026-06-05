fn main() {
    println!("cargo:rerun-if-changed=kernel/linker.ld");
    println!("cargo:rerun-if-changed=user/gui/src/main.rs");
    println!("cargo:rerun-if-changed=user/gui/linker.ld");
}
