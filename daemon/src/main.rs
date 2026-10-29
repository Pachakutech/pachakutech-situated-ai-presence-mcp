mod actors;
mod avatar;
mod highlight;
mod overlay;
mod placement;
mod screen_heat;
mod screen_patch;
mod screen_text;
mod pipeline;
mod avatar_expand;
mod protocol;
mod gate;
mod ledger;
mod ui_input;
mod task;
mod registry;
mod socket;
mod speech;
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

/// PRESENCE_AVATAR_AUTOLOAD=1: show the avatar with demo sway as soon as the
/// daemon starts (the old dev behaviour). Default: the avatar appears only
/// while a presence (spawnPresence) owns it; otherwise the layer shows the
/// idle hyperbubble disc.
fn avatar_autoload() -> bool {
    matches!(std::env::var("PRESENCE_AVATAR_AUTOLOAD").as_deref(), Ok("1") | Ok("true") | Ok("yes"))
}

fn avatar_manifest() -> PathBuf {
    if let Ok(over) = std::env::var("PRESENCE_AVATAR_MANIFEST") {
        if !over.is_empty() {
            return PathBuf::from(over);
        }
    }
    // Absolute path baked in at build time. On the machine that compiled this
    // binary it is the checkout's assets/, so an export is what the next start
    // loads. An install on another machine does not have that path.
    let compiled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/avatar_manifest.json");
    if compiled.exists() {
        return compiled;
    }
    // npm ships a copy beside the binary: native/linux-x64/assets/. stage-daemon
    // fills it from assets/ at package time. It is not a second file to keep by hand.
    let beside_exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("assets/avatar_manifest.json")));
    let candidates = [
        beside_exe.unwrap_or_else(|| PathBuf::from("/nonexistent")),
        PathBuf::from("assets/avatar_manifest.json"),
        PathBuf::from("../assets/avatar_manifest.json"),
    ];
    candidates.into_iter().find(|p| p.exists()).unwrap_or_else(|| PathBuf::from("assets/avatar_manifest.json"))
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

fn command_socket_path() -> PathBuf {
    if let Ok(over) = std::env::var("PRESENCE_COMMAND_SOCKET") {
        if !over.is_empty() {
            return PathBuf::from(over);
        }
    }
    socket_path().with_file_name("command.sock")
}

fn sidecar_socket_path() -> PathBuf {
    if let Ok(over) = std::env::var("PRESENCE_SIDECAR_SOCKET") {
        if !over.is_empty() {
            return PathBuf::from(over);
        }
    }
    socket_path().with_file_name("sidecar.sock")
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

    let autoload = avatar_autoload();
    let manifest = avatar_manifest();
    let mut avatar = match avatar::AvatarActor::load(&manifest) {
        Ok(a) if a.jaw_joint.is_none() => {
            eprintln!("[avatar] jaw_joint is not a joint on {}", manifest.display());
            if autoload { gpu.destroy(&vk); std::process::exit(1); }
            None
        }
        Ok(a) if a.splat_count() as u32 > AVATAR_SLOTS => {
            eprintln!("[avatar] cloud has {} splats; the reserved range holds {AVATAR_SLOTS}", a.splat_count());
            if autoload { gpu.destroy(&vk); std::process::exit(1); }
            None
        }
        Ok(mut a) => {
            a.autoload = autoload;
            a.visible = autoload;
            a.demo_motion = autoload;
            println!(
                "[avatar] {} splats | {} joints | jaw {} | {} | {} | {}",
                a.splat_count(),
                a.rig.names.len(),
                a.rig.names[a.jaw_joint.unwrap_or(0)],
                a.binding.fingerprint(),
                manifest.display(),
                if autoload { "AUTOLOAD: visible with demo motion" } else { "idle: appears when a presence is spawned" }
            );
            Some(a)
        }
        Err(e) => {
            eprintln!("[avatar] failed to load {}: {e}", manifest.display());
            if autoload { gpu.destroy(&vk); std::process::exit(1); }
            eprintln!("[avatar] continuing without an avatar body; spawnPresence will report avatar:false");
            None
        }
    };

    let mut expand = None;
    if let Some(actor) = avatar.as_mut() {
        match avatar_expand::AvatarExpand::new(&vk, &pipeline, actor) {
            Ok(mut ex) => {
                match ex.check(actor, &pipeline) {
                    Ok(report) => {
                        ex.set_live(true);
                        println!("[avatar] gpu expand on: {report}");
                    }
                    Err(e) => {
                        eprintln!("[avatar] gpu expand check failed, CPU path stays: {e}");
                    }
                }
                expand = Some(ex);
            }
            Err(e) => eprintln!("[avatar] gpu expand unavailable, CPU path stays: {e}"),
        }
    }

    let path = socket_path();
    let command = command_socket_path();
    let sidecar = sidecar_socket_path();
    let result = overlay::run(
        &mut overlay,
        &mut gpu,
        &vk,
        &pipeline,
        &path,
        &command,
        &sidecar,
        &mut avatar,
        expand.as_mut(),
    );
    gpu.destroy(&vk);
    if let Err(e) = result {
        eprintln!("[overlay] {e}");
        std::process::exit(1);
    }
}
