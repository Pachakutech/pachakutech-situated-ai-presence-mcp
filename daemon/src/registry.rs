//! Wires the socket protocol to Control/Presence actors and, now, to the
//! live `SplatPipeline`. GPU work on ingest is a bump-allocated upload of
//! the canonical cloud; pose/skinning apply is still TODO on animate.

use crate::actors::gpu_layout::to_gpu_splat;
use crate::actors::{ControlActor, PresenceActor, SceneMemory};
use crate::gate::Tier;
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
    /// One screen box per presence. The mark fades; the box stays until retire.
    interests: InterestBook,
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
            self.interests.retire(&id);
        }
    }

    pub fn mark_colors(&self) -> Vec<(String, crate::highlight::MarkColor)> {
        self.tasks.mark_colors()
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
    /// `secs` is clamped to 1..=360. The returned box is the one that will be drawn.
    pub fn declare_interest(
        &mut self,
        presence_id: &str,
        phrase_id: Option<&str>,
        description: &str,
        raw: UvBox,
        secs: u32,
        now: Instant,
    ) -> Result<(UvBox, u32), String> {
        if !self.has_presence(presence_id) {
            return Err(format!("no live presence with id {presence_id}"));
        }
        let key = phrase_id.map(str::trim).filter(|id| !id.is_empty()).unwrap_or(presence_id);
        let secs = secs.clamp(1, MAX_SECS);
        let got = self.interests.declare(key, raw, secs, now)?;
        println!(
            "[registry] highlight {presence_id} uv {:.3},{:.3} {:.3}x{:.3} for {secs}s — {description}",
            got.u, got.v, got.w, got.h
        );
        Ok((got, secs))
    }

    /// Still the declared box after the mark has faded.
    /// The point gesture reads [`Self::visible_interests`], not this.
    #[allow(dead_code)]
    pub fn interest(&self, presence_id: &str) -> Option<UvBox> {
        self.interests.get(presence_id)
    }

    pub fn visible_interests(&self, now: Instant) -> Vec<(String, UvBox)> {
        self.interests.visible(now)
    }

    pub fn hold_interest_for_speech(&mut self, presence_id: &str) {
        self.interests.hold_for_speech(presence_id);
    }

    pub fn retire_interest(&mut self, id: &str) {
        self.interests.retire(id);
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
        self.interests.retire(presence_id);
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
}
