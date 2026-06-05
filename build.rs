fn main() {
    println!("cargo:rerun-if-changed=kernel/linker.ld");
}
