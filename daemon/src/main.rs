mod actors;
mod pipeline;
mod protocol;
mod registry;
mod socket;
mod vulkan;

use pipeline::SplatPipeline;
use std::path::PathBuf;
use vulkan::VulkanContext;

const PIPELINE_CAPACITY: u32 = 256;

fn socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime_dir).join("pachakutech").join("presence.sock")
}

fn main() {
    println!("pachakutech-presence-daemon starting...");

    let vk = match VulkanContext::init() {
        Ok(vk) => {
            println!(
                "[vulkan] device: {} | zero-copy dma_buf import: {}",
                vk.device_name,
                if vk.supports_dma_buf_import { "yes" } else { "no (falls back to a copy path once one exists)" }
            );
            vk
        }
        Err(e) => {
            eprintln!("[vulkan] failed to initialize: {e}");
            eprintln!(
                "This is expected on a machine with no GPU/Vulkan driver (e.g. most CI and \
                 sandboxed dev environments). The daemon needs a real Vulkan-capable machine \
                 to run past this point — see README.md."
            );
            std::process::exit(1);
        }
    };

    let pipeline = match SplatPipeline::new(&vk, PIPELINE_CAPACITY) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[pipeline] failed to create: {e}");
            std::process::exit(1);
        }
    };
    match pipeline.smoke() {
        Ok(summary) => println!("[pipeline] {summary}"),
        Err(e) => {
            eprintln!("[pipeline] smoke tick failed: {e}");
            std::process::exit(1);
        }
    }

    let path = socket_path();
    if let Err(e) = socket::serve(&path, &vk, &pipeline) {
        eprintln!("[socket] fatal: {e}");
        std::process::exit(1);
    }
}
