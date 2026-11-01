//! One background thread that prepares pending project sources.
//! The overlay thread pushes job descriptors and later applies the outcome.
//! This module does not open a viewer.

use crate::adapt::{self, MapJob, Prepared};
use crate::docmap::DocMap;
use crate::project::{Project, ProjectStore, SourceStatus};
use crate::protocol::{Proposal, ProposalResult};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread;

/// Outcomes waiting on the overlay. A full channel blocks the worker.
const RESULT_BOUND: usize = 2;

pub struct MapReport {
    pub project_id: String,
    pub source_id: String,
    pub prepared: Prepared,
}

pub struct WorkerEnds {
    jobs: Receiver<MapJob>,
    results: SyncSender<MapReport>,
    docmap_root: PathBuf,
}

/// Job list the registry can push while it already holds its mutex.
pub struct MapInbox {
    jobs_tx: Sender<MapJob>,
    jobs_rx: Option<Receiver<MapJob>>,
    results_tx: Option<SyncSender<MapReport>>,
    results_rx: Receiver<MapReport>,
    tracked: HashSet<(String, String)>,
}

impl MapInbox {
    pub fn open() -> Self {
        let (jobs_tx, jobs_rx) = mpsc::channel();
        let (results_tx, results_rx) = mpsc::sync_channel(RESULT_BOUND);
        Self {
            jobs_tx,
            jobs_rx: Some(jobs_rx),
            results_tx: Some(results_tx),
            results_rx,
            tracked: HashSet::new(),
        }
    }

    /// Moves the worker ends out once. A later call returns nothing.
    pub fn take_worker(&mut self, docmap_root: PathBuf) -> Option<WorkerEnds> {
        Some(WorkerEnds { jobs: self.jobs_rx.take()?, results: self.results_tx.take()?, docmap_root })
    }

    /// Copies the descriptor onto the queue. A pair already queued or in flight is left there.
    pub fn enqueue(&mut self, job: MapJob) -> bool {
        let key = (job.project_id.clone(), job.source_id.clone());
        if self.tracked.contains(&key) {
            return false;
        }
        if self.jobs_tx.send(job).is_err() {
            return false;
        }
        self.tracked.insert(key);
        true
    }

    pub fn pop(&mut self) -> Option<MapJob> {
        self.jobs_rx.as_ref()?.try_recv().ok()
    }

    pub fn drain(&mut self) -> Vec<MapReport> {
        let mut out = Vec::new();
        while let Ok(report) = self.results_rx.try_recv() {
            out.push(report);
        }
        out
    }

    pub fn forget(&mut self, project_id: &str, source_id: &str) {
        self.tracked.remove(&(project_id.to_string(), source_id.to_string()));
    }
}

/// Starts the `map-source` thread. It prepares one job at a time.
pub fn spawn(ends: WorkerEnds) {
    let _ = thread::Builder::new().name("map-source".into()).spawn(move || {
        let mut docmap = DocMap::new(ends.docmap_root);
        let probe = crate::intake::LiveProbe;
        while let Ok(job) = ends.jobs.recv() {
            let prepared = match adapt::prepare(&job, &mut docmap, &probe, &adapt::LiveCrossref, &accessed_day(), adapt::crossref_enabled()) {
                Ok(prepared) => prepared,
                Err(reason) => Prepared::Failed { reason: reason.to_string() },
            };
            eprintln!("{}", format_map_line(&job.project_id, &job.source_id, &prepared));
            let report = MapReport { project_id: job.project_id, source_id: job.source_id, prepared };
            if ends.results.send(report).is_err() {
                break;
            }
        }
    });
}

pub fn format_map_line(project_id: &str, source_id: &str, prepared: &Prepared) -> String {
    match prepared {
        Prepared::Mapped { doc_key, .. } => format!("[map] {project_id} {source_id} mapped {doc_key}"),
        Prepared::Failed { reason } => format!("[map] {project_id} {source_id} failed {reason}"),
    }
}

/// Jobs for a proposal that has already returned success. A draft contributes none.
pub fn jobs_for_success(proposal: &Proposal, result: &ProposalResult, store: &ProjectStore) -> Vec<MapJob> {
    if result.status != "ok" {
        return Vec::new();
    }
    let Some(project_id) = project_id_of(proposal, result) else {
        return Vec::new();
    };
    let Some(project) = store.get(&project_id) else {
        return Vec::new();
    };
    if project.draft {
        return Vec::new();
    }
    pending_jobs(project)
}

/// Pending sources of confirmed projects. Mapped and failed sources are absent.
pub fn startup_jobs(store: &ProjectStore) -> Vec<MapJob> {
    let mut projects = store.projects();
    projects.sort_by(|left, right| left.project_id.cmp(&right.project_id));
    let mut jobs = Vec::new();
    for project in projects {
        if project.draft {
            continue;
        }
        jobs.extend(pending_jobs(project));
    }
    jobs
}

fn project_id_of(proposal: &Proposal, result: &ProposalResult) -> Option<String> {
    match proposal {
        Proposal::CreateProject(_) => result.detail.as_ref()?.get("projectId")?.as_str().map(str::to_string),
        Proposal::AddSources(body) => Some(body.project_id.clone()),
        Proposal::ConfirmProject(body) => Some(body.project_id.clone()),
        _ => None,
    }
}

fn pending_jobs(project: &Project) -> Vec<MapJob> {
    project
        .sources
        .iter()
        .filter(|source| source.status == SourceStatus::Pending)
        .map(|source| MapJob {
            project_id: project.project_id.clone(),
            source_id: source.source_id.clone(),
            kind: source.kind,
            reference: source.reference.clone(),
            allow_private: project.allow_private_network,
        })
        .collect()
}

fn accessed_day() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let z = secs / 86400 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::{apply, prepare, Crossref, Prepared};
    use crate::docmap::DocMap;
    use crate::gate::Tier;
    use crate::intake::{BodyReply, FetchReply, PageBody, PdfReply, Probe};
    use crate::project::{route_project_with, AskInput, AskKind, CitationMeta, ProjectStore, SourceInput, SourceKind, SourceStatus};
    use crate::protocol::Proposal;
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Off;
    impl Crossref for Off {
        fn resolve(&self, _doi: &str) -> Option<CitationMeta> {
            None
        }
    }

    struct Dead;
    impl Probe for Dead {
        fn fetch(&self, _url: &str) -> FetchReply {
            FetchReply::Unreachable
        }
        fn pdf_first_page(&self, _path: &Path) -> PdfReply {
            PdfReply::NoText
        }
        fn fetch_body(&self, url: &str) -> BodyReply {
            if url.contains("good.example") {
                return BodyReply::Page(PageBody {
                    status: 200,
                    content_type: Some("text/plain".to_string()),
                    location: None,
                    challenge: false,
                    body: "a mapped page\n".to_string(),
                });
            }
            BodyReply::Page(PageBody {
                status: 403,
                content_type: Some("text/html".to_string()),
                location: None,
                challenge: false,
                body: "<html><title>Just a moment...</title></html>".to_string(),
            })
        }
    }

    struct Reachable;
    impl Probe for Reachable {
        fn fetch(&self, _url: &str) -> FetchReply {
            FetchReply::Response { status: 200, content_length: Some(4), location: None, challenge: false }
        }
        fn pdf_first_page(&self, _path: &Path) -> PdfReply {
            PdfReply::NoText
        }
    }

    struct Bench {
        root: PathBuf,
        store: ProjectStore,
        map: DocMap,
    }

    impl Drop for Bench {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn bench(name: &str) -> Bench {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!("presence-c9b-{name}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let store = ProjectStore::open(&root.join("projects")).unwrap();
        let map = DocMap::new(root.join("docmap"));
        Bench { root, store, map }
    }

    fn file_project(bench: &mut Bench, paths: &[PathBuf]) -> String {
        let sources: Vec<SourceInput> = paths
            .iter()
            .map(|path| SourceInput { kind: SourceKind::File, reference: path.display().to_string() })
            .collect();
        bench
            .store
            .create(
                Tier::T1User,
                "Map",
                &[AskInput { text: "where".into(), kind: AskKind::Question }],
                &sources,
                &[],
                1,
            )
            .unwrap()
    }

    fn slice_fn<'a>(src: &'a str, sig: &str) -> &'a str {
        let start = src.find(sig).unwrap_or_else(|| panic!("missing {sig}"));
        let rest = &src[start..];
        let mut depth = 0i32;
        let mut begun = false;
        for (index, ch) in rest.char_indices() {
            if ch == '{' {
                depth += 1;
                begun = true;
            } else if ch == '}' {
                depth -= 1;
                if begun && depth == 0 {
                    return &rest[..=index];
                }
            }
        }
        panic!("unclosed {sig}");
    }

    #[test]
    fn w1_overlay_starts_the_worker_under_another_name() {
        let overlay = include_str!("overlay.rs");
        let socket = include_str!("socket.rs");
        assert!(overlay.contains("map_worker::spawn"));
        assert!(!overlay.contains("map_pending") && !overlay.contains("map_project"));
        assert!(!socket.contains("map_pending") && !socket.contains("map_project"));
        assert!(include_str!("map_worker.rs").contains("name(\"map-source\""));
    }

    #[test]
    fn w2_a_queued_file_prepares_and_applies_without_the_socket() {
        let mut bench = bench("w2");
        let path = bench.root.join("page.txt");
        fs::write(&path, "alpha passage\n").unwrap();
        let id = file_project(&mut bench, &[path.clone()]);
        let mut inbox = MapInbox::open();
        let noted = inbox.enqueue(MapJob {
            project_id: id.clone(),
            source_id: "sr-1".into(),
            kind: SourceKind::File,
            reference: path.display().to_string(),
            allow_private: false,
        });
        assert!(noted);
        let job = inbox.pop().unwrap();
        assert!(inbox.pop().is_none());
        let prepared = prepare(&job, &mut bench.map, &Dead, &Off, "2026-11-01", false).unwrap();
        let Prepared::Mapped { doc_key, .. } = &prepared else { panic!("file did not map") };
        assert!(bench.map.has_map(doc_key));
        assert!(!bench.map.read_passages(doc_key).unwrap().is_empty());
        let key = doc_key.clone();
        apply(&mut bench.store, &id, "sr-1", prepared).unwrap();
        let source = &bench.store.get(&id).unwrap().sources[0];
        assert_eq!(source.status, SourceStatus::Mapped);
        assert_eq!(source.doc_key.as_deref(), Some(key.as_str()));
        let again = Prepared::Failed { reason: "unreachable".to_string() };
        apply(&mut bench.store, &id, "sr-1", again).unwrap();
        assert_eq!(bench.store.get(&id).unwrap().sources[0].status, SourceStatus::Mapped);
    }

    #[test]
    fn w3_a_failed_web_source_leaves_the_next_mapped() {
        let mut bench = bench("w3");
        let path = bench.root.join("next.txt");
        fs::write(&path, "still here\n").unwrap();
        let id = file_project(&mut bench, &[path.clone()]);
        let web = MapJob {
            project_id: id.clone(),
            source_id: "sr-9".into(),
            kind: SourceKind::Web,
            reference: "https://blocked.example/poem".into(),
            allow_private: false,
        };
        bench.store.add_sources(&id, &[SourceInput { kind: SourceKind::Web, reference: web.reference.clone() }]).unwrap();
        let first = prepare(&web, &mut bench.map, &Dead, &Off, "2026-11-01", false).unwrap();
        let Prepared::Failed { reason } = &first else { panic!("web source mapped") };
        assert_eq!(reason, "http-403");
        apply(&mut bench.store, &id, "sr-2", first).unwrap();
        let file_job = MapJob {
            project_id: id.clone(),
            source_id: "sr-1".into(),
            kind: SourceKind::File,
            reference: path.display().to_string(),
            allow_private: false,
        };
        let second = prepare(&file_job, &mut bench.map, &Dead, &Off, "2026-11-01", false).unwrap();
        apply(&mut bench.store, &id, "sr-1", second).unwrap();
        assert_eq!(bench.store.get(&id).unwrap().sources[1].status, SourceStatus::Failed);
        assert_eq!(bench.store.get(&id).unwrap().sources[1].reason.as_deref(), Some("http-403"));
        assert_eq!(bench.store.get(&id).unwrap().sources[0].status, SourceStatus::Mapped);
        let quiet = Prepared::Mapped { doc_key: "abc".into(), meta: CitationMeta::default() };
        apply(&mut bench.store, &id, "sr-2", quiet).unwrap();
        assert_eq!(bench.store.get(&id).unwrap().sources[1].status, SourceStatus::Failed);
    }

    #[test]
    fn w8_a_draft_is_not_queued_and_confirm_queues_the_pending_source() {
        let mut bench = bench("w8");
        let draft_wire = json!({
            "kind": "createProject",
            "proposalId": "c",
            "title": "Draft",
            "asks": [{ "text": "a question", "kind": "question" }],
            "sources": [{ "url": "https://example.com/c9b" }],
        });
        let draft_proposal: Proposal = serde_json::from_value(draft_wire).unwrap();
        let drafted = route_project_with(&draft_proposal, Tier::T3Sidecar, Some(&mut bench.store), 1, &Reachable).unwrap();
        assert_eq!(drafted.status, "ok", "{:?}", drafted.error);
        assert!(jobs_for_success(&draft_proposal, &drafted, &bench.store).is_empty());
        let draft_id = drafted.detail.as_ref().unwrap()["projectId"].as_str().unwrap().to_string();
        assert!(bench.store.get(&draft_id).unwrap().draft);

        let confirm_wire = json!({
            "kind": "confirmProject",
            "proposalId": "k",
            "projectId": draft_id,
        });
        let confirm: Proposal = serde_json::from_value(confirm_wire).unwrap();
        let confirmed = route_project_with(&confirm, Tier::T1User, Some(&mut bench.store), 2, &Reachable).unwrap();
        assert_eq!(confirmed.status, "ok", "{:?}", confirmed.error);
        assert!(!bench.store.get(&draft_id).unwrap().draft);
        assert_eq!(bench.store.get(&draft_id).unwrap().sources[0].status, SourceStatus::Pending);
        let jobs = jobs_for_success(&confirm, &confirmed, &bench.store);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].source_id, "sr-1");
        assert_eq!(jobs[0].kind, SourceKind::Web);

        let path = bench.root.join("direct.txt");
        fs::write(&path, "direct\n").unwrap();
        let create_wire = json!({
            "kind": "createProject",
            "proposalId": "d",
            "title": "Direct",
            "asks": [{ "text": "a question", "kind": "question" }],
            "sources": [{ "path": path.display().to_string() }],
        });
        let create: Proposal = serde_json::from_value(create_wire).unwrap();
        let created = route_project_with(&create, Tier::T1User, Some(&mut bench.store), 3, &Reachable).unwrap();
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let direct = jobs_for_success(&create, &created, &bench.store);
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].source_id, "sr-1");
        let direct_id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        bench.store.set_source_status(&direct_id, "sr-1", SourceStatus::Mapped, None).unwrap();
        let add_wire = json!({
            "kind": "addSources",
            "proposalId": "a",
            "projectId": direct_id,
            "sources": [{ "url": "https://example.com/added" }],
        });
        let add: Proposal = serde_json::from_value(add_wire).unwrap();
        let added = route_project_with(&add, Tier::T1User, Some(&mut bench.store), 4, &Reachable).unwrap();
        assert_eq!(added.status, "ok", "{:?}", added.error);
        let more = jobs_for_success(&add, &added, &bench.store);
        assert_eq!(more.len(), 1);
        assert_eq!(more[0].source_id, "sr-2");
    }

    #[test]
    fn w9_apply_does_not_fetch_and_prepare_does() {
        let adapt = include_str!("adapt.rs");
        let apply_fn = slice_fn(adapt, "pub fn apply(");
        for word in ["Command::new", "curl", "pdftotext"] {
            assert!(!apply_fn.contains(word), "{word}");
        }
        let prepare_fn = slice_fn(adapt, "pub fn prepare(");
        assert!(prepare_fn.contains("map_web") && prepare_fn.contains("map_file"));
        assert!(slice_fn(adapt, "fn map_pdf(").contains("pdftotext"));
        let crossref = slice_fn(adapt, "fn resolve(");
        assert!(crossref.contains("Command::new"));
        assert!(crossref.contains("curl"));
        assert!(!apply_fn.contains("fn resolve"));
    }

    #[test]
    fn w10_the_log_line_is_the_status_and_not_the_page() {
        let meta = CitationMeta { title: Some("PAGE BODY The Love Song".into()), ..CitationMeta::default() };
        let mapped = format_map_line("pj-1", "sr-1", &Prepared::Mapped { doc_key: "abc123".into(), meta });
        assert_eq!(mapped, "[map] pj-1 sr-1 mapped abc123");
        assert!(!mapped.contains("PAGE BODY"));
        assert!(!mapped.contains("Love Song"));
        assert!(!mapped.contains("HTTP/"));
        let failed = format_map_line("pj-1", "sr-2", &Prepared::Failed { reason: "http-403".into() });
        assert_eq!(failed, "[map] pj-1 sr-2 failed http-403");
        assert!(!failed.contains("cf-mitigated"));
        assert!(!failed.contains("HTTP/"));
        assert!(!failed.contains('\n'));
    }

    #[test]
    fn w11_startup_lists_pending_sources_and_skips_the_rest() {
        let mut bench = bench("w11");
        let pending = bench.root.join("pending.txt");
        let mapped = bench.root.join("mapped.txt");
        let failed = bench.root.join("failed.txt");
        fs::write(&pending, "p\n").unwrap();
        fs::write(&mapped, "m\n").unwrap();
        fs::write(&failed, "f\n").unwrap();
        let live = file_project(&mut bench, &[mapped, failed, pending]);
        bench.store.set_source_status(&live, "sr-1", SourceStatus::Mapped, None).unwrap();
        bench.store.set_source_status(&live, "sr-2", SourceStatus::Failed, Some("unreachable".into())).unwrap();
        let draft_wire = json!({
            "kind": "createProject",
            "proposalId": "c",
            "title": "Draft",
            "asks": [{ "text": "a question", "kind": "question" }],
            "sources": [{ "url": "https://example.com/draft" }],
        });
        let draft_proposal: Proposal = serde_json::from_value(draft_wire).unwrap();
        let drafted = route_project_with(&draft_proposal, Tier::T3Sidecar, Some(&mut bench.store), 1, &Reachable).unwrap();
        assert_eq!(drafted.detail.unwrap()["draft"], true);
        let jobs = startup_jobs(&bench.store);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].project_id, live);
        assert_eq!(jobs[0].source_id, "sr-3");
        assert!(jobs.iter().all(|job| job.source_id != "sr-1" && job.source_id != "sr-2"));
    }

    #[test]
    fn a_queued_pair_is_not_pushed_again_until_it_is_forgotten() {
        let mut inbox = MapInbox::open();
        let job = MapJob {
            project_id: "pj-1".into(),
            source_id: "sr-1".into(),
            kind: SourceKind::File,
            reference: "/tmp/a".into(),
            allow_private: false,
        };
        assert!(inbox.enqueue(job.clone()));
        assert!(!inbox.enqueue(job.clone()));
        let popped = inbox.pop().unwrap();
        assert!(inbox.pop().is_none());
        assert!(!inbox.enqueue(job.clone()));
        inbox.forget(&popped.project_id, &popped.source_id);
        assert!(inbox.enqueue(job));
    }

    #[test]
    fn the_result_channel_holds_two() {
        let (tx, rx) = mpsc::sync_channel::<u8>(RESULT_BOUND);
        assert!(tx.try_send(1).is_ok());
        assert!(tx.try_send(2).is_ok());
        assert!(tx.try_send(3).is_err());
        assert_eq!(rx.try_recv().unwrap(), 1);
    }
}
