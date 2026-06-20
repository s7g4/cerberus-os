use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Register custom cfg name for formal verification
    println!("cargo::rustc-check-cfg=cfg(kani)");

    // Determine the build output directory
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Copy memory.x to the build output directory so the linker can locate it
    fs::copy("memory.x", out_dir.join("memory.x")).unwrap();

    // Tell Cargo to add the output directory to the linker search path
    println!("cargo:rustc-link-search={}", out_dir.display());

    // Re-run this build script if memory.x or build.rs changes
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");
}
