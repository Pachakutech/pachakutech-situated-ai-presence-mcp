//! The Unix socket server. Three listeners, one tier each. The MCP binding
//! and the CLI use `presence.sock` (T2). `command.sock` is T1 and
//! `sidecar.sock` is T3. A connection cannot claim a different tier.

use crate::gate::Tier;
use crate::ledger::SubPoll;
use crate::pipeline::SplatPipeline;
use crate::protocol::{AvatarCommand, Proposal, ProposalResult};
use crate::registry::Registry;
use crate::screen_text::TextMemory;
use crate::speech::SpeechBoard;
use serde_json::json;
use std::io::{self, Read, Write};
use std::time::Instant;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct Client {
    stream: UnixStream,
    buf: Vec<u8>,
    tier: Tier,
    /// Last seq delivered to a subscriber. `None` means this connection is not subscribed.
    subscribed_through: Option<u64>,
    last_observe: Option<Instant>,
}

/// Lines drained from one client before the overlay draws again.
const LINES_PER_FRAME: usize = 32;
/// Ledger events pushed to one subscriber before the overlay draws again.
const SUBSCRIBE_FLUSH: usize = 32;
/// `observePhrase` only. Enforced here, not in the ledger, so a burst of
/// direct appends in tests is still a burst.
const OBSERVE_GAP: std::time::Duration = std::time::Duration::from_millis(50);

struct Endpoint {
    listener: UnixListener,
    path: PathBuf,
    tier: Tier,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Non-blocking Unix JSONL server, pumped from the overlay event loop.
pub struct SocketServer {
    listeners: Vec<Endpoint>,
    clients: Vec<Client>,
    registry: Mutex<Registry>,
    avatar_commands: Vec<AvatarCommand>,
    text: Arc<TextMemory>,
    speech: SpeechBoard,
}

enum ClientRead {
    Keep,
    Closed,
    Shutdown,
}

/// Bind one socket at `0600`. On a failure after the file exists, the file is removed.
fn bind_endpoint(path: &Path, tier: Tier) -> io::Result<Endpoint> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let listener = UnixListener::bind(path)?;
    if let Err(e) = listener.set_nonblocking(true) {
        drop(listener);
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        drop(listener);
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    Ok(Endpoint { listener, path: path.to_path_buf(), tier })
}

fn listen_or_log(path: &Path, tier: Tier, listeners: &mut Vec<Endpoint>) {
    match bind_endpoint(path, tier) {
        Ok(endpoint) => {
            println!("[socket] listening on {} ({})", endpoint.path.display(), endpoint.tier.wire_name());
            listeners.push(endpoint);
        }
        Err(e) => eprintln!("[socket] {} bind failed: {e}", path.display()),
    }
}

impl SocketServer {
    /// `presence` failing is fatal. `command` and `sidecar` are logged and skipped.
    pub fn bind(
        presence: &Path,
        command: &Path,
        sidecar: &Path,
        text: Arc<TextMemory>,
        speech: SpeechBoard,
    ) -> io::Result<Self> {
        let mut listeners = Vec::new();
        let presence_ep = bind_endpoint(presence, Tier::T2Client)?;
        println!(
            "[socket] listening on {} ({})",
            presence_ep.path.display(),
            presence_ep.tier.wire_name()
        );
        listeners.push(presence_ep);
        listen_or_log(command, Tier::T1User, &mut listeners);
        listen_or_log(sidecar, Tier::T3Sidecar, &mut listeners);
        Ok(Self {
            listeners,
            clients: Vec::new(),
            registry: Mutex::new(Registry::new()),
            avatar_commands: Vec::new(),
            text,
            speech,
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

    /// Drop a mark that was held for the utterance. The box stays for the next aim.
    pub fn conceal_speech_interest(&self, id: &str) {
        self.registry.lock().expect("registry mutex poisoned").conceal_speech_interest(id, Instant::now());
    }

    pub fn listener_fds(&self) -> impl Iterator<Item = RawFd> + '_ {
        self.listeners.iter().map(|endpoint| endpoint.listener.as_raw_fd())
    }

    pub fn client_fds(&self) -> impl Iterator<Item = RawFd> + '_ {
        self.clients.iter().map(|c| c.stream.as_raw_fd())
    }

    /// `Ok(true)` means a client asked the process to exit after its reply was written.
    pub fn pump(&mut self, pipeline: &SplatPipeline) -> io::Result<bool> {
        for i in 0..self.listeners.len() {
            let tier = self.listeners[i].tier;
            loop {
                let accepted = match self.listeners[i].listener.accept() {
                    Ok((stream, _)) => Some(stream),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => None,
                    Err(e) => return Err(e),
                };
                let Some(stream) = accepted else { break };
                stream.set_nonblocking(true)?;
                self.clients.push(Client {
                    stream,
                    buf: Vec::new(),
                    tier,
                    subscribed_through: None,
                    last_observe: None,
                });
            }
        }

        let mut i = 0;
        while i < self.clients.len() {
            match read_client(&mut self.clients[i], pipeline, &self.registry, &mut self.avatar_commands, &self.text, &self.speech) {
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
        flush_subscribers(&mut self.clients, &self.registry);
        Ok(false)
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
    text: &TextMemory,
    speech: &SpeechBoard,
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
    let mut taken = 0;
    while taken < LINES_PER_FRAME {
        let Some(pos) = client.buf.iter().position(|&b| b == b'\n') else { break };
        taken += 1;
        let line = client.buf.drain(..=pos).collect::<Vec<_>>();
        let line = String::from_utf8_lossy(&line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut subscribe_since: Option<u64> = None;
        let (result, stop) = match serde_json::from_str::<Proposal>(line) {
            Ok(proposal) => {
                subscribe_since = match &proposal {
                    Proposal::SubscribeLedger(body) => Some(body.since.unwrap_or(0)),
                    _ => None,
                };
                match crate::gate::check(client.tier, &proposal) {
                    Err(rejected) => (rejected, false),
                    Ok(()) if matches!(proposal, Proposal::ObservePhrase(_)) && !observe_slot_open(client) => {
                        (ProposalResult::err(proposal.proposal_id(), "rate: observePhrase"), false)
                    }
                    Ok(()) => dispatch(proposal, pipeline, registry, avatar_commands, text, speech, client.tier),
                }
            }
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
        if result.status == "ok" {
            if let Some(since) = subscribe_since {
                client.subscribed_through = Some(since);
            }
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
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn observe_slot_open(client: &mut Client) -> bool {
    let now = Instant::now();
    if client.last_observe.is_some_and(|prev| now.duration_since(prev) < OBSERVE_GAP) {
        return false;
    }
    client.last_observe = Some(now);
    true
}

enum LineWrite {
    Done,
    Blocked,
    Broken,
}

/// One `write` of the whole line. A short write has already mixed bytes onto the socket.
fn write_line(stream: &mut UnixStream, bytes: &[u8]) -> LineWrite {
    match stream.write(bytes) {
        Ok(n) if n == bytes.len() => LineWrite::Done,
        Ok(_) => LineWrite::Broken,
        Err(e) if e.kind() == io::ErrorKind::Interrupted => match stream.write(bytes) {
            Ok(n) if n == bytes.len() => LineWrite::Done,
            Ok(_) => LineWrite::Broken,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => LineWrite::Blocked,
            Err(_) => LineWrite::Broken,
        },
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => LineWrite::Blocked,
        Err(_) => LineWrite::Broken,
    }
}

fn event_seq(line: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(line).ok()?.get("ledgerEvent")?.get("seq")?.as_u64()
}

/// `true` means the client socket is no longer usable.
fn unsubscribe_or_drop(client: &mut Client, floor: u64) -> bool {
    let line = format!("{}\n", json!({ "ledgerOverflow": { "floor": floor } }));
    match write_line(&mut client.stream, line.as_bytes()) {
        LineWrite::Done => {
            client.subscribed_through = None;
            false
        }
        LineWrite::Blocked | LineWrite::Broken => true,
    }
}

/// Copy the pending lines, drop the registry lock, then write. A blocked subscriber
/// is unsubscribed or dropped. It does not stall the frame.
fn flush_subscribers(clients: &mut Vec<Client>, registry: &Mutex<Registry>) {
    let mut i = 0;
    while i < clients.len() {
        let Some(cursor) = clients[i].subscribed_through else {
            i += 1;
            continue;
        };
        let polled = registry.lock().expect("registry mutex poisoned").ledger().poll_subscriber(cursor, SUBSCRIBE_FLUSH);
        let drop_client = match polled {
            SubPoll::Overflow { floor } => unsubscribe_or_drop(&mut clients[i], floor),
            SubPoll::Lines { lines, through } => {
                let mut delivered = cursor;
                let mut blocked_at: Option<u64> = None;
                let mut broken = false;
                for line in &lines {
                    let payload = format!("{line}\n");
                    match write_line(&mut clients[i].stream, payload.as_bytes()) {
                        LineWrite::Done => match event_seq(line) {
                            Some(seq) => delivered = seq,
                            None => {
                                broken = true;
                                break;
                            }
                        },
                        LineWrite::Blocked => {
                            blocked_at = Some(delivered);
                            break;
                        }
                        LineWrite::Broken => {
                            broken = true;
                            break;
                        }
                    }
                }
                if broken {
                    true
                } else if let Some(floor) = blocked_at {
                    unsubscribe_or_drop(&mut clients[i], floor)
                } else {
                    clients[i].subscribed_through = Some(through);
                    false
                }
            }
        };
        if drop_client {
            clients.remove(i);
        } else {
            i += 1;
        }
    }
}

fn dispatch(
    proposal: Proposal,
    pipeline: &SplatPipeline,
    registry: &Mutex<Registry>,
    avatar_commands: &mut Vec<AvatarCommand>,
    text: &TextMemory,
    speech: &SpeechBoard,
    tier: Tier,
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
        Proposal::QueryScreenText { .. } => {
            return (ProposalResult::ok(&proposal_id, Some(text.snapshot_json(Instant::now()))), false);
        }
        Proposal::QuerySpeech { .. } => {
            return (ProposalResult::ok(&proposal_id, Some(speech.json())), false);
        }
        _ => {}
    }
    let mut reg = registry.lock().expect("registry mutex poisoned");

    if let Some(result) = crate::ledger::route_ledger(&proposal, tier, reg.ledger_mut(), now_ms()) {
        return (result, false);
    }

    if let Some(result) = route_presence(&proposal, &mut reg, avatar_commands) {
        return (result, false);
    }

    let result = match proposal {
        Proposal::ConcealHighlight { presence_id, .. } => {
            let concealed = reg.conceal_for_jump(&presence_id, Instant::now());
            ProposalResult::ok(&proposal_id, Some(json!({ "concealed": concealed })))
        }
        Proposal::HighlightRegion { presence_id, description, u, v, w, h, duration_seconds, until_speech, .. } => {
            match reg.declare_interest(
                &presence_id,
                &description,
                crate::highlight::UvBox { u, v, w, h },
                duration_seconds,
                Instant::now(),
            ) {
                Ok((box_, secs)) => {
                    if until_speech { reg.hold_interest_for_speech(&presence_id); }
                    ProposalResult::ok(
                        &proposal_id,
                        Some(json!({
                            "regionId": format!("r-{proposal_id}"),
                            "resolvedBounds": [box_.u, box_.v, box_.w, box_.h],
                            "durationSeconds": secs,
                            "untilSpeech": until_speech,
                        })),
                    )
                }
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
        | Proposal::AvatarWalk { .. }
        | Proposal::QueryScreenText { .. }
        | Proposal::QuerySpeech { .. } => ProposalResult::ok(&proposal_id, None),
        Proposal::ListPhraseFlags(_) | Proposal::SubscribeLedger(_) | Proposal::ObservePhrase(_) => {
            ProposalResult::err(&proposal_id, "unrouted ledger proposal")
        }
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

    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    impl Scratch {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("presence-c1-{label}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    fn boards() -> (Arc<TextMemory>, SpeechBoard) {
        (
            Arc::new(TextMemory::new(Arc::new(crate::screen_text::GlassStamp::new()))),
            SpeechBoard::new(),
        )
    }

    fn mode_is_0600(path: &Path) {
        let mode = std::fs::metadata(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{}", path.display());
    }

    #[test]
    fn endpoints_are_0600_and_removed_on_drop() {
        let scratch = Scratch::new("a8");
        let presence = scratch.0.join("presence.sock");
        let command = scratch.0.join("command.sock");
        let sidecar = scratch.0.join("sidecar.sock");
        let (text, speech) = boards();
        let server = SocketServer::bind(&presence, &command, &sidecar, text, speech).unwrap();
        assert_eq!(server.listeners.len(), 3);
        assert_eq!(
            server.listeners.iter().map(|endpoint| endpoint.tier).collect::<Vec<_>>(),
            vec![Tier::T2Client, Tier::T1User, Tier::T3Sidecar]
        );
        for endpoint in &server.listeners {
            mode_is_0600(&endpoint.path);
        }
        let paths: Vec<_> = server.listeners.iter().map(|endpoint| endpoint.path.clone()).collect();
        drop(server);
        for path in paths {
            assert!(!path.exists(), "{} still present", path.display());
        }
    }

    #[test]
    fn missing_side_socket_is_fail_soft() {
        let scratch = Scratch::new("a8b");
        let presence = scratch.0.join("presence.sock");
        let sidecar = scratch.0.join("sidecar.sock");
        let command_parent = scratch.0.join("not-a-directory");
        std::fs::write(&command_parent, b"x").unwrap();
        let command = command_parent.join("command.sock");
        let Err(blocked) = bind_endpoint(&command, Tier::T1User) else {
            panic!("command parent is a file");
        };
        assert_eq!(blocked.kind(), std::io::ErrorKind::AlreadyExists);
        let (text, speech) = boards();
        let server = SocketServer::bind(&presence, &command, &sidecar, text, speech).unwrap();
        assert_eq!(server.listeners.len(), 2);
        assert_eq!(server.listeners[0].tier, Tier::T2Client);
        assert_eq!(server.listeners[1].tier, Tier::T3Sidecar);
        mode_is_0600(&presence);
        mode_is_0600(&sidecar);
        assert!(!command.exists());
        drop(server);
        assert!(!presence.exists());
        assert!(!sidecar.exists());
        assert!(command_parent.is_file());
    }
}
