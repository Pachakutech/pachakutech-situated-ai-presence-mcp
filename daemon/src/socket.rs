//! The Unix socket server. One connection at a time is fine for v0 — the
//! MCP Binding is the only expected client, and it's a single local process.

use crate::pipeline::SplatPipeline;
use crate::protocol::{Proposal, ProposalResult};
use crate::registry::Registry;
use crate::vulkan::VulkanContext;
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Mutex;

pub fn serve(socket_path: &Path, vk: &VulkanContext, pipeline: &SplatPipeline) -> std::io::Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    println!("[socket] listening on {}", socket_path.display());

    let registry = Mutex::new(Registry::new());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream, vk, pipeline, &registry),
            Err(e) => eprintln!("[socket] connection error: {e}"),
        }
    }
    Ok(())
}

fn handle_connection(
    stream: UnixStream,
    vk: &VulkanContext,
    pipeline: &SplatPipeline,
    registry: &Mutex<Registry>,
) {
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("[socket] failed to clone stream: {e}");
            return;
        }
    };
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            Ok(_) => continue,
            Err(e) => {
                eprintln!("[socket] read error: {e}");
                break;
            }
        };

        let result = match serde_json::from_str::<Proposal>(&line) {
            Ok(proposal) => dispatch(proposal, vk, pipeline, registry),
            Err(e) => {
                eprintln!("[socket] malformed proposal: {e}");
                ProposalResult::err("unknown", format!("malformed proposal: {e}"))
            }
        };

        let response = serde_json::to_string(&result).unwrap_or_else(|_| {
            json!({ "status": "error", "error": "failed to serialize result" }).to_string()
        });
        if let Err(e) = writeln!(writer, "{response}") {
            eprintln!("[socket] write error: {e}");
            break;
        }
    }
}

fn dispatch(
    proposal: Proposal,
    vk: &VulkanContext,
    pipeline: &SplatPipeline,
    registry: &Mutex<Registry>,
) -> ProposalResult {
    let proposal_id = proposal.proposal_id().to_string();
    let mut reg = registry.lock().expect("registry mutex poisoned");

    match proposal {
        Proposal::HighlightRegion { description, duration_seconds, .. } => {
            reg.highlight_region(vk, &description, duration_seconds);
            ProposalResult::ok(&proposal_id, Some(json!({ "regionId": format!("r-{proposal_id}") })))
        }
        Proposal::SpawnPresence { presence_id, source_context, style_hint, artifact_id, .. } => {
            reg.spawn_presence(&presence_id, &source_context, style_hint.as_deref(), artifact_id.as_deref());
            ProposalResult::ok(&proposal_id, Some(json!({ "presenceId": presence_id })))
        }
        Proposal::AnimatePresence { presence_id, text, .. } => {
            match reg.animate_presence(&presence_id, &text) {
                Ok(()) => ProposalResult::ok(&proposal_id, None),
                Err(e) => ProposalResult::err(&proposal_id, e),
            }
        }
        Proposal::RetirePresence { presence_id, .. } => match reg.retire_presence(&presence_id) {
            Ok(()) => ProposalResult::ok(&proposal_id, None),
            Err(e) => ProposalResult::err(&proposal_id, e),
        },
        Proposal::AddArtifact { artifact_id, description, source_uri, .. } => {
            reg.add_artifact(&artifact_id, &description, source_uri.as_deref(), pipeline);
            ProposalResult::ok(&proposal_id, Some(json!({ "artifactId": artifact_id })))
        }
        Proposal::RetireArtifact { artifact_id, .. } => match reg.retire_artifact(&artifact_id, pipeline) {
            Ok(()) => ProposalResult::ok(&proposal_id, None),
            Err(e) => ProposalResult::err(&proposal_id, e),
        },
    }
}
