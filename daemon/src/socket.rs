//! The Unix socket server. One connection at a time is fine for v0 — the
//! MCP Binding is the only expected client, and it's a single local process.

use crate::pipeline::SplatPipeline;
use crate::protocol::{AvatarCommand, Proposal, ProposalResult};
use crate::registry::Registry;
use serde_json::json;
use std::io::{self, Read, Write};
use std::time::Instant;
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

    pub fn set_avatar_available(&mut self, available: bool) {
        self.registry.lock().expect("registry mutex poisoned").set_avatar_available(available);
    }

    pub fn take_avatar_commands(&mut self) -> Vec<AvatarCommand> {
        std::mem::take(&mut self.avatar_commands)
    }

    /// UV boxes whose mark should be on screen. The overlay thread calls this.
    pub fn visible_interests(&self) -> Vec<(String, crate::highlight::UvBox)> {
        self.registry.lock().expect("registry mutex poisoned").visible_interests(Instant::now())
    }

    /// The mark still showing for this presence. A faded box stays in the book
    /// and is not returned here; the point follows the mark, not that leftover.
    pub fn visible_interest(&self, id: &str) -> Option<crate::highlight::UvBox> {
        self.visible_interests().into_iter().find(|(i, _)| i == id).map(|(_, b)| b)
    }

    pub fn listener_fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }

    pub fn client_fds(&self) -> impl Iterator<Item = RawFd> + '_ {
        self.clients.iter().map(|c| c.stream.as_raw_fd())
    }

    /// `Ok(true)` means a client asked the process to exit after its reply was written.
    pub fn pump(&mut self, pipeline: &SplatPipeline) -> io::Result<bool> {
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
            match read_client(&mut self.clients[i], pipeline, &self.registry, &mut self.avatar_commands) {
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
            Ok(proposal) => dispatch(proposal, pipeline, registry, avatar_commands),
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
            // DEBUG path (no presence). Validate here so the client gets the error.
            if let Err(e) = crate::speech::pipeline::normalize_text(text) {
                return (ProposalResult::err(&proposal_id, e), false);
            }
            avatar_commands.push(AvatarCommand::Speak { presence_id: None, text: text.clone() });
            return (ProposalResult::ok(&proposal_id, None), false);
        }
        Proposal::AvatarStop { .. } => {
            avatar_commands.push(AvatarCommand::StopSpeech { presence_id: None });
            return (ProposalResult::ok(&proposal_id, None), false);
        }
        _ => {}
    }
    let mut reg = registry.lock().expect("registry mutex poisoned");

    if let Some(result) = route_presence(&proposal, &mut reg, avatar_commands) {
        return (result, false);
    }

    let result = match proposal {
        Proposal::HighlightRegion { presence_id, description, u, v, w, h, duration_seconds, .. } => {
            match reg.declare_interest(
                &presence_id,
                &description,
                crate::highlight::UvBox { u, v, w, h },
                duration_seconds,
                Instant::now(),
            ) {
                Ok((box_, secs)) => ProposalResult::ok(
                    &proposal_id,
                    Some(json!({
                        "regionId": format!("r-{proposal_id}"),
                        "resolvedBounds": [box_.u, box_.v, box_.w, box_.h],
                        "durationSeconds": secs,
                    })),
                ),
                Err(e) => ProposalResult::err(&proposal_id, e),
            }
        }
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
        | Proposal::SpawnPresence { .. }
        | Proposal::AnimatePresence { .. }
        | Proposal::RetirePresence { .. }
        | Proposal::StopPresence { .. }
        | Proposal::AvatarSpeak { .. }
        | Proposal::AvatarStop { .. }
        | Proposal::AvatarWalk { .. } => ProposalResult::ok(&proposal_id, None),
    };
    (result, false)
}

/// Presence-scoped control plane. Pure bookkeeping + command emission (no GPU),
/// so it is unit-testable. Returns None for kinds it does not own.
///
/// - spawnPresence: creates the presence; if the (single) avatar body is free
///   and loaded, binds it to this presence and shows it.
/// - animatePresence: the text is SPOKEN by that presence's avatar.
/// - stopPresence: stops that presence's speech, face to neutral.
/// - retirePresence: stops its speech, releases and hides the body.
pub fn route_presence(proposal: &Proposal, reg: &mut Registry, cmds: &mut Vec<AvatarCommand>) -> Option<ProposalResult> {
    match proposal {
        Proposal::SpawnPresence { proposal_id, presence_id, source_context, style_hint, artifact_id } => {
            let granted = reg.spawn_presence(presence_id, source_context, style_hint.as_deref(), artifact_id.as_deref());
            if granted {
                cmds.push(AvatarCommand::Bind { presence_id: presence_id.clone() });
            }
            let note = if granted {
                "avatar body bound"
            } else if reg.avatar_owner().is_some() {
                "no avatar body: it is bound to another presence (one body is supported)"
            } else {
                "no avatar body: avatar assets are not loaded"
            };
            Some(ProposalResult::ok(proposal_id, Some(json!({ "presenceId": presence_id, "avatar": granted, "note": note }))))
        }
        Proposal::AnimatePresence { proposal_id, presence_id, text } => {
            if !reg.has_presence(presence_id) {
                return Some(ProposalResult::err(proposal_id, format!("no live presence with id {presence_id}")));
            }
            if !reg.has_avatar(presence_id) {
                return Some(ProposalResult::err(proposal_id, format!("presence {presence_id} has no avatar body to speak with")));
            }
            if let Err(e) = crate::speech::pipeline::normalize_text(text) {
                return Some(ProposalResult::err(proposal_id, e));
            }
            let _ = reg.animate_presence(presence_id, text);
            cmds.push(AvatarCommand::Speak { presence_id: Some(presence_id.clone()), text: text.clone() });
            Some(ProposalResult::ok(proposal_id, None))
        }
        Proposal::StopPresence { proposal_id, presence_id } => {
            if !reg.has_presence(presence_id) {
                return Some(ProposalResult::err(proposal_id, format!("no live presence with id {presence_id}")));
            }
            if reg.has_avatar(presence_id) {
                cmds.push(AvatarCommand::StopSpeech { presence_id: Some(presence_id.clone()) });
            }
            Some(ProposalResult::ok(proposal_id, None))
        }
        Proposal::RetirePresence { proposal_id, presence_id } => Some(match reg.retire_presence(presence_id) {
            Ok(owned) => {
                if owned {
                    cmds.push(AvatarCommand::Unbind { presence_id: presence_id.clone() });
                }
                ProposalResult::ok(proposal_id, None)
            }
            Err(e) => ProposalResult::err(proposal_id, e),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(json: &str) -> Proposal { serde_json::from_str(json).unwrap() }
    fn run(reg: &mut Registry, cmds: &mut Vec<AvatarCommand>, json: &str) -> ProposalResult {
        route_presence(&p(json), reg, cmds).expect("routed")
    }
    const SPAWN: &str = r#"{"kind":"spawnPresence","proposalId":"1","presenceId":"%ID%","sourceContext":"x"}"#;
    fn spawn(id: &str) -> String { SPAWN.replace("%ID%", id) }

    #[test]
    fn speak_targets_a_presence_and_requires_its_body() {
        let (mut reg, mut cmds) = (Registry::new(), vec![]);
        reg.set_avatar_available(true);
        let r = run(&mut reg, &mut cmds, &spawn("p-a"));
        assert_eq!(r.status, "ok");
        assert_eq!(r.detail.as_ref().unwrap()["avatar"], true);
        assert!(matches!(cmds.as_slice(), [AvatarCommand::Bind { presence_id }] if presence_id == "p-a"));
        cmds.clear();
        let r = run(&mut reg, &mut cmds, r#"{"kind":"animatePresence","proposalId":"2","presenceId":"p-a","text":"Hello  there"}"#);
        assert_eq!(r.status, "ok");
        assert!(matches!(cmds.as_slice(), [AvatarCommand::Speak { presence_id: Some(id), text }] if id == "p-a" && text == "Hello  there"));
        cmds.clear();
        // unknown presence, empty text
        assert_eq!(run(&mut reg, &mut cmds, r#"{"kind":"animatePresence","proposalId":"3","presenceId":"nope","text":"hi"}"#).status, "error");
        assert_eq!(run(&mut reg, &mut cmds, r#"{"kind":"animatePresence","proposalId":"4","presenceId":"p-a","text":"   "}"#).status, "error");
        assert!(cmds.is_empty());
    }

    #[test]
    fn second_presence_has_no_body_and_cannot_speak() {
        let (mut reg, mut cmds) = (Registry::new(), vec![]);
        reg.set_avatar_available(true);
        run(&mut reg, &mut cmds, &spawn("p-a"));
        cmds.clear();
        let r = run(&mut reg, &mut cmds, &spawn("p-b"));
        assert_eq!(r.detail.as_ref().unwrap()["avatar"], false);
        assert!(cmds.is_empty());
        let r = run(&mut reg, &mut cmds, r#"{"kind":"animatePresence","proposalId":"2","presenceId":"p-b","text":"hi"}"#);
        assert_eq!(r.status, "error");
        assert!(r.error.unwrap().contains("no avatar body"));
        // retiring the owner frees the body for the next spawn
        let r = run(&mut reg, &mut cmds, r#"{"kind":"retirePresence","proposalId":"3","presenceId":"p-a"}"#);
        assert_eq!(r.status, "ok");
        assert!(matches!(cmds.as_slice(), [AvatarCommand::Unbind { presence_id }] if presence_id == "p-a"));
        cmds.clear();
        let r = run(&mut reg, &mut cmds, &spawn("p-c"));
        assert_eq!(r.detail.as_ref().unwrap()["avatar"], true);
    }

    #[test]
    fn stop_and_retire_are_scoped_and_idempotent_errors() {
        let (mut reg, mut cmds) = (Registry::new(), vec![]);
        reg.set_avatar_available(true);
        run(&mut reg, &mut cmds, &spawn("p-a"));
        cmds.clear();
        assert_eq!(run(&mut reg, &mut cmds, r#"{"kind":"stopPresence","proposalId":"s","presenceId":"p-a"}"#).status, "ok");
        assert!(matches!(cmds.as_slice(), [AvatarCommand::StopSpeech { presence_id: Some(id) }] if id == "p-a"));
        cmds.clear();
        assert_eq!(run(&mut reg, &mut cmds, r#"{"kind":"stopPresence","proposalId":"s","presenceId":"ghost"}"#).status, "error");
        assert_eq!(run(&mut reg, &mut cmds, r#"{"kind":"retirePresence","proposalId":"r","presenceId":"p-a"}"#).status, "ok");
        assert_eq!(run(&mut reg, &mut cmds, r#"{"kind":"retirePresence","proposalId":"r2","presenceId":"p-a"}"#).status, "error");
    }

    #[test]
    fn highlight_json_carries_the_uv_box() {
        let proposal = p(
            r#"{"kind":"highlightRegion","proposalId":"9","presenceId":"p-a","description":"the banner","u":0.1,"v":0.2,"w":0.3,"h":0.4,"durationSeconds":12}"#,
        );
        match proposal {
            Proposal::HighlightRegion { presence_id, description, u, v, w, h, duration_seconds, .. } => {
                assert_eq!(presence_id, "p-a");
                assert_eq!(description, "the banner");
                assert_eq!((u, v, w, h), (0.1, 0.2, 0.3, 0.4));
                assert_eq!(duration_seconds, 12);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_highlight_needs_a_live_presence_and_retire_drops_the_box() {
        use crate::highlight::UvBox;
        use std::time::Instant;
        let mut reg = Registry::new();
        let now = Instant::now();
        let raw = UvBox { u: 0.1, v: 0.2, w: 0.3, h: 0.4 };
        let err = reg.declare_interest("p-a", "banner", raw, 5, now).unwrap_err();
        assert!(err.contains("no live presence with id p-a"), "{err}");
        let mut cmds = vec![];
        run(&mut reg, &mut cmds, &spawn("p-a"));
        assert!(!reg.has_avatar("p-a"));
        let (got, secs) = reg.declare_interest("p-a", "banner", raw, 400, now).unwrap();
        assert_eq!(secs, 360);
        assert_eq!(got, raw);
        assert_eq!(reg.visible_interests(now).len(), 1);
        assert_eq!(reg.interest("p-a"), Some(raw));
        assert_eq!(
            run(&mut reg, &mut cmds, r#"{"kind":"retirePresence","proposalId":"r","presenceId":"p-a"}"#).status,
            "ok"
        );
        assert_eq!(reg.interest("p-a"), None);
        assert!(reg.visible_interests(now).is_empty());
    }

    #[test]
    fn without_loaded_assets_spawn_succeeds_but_has_no_body() {
        let (mut reg, mut cmds) = (Registry::new(), vec![]);
        let r = run(&mut reg, &mut cmds, &spawn("p-a"));
        assert_eq!(r.status, "ok");
        assert_eq!(r.detail.as_ref().unwrap()["avatar"], false);
        assert!(cmds.is_empty());
    }
}
