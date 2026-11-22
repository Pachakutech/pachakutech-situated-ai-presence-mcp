//! Wires the socket protocol to Control/Presence actors and, now, to the
//! live `SplatPipeline`. GPU work on ingest is a bump-allocated upload of
//! the canonical cloud; pose/skinning apply is still TODO on animate.

use crate::actors::gpu_layout::to_gpu_splat;
use crate::actors::{ControlActor, PresenceActor, SceneMemory};
use crate::gate::Tier;
use crate::control_row::{Paint, Px as RowPx, RowInput};
use crate::highlight::{InterestBook, UvBox, MAX_SECS};
use crate::docmap::DocMap;
use crate::ledger::Ledger;
use crate::map_worker::{self, MapInbox};
use crate::project::ProjectStore;
use crate::protocol::{Proposal, ProposalResult};
use crate::task::TaskBook;
use crate::tour::TourDesk;
use crate::pipeline::{SplatPipeline, INACTIVE_SPLAT};
use std::collections::HashMap;
use std::time::Instant;

struct GpuRange {
    start: u32,
    count: u32,
}

pub struct Registry {
    memory: SceneMemory,
    control: ControlActor,
    presence: PresenceActor,
    next_slot: u32,
    next_owner: u32,
    frame: u32,
    gpu_ranges: HashMap<String, GpuRange>,
    /// Whether the daemon has a loaded avatar asset to hand out.
    avatar_available: bool,
    /// The single avatar body belongs to at most one presence at a time.
    avatar_owner: Option<String>,
    /// One screen box per presence, or one box per phrase when the tour sends a phrase id.
    /// The mark fades; the box stays until retire.
    interests: InterestBook,
    /// The one mark this presence is pointing at. The key is a phrase id, or the
    /// presence id when the declare had none. The avatar samples that mark's visible box.
    aim_of: HashMap<String, String>,
    /// Connection that last painted each until-conceal mark. Its close retires those marks.
    held_owner: HashMap<String, u64>,
    /// Glass-session phrase log. Retiring a presence does not clear it.
    ledger: Ledger,
    /// Task queue for this run. Retire dismisses that presence's tasks.
    tasks: TaskBook,
    /// Passage index. The directory is created on a successful build, not here.
    docmap: DocMap,
    /// The project tour for this run. The cursor itself lives on the project.
    tour: TourDesk,
    /// Descriptors for the map worker. Prepare runs on that thread.
    map_inbox: MapInbox,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            memory: SceneMemory::new(),
            control: ControlActor::new(),
            presence: PresenceActor::new(),
            next_slot: 0,
            next_owner: 1,
            frame: 1,
            gpu_ranges: HashMap::new(),
            avatar_available: false,
            avatar_owner: None,
            interests: InterestBook::new(),
            aim_of: HashMap::new(),
            held_owner: HashMap::new(),
            ledger: Ledger::new(),
            tasks: TaskBook::new(),
            docmap: DocMap::new(crate::docmap::docmap_root_from_env()),
            tour: TourDesk::default(),
            map_inbox: MapInbox::open(),
        }
    }

    /// The worker end. The overlay starts the thread. A second call returns nothing.
    pub fn take_map_ends(&mut self) -> Option<map_worker::WorkerEnds> {
        let root = self.docmap.root().to_path_buf();
        self.map_inbox.take_worker(root)
    }

    /// After a successful create, add, or confirm. A draft is not queued.
    pub fn note_project_sources(&mut self, proposal: &Proposal, result: &ProposalResult) {
        let jobs = {
            let Some(store) = self.ledger.projects_mut() else { return };
            map_worker::jobs_for_success(proposal, result, store)
        };
        for job in jobs {
            self.map_inbox.enqueue(job);
        }
    }

    /// Pending sources already on disk. Mapped and failed sources are absent.
    pub fn queue_pending_sources(&mut self) {
        let jobs = {
            let Some(store) = self.ledger.projects_mut() else { return };
            map_worker::startup_jobs(store)
        };
        for job in jobs {
            self.map_inbox.enqueue(job);
        }
    }

    pub fn docmap_mut(&mut self) -> &mut DocMap {
        &mut self.docmap
    }

    /// Opens the project directory. `new` does not call this, so a test registry
    /// does not create `~/.local/share/pachakutech/projects`.
    pub fn open_projects(&mut self, root: &std::path::Path) -> std::io::Result<()> {
        self.ledger.open_projects(root)
    }

    pub fn projects_mut(&mut self) -> Option<&mut ProjectStore> {
        self.ledger.projects_mut()
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    pub fn task_book(&self) -> &TaskBook {
        &self.tasks
    }

    pub fn tour_phase_name(&self) -> Option<&'static str> {
        crate::tour::phase_name(&self.tour)
    }

    pub fn task_book_mut(&mut self) -> &mut TaskBook {
        &mut self.tasks
    }

    pub fn task_and_ledger(&mut self) -> (&mut TaskBook, &mut Ledger) {
        (&mut self.tasks, &mut self.ledger)
    }

    /// `startProjectTour` and `reportSourceOpen` need the task book and the project store in sequence.
    pub fn route_tour_proposal(&mut self, proposal: &Proposal, tier: Tier, now_ms: u64) -> Option<ProposalResult> {
        match proposal {
            Proposal::StartProjectTour(body) => {
                if !crate::gate::allowed(tier, proposal) {
                    return Some(ProposalResult::err(
                        &body.proposal_id,
                        format!("not_allowed_for_tier:{}", proposal.kind_name()),
                    ));
                }
                let Some(mut store) = self.ledger.take_projects() else {
                    return Some(ProposalResult::err(&body.proposal_id, "projects_unavailable"));
                };
                let planned = crate::tour::route_start(body, tier, &mut store, &mut self.tour, now_ms);
                let result = crate::tour::commit(planned, &mut self.tasks, &mut self.ledger, &mut self.tour, &mut store, now_ms);
                self.ledger.give_projects(store);
                Some(result)
            }
            Proposal::ReportSourceOpen(body) => {
                if !crate::gate::allowed(tier, proposal) {
                    return Some(ProposalResult::err(
                        &body.proposal_id,
                        format!("not_allowed_for_tier:{}", proposal.kind_name()),
                    ));
                }
                let Some(mut store) = self.ledger.take_projects() else {
                    return Some(ProposalResult::err(&body.proposal_id, "projects_unavailable"));
                };
                let planned = crate::tour::plan_report(body, &mut store, &mut self.tour, now_ms);
                let result = crate::tour::commit(planned, &mut self.tasks, &mut self.ledger, &mut self.tour, &mut store, now_ms);
                self.ledger.give_projects(store);
                Some(result)
            }
            Proposal::ConfirmTour(body) => {
                if !crate::gate::allowed(tier, proposal) {
                    return Some(ProposalResult::err(
                        &body.proposal_id,
                        format!("not_allowed_for_tier:{}", proposal.kind_name()),
                    ));
                }
                let Some(mut store) = self.ledger.take_projects() else {
                    return Some(ProposalResult::err(&body.proposal_id, "projects_unavailable"));
                };
                let awaiting = crate::tour::tour_project(&self.tour) == Some(body.project_id.as_str())
                    && crate::tour::phase_name(&self.tour) == Some("awaitingStart");
                if !awaiting {
                    self.ledger.give_projects(store);
                    return Some(ProposalResult::err(&body.proposal_id, "no_active_project_tour"));
                }
                let mut opener = crate::tour::ParkOpener;
                let _took = crate::tour::on_hotkey(
                    &mut self.tour,
                    &mut store,
                    &mut self.tasks,
                    &mut self.ledger,
                    &mut opener,
                    crate::ui_input::Hotkey::Start,
                    now_ms,
                );
                let state = crate::tour::phase_name(&self.tour).unwrap_or("awaitingStart");
                let result = ProposalResult::ok(
                    &body.proposal_id,
                    Some(serde_json::json!({
                        "projectId": body.project_id,
                        "state": state,
                    })),
                );
                self.ledger.give_projects(store);
                Some(result)
            }
            _ => None,
        }
    }

    /// `selectLoci` reads the project and the doc map together. Other project
    /// kinds stay on the store route.
    pub fn route_project_proposal(&mut self, proposal: &Proposal, tier: Tier, now_ms: u64) -> Option<ProposalResult> {
        if let Some(result) = crate::select::route(proposal, tier, self.ledger.projects_mut(), &self.docmap) {
            return Some(result);
        }
        let mut result = crate::project::route_project(proposal, tier, self.ledger.projects_mut(), now_ms)?;
        self.attach_open_request(proposal, &mut result);
        Some(result)
    }

    /// The park lives on the tour desk, not in the project file. It is copied onto this reply only.
    fn attach_open_request(&self, proposal: &Proposal, result: &mut ProposalResult) {
        let Proposal::QueryProject(body) = proposal else {
            return;
        };
        if result.status != "ok" {
            return;
        }
        let Some(detail) = result.detail.as_mut() else {
            return;
        };
        if crate::tour::tour_project(&self.tour) != Some(body.project_id.as_str()) {
            return;
        }
        if let Some(req) = crate::tour::open_request_for(&self.tour, &body.project_id) {
            detail["openRequest"] = req;
            return;
        }
        let Some(id) = crate::tour::task_id(&self.tour) else {
            return;
        };
        let live = self.tasks.query_value(id).is_some_and(|view| {
            matches!(view["state"].as_str(), Some("pending") | Some("awaitingStart") | Some("active") | Some("paused"))
        });
        if live {
            detail["tourTaskId"] = serde_json::json!(id);
        }
    }

    pub fn tick_tasks(&mut self, now_ms: u64) {
        self.apply_finished_maps();
        self.tasks.tick(&mut self.ledger, now_ms);
        let done = match crate::tour::task_id(&self.tour) {
            Some(id) => self.tasks.query_value(id).is_some_and(|view| view["state"] == "done"),
            None => false,
        };
        if !done {
            return;
        }
        let Some(mut store) = self.ledger.take_projects() else {
            return;
        };
        let mut opener = crate::tour::ParkOpener;
        let _ = crate::tour::follow_done(&mut self.tour, &mut store, &mut self.tasks, &mut self.ledger, &mut opener, now_ms);
        self.ledger.give_projects(store);
    }

    pub fn apply_tint_samples(&mut self, samples: Vec<crate::task::TintSample>) {
        let dropped = self.tasks.apply_tint_samples(&mut self.ledger, samples);
        for id in dropped {
            self.retire_interest(&id);
        }
    }

    pub fn mark_colors(&self) -> Vec<(String, crate::highlight::MarkColor)> {
        self.tasks.mark_colors()
    }

    /// Pointer and key events for the hover row. An enter remembers the interest box.
    pub fn apply_row_input(&mut self, input: Vec<RowInput>, screen_w: i32, screen_h: i32, now_ms: u64) {
        self.tasks.set_row_screen(screen_w, screen_h);
        for ev in input {
            match ev {
                RowInput::Enter { phrase_id, x, y, w, h } => {
                    let uv = self.interests.get(&phrase_id);
                    let (book, ledger) = self.task_and_ledger();
                    book.row_enter(&phrase_id, RowPx { x, y, w, h }, uv, now_ms, ledger);
                }
                other => {
                    let (book, ledger) = self.task_and_ledger();
                    book.apply_row(other, ledger, now_ms);
                }
            }
        }
    }

    pub fn note_row_scroll(&mut self) {
        self.tasks.note_row_scroll();
    }

    pub fn row_paint(&self, screen_w: i32, screen_h: i32) -> Option<Paint> {
        self.tasks.row_paint(screen_w, screen_h)
    }

    pub fn row_held_phrase(&self) -> Option<String> {
        self.tasks.row_held_phrase()
    }

    pub fn row_hold_json(&self) -> Option<serde_json::Value> {
        let phrase = self.tasks.row_held_phrase()?;
        let uv = self.tasks.held_uv(&phrase)?;
        Some(serde_json::json!({
            "phraseId": phrase,
            "u": uv.u,
            "v": uv.v,
            "w": uv.w,
            "h": uv.h,
        }))
    }

    fn apply_finished_maps(&mut self) {
        let reports = self.map_inbox.drain();
        for report in reports {
            let project_id = report.project_id.clone();
            let source_id = report.source_id.clone();
            if let Some(store) = self.ledger.projects_mut() {
                if let Err(err) = crate::adapt::apply(store, &project_id, &source_id, report.prepared) {
                    eprintln!("[map] {project_id} {source_id} failed {err}");
                }
            }
            self.map_inbox.forget(&project_id, &source_id);
        }
    }

    pub fn note_task_highlight(&mut self, presence_id: &str, phrase_id: Option<&str>, now_ms: u64) {
        if let Some(phrase) = phrase_id.map(str::trim).filter(|id| !id.is_empty()) {
            self.tasks.note_phrase_mark(phrase, &mut self.ledger, now_ms);
        } else {
            self.tasks.note_highlight(presence_id, &mut self.ledger, now_ms);
        }
    }

    pub fn note_task_hidden(&mut self, presence_id: &str) {
        self.tasks.note_hidden_presence(presence_id);
    }

    /// The presence must already be in the daemon map. It does not need the body.
    /// `secs` is clamped to 1..=360. The returned box is the measured rect the book keeps.
    /// The mark surface pads around that rect. This value does not.
    /// `aim` names this mark as the presence's point. A declare with no phrase id
    /// is the aim either way. A phrase declare with `aim` false leaves the key alone.
    pub fn declare_interest(
        &mut self,
        presence_id: &str,
        phrase_id: Option<&str>,
        description: &str,
        raw: UvBox,
        secs: u32,
        now: Instant,
        aim: bool,
    ) -> Result<(UvBox, u32), String> {
        if !self.has_presence(presence_id) {
            return Err(format!("no live presence with id {presence_id}"));
        }
        let phrase = phrase_id.map(str::trim).filter(|id| !id.is_empty());
        let key = phrase.unwrap_or(presence_id);
        let secs = secs.clamp(1, MAX_SECS);
        // A scroll already queued may move the box. Until that tick, a quiet row keeps the open box.
        if !self.tasks.row_scroll_pending() {
            if let Some(frozen) = self.tasks.held_uv(key) {
                let got = self.interests.declare(key, frozen, secs, now)?;
                println!(
                    "[registry] highlight {presence_id} uv {:.3},{:.3} {:.3}x{:.3} for {secs}s — {description}",
                    got.u, got.v, got.w, got.h
                );
                return Ok((got, secs));
            }
        }
        let got = self.interests.declare(key, raw, secs, now)?;
        if phrase.is_none() || aim {
            self.aim_of.insert(presence_id.to_string(), key.to_string());
        }
        println!(
            "[registry] highlight {presence_id} uv {:.3},{:.3} {:.3}x{:.3} for {secs}s — {description}",
            got.u, got.v, got.w, got.h
        );
        Ok((got, secs))
    }

    /// Still the declared box after the mark has faded.
    /// The point gesture reads [`Self::visible_aim`], not this.
    #[allow(dead_code)]
    pub fn interest(&self, presence_id: &str) -> Option<UvBox> {
        self.interests.get(presence_id)
    }

    /// The box this presence is pointing at, while that mark is still visible.
    /// A faded mark is not returned. An empty sample is the talking lift.
    pub fn visible_aim(&self, presence_id: &str, now: Instant) -> Option<UvBox> {
        let key = self.aim_of.get(presence_id)?;
        self.interests.visible(now).into_iter().find(|(id, _)| id == key).map(|(_, box_)| box_)
    }

    /// Phrase ids the presences are pointing at, while those marks are still visible.
    /// One id per aim. The heat field uses these and ignores companion bars.
    pub fn visible_aim_ids(&self, now: Instant) -> Vec<String> {
        let showing = self.interests.visible(now);
        let mut ids = Vec::new();
        for aim in self.aim_of.values() {
            if ids.iter().any(|id: &String| id == aim) {
                continue;
            }
            if showing.iter().any(|(id, _)| id == aim) {
                ids.push(aim.clone());
            }
        }
        ids
    }

    pub fn visible_interests(&self, now: Instant) -> Vec<(String, UvBox)> {
        self.interests.visible(now)
    }

    pub fn hold_interest_for_speech(&mut self, presence_id: &str) {
        self.interests.hold_for_speech(presence_id);
    }

    pub fn hold_interest_until_conceal(&mut self, id: &str) {
        self.interests.hold_until_conceal(id);
    }

    /// This connection now owns the held mark. A later paint by someone else takes the ownership.
    pub fn claim_held(&mut self, conn: u64, key: &str) {
        let key = key.trim();
        if key.is_empty() {
            return;
        }
        self.held_owner.insert(key.to_string(), conn);
    }

    /// The mark is no longer held for a connection. Closing that connection leaves it on its lease.
    pub fn forget_held(&mut self, key: &str) {
        self.held_owner.remove(key.trim());
    }

    /// The connection that painted these held marks is gone. Another connection's marks stay.
    pub fn release_connection(&mut self, conn: u64) {
        let keys: Vec<String> = self
            .held_owner
            .iter()
            .filter(|(_, owner)| **owner == conn)
            .map(|(key, _)| key.clone())
            .collect();
        if keys.is_empty() {
            return;
        }
        eprintln!("[highlight] connection {conn} closed; retiring {}", keys.join(" "));
        for key in &keys {
            self.held_owner.remove(key);
            self.retire_interest(key);
        }
    }

    pub fn retire_interest(&mut self, id: &str) {
        self.interests.retire(id);
        self.aim_of.retain(|_, aim| aim != id);
    }

    /// Hide a mark that was held for the utterance. The box stays.
    pub fn conceal_speech_interest(&mut self, presence_id: &str, now: Instant) -> bool {
        let hid = self.interests.conceal_if_speech_held(presence_id, now);
        if hid {
            println!("[highlight] {presence_id} speech ended");
        }
        hid
    }

    /// Hide a speech-held tint while a page jumps. The utterance keeps playing.
    pub fn conceal_for_jump(&mut self, presence_id: &str, now: Instant) -> bool {
        let hid = self.interests.conceal_if_speech_held(presence_id, now);
        if hid {
            println!("[highlight] {presence_id} down for the jump");
        }
        hid
    }

    pub fn set_avatar_available(&mut self, available: bool) {
        self.avatar_available = available;
    }

    /// Spawns the presence. Returns true if it was granted the avatar body
    /// (one body exists; later presences are tracked but have no body).
    pub fn spawn_presence(
        &mut self,
        presence_id: &str,
        source_context: &str,
        style_hint: Option<&str>,
        artifact_id: Option<&str>,
    ) -> bool {
        self.presence.spawn(&self.memory, presence_id, source_context, style_hint, artifact_id);
        if self.avatar_available && self.avatar_owner.is_none() {
            self.avatar_owner = Some(presence_id.to_string());
        }
        self.has_avatar(presence_id)
    }

    pub fn has_presence(&self, presence_id: &str) -> bool {
        self.presence.contains(presence_id)
    }

    pub fn has_avatar(&self, presence_id: &str) -> bool {
        self.avatar_owner.as_deref() == Some(presence_id) && self.presence.contains(presence_id)
    }

    pub fn avatar_owner(&self) -> Option<&str> {
        self.avatar_owner.as_deref()
    }

    pub fn animate_presence(&mut self, presence_id: &str, text: &str) -> Result<(), String> {
        self.presence.animate(presence_id, text)
    }

    /// Retires the presence. Returns true if it owned the avatar body (now released).
    pub fn retire_presence(&mut self, presence_id: &str) -> Result<bool, String> {
        self.presence.retire(presence_id)?;
        self.retire_interest(presence_id);
        self.aim_of.remove(presence_id);
        self.tasks.dismiss_presence(presence_id);
        let owned = self.avatar_owner.as_deref() == Some(presence_id);
        if owned {
            self.avatar_owner = None;
        }
        Ok(owned)
    }

    pub fn add_artifact(
        &mut self,
        artifact_id: &str,
        description: &str,
        source_uri: Option<&str>,
        pipeline: &SplatPipeline,
    ) {
        self.control.ingest_artifact(&mut self.memory, artifact_id, description, source_uri);
        self.upload_artifact(artifact_id, pipeline);
    }

    pub fn retire_artifact(&mut self, artifact_id: &str, pipeline: &SplatPipeline) -> Result<(), String> {
        self.deactivate_artifact(artifact_id, pipeline);
        self.control.retire_artifact(&mut self.memory, artifact_id)
    }

    fn upload_artifact(&mut self, artifact_id: &str, pipeline: &SplatPipeline) {
        let Some(cloud) = self.memory.get(artifact_id) else {
            return;
        };
        if cloud.splats.is_empty() {
            return;
        }
        let ceiling = pipeline.artifact_slot_limit();
        let remaining = ceiling.saturating_sub(self.next_slot);
        if remaining == 0 {
            eprintln!("[registry] artifact slot range full — {artifact_id} held in Scene Memory only");
            return;
        }
        let n = (cloud.splats.len() as u32).min(remaining);
        if n < cloud.splats.len() as u32 {
            eprintln!(
                "[registry] {artifact_id}: truncated {} -> {n} splats (artifact slots {ceiling})",
                cloud.splats.len()
            );
        }
        let owner = self.next_owner;
        self.next_owner = self.next_owner.wrapping_add(1);
        self.frame = self.frame.saturating_add(1);
        let start = self.next_slot;
        for (i, splat) in cloud.splats.iter().take(n as usize).enumerate() {
            let skin = cloud.skinning.as_ref().and_then(|s| s.get(i));
            pipeline.write_splat(
                start + i as u32,
                &to_gpu_splat(splat, skin, owner, self.frame, 255),
            );
        }
        self.next_slot += n;
        self.gpu_ranges.insert(artifact_id.to_string(), GpuRange { start, count: n });
        if let Err(e) = pipeline.tick(self.frame, 600) {
            eprintln!("[registry] tick after ingest {artifact_id}: {e}");
        } else {
            println!("[registry] uploaded {n} splats for {artifact_id} at slot {start}");
        }
    }

    fn deactivate_artifact(&mut self, artifact_id: &str, pipeline: &SplatPipeline) {
        let Some(range) = self.gpu_ranges.remove(artifact_id) else {
            return;
        };
        for i in 0..range.count {
            pipeline.write_splat(range.start + i, &INACTIVE_SPLAT);
        }
        self.frame = self.frame.saturating_add(1);
        if let Err(e) = pipeline.tick(self.frame, 600) {
            eprintln!("[registry] tick after retire {artifact_id}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::Tier;
    use serde_json::json;

    #[test]
    fn a_successful_create_is_noted_onto_the_queue_and_a_draft_is_not() {
        let root = std::env::temp_dir().join(format!("presence-c9b-reg-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut reg = Registry::new();
        reg.open_projects(&root.join("projects")).unwrap();
        let path = root.join("page.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let create = json!({
            "kind": "createProject",
            "proposalId": "c",
            "title": "Direct",
            "asks": [{ "text": "a question", "kind": "question" }],
            "sources": [{ "path": path.display().to_string() }],
        });
        let proposal: Proposal = serde_json::from_value(create).unwrap();
        let result = reg.route_project_proposal(&proposal, Tier::T1User, 1).unwrap();
        assert_eq!(result.status, "ok", "{:?}", result.error);
        reg.note_project_sources(&proposal, &result);
        let job = reg.map_inbox.pop().unwrap();
        assert_eq!(job.source_id, "sr-1");
        assert!(reg.map_inbox.pop().is_none());

        let draft_body = json!({
            "kind": "createProject",
            "proposalId": "d",
            "title": "Draft",
            "asks": [{ "text": "a question", "kind": "question" }],
            "sources": [{ "url": "https://example.com/c9b-draft" }],
        });
        let draft: Proposal = serde_json::from_value(draft_body).unwrap();
        let drafted = reg.route_project_proposal(&draft, Tier::T3Sidecar, 2).unwrap();
        assert_eq!(drafted.status, "ok", "{:?}", drafted.error);
        reg.note_project_sources(&draft, &drafted);
        assert!(reg.map_inbox.pop().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn w1_query_project_carries_the_parked_spec() {
        let root = std::env::temp_dir().join(format!("presence-c11c-reg-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut reg = Registry::new();
        reg.open_projects(&root.join("projects")).unwrap();
        let create = json!({
            "kind": "createProject",
            "proposalId": "c",
            "title": "Window",
            "asks": [{ "text": "Is X supported?", "kind": "question" }],
            "sources": [{ "url": "https://example.com/a" }],
        });
        let proposal: Proposal = serde_json::from_value(create).unwrap();
        let created = reg.route_project_proposal(&proposal, Tier::T1User, 1).unwrap();
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        reg.projects_mut().unwrap().set_source_status(&id, "sr-1", crate::project::SourceStatus::Mapped, None).unwrap();
        let other = json!({
            "kind": "createProject",
            "proposalId": "o",
            "title": "Other",
            "asks": [{ "text": "Is X supported?", "kind": "question" }],
            "sources": [{ "url": "https://example.com/b" }],
        });
        let other_proposal: Proposal = serde_json::from_value(other).unwrap();
        let other_created = reg.route_project_proposal(&other_proposal, Tier::T1User, 2).unwrap();
        let other_id = other_created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let start = json!({
            "kind": "startProjectTour",
            "proposalId": "g",
            "projectId": id,
            "mode": "explain",
        });
        let start_proposal: Proposal = serde_json::from_value(start).unwrap();
        let started = reg.route_tour_proposal(&start_proposal, Tier::T1User, 3).unwrap();
        assert_eq!(started.status, "ok", "{:?}", started.error);
        assert!(started.detail.as_ref().unwrap().get("taskId").is_none());
        assert_eq!(
            reg.projects_mut().unwrap().get(&id).unwrap().sources[0].status,
            crate::project::SourceStatus::Mapped
        );
        let query = json!({
            "kind": "queryProject",
            "proposalId": "q",
            "projectId": id,
        });
        let query_proposal: Proposal = serde_json::from_value(query).unwrap();
        let viewed = reg.route_project_proposal(&query_proposal, Tier::T2Client, 4).unwrap();
        assert_eq!(viewed.status, "ok", "{:?}", viewed.error);
        let req = &viewed.detail.as_ref().unwrap()["openRequest"];
        assert_eq!(req["kind"], "web");
        assert_eq!(req["sourceId"], "sr-1");
        assert_eq!(req["reference"], "https://example.com/a");
        assert_eq!(req["port"], 9222);
        let profile = req["profile"].as_str().unwrap();
        assert!(profile.ends_with("/pachakutech/locate-chromium"), "{profile}");
        assert!(viewed.detail.as_ref().unwrap().get("tourTaskId").is_none());
        let quiet = json!({
            "kind": "queryProject",
            "proposalId": "q2",
            "projectId": other_id,
        });
        let quiet_proposal: Proposal = serde_json::from_value(quiet).unwrap();
        let other_view = reg.route_project_proposal(&quiet_proposal, Tier::T2Client, 5).unwrap();
        assert!(other_view.detail.unwrap().get("openRequest").is_none());
        let saved = std::fs::read_to_string(root.join("projects").join(&id).join("project.json")).unwrap();
        assert!(!saved.contains("openRequest"));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn scratch_reg(label: &str) -> (Registry, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("presence-tour-confirm-{label}-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut reg = Registry::new();
        reg.open_projects(&root.join("projects")).unwrap();
        (reg, root)
    }

    fn mapped_web(reg: &mut Registry, url: &str, now: u64) -> String {
        let create = json!({
            "kind": "createProject",
            "proposalId": format!("c{now}"),
            "title": "Confirm",
            "asks": [{ "text": "Is X supported?", "kind": "question" }],
            "sources": [{ "url": url }],
        });
        let proposal: Proposal = serde_json::from_value(create).unwrap();
        let created = reg.route_project_proposal(&proposal, Tier::T1User, now).unwrap();
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        reg.projects_mut()
            .unwrap()
            .set_source_status(&id, "sr-1", crate::project::SourceStatus::Mapped, None)
            .unwrap();
        id
    }

    fn start_listed(reg: &mut Registry, id: &str, now: u64) {
        let start = json!({
            "kind": "startProjectTour",
            "proposalId": "g",
            "projectId": id,
            "mode": "explain",
        });
        let proposal: Proposal = serde_json::from_value(start).unwrap();
        let started = reg.route_tour_proposal(&proposal, Tier::T2Client, now).unwrap();
        assert_eq!(started.status, "ok", "{:?}", started.error);
        assert_eq!(started.detail.as_ref().unwrap()["state"], "awaitingStart");
    }

    fn confirm_proposal(id: &str, proposal_id: &str) -> Proposal {
        serde_json::from_value(json!({
            "kind": "confirmTour",
            "proposalId": proposal_id,
            "projectId": id,
        }))
        .unwrap()
    }

    fn parked_reference(reg: &mut Registry, id: &str, now: u64) -> Option<serde_json::Value> {
        let query = json!({
            "kind": "queryProject",
            "proposalId": format!("q{now}"),
            "projectId": id,
        });
        let proposal: Proposal = serde_json::from_value(query).unwrap();
        let viewed = reg.route_project_proposal(&proposal, Tier::T2Client, now).unwrap();
        assert_eq!(viewed.status, "ok", "{:?}", viewed.error);
        viewed.detail.unwrap().get("openRequest").cloned()
    }

    #[test]
    fn r2_confirm_tour_parks_the_listed_source() {
        let (mut reg, root) = scratch_reg("r2");
        let id = mapped_web(&mut reg, "https://example.com/a", 1);
        start_listed(&mut reg, &id, 2);
        assert!(parked_reference(&mut reg, &id, 3).is_none());
        for (tier, proposal_id) in [(Tier::T2Client, "t2"), (Tier::T3Sidecar, "t3")] {
            let refused = reg.route_tour_proposal(&confirm_proposal(&id, proposal_id), tier, 4).unwrap();
            assert_eq!(refused.error.as_deref(), Some("not_allowed_for_tier:confirmTour"));
        }
        assert!(parked_reference(&mut reg, &id, 5).is_none());
        assert_eq!(reg.tour_phase_name(), Some("awaitingStart"));
        let confirmed = reg.route_tour_proposal(&confirm_proposal(&id, "k"), Tier::T1User, 6).unwrap();
        assert_eq!(confirmed.status, "ok", "{:?}", confirmed.error);
        assert_eq!(confirmed.detail.as_ref().unwrap()["state"], "active");
        let req = parked_reference(&mut reg, &id, 7).unwrap();
        assert_eq!(req["kind"], "web");
        assert_eq!(req["sourceId"], "sr-1");
        assert_eq!(req["reference"], "https://example.com/a");
        assert_eq!(req["port"], 9222);
        assert_eq!(
            reg.projects_mut().unwrap().get(&id).unwrap().sources[0].status,
            crate::project::SourceStatus::Mapped
        );
        for extra in [r#","url":"https://evil.test""#, r#","path":"/tmp/a""#, r#","mode":"explain""#] {
            let raw = format!(
                r#"{{"kind":"confirmTour","proposalId":"bad","projectId":"{id}"{extra}}}"#,
                id = id,
                extra = extra,
            );
            assert!(serde_json::from_str::<Proposal>(&raw).is_err(), "{extra}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn r3_confirm_tour_leaves_any_other_desk() {
        let (mut reg, root) = scratch_reg("r3-other");
        let id = mapped_web(&mut reg, "https://example.com/a", 1);
        let other = mapped_web(&mut reg, "https://example.com/b", 2);
        start_listed(&mut reg, &id, 3);
        let task_id = awaiting_task(&mut reg, 4);
        let wrong = reg.route_tour_proposal(&confirm_proposal(&other, "k"), Tier::T1User, 5).unwrap();
        assert_eq!(wrong.error.as_deref(), Some("no_active_project_tour"));
        assert_eq!(reg.task_book().query_value(&task_id).unwrap()["state"], "awaitingStart");
        assert_eq!(reg.tour_phase_name(), Some("awaitingStart"));
        assert!(parked_reference(&mut reg, &id, 6).is_none());
        let _ = std::fs::remove_dir_all(&root);

        let (mut reg, root) = scratch_reg("r3-active");
        let id = mapped_web(&mut reg, "https://example.com/a", 1);
        let start = json!({
            "kind": "startProjectTour",
            "proposalId": "g",
            "projectId": id,
            "mode": "explain",
        });
        let start_proposal: Proposal = serde_json::from_value(start).unwrap();
        let started = reg.route_tour_proposal(&start_proposal, Tier::T1User, 2).unwrap();
        assert_eq!(started.status, "ok", "{:?}", started.error);
        let before = parked_reference(&mut reg, &id, 3).unwrap();
        let late = reg.route_tour_proposal(&confirm_proposal(&id, "k"), Tier::T1User, 4).unwrap();
        assert_eq!(late.error.as_deref(), Some("no_active_project_tour"));
        assert_eq!(parked_reference(&mut reg, &id, 5).unwrap(), before);
        let _ = std::fs::remove_dir_all(&root);

        let (mut reg, root) = scratch_reg("r3-task");
        let id = mapped_web(&mut reg, "https://example.com/a", 1);
        let task_id = awaiting_task(&mut reg, 2);
        start_listed(&mut reg, &id, 3);
        let confirmed = reg.route_tour_proposal(&confirm_proposal(&id, "k"), Tier::T1User, 4).unwrap();
        assert_eq!(confirmed.status, "ok", "{:?}", confirmed.error);
        assert_eq!(reg.task_book().query_value(&task_id).unwrap()["state"], "active");
        assert_eq!(reg.tour_phase_name(), Some("awaitingStart"));
        assert!(parked_reference(&mut reg, &id, 5).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn awaiting_task(reg: &mut Registry, now: u64) -> String {
        let raw = r#"{"kind":"createTask","proposalId":"c","presenceId":"p-1","mode":"explain","items":[{"query":"the line","scroll":true}]}"#;
        let proposal: Proposal = serde_json::from_str(raw).unwrap();
        let (book, ledger) = reg.task_and_ledger();
        let created = crate::task::route_task(&proposal, Tier::T2Client, book, ledger, true, now).unwrap();
        let task_id = created.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        assert_eq!(reg.task_book().query_value(&task_id).unwrap()["state"], "awaitingStart");
        task_id
    }

    fn hand_rows(path: &std::path::Path) -> Vec<serde_json::Value> {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        text.lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|row| matches!(row["kind"].as_str(), Some("skip" | "accept" | "deny")))
            .collect()
    }

    #[test]
    fn a_bound_hand_lands_in_the_project_event_log() {
        use crate::highlight::MarkColor;
        use crate::ledger::LedgerKind;
        use crate::project::{LocusInput, MatchKind, SourceStatus};
        use crate::task::{hand_condition, TintMark, TintSample, TourSeed, TourSeedItem, BTN_LEFT, DOUBLE_CLICK_MS};

        let root = std::env::temp_dir().join(format!("presence-hand-reg-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut reg = Registry::new();
        reg.open_projects(&root.join("projects")).unwrap();
        let create = json!({
            "kind": "createProject",
            "proposalId": "c",
            "title": "Hand",
            "asks": [{ "text": "Is X supported?", "kind": "question" }],
            "sources": [{ "url": "https://example.com/hand" }],
        });
        let proposal: Proposal = serde_json::from_value(create).unwrap();
        let created = reg.route_project_proposal(&proposal, Tier::T1User, 1).unwrap();
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        reg.projects_mut().unwrap().set_source_status(&id, "sr-1", SourceStatus::Mapped, None).unwrap();
        let locus = reg
            .projects_mut()
            .unwrap()
            .upsert_locus(
                &id,
                LocusInput {
                    source_id: "sr-1".into(),
                    ask_id: Some("ak-1".into()),
                    doc_key: "doc-sr-1".into(),
                    span_start: 0,
                    span_end: 10,
                    words: "the quoted line".into(),
                    page: None,
                    line: Some(1),
                    heading_path: None,
                    stance: None,
                    stance_by: None,
                    stance_conf: None,
                    match_kind: Some(MatchKind::Exact),
                    narration: None,
                    ocr_only: false,
                    view: None,
                },
            )
            .unwrap();
        assert!(locus.starts_with("lc-"));
        let start = json!({
            "kind": "startProjectTour",
            "proposalId": "g",
            "projectId": id,
            "askId": "ak-1",
            "mode": "explain",
            "pace": "flow",
        });
        let start_proposal: Proposal = serde_json::from_value(start).unwrap();
        let started = reg.route_tour_proposal(&start_proposal, Tier::T1User, 2).unwrap();
        assert_eq!(started.status, "ok", "{:?}", started.error);
        assert!(started.detail.as_ref().unwrap().get("taskId").is_none());
        let report = json!({
            "kind": "reportSourceOpen",
            "proposalId": "r",
            "projectId": id,
            "sourceId": "sr-1",
            "ok": true,
        });
        let report_proposal: Proposal = serde_json::from_value(report).unwrap();
        let reported = reg.route_tour_proposal(&report_proposal, Tier::T1User, 3).unwrap();
        assert_eq!(reported.status, "ok", "{:?}", reported.error);
        let task_id = reported.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        let view = reg.task_book().query_value(&task_id).unwrap();
        assert_eq!(view["items"][0]["phraseId"], locus);
        assert!(!reg.ledger().holds("ph-1"));
        {
            let (book, ledger) = reg.task_and_ledger();
            book.begin_task(&task_id, ledger, 4_000).unwrap();
        }
        reg.note_task_highlight("driver", None, 4_100);
        assert_eq!(hand_condition(reg.ledger(), &locus), "skip");
        let events = root.join("projects").join(&id).join("events.jsonl");
        assert!(hand_rows(&events).is_empty());
        assert_eq!(
            reg.task_book().mark_colors().into_iter().find(|(mark, _)| mark == "driver").map(|(_, color)| color),
            Some(MarkColor::Yellow)
        );
        let mark = Some(TintMark { id: "driver".into(), generation: 1 });
        reg.apply_tint_samples(vec![
            TintSample { down: true, button: BTN_LEFT, ts_ms: 5_000, mark: mark.clone() },
            TintSample { down: false, button: BTN_LEFT, ts_ms: 5_100, mark: mark.clone() },
        ]);
        let hands = hand_rows(&events);
        assert_eq!(hands.len(), 1);
        assert_eq!(hands[0]["kind"], "accept");
        assert_eq!(hands[0]["locusId"], locus);
        assert_eq!(hands[0]["phraseId"], locus);
        assert_eq!(hands[0]["by"], "T0");
        assert_eq!(hands[0]["askId"], "ak-1");
        assert_eq!(hands[0]["taskId"], task_id);
        let accept_pseq = hands[0]["pseq"].as_u64().unwrap();
        assert!(accept_pseq >= 1);
        reg.apply_tint_samples(vec![
            TintSample { down: true, button: BTN_LEFT, ts_ms: 5_200, mark: mark.clone() },
            TintSample { down: false, button: BTN_LEFT, ts_ms: 5_300, mark },
        ]);
        let hands = hand_rows(&events);
        assert_eq!(hands.len(), 2);
        assert_eq!(hands[1]["kind"], "skip");
        assert_eq!(hands[1]["locusId"], locus);
        assert_eq!(hands[1]["phraseId"], locus);
        assert_eq!(hands[1]["by"], "T0");
        assert!(hands[1]["pseq"].as_u64().unwrap() > accept_pseq);
        assert_eq!(hand_condition(reg.ledger(), &locus), "skip");
        assert_eq!(reg.task_book().query_value(&task_id).unwrap()["index"], 0);
        assert_eq!(reg.task_book().query_value(&task_id).unwrap()["state"], "active");
        let query = json!({
            "kind": "queryProject",
            "proposalId": "q",
            "projectId": id,
        });
        let query_proposal: Proposal = serde_json::from_value(query).unwrap();
        let viewed = reg.route_project_proposal(&query_proposal, Tier::T2Client, 6).unwrap();
        assert_eq!(viewed.status, "ok", "{:?}", viewed.error);
        let detail = viewed.detail.unwrap();
        assert!(detail.get("decisions").is_none());
        assert_eq!(detail["pseq"], hands[1]["pseq"]);
        assert!(5_200 - 5_000 < DOUBLE_CLICK_MS);
        let task_src = include_str!("task.rs");
        let tint_at = task_src.find("fn apply_tint_sample").expect("tint");
        let tint_end = task_src[tint_at..].find("fn note_highlight").expect("note") + tint_at;
        assert!(!task_src[tint_at..tint_end].contains("DOUBLE_CLICK_MS"));
        let skip_rows = detail["register"]["skip"].as_array().unwrap();
        assert_eq!(skip_rows.len(), 1);
        assert_eq!(skip_rows[0]["locusId"], locus);
        assert_eq!(skip_rows[0]["phraseId"], locus);
        assert_eq!(skip_rows[0]["askId"], "ak-1");
        assert_eq!(skip_rows[0]["sourceId"], "sr-1");
        assert_eq!(skip_rows[0]["taskId"], task_id);
        assert_eq!(skip_rows[0]["words"], "the quoted line");
        assert!(detail["register"]["accept"].as_array().unwrap().is_empty());
        assert!(detail["register"]["deny"].as_array().unwrap().is_empty());
        assert!(detail["keeps"].as_array().unwrap().is_empty());
        let still = hand_rows(&events);
        assert_eq!(still.len(), 2);
        assert_eq!(still[0]["kind"], "accept");
        assert_eq!(still[1]["kind"], "skip");
        let paged = json!({
            "kind": "queryProject",
            "proposalId": "q3",
            "projectId": id,
            "sincePseq": 999999,
        });
        let paged_proposal: Proposal = serde_json::from_value(paged).unwrap();
        let paged_view = reg.route_project_proposal(&paged_proposal, Tier::T2Client, 7).unwrap();
        let paged_detail = paged_view.detail.unwrap();
        assert!(paged_detail["loci"].as_array().unwrap().is_empty());
        assert_eq!(paged_detail["register"]["skip"][0]["locusId"], locus);
        assert_eq!(hand_rows(&events).len(), 2);

        let saved = hand_rows(&events);
        let mut fresh = crate::ledger::Ledger::new();
        fresh.open_projects(&root.join("projects")).unwrap();
        fresh.bind_shown_locus(&locus, &id, &locus, Some("ak-1".into())).unwrap();
        assert_eq!(fresh.latest_hand(&locus), Some(LedgerKind::Skip));
        assert_eq!(hand_condition(&fresh, &locus), "skip");
        let mut book = crate::task::TaskBook::new();
        let reopened = book
            .insert_tour_chunk(
                &mut fresh,
                TourSeed {
                    origin_user: true,
                    mode: crate::protocol::TaskMode::Explain,
                    pace: crate::protocol::TaskPace::Flow,
                    doc_key: None,
                    items: vec![TourSeedItem { query: "the quoted line".into(), narration: String::new(), page: None }],
                    locus_ids: vec![locus.clone()],
                    again: false,
                },
                50_000,
            )
            .unwrap();
        assert!(book.admit().is_some());
        book.begin_task(&reopened, &mut fresh, 50_100).unwrap();
        book.note_highlight("driver", &mut fresh, 50_200);
        assert_eq!(
            book.mark_colors().into_iter().find(|(mark, _)| mark == "driver").map(|(_, color)| color),
            Some(MarkColor::Yellow)
        );
        assert_eq!(hand_rows(&events), saved);
        assert_eq!(fresh.latest_hand(&locus), Some(LedgerKind::Skip));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_closed_connection_retires_only_the_held_marks_it_owns() {
        use std::time::{Duration, Instant};
        let mut reg = Registry::new();
        reg.spawn_presence("p-a", "tour", None, None);
        let now = Instant::now();
        let raw = crate::highlight::UvBox { u: 0.1, v: 0.2, w: 0.3, h: 0.1 };
        reg.declare_interest("p-a", Some("ph-1"), "a", raw, 360, now, false).unwrap();
        reg.hold_interest_until_conceal("ph-1");
        reg.claim_held(1, "ph-1");
        reg.declare_interest("p-a", Some("ph-2"), "b", raw, 360, now, false).unwrap();
        reg.hold_interest_until_conceal("ph-2");
        reg.claim_held(2, "ph-2");
        reg.declare_interest("p-a", Some("ph-3"), "c", raw, 360, now, false).unwrap();

        let ids = |reg: &Registry, at: Instant| -> Vec<String> {
            reg.visible_interests(at).into_iter().map(|(id, _)| id).collect()
        };
        reg.release_connection(9);
        let staying = ids(&reg, now + Duration::from_secs(10_000));
        assert!(staying.contains(&"ph-1".into()), "{staying:?}");
        assert!(staying.contains(&"ph-2".into()), "{staying:?}");
        assert!(!staying.contains(&"ph-3".into()), "{staying:?}");
        assert!(ids(&reg, now + Duration::from_secs(10)).contains(&"ph-3".into()));

        reg.release_connection(1);
        let after = ids(&reg, now + Duration::from_secs(10_000));
        assert!(!after.contains(&"ph-1".into()), "{after:?}");
        assert!(after.contains(&"ph-2".into()), "{after:?}");
        assert_eq!(reg.interest("ph-3").unwrap().u, 0.1);

        reg.declare_interest("p-a", Some("ph-1"), "a", raw, 360, now, false).unwrap();
        reg.hold_interest_until_conceal("ph-1");
        reg.claim_held(2, "ph-1");
        reg.release_connection(1);
        assert!(ids(&reg, now + Duration::from_secs(10_000)).contains(&"ph-1".into()));

        reg.declare_interest("p-a", Some("ph-2"), "b", raw, 360, now, false).unwrap();
        reg.forget_held("ph-2");
        reg.release_connection(2);
        let left = ids(&reg, now + Duration::from_secs(10));
        assert!(!left.contains(&"ph-1".into()), "{left:?}");
        assert!(left.contains(&"ph-2".into()), "{left:?}");
        assert!(left.contains(&"ph-3".into()), "{left:?}");
        let later = ids(&reg, now + Duration::from_secs(10_000));
        assert!(!later.contains(&"ph-2".into()), "{later:?}");
        assert_eq!(reg.interest("ph-3").unwrap().u, 0.1);
        assert_eq!(reg.interest("ph-1"), None);
    }

    fn tour_presence() -> (Registry, std::time::Instant, crate::highlight::UvBox) {
        let mut reg = Registry::new();
        reg.spawn_presence("p-a", "tour", None, None);
        reg.spawn_presence("p-b", "other", None, None);
        let now = std::time::Instant::now();
        let allyl = crate::highlight::UvBox { u: 0.2, v: 0.4, w: 0.3, h: 0.05 };
        (reg, now, allyl)
    }

    #[test]
    fn the_named_phrase_is_the_aim() {
        let (mut reg, now, allyl) = tour_presence();
        reg.declare_interest("p-a", Some("lc-1"), "allyl", allyl, 360, now, true).unwrap();
        assert_eq!(reg.visible_aim("p-a", now), Some(allyl));
        assert_eq!(reg.visible_aim("p-b", now), None);
        let keys: Vec<String> = reg.visible_interests(now).into_iter().map(|(id, _)| id).collect();
        assert_eq!(keys, vec!["lc-1".to_string()]);
        reg.hold_interest_until_conceal("lc-1");
        let later = now + std::time::Duration::from_secs(10_000);
        assert_eq!(reg.visible_aim("p-a", later), Some(allyl));
    }

    #[test]
    fn closing_the_connection_clears_its_aim() {
        let (mut reg, now, allyl) = tour_presence();
        reg.declare_interest("p-a", Some("lc-1"), "allyl", allyl, 360, now, true).unwrap();
        reg.hold_interest_until_conceal("lc-1");
        reg.claim_held(1, "lc-1");
        reg.release_connection(1);
        assert_eq!(reg.visible_aim("p-a", now), None);
        assert_eq!(reg.interest("lc-1"), None);
    }

    #[test]
    fn another_phrase_leaves_the_aim() {
        let (mut reg, now, allyl) = tour_presence();
        reg.declare_interest("p-a", Some("lc-1"), "allyl", allyl, 360, now, true).unwrap();
        let other = crate::highlight::UvBox { u: 0.5, v: 0.6, w: 0.2, h: 0.04 };
        reg.declare_interest("p-a", Some("lc-2"), "inductive", other, 360, now, false).unwrap();
        assert_eq!(reg.visible_aim("p-a", now), Some(allyl));
        let keys: Vec<String> = reg.visible_interests(now).into_iter().map(|(id, _)| id).collect();
        assert!(keys.contains(&"lc-1".into()), "{keys:?}");
        assert!(keys.contains(&"lc-2".into()), "{keys:?}");
    }

    #[test]
    fn a_later_declare_moves_the_aim_box() {
        let (mut reg, now, allyl) = tour_presence();
        reg.declare_interest("p-a", Some("lc-1"), "allyl", allyl, 360, now, true).unwrap();
        let moved = crate::highlight::UvBox { u: 0.22, v: 0.55, w: 0.28, h: 0.04 };
        reg.declare_interest("p-a", Some("lc-1"), "allyl", moved, 360, now, false).unwrap();
        assert_eq!(reg.visible_aim("p-a", now), Some(moved));
    }

    #[test]
    fn concealing_the_aim_phrase_clears_it() {
        let (mut reg, now, allyl) = tour_presence();
        reg.declare_interest("p-a", Some("lc-1"), "allyl", allyl, 360, now, true).unwrap();
        let other = crate::highlight::UvBox { u: 0.5, v: 0.6, w: 0.2, h: 0.04 };
        reg.declare_interest("p-a", Some("lc-2"), "inductive", other, 360, now, false).unwrap();
        reg.retire_interest("lc-2");
        assert_eq!(reg.visible_aim("p-a", now), Some(allyl));
        reg.retire_interest("lc-1");
        assert_eq!(reg.visible_aim("p-a", now), None);
        assert!(reg.visible_interests(now).iter().all(|(id, _)| id != "lc-1"));
    }

    #[test]
    fn a_declare_without_a_phrase_id_is_the_aim() {
        let (mut reg, now, allyl) = tour_presence();
        reg.declare_interest("p-a", Some("lc-1"), "allyl", allyl, 360, now, true).unwrap();
        let taught = crate::highlight::UvBox { u: 0.4, v: 0.5, w: 0.2, h: 0.1 };
        reg.declare_interest("p-a", None, "banner", taught, 360, now, false).unwrap();
        assert_eq!(reg.visible_aim("p-a", now), Some(taught));
        let keys: Vec<String> = reg.visible_interests(now).into_iter().map(|(id, _)| id).collect();
        assert!(keys.contains(&"p-a".into()), "{keys:?}");
        assert!(keys.contains(&"lc-1".into()), "{keys:?}");
    }

    #[test]
    fn a_faded_aim_is_not_returned() {
        let (mut reg, now, allyl) = tour_presence();
        reg.declare_interest("p-a", Some("lc-1"), "allyl", allyl, 1, now, true).unwrap();
        assert_eq!(reg.visible_aim("p-a", now), Some(allyl));
        let later = now + std::time::Duration::from_secs(2);
        assert_eq!(reg.visible_aim("p-a", later), None);
        assert_eq!(reg.interest("lc-1").map(|box_| box_.u), Some(allyl.u));
    }

    #[test]
    fn an_aim_past_three_marks_is_still_the_point() {
        let (mut reg, now, allyl) = tour_presence();
        let boxes = [
            ("lc-1", allyl),
            ("lc-2", crate::highlight::UvBox { u: 0.1, v: 0.1, w: 0.1, h: 0.1 }),
            ("lc-3", crate::highlight::UvBox { u: 0.2, v: 0.1, w: 0.1, h: 0.1 }),
            ("lc-4", crate::highlight::UvBox { u: 0.3, v: 0.1, w: 0.1, h: 0.1 }),
        ];
        for (i, (id, box_)) in boxes.iter().enumerate() {
            reg.declare_interest("p-a", Some(id), id, *box_, 360, now, i == 3).unwrap();
        }
        reg.declare_interest("p-b", Some("lc-2"), "companion", boxes[1].1, 360, now, true).unwrap();
        assert_eq!(reg.visible_interests(now).len(), 4);
        assert_eq!(reg.visible_aim("p-a", now), Some(boxes[3].1));
        assert_eq!(reg.visible_aim("p-b", now), Some(boxes[1].1));
        let aims = reg.visible_aim_ids(now);
        assert_eq!(aims.len(), 2);
        assert!(aims.iter().any(|id| id == "lc-4"), "{aims:?}");
        assert!(aims.iter().any(|id| id == "lc-2"), "{aims:?}");
    }

    fn hover_pair(reg: &mut Registry) -> String {
        use crate::protocol::{TaskMode, TaskPace};
        use crate::task::{TourSeed, TourSeedItem};
        let (book, ledger) = reg.task_and_ledger();
        let id = book
            .insert_tour_chunk(
                ledger,
                TourSeed {
                    origin_user: true,
                    mode: TaskMode::Select,
                    pace: TaskPace::Wait,
                    doc_key: None,
                    items: vec![
                        TourSeedItem { query: "words a".into(), narration: String::new(), page: None },
                        TourSeedItem { query: "words b".into(), narration: String::new(), page: None },
                    ],
                    locus_ids: vec!["lc-a".into(), "lc-b".into()],
                    again: false,
                },
                1_000,
            )
            .unwrap();
        book.admit().unwrap();
        book.begin_task(&id, ledger, 2_000).unwrap();
        id
    }

    #[test]
    fn hover_holds_the_box_and_the_leave_moves_the_index() {
        use crate::control_row::{Px, RowHit};
        let mut reg = Registry::new();
        reg.spawn_presence("p", "ctx", None, None);
        let now = Instant::now();
        let a = UvBox { u: 0.10, v: 0.20, w: 0.30, h: 0.04 };
        let b = UvBox { u: 0.10, v: 0.50, w: 0.30, h: 0.04 };
        let jumped = UvBox { u: 0.55, v: 0.60, w: 0.20, h: 0.08 };
        let id = hover_pair(&mut reg);
        reg.declare_interest("p", Some("lc-a"), "a", a, 360, now, true).unwrap();
        reg.declare_interest("p", Some("lc-b"), "b", b, 360, now, false).unwrap();
        assert_eq!(reg.visible_aim("p", now), Some(a));
        let hands = reg.ledger().events().iter().filter(|ev| {
            matches!(ev.kind, crate::ledger::LedgerKind::Accept | crate::ledger::LedgerKind::Deny | crate::ledger::LedgerKind::Skip)
        }).count();
        {
            let (book, ledger) = reg.task_and_ledger();
            book.row_enter("lc-a", Px { x: 100, y: 400, w: 200, h: 40 }, Some(a), 3_000, ledger);
        }
        assert_eq!(
            reg.ledger().events().iter().filter(|ev| {
                matches!(ev.kind, crate::ledger::LedgerKind::Accept | crate::ledger::LedgerKind::Deny | crate::ledger::LedgerKind::Skip)
            }).count(),
            hands
        );
        assert_eq!(reg.visible_aim("p", now), Some(a));
        assert_eq!(reg.task_book().row_color("lc-a"), crate::highlight::MarkColor::Yellow);
        let (got, _) = reg.declare_interest("p", Some("lc-a"), "jump", jumped, 360, now, true).unwrap();
        assert_eq!(got, a);
        assert_eq!(reg.visible_aim("p", now), Some(a));
        let hold = reg.row_hold_json().unwrap();
        assert_eq!(hold["phraseId"], "lc-a");
        assert_eq!(hold["u"], a.u);
        {
            let (book, ledger) = reg.task_and_ledger();
            let paint = book.row_paint(1920, 1080).unwrap();
            let check = paint.squares.iter().find(|(hit, _)| *hit == RowHit::Check).unwrap().1;
            book.row_press_at(check.x + 2, check.y + 2);
            book.row_release_at(check.x + 2, check.y + 2, ledger, 3_100);
        }
        assert_eq!(reg.visible_aim("p", now), Some(a));
        assert_eq!(reg.task_book().query_value(&id).unwrap()["index"], 0);
        reg.tick_tasks(3_100);
        assert_eq!(reg.task_book().query_value(&id).unwrap()["index"], 0);
        reg.tick_tasks(5_100);
        assert_eq!(reg.task_book().query_value(&id).unwrap()["index"], 1);
        assert_eq!(reg.visible_aim("p", now), Some(a));
        reg.declare_interest("p", Some("lc-b"), "next", b, 360, now, true).unwrap();
        assert_eq!(reg.visible_aim("p", now), Some(b));
    }

    #[test]
    fn a_scroll_lets_the_next_box_land_and_then_hides_the_row() {
        use crate::control_row::Px;
        let mut reg = Registry::new();
        reg.spawn_presence("p", "ctx", None, None);
        let now = Instant::now();
        let a = UvBox { u: 0.10, v: 0.20, w: 0.30, h: 0.04 };
        let moved = UvBox { u: 0.20, v: 0.25, w: 0.20, h: 0.04 };
        hover_pair(&mut reg);
        reg.declare_interest("p", Some("lc-a"), "a", a, 360, now, true).unwrap();
        {
            let (book, ledger) = reg.task_and_ledger();
            book.row_enter("lc-a", Px { x: 100, y: 400, w: 200, h: 40 }, Some(a), 3_000, ledger);
        }
        reg.note_row_scroll();
        let (got, _) = reg.declare_interest("p", Some("lc-a"), "scrolled", moved, 360, now, false).unwrap();
        assert_eq!(got, moved);
        assert!(reg.task_book().row_is_open());
        reg.tick_tasks(3_050);
        assert!(!reg.task_book().row_is_open());
        assert!(reg.row_hold_json().is_none());
    }
}
