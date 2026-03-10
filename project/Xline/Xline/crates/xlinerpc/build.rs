fn main() {
    // No longer compile proto files
    println!("cargo:rerun-if-changed=build.rs");
}