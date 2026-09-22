//! Compiles `shaders/*.comp` to SPIR-V at build time so `pipeline.rs` can
//! `include_bytes!` them directly into the binary — no runtime file I/O,
//! no "find the shaders directory relative to the binary" problem.
//!
//! This needs `glslangValidator` (or a symlink/wrapper named that) on
//! PATH at build time — part of the Vulkan SDK, and on Arch/Omarchy it's
//! the `glslang` package (`pacman -S glslang`). That's a real, ordinary
//! build-time dependency for a Vulkan compute project (shader compilation
//! is always a build step somewhere); it's not needed at runtime, since
//! the compiled SPIR-V is embedded in the binary.

use std::process::Command;

fn compile(shader_name: &str, out_dir: &str) {
    let src = format!("shaders/{shader_name}");
    let out = format!("{out_dir}/{shader_name}.spv");
    println!("cargo:rerun-if-changed={src}");

    let status = Command::new("glslangValidator")
        .args(["-V", "--target-env", "vulkan1.1", &src, "-o", &out])
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => panic!(
            "glslangValidator exited with {s} compiling {src} — check the shader for errors \
             (run the same command by hand for the full diagnostic)"
        ),
        Err(e) => panic!(
            "couldn't run glslangValidator to compile {src}: {e}. Install it — on Arch/Omarchy: \
             `sudo pacman -S glslang`; on Debian/Ubuntu: `apt install glslang-tools`."
        ),
    }
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    // math_utils.comp is #include-only (no #version/main). gltf_to_splat.comp
    // is not dispatched yet, but compiling it here stops it rotting.
    for shader in ["splat_projection.comp", "splat_eviction.comp", "gltf_to_splat.comp"] {
        compile(shader, &out_dir);
    }
    println!("cargo:rerun-if-changed=shaders/math_utils.comp");
}
