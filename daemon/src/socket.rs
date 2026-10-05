//! The Unix socket server. One connection at a time is fine for v0 — the
//! MCP Binding is the only expected client, and it's a single local process.

use crate::pipeline::SplatPipeline;
use crate::protocol::{AvatarCommand, Proposal, ProposalResult};
use crate::registry::Registry;
use crate::vulkan::VulkanContext;
use serde_json::json;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct Client {
    stream: UnixStream,
    buf: Vec<u8>,
}

/// Non-blocking Unix JSONL server, pumped from the overlay event loop.
pub struct SocketServer {
    listener: UnixListener,
    path: PathBuf,
    clients: Vec<Client>,
    registry: Mutex<Registry>,
    avatar_commands: Vec<AvatarCommand>,
}

enum ClientRead {
    Keep,
    Closed,
    Shutdown,
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
        Ok(Self {
            listener,
            path: socket_path.to_path_buf(),
            clients: Vec::new(),
            registry: Mutex::new(Registry::new()),
            avatar_commands: Vec::new(),
        })
    }

    pub fn take_avatar_commands(&mut self) -> Vec<AvatarCommand> {
        std::mem::take(&mut self.avatar_commands)
    }

    pub fn listener_fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }

    pub fn client_fds(&self) -> impl Iterator<Item = RawFd> + '_ {
        self.clients.iter().map(|c| c.stream.as_raw_fd())
    }

    /// `Ok(true)` means a client asked the process to exit after its reply was written.
    pub fn pump(&mut self, vk: &VulkanContext, pipeline: &SplatPipeline) -> io::Result<bool> {
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
            match read_client(&mut self.clients[i], vk, pipeline, &self.registry, &mut self.avatar_commands) {
                Ok(ClientRead::Keep) => i += 1,
                Ok(ClientRead::Closed) | Err(_) => {
                    self.clients.remove(i);
                }
                Ok(ClientRead::Shutdown) => {
                    self.clients.remove(i);
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

impl Drop for SocketServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// `presence.pid` beside the socket. Removed on drop only when it still names this process.
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    pub fn create(socket_path: &Path) -> io::Result<Self> {
        let path = socket_path.with_extension("pid");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, format!("{}\n", std::process::id()))?;
        Ok(Self { path })
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return;
        };
        if text.trim() == std::process::id().to_string() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Keep the client, drop it, or leave the overlay loop after the reply is flushed.
fn read_client(
    client: &mut Client,
    vk: &VulkanContext,
    pipeline: &SplatPipeline,
    registry: &Mutex<Registry>,
    avatar_commands: &mut Vec<AvatarCommand>,
) -> io::Result<ClientRead> {
    let mut tmp = [0u8; 4096];
    loop {
        match client.stream.read(&mut tmp) {
            Ok(0) => return Ok(ClientRead::Closed),
            Ok(n) => client.buf.extend_from_slice(&tmp[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(e),
        }
    }
    let mut shutdown = false;
    while let Some(pos) = client.buf.iter().position(|&b| b == b'\n') {
        let line = client.buf.drain(..=pos).collect::<Vec<_>>();
        let line = String::from_utf8_lossy(&line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (result, stop) = match serde_json::from_str::<Proposal>(line) {
            Ok(proposal) => dispatch(proposal, vk, pipeline, registry, avatar_commands),
            Err(e) => {
                eprintln!("[socket] malformed proposal: {e}");
                (ProposalResult::err("unknown", format!("malformed proposal: {e}")), false)
            }
        };
        let response = serde_json::to_string(&result).unwrap_or_else(|_| {
            json!({ "status": "error", "error": "failed to serialize result" }).to_string()
        });
        if writeln!(client.stream, "{response}").is_err() || client.stream.flush().is_err() {
            return Ok(ClientRead::Closed);
        }
        if stop {
            shutdown = true;
            break;
        }
    }
    if shutdown {
        Ok(ClientRead::Shutdown)
    } else {
        Ok(ClientRead::Keep)
    }
}

/// Second value is true when the overlay loop should exit after this reply.
fn dispatch(
    proposal: Proposal,
    vk: &VulkanContext,
    pipeline: &SplatPipeline,
    registry: &Mutex<Registry>,
    avatar_commands: &mut Vec<AvatarCommand>,
) -> (ProposalResult, bool) {
    let proposal_id = proposal.proposal_id().to_string();
    if matches!(proposal, Proposal::Shutdown { .. }) {
        return (ProposalResult::ok(&proposal_id, None), true);
    }
    match &proposal {
        Proposal::AvatarRest { .. } => {
            avatar_commands.push(AvatarCommand::Rest);
            return (ProposalResult::ok(&proposal_id, None), false);
        }
        Proposal::AvatarFace { jaw_open, morph_index, morph_weight, .. } => {
            avatar_commands.push(AvatarCommand::Face {
                jaw_open: *jaw_open,
                morph_index: *morph_index,
                morph_weight: morph_weight.unwrap_or(0.0),
            });
            return (ProposalResult::ok(&proposal_id, None), false);
        }
        Proposal::AvatarWalk { x, z, .. } => {
            avatar_commands.push(AvatarCommand::Walk { x: *x, z: *z });
            return (ProposalResult::ok(&proposal_id, None), false);
        }
        Proposal::AvatarSpeak { text, .. } => {
            // Validate here so the client gets the error; synthesis happens off-thread.
            if let Err(e) = crate::speech::pipeline::normalize_text(text) {
                return (ProposalResult::err(&proposal_id, e), false);
            }
            avatar_commands.push(AvatarCommand::Speak { text: text.clone() });
            return (ProposalResult::ok(&proposal_id, None), false);
        }
        Proposal::AvatarStop { .. } => {
            avatar_commands.push(AvatarCommand::StopSpeech);
            return (ProposalResult::ok(&proposal_id, None), false);
        }
        _ => {}
    }
    let mut reg = registry.lock().expect("registry mutex poisoned");

    let result = match proposal {
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
        Proposal::Shutdown { .. }
        | Proposal::AvatarRest { .. }
        | Proposal::AvatarFace { .. }
        | Proposal::AvatarSpeak { .. }
        | Proposal::AvatarStop { .. }
        | Proposal::AvatarWalk { .. } => ProposalResult::ok(&proposal_id, None),
    };
    (result, false)
}
