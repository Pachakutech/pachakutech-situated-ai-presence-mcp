mod actors;
mod avatar;
mod overlay;
mod pipeline;
mod protocol;
mod registry;
mod socket;
mod splat_sprites;
mod vulkan;

use overlay::{Overlay, OverlayGpu};
use pipeline::SplatPipeline;
use std::path::PathBuf;
use vulkan::VulkanContext;

/// Artifact ingest keeps the low slots. The avatar cloud is uploaded at
/// `ARTIFACT_SLOTS` and is not handed out by the registry.
const ARTIFACT_SLOTS: u32 = 4096;
const AVATAR_SLOTS: u32 = 50_000;
const PIPELINE_CAPACITY: u32 = ARTIFACT_SLOTS + AVATAR_SLOTS;

fn avatar_manifest() -> PathBuf {
    if let Ok(over) = std::env::var("PRESENCE_AVATAR_MANIFEST") {
        if !over.is_empty() {
            return PathBuf::from(over);
        }
    }
    let compiled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json");
    let candidates = [
        PathBuf::from("assets/avatar_manifest.json"),
        PathBuf::from("../assets/avatar_manifest.json"),
        compiled.clone(),
    ];
    candidates.into_iter().find(|p| p.exists()).unwrap_or(compiled)
}

fn socket_path() -> PathBuf {
    // Same override the Node client reads, so `presence daemon stop` reaches
    // the process it started.
    if let Ok(over) = std::env::var("PRESENCE_DAEMON_SOCKET") {
        if !over.is_empty() {
            return PathBuf::from(over);
        }
    }
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime_dir).join("pachakutech").join("presence.sock")
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(|s| s.as_str()) == Some("avatar-debug") {
        // Headless: no Wayland/Vulkan needed. See avatar/debug.rs.
        if let Err(e) = avatar::debug::run(&argv[2..]) {
            eprintln!("[avatar-debug] {e}");
            std::process::exit(1);
        }
        return;
    }
    println!("pachakutech-presence-daemon starting...");

    let mut overlay = match Overlay::connect() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[overlay] {e}");
            std::process::exit(1);
        }
    };

    let vk = match VulkanContext::init_for_wayland(overlay.display_ptr(), match overlay.surface_ptr() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[overlay] {e}");
            std::process::exit(1);
        }
    }) {
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

    let mut gpu = match OverlayGpu::new(&vk, overlay.extent(), overlay.capture_extent()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("[overlay] gpu: {e}");
            std::process::exit(1);
        }
    };

    let pipeline = match SplatPipeline::new(&vk, PIPELINE_CAPACITY, ARTIFACT_SLOTS) {
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

    let manifest = avatar_manifest();
    let mut avatar = match avatar::AvatarActor::load(&manifest) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[avatar] failed to load {}: {e}", manifest.display());
            gpu.destroy(&vk);
            std::process::exit(1);
        }
    };
    let Some(jaw) = avatar.jaw_joint else {
        eprintln!("[avatar] jaw_joint is not a joint on {}", manifest.display());
        gpu.destroy(&vk);
        std::process::exit(1);
    };
    if avatar.splat_count() as u32 > AVATAR_SLOTS {
        eprintln!(
            "[avatar] cloud has {} splats; the reserved range holds {AVATAR_SLOTS}",
            avatar.splat_count()
        );
        gpu.destroy(&vk);
        std::process::exit(1);
    }
    println!(
        "[avatar] {} splats | {} joints | jaw {} | {} | {}",
        avatar.splat_count(),
        avatar.rig.names.len(),
        avatar.rig.names[jaw],
        avatar.binding.fingerprint(),
        manifest.display()
    );

    let path = socket_path();
    let result = overlay::run(&mut overlay, &mut gpu, &vk, &pipeline, &path, &mut avatar);
    gpu.destroy(&vk);
    if let Err(e) = result {
        eprintln!("[overlay] {e}");
        std::process::exit(1);
    }
}
