//! The Unix socket server. One connection at a time is fine for v0 — the
//! MCP Binding is the only expected client, and it's a single local process.

use crate::pipeline::SplatPipeline;
use crate::protocol::{Proposal, ProposalResult};
use crate::registry::Registry;
use crate::vulkan::VulkanContext;
use serde_json::json;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Mutex;

struct Client {
    stream: UnixStream,
    buf: Vec<u8>,
}

/// Non-blocking Unix JSONL server, pumped from the overlay event loop.
pub struct SocketServer {
    listener: UnixListener,
    clients: Vec<Client>,
    registry: Mutex<Registry>,
}

impl SocketServer {
    pub fn bind(socket_path: &Path) -> io::Result<Self> {
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let listener = UnixListener::bind(socket_path)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, clients: Vec::new(), registry: Mutex::new(Registry::new()) })
    }

    pub fn listener_fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }

    pub fn client_fds(&self) -> impl Iterator<Item = RawFd> + '_ {
        self.clients.iter().map(|c| c.stream.as_raw_fd())
    }

    pub fn pump(&mut self, vk: &VulkanContext, pipeline: &SplatPipeline) -> io::Result<()> {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true)?;
                    self.clients.push(Client { stream, buf: Vec::new() });
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }

        let mut i = 0;
        while i < self.clients.len() {
            match read_client(&mut self.clients[i], vk, pipeline, &self.registry) {
                Ok(true) => i += 1,
                Ok(false) | Err(_) => {
                    self.clients.remove(i);
                }
            }
        }
        Ok(())
    }
}

/// Returns Ok(true) to keep the client, Ok(false) if it closed.
fn read_client(
    client: &mut Client,
    vk: &VulkanContext,
    pipeline: &SplatPipeline,
    registry: &Mutex<Registry>,
) -> io::Result<bool> {
    let mut tmp = [0u8; 4096];
    loop {
        match client.stream.read(&mut tmp) {
            Ok(0) => return Ok(false),
            Ok(n) => client.buf.extend_from_slice(&tmp[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
        }
    }
    while let Some(pos) = client.buf.iter().position(|&b| b == b'\n') {
        let line = client.buf.drain(..=pos).collect::<Vec<_>>();
        let line = String::from_utf8_lossy(&line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let result = match serde_json::from_str::<Proposal>(line) {
            Ok(proposal) => dispatch(proposal, vk, pipeline, registry),
            Err(e) => {
                eprintln!("[socket] malformed proposal: {e}");
                ProposalResult::err("unknown", format!("malformed proposal: {e}"))
            }
        };
        let response = serde_json::to_string(&result).unwrap_or_else(|_| {
            json!({ "status": "error", "error": "failed to serialize result" }).to_string()
        });
        if writeln!(client.stream, "{response}").is_err() {
            return Ok(false);
        }
    }
    Ok(true)
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
