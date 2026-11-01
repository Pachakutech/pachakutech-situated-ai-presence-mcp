//! Choose loci for one ask. Words come from the adapter span.
//! A stance label is stored with the locus. It is not a keep, and this
//! module does not open a viewer.

use crate::docmap::{DocMap, IndexedPassage};
use crate::gate::Tier;
use crate::project::{self, LocusInput, MatchKind, Project, ProjectStore, Source, SourceStatus, Stance, StoreError};
use crate::protocol::{Proposal, ProposalResult, SelectLociBody, StanceLabelIn};
use serde_json::json;
use std::collections::HashSet;

const PER_SOURCE: usize = 5;
const PER_CALL: usize = 40;
const MISSED_QUOTE: &str = "I couldn't find the quoted words. The closest passage is this one.";

pub struct Judgement {
    pub relevant: bool,
    pub confidence: f32,
    pub stance: Stance,
}

pub trait Judge {
    fn judge(&self, query: &str, span: &str) -> Result<Judgement, ()>;
}

/// Keeps every shortlisted span and leaves stance unknown.
pub struct KeepJudge;

impl Judge for KeepJudge {
    fn judge(&self, _query: &str, _span: &str) -> Result<Judgement, ()> {
        Ok(Judgement { relevant: true, confidence: 0.0, stance: Stance::Unknown })
    }
}

pub fn route(proposal: &Proposal, tier: Tier, store: Option<&mut ProjectStore>, docmap: &DocMap) -> Option<ProposalResult> {
    let Proposal::SelectLoci(body) = proposal else {
        return None;
    };
    Some(handle(body, tier, store, docmap, &KeepJudge))
}

pub fn handle(
    body: &SelectLociBody,
    tier: Tier,
    store: Option<&mut ProjectStore>,
    docmap: &DocMap,
    judge: &dyn Judge,
) -> ProposalResult {
    if !matches!(tier, Tier::T1User | Tier::T2Client) {
        return ProposalResult::err(&body.proposal_id, "not_allowed_for_tier:selectLoci");
    }
    let Some(store) = store else {
        return ProposalResult::err(&body.proposal_id, "projects_unavailable");
    };
    let Some(project) = store.get(&body.project_id).cloned() else {
        return ProposalResult::err(&body.proposal_id, StoreError::NoSuchProject.as_wire());
    };
    if project.draft {
        return ProposalResult::err(&body.proposal_id, StoreError::DraftUnconfirmed.as_wire());
    }
    let Some(ask) = project.asks.iter().find(|ask| ask.ask_id == body.ask_id) else {
        return ProposalResult::err(&body.proposal_id, StoreError::NoSuchAsk.as_wire());
    };
    let ask_text = ask.text.clone();
    let mut chosen = if citation_mode(&project) {
        cite(&project, docmap, &ask_text)
    } else {
        research(&project, docmap, &ask_text, judge)
    };
    apply_labels(&mut chosen, &body.labels);
    let existing = match store.stored_locus_ids(&body.project_id) {
        Ok(ids) => ids,
        Err(err) => return ProposalResult::err(&body.proposal_id, err.as_wire()),
    };
    let mut known: HashSet<String> = existing.into_iter().collect();
    for row in &chosen {
        known.insert(project::locus_id(&row.doc_key, row.span_start, row.span_end, &row.words));
    }
    if known.len() > project::LOCUS_CAP {
        return ProposalResult::err(&body.proposal_id, StoreError::QuotaLoci.as_wire());
    }
    let mut ids = Vec::with_capacity(chosen.len());
    for row in chosen {
        match store.upsert_locus(&body.project_id, row.into_input(&body.ask_id)) {
            Ok(id) => ids.push(id),
            Err(err) => return ProposalResult::err(&body.proposal_id, err.as_wire()),
        }
    }
    ProposalResult::ok(
        &body.proposal_id,
        Some(json!({
            "projectId": body.project_id,
            "askId": body.ask_id,
            "locusIds": ids,
        })),
    )
}

struct Chosen {
    source_id: String,
    doc_key: String,
    span_start: u64,
    span_end: u64,
    words: String,
    page: Option<String>,
    line: Option<u32>,
    heading_path: Option<Vec<String>>,
    match_kind: MatchKind,
    narration: Option<String>,
    stance: Option<Stance>,
    stance_by: Option<Tier>,
    stance_conf: Option<f32>,
}

impl Chosen {
    fn into_input(self, ask_id: &str) -> LocusInput {
        LocusInput {
            source_id: self.source_id,
            ask_id: Some(ask_id.to_string()),
            doc_key: self.doc_key,
            span_start: self.span_start,
            span_end: self.span_end,
            words: self.words,
            page: self.page,
            line: self.line,
            heading_path: self.heading_path,
            stance: self.stance,
            stance_by: self.stance_by,
            stance_conf: self.stance_conf,
            match_kind: Some(self.match_kind),
            narration: self.narration,
            ocr_only: false,
            view: None,
        }
    }
}

struct SliceHit {
    start: u64,
    end: u64,
    words: String,
    page: Option<String>,
    line: u32,
    heading: Vec<String>,
}

struct Cand {
    source_index: usize,
    source_id: String,
    doc_key: String,
    position: usize,
    span_start: u64,
    span_end: u64,
    words: String,
    page: Option<String>,
    line: u32,
    heading: Vec<String>,
    score: u32,
}

fn citation_mode(project: &Project) -> bool {
    project.sources.iter().any(|source| filled(&source.quote).is_some() || filled(&source.claim).is_some())
}

fn cite(project: &Project, docmap: &DocMap, ask: &str) -> Vec<Chosen> {
    let mut out = Vec::new();
    for source in &project.sources {
        if source.status == SourceStatus::Failed {
            continue;
        }
        out.push(cite_source(source, &load_passages(docmap, &source.doc_key), ask));
    }
    out
}

fn cite_source(source: &Source, passages: &[IndexedPassage], ask: &str) -> Chosen {
    let quote = filled(&source.quote);
    let query = filled(&source.claim).unwrap_or(ask);
    if let Some(quote) = quote {
        if let Some(hit) = find_slice(passages, |text| exact_in(text, quote)) {
            return from_hit(source, hit, MatchKind::Exact, None);
        }
        if let Some(hit) = find_slice(passages, |text| loose_in(text, quote)) {
            return from_hit(source, hit, MatchKind::Exact, None);
        }
    }
    if let Some((index, _)) = best_passage(passages, query) {
        let (start, end) = passage_bounds(passages, index);
        let passage = &passages[index];
        let narration = quote.map(|_| MISSED_QUOTE.to_string());
        return chosen(
            source,
            start,
            end,
            passage.text.clone(),
            page_of(passage),
            Some(passage.line),
            heading_of(passage),
            MatchKind::Near,
            narration,
        );
    }
    none_locus(source)
}

fn research(project: &Project, docmap: &DocMap, ask: &str, judge: &dyn Judge) -> Vec<Chosen> {
    let mut all = Vec::new();
    for (source_index, source) in project.sources.iter().enumerate() {
        if source.status != SourceStatus::Mapped {
            continue;
        }
        let mut rows = shortlist_source(source_index, source, &load_passages(docmap, &source.doc_key), ask);
        rows.sort_by(|left, right| right.score.cmp(&left.score).then(left.position.cmp(&right.position)));
        rows.truncate(PER_SOURCE);
        all.extend(rows);
    }
    all.sort_by(|left, right| {
        right.score.cmp(&left.score).then(left.source_index.cmp(&right.source_index)).then(left.position.cmp(&right.position))
    });
    all.truncate(PER_CALL);
    all.sort_by(|left, right| left.source_index.cmp(&right.source_index).then(left.span_start.cmp(&right.span_start)));
    let mut out = Vec::new();
    for cand in all {
        match judge.judge(ask, &cand.words) {
            Ok(judgement) if !judgement.relevant => continue,
            Ok(judgement) => {
                let mut row = cand.into_chosen();
                apply_judgement(&mut row, &judgement);
                out.push(row);
            }
            Err(()) => out.push(cand.into_chosen()),
        }
    }
    out
}

fn shortlist_source(source_index: usize, source: &Source, passages: &[IndexedPassage], ask: &str) -> Vec<Cand> {
    let doc_key = source.doc_key.clone().unwrap_or_default();
    let mut base = 0u64;
    let mut rows = Vec::new();
    for (position, passage) in passages.iter().enumerate() {
        let len = passage.text.chars().count() as u64;
        let score = token_score(ask, &passage.text);
        if score > 0 {
            rows.push(Cand {
                source_index,
                source_id: source.source_id.clone(),
                doc_key: doc_key.clone(),
                position,
                span_start: base,
                span_end: base + len,
                words: passage.text.clone(),
                page: page_of(passage),
                line: passage.line,
                heading: passage.heading_path.clone(),
                score,
            });
        }
        base += len;
    }
    rows
}

impl Cand {
    fn into_chosen(self) -> Chosen {
        Chosen {
            source_id: self.source_id,
            doc_key: self.doc_key,
            span_start: self.span_start,
            span_end: self.span_end,
            words: self.words,
            page: self.page,
            line: Some(self.line),
            heading_path: if self.heading.is_empty() { None } else { Some(self.heading) },
            match_kind: MatchKind::Near,
            narration: None,
            stance: Some(Stance::Unknown),
            stance_by: None,
            stance_conf: None,
        }
    }
}

fn apply_judgement(row: &mut Chosen, judgement: &Judgement) {
    if judgement.stance == Stance::Unknown {
        return;
    }
    row.stance = Some(judgement.stance);
    row.stance_by = Some(Tier::T3Sidecar);
    row.stance_conf = Some(clamp_conf(Some(judgement.confidence)));
}

fn apply_labels(rows: &mut [Chosen], labels: &[StanceLabelIn]) {
    for row in rows.iter_mut() {
        for label in labels {
            if label.source_id == row.source_id && label.span_start == row.span_start && label.span_end == row.span_end {
                row.stance = Some(parse_stance(&label.stance));
                row.stance_by = Some(Tier::T2Client);
                row.stance_conf = Some(clamp_conf(label.confidence));
            }
        }
    }
}

fn from_hit(source: &Source, hit: SliceHit, kind: MatchKind, narration: Option<String>) -> Chosen {
    chosen(source, hit.start, hit.end, hit.words, hit.page, Some(hit.line), hit.heading, kind, narration)
}

fn none_locus(source: &Source) -> Chosen {
    chosen(source, 0, 0, String::new(), None, None, Vec::new(), MatchKind::None, None)
}

fn chosen(
    source: &Source,
    span_start: u64,
    span_end: u64,
    words: String,
    page: Option<String>,
    line: Option<u32>,
    heading: Vec<String>,
    kind: MatchKind,
    narration: Option<String>,
) -> Chosen {
    Chosen {
        source_id: source.source_id.clone(),
        doc_key: source.doc_key.clone().unwrap_or_default(),
        span_start,
        span_end,
        words,
        page,
        line,
        heading_path: if heading.is_empty() { None } else { Some(heading) },
        match_kind: kind,
        narration,
        stance: Some(Stance::Unknown),
        stance_by: None,
        stance_conf: None,
    }
}

fn find_slice(passages: &[IndexedPassage], mut hit: impl FnMut(&str) -> Option<(usize, usize, String)>) -> Option<SliceHit> {
    let mut base = 0u64;
    for passage in passages {
        if let Some((start, end, words)) = hit(&passage.text) {
            return Some(SliceHit {
                start: base + start as u64,
                end: base + end as u64,
                words,
                page: page_of(passage),
                line: passage.line,
                heading: passage.heading_path.clone(),
            });
        }
        base += passage.text.chars().count() as u64;
    }
    None
}

fn exact_in(text: &str, quote: &str) -> Option<(usize, usize, String)> {
    if quote.is_empty() {
        return None;
    }
    let byte = text.find(quote)?;
    let start = text[..byte].chars().count();
    let words = quote.to_string();
    Some((start, start + words.chars().count(), words))
}

fn loose_in(text: &str, quote: &str) -> Option<(usize, usize, String)> {
    let (needle, _) = alnum_index(quote);
    if needle.is_empty() {
        return None;
    }
    let (hay, idxs) = alnum_index(text);
    let byte = hay.find(&needle)?;
    let start_alnum = hay[..byte].chars().count();
    let needle_chars = needle.chars().count();
    let first = *idxs.get(start_alnum)?;
    let last = *idxs.get(start_alnum + needle_chars - 1)?;
    let words: String = text.chars().skip(first).take(last - first + 1).collect();
    Some((first, last + 1, words))
}

fn alnum_index(text: &str) -> (String, Vec<usize>) {
    let mut kept = String::new();
    let mut idxs = Vec::new();
    for (index, ch) in text.chars().enumerate() {
        if ch.is_alphanumeric() {
            kept.push(ch);
            idxs.push(index);
        }
    }
    (kept, idxs)
}

fn best_passage(passages: &[IndexedPassage], query: &str) -> Option<(usize, u32)> {
    let mut best: Option<(usize, u32)> = None;
    for (index, passage) in passages.iter().enumerate() {
        let score = token_score(query, &passage.text);
        if score == 0 {
            continue;
        }
        match best {
            Some((_, prev)) if score <= prev => {}
            _ => best = Some((index, score)),
        }
    }
    best
}

fn passage_bounds(passages: &[IndexedPassage], index: usize) -> (u64, u64) {
    let mut base = 0u64;
    for (at, passage) in passages.iter().enumerate() {
        let len = passage.text.chars().count() as u64;
        if at == index {
            return (base, base + len);
        }
        base += len;
    }
    (0, 0)
}

fn load_passages(docmap: &DocMap, doc_key: &Option<String>) -> Vec<IndexedPassage> {
    let Some(key) = doc_key.as_deref().filter(|key| !key.is_empty()) else {
        return Vec::new();
    };
    docmap.read_passages(key).unwrap_or_default()
}

fn page_of(passage: &IndexedPassage) -> Option<String> {
    passage.page.map(|page| page.to_string())
}

fn heading_of(passage: &IndexedPassage) -> Vec<String> {
    passage.heading_path.clone()
}

fn filled(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|text| !text.is_empty())
}

fn token_score(query: &str, text: &str) -> u32 {
    let wanted = tokens(query);
    if wanted.is_empty() {
        return 0;
    }
    let have = tokens(text);
    wanted.intersection(&have).count() as u32
}

fn tokens(text: &str) -> HashSet<String> {
    text.split(|ch: char| !ch.is_alphanumeric()).filter(|word| !word.is_empty()).map(|word| word.to_lowercase()).collect()
}

fn parse_stance(raw: &str) -> Stance {
    match raw.trim().to_ascii_lowercase().as_str() {
        "supports" => Stance::Supports,
        "contrary" => Stance::Contrary,
        "mentions" => Stance::Mentions,
        _ => Stance::Unknown,
    }
}

fn clamp_conf(value: Option<f32>) -> f32 {
    match value {
        Some(number) if (0.0..=1.0).contains(&number) => number,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::{self, Crossref};
    use crate::gate::{self, Tier};
    use crate::intake::{FetchReply, PdfReply, Probe};
    use crate::project::{route_project_with, AskInput, AskKind, CitationMeta, Locus, ProjectStore, SourceInput, SourceKind};
    use crate::protocol::Proposal;
    use serde_json::Value;
    use std::cell::Cell;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Off;
    impl Crossref for Off {
        fn resolve(&self, _doi: &str) -> Option<CitationMeta> {
            None
        }
    }

    struct Silent;
    impl Probe for Silent {
        fn fetch(&self, _url: &str) -> FetchReply {
            FetchReply::Unreachable
        }
        fn pdf_first_page(&self, _path: &Path) -> PdfReply {
            PdfReply::NoText
        }
    }

    struct Scratch {
        root: PathBuf,
        store: ProjectStore,
        map: DocMap,
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn scratch(name: &str) -> Scratch {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "presence-c10-{name}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let store = ProjectStore::open(&root.join("projects")).unwrap();
        let map = DocMap::new(root.join("docmap"));
        Scratch { root, store, map }
    }

    fn map_it(scratch: &mut Scratch, id: &str) {
        adapt::map_project(&mut scratch.store, &mut scratch.map, &Silent, id, "2026-10-31", false, &Off).unwrap();
    }

    fn create_cited(scratch: &mut Scratch, body: &str, ask: &str, quote: Option<&str>, claim: Option<&str>) -> String {
        let path = scratch.root.join("page.txt");
        fs::write(&path, body).unwrap();
        let mut source = json!({ "path": path.display().to_string() });
        if let Some(quote) = quote {
            source["quote"] = json!(quote);
        }
        if let Some(claim) = claim {
            source["claim"] = json!(claim);
        }
        let wire = json!({
            "kind": "createProject",
            "proposalId": "c",
            "title": "Cite",
            "asks": [{ "text": ask, "kind": "question" }],
            "sources": [source],
        });
        let proposal: Proposal = serde_json::from_value(wire).unwrap();
        let result = route_project_with(&proposal, Tier::T1User, Some(&mut scratch.store), 1, &Silent).unwrap();
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let id = result.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        map_it(scratch, &id);
        id
    }

    fn select_value(scratch: &mut Scratch, id: &str, ask: &str, tier: Tier, labels: Value, judge: &dyn Judge) -> ProposalResult {
        let wire = json!({
            "kind": "selectLoci",
            "proposalId": "s",
            "projectId": id,
            "askId": ask,
            "labels": labels,
        });
        let proposal: Proposal = serde_json::from_value(wire).unwrap();
        let Proposal::SelectLoci(body) = &proposal else { panic!("selectLoci") };
        handle(body, tier, Some(&mut scratch.store), &scratch.map, judge)
    }

    fn select(scratch: &mut Scratch, id: &str, ask: &str, tier: Tier, judge: &dyn Judge) -> ProposalResult {
        select_value(scratch, id, ask, tier, json!([]), judge)
    }

    fn events_len(scratch: &Scratch, id: &str) -> u64 {
        fs::metadata(scratch.root.join("projects").join(id).join("events.jsonl")).map(|meta| meta.len()).unwrap_or(0)
    }

    fn loci_file_len(scratch: &Scratch, id: &str) -> u64 {
        fs::metadata(scratch.root.join("projects").join(id).join("loci.jsonl")).map(|meta| meta.len()).unwrap_or(0)
    }

    fn one_locus(scratch: &Scratch, id: &str) -> Locus {
        let mut rows = scratch.store.loci(id, None, Some(20)).unwrap();
        assert_eq!(rows.len(), 1);
        rows.pop().unwrap()
    }

    struct CountingJudge {
        calls: Cell<u32>,
    }

    impl Judge for CountingJudge {
        fn judge(&self, _query: &str, _span: &str) -> Result<Judgement, ()> {
            self.calls.set(self.calls.get() + 1);
            Ok(Judgement { relevant: true, confidence: 0.0, stance: Stance::Unknown })
        }
    }

    struct ErrJudge;

    impl Judge for ErrJudge {
        fn judge(&self, _query: &str, _span: &str) -> Result<Judgement, ()> {
            Err(())
        }
    }

    struct SupportsJudge;

    impl Judge for SupportsJudge {
        fn judge(&self, _query: &str, _span: &str) -> Result<Judgement, ()> {
            Ok(Judgement { relevant: true, confidence: 0.25, stance: Stance::Supports })
        }
    }

    struct DropJudge;

    impl Judge for DropJudge {
        fn judge(&self, _query: &str, _span: &str) -> Result<Judgement, ()> {
            Ok(Judgement { relevant: false, confidence: 0.9, stance: Stance::Contrary })
        }
    }

    #[test]
    fn u1_verbatim_quote_is_one_exact_locus() {
        let mut scratch = scratch("u1");
        let id = create_cited(
            &mut scratch,
            "The refund policy is stated here.\n",
            "where is the refund policy",
            Some("refund policy"),
            None,
        );
        let before = events_len(&scratch, &id);
        let judge = CountingJudge { calls: Cell::new(0) };
        let result = select(&mut scratch, &id, "ak-1", Tier::T1User, &judge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let detail = result.detail.unwrap();
        assert_eq!(detail["projectId"], id);
        assert_eq!(detail["askId"], "ak-1");
        let locus = one_locus(&scratch, &id);
        assert_eq!(detail["locusIds"], json!([locus.locus_id]));
        assert_eq!(locus.match_kind, Some(MatchKind::Exact));
        assert_eq!(locus.words, "refund policy");
        assert_eq!(locus.span_start, 4);
        assert_eq!(locus.span_end, 17);
        assert_eq!(locus.line, Some(1));
        assert!(locus.narration.is_none());
        assert_eq!(locus.stance, Some(Stance::Unknown));
        assert!(locus.stance_by.is_none());
        assert_eq!(judge.calls.get(), 0);
        let again = select(&mut scratch, &id, "ak-1", Tier::T2Client, &KeepJudge);
        assert_eq!(again.status, "ok", "{:?}", again.error);
        assert_eq!(again.detail.unwrap()["locusIds"], json!([locus.locus_id]));
        assert_eq!(scratch.store.stored_locus_ids(&id).unwrap().len(), 1);
        assert_eq!(events_len(&scratch, &id), before);
    }

    #[test]
    fn u2_whitespace_and_punctuation_stay_exact() {
        let mut scratch = scratch("u2");
        let id = create_cited(
            &mut scratch,
            "The refund,   policy is stated here.\n",
            "where is the refund policy",
            Some("refund policy"),
            None,
        );
        let result = select(&mut scratch, &id, "ak-1", Tier::T1User, &KeepJudge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let locus = one_locus(&scratch, &id);
        assert_eq!(locus.match_kind, Some(MatchKind::Exact));
        assert_eq!(locus.words, "refund,   policy");
        assert_ne!(locus.words, "refund policy");
        assert!(locus.narration.is_none());
    }

    #[test]
    fn u3_missing_quote_is_near_with_narration() {
        let mut scratch = scratch("u3");
        let id = create_cited(
            &mut scratch,
            "The refund policy lives in section two.\n",
            "where is the refund policy",
            Some("quoted words that are absent"),
            None,
        );
        let result = select(&mut scratch, &id, "ak-1", Tier::T1User, &KeepJudge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let locus = one_locus(&scratch, &id);
        assert_eq!(locus.match_kind, Some(MatchKind::Near));
        assert_eq!(locus.words, "The refund policy lives in section two.");
        assert_ne!(locus.words, "quoted words that are absent");
        assert_eq!(
            locus.narration.as_deref(),
            Some("I couldn't find the quoted words. The closest passage is this one.")
        );
    }

    #[test]
    fn u4_no_overlap_is_none_with_empty_words() {
        let mut scratch = scratch("u4");
        let id = create_cited(
            &mut scratch,
            "apples and oranges\n",
            "moonlight sonata",
            Some("quantum ledger"),
            Some("zebra habitat"),
        );
        let result = select(&mut scratch, &id, "ak-1", Tier::T1User, &KeepJudge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let locus = one_locus(&scratch, &id);
        assert_eq!(locus.match_kind, Some(MatchKind::None));
        assert_eq!(locus.words, "");
        assert_eq!((locus.span_start, locus.span_end), (0, 0));
        assert!(locus.narration.is_none());
        assert!(!locus.doc_key.is_empty());
    }

    #[test]
    fn u5_research_caps_and_order() {
        let mut scratch = scratch("u5");
        let mut inputs = Vec::new();
        for index in 1..=10 {
            let mut body = String::new();
            for line in 0..6 {
                body.push_str(&format!("refund policy {index:02}{line}\n"));
            }
            body.push_str("zebra habitat\n");
            let path = scratch.root.join(format!("s{index}.txt"));
            fs::write(&path, body).unwrap();
            inputs.push(SourceInput { kind: SourceKind::File, reference: path.display().to_string() });
        }
        let id = scratch
            .store
            .create(
                Tier::T1User,
                "Research",
                &[AskInput { text: "refund policy".into(), kind: AskKind::Question }],
                &inputs,
                &[],
                1,
            )
            .unwrap();
        map_it(&mut scratch, &id);
        let pending = scratch.root.join("pending.txt");
        fs::write(&pending, "refund policy pending\nzebra habitat\n").unwrap();
        scratch
            .store
            .add_sources(&id, &[SourceInput { kind: SourceKind::File, reference: pending.display().to_string() }])
            .unwrap();
        let result = select(&mut scratch, &id, "ak-1", Tier::T1User, &KeepJudge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let ids = result.detail.unwrap()["locusIds"].as_array().unwrap().clone();
        assert_eq!(ids.len(), 40);
        let loci = scratch.store.loci(&id, None, Some(500)).unwrap();
        assert_eq!(loci.len(), 40);
        for (index, locus) in loci.iter().enumerate() {
            let source_n = index / 5 + 1;
            assert_eq!(ids[index], json!(locus.locus_id));
            assert_eq!(locus.source_id, format!("sr-{source_n}"));
            assert_eq!(locus.words, format!("refund policy {source_n:02}{}", index % 5));
            assert_eq!(locus.span_start, (index % 5 * 17) as u64);
            assert_eq!(locus.match_kind, Some(MatchKind::Near));
            assert!(locus.narration.is_none());
            assert_eq!(locus.stance, Some(Stance::Unknown));
            assert!(locus.stance_by.is_none());
        }
        assert!(loci.iter().all(|locus| !locus.words.ends_with('5') && locus.words != "zebra habitat"));
        assert!(loci.iter().all(|locus| locus.source_id != "sr-9" && locus.source_id != "sr-10" && locus.source_id != "sr-11"));
    }

    #[test]
    fn u6_judge_error_keeps_the_span_unknown() {
        let mut scratch = scratch("u6");
        let path = scratch.root.join("a.txt");
        fs::write(&path, "refund policy lives here\n").unwrap();
        let id = scratch
            .store
            .create(
                Tier::T1User,
                "Research",
                &[AskInput { text: "refund policy".into(), kind: AskKind::Question }],
                &[SourceInput { kind: SourceKind::File, reference: path.display().to_string() }],
                &[],
                1,
            )
            .unwrap();
        map_it(&mut scratch, &id);
        let result = select(&mut scratch, &id, "ak-1", Tier::T1User, &ErrJudge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let locus = one_locus(&scratch, &id);
        assert_eq!(locus.words, "refund policy lives here");
        assert_eq!(locus.stance, Some(Stance::Unknown));
        assert!(locus.stance_by.is_none());
        assert!(locus.stance_conf.is_none());
        assert!(scratch.store.keeps(&id).unwrap().is_empty());
    }

    #[test]
    fn u7_stance_does_not_change_keeps() {
        let mut scratch = scratch("u7");
        let path = scratch.root.join("a.txt");
        let text = "refund policy lives here\n";
        fs::write(&path, text).unwrap();
        let id = scratch
            .store
            .create(
                Tier::T1User,
                "Research",
                &[AskInput { text: "refund policy".into(), kind: AskKind::Question }],
                &[SourceInput { kind: SourceKind::File, reference: path.display().to_string() }],
                &[],
                1,
            )
            .unwrap();
        map_it(&mut scratch, &id);
        let before = events_len(&scratch, &id);
        let span_end = "refund policy lives here".chars().count() as u64;
        let labels = json!([
            { "sourceId": "sr-1", "spanStart": 0, "spanEnd": span_end, "stance": "mentions", "confidence": 2.0 },
            { "sourceId": "sr-1", "spanStart": 99, "spanEnd": 100, "stance": "supports", "confidence": 0.5 }
        ]);
        let result = select_value(&mut scratch, &id, "ak-1", Tier::T1User, labels, &SupportsJudge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        let locus = one_locus(&scratch, &id);
        assert_eq!(locus.stance, Some(Stance::Mentions));
        assert_eq!(locus.stance_by, Some(Tier::T2Client));
        assert_eq!(locus.stance_conf, Some(0.0));
        assert!(scratch.store.keeps(&id).unwrap().is_empty());
        assert_eq!(scratch.store.stored_locus_ids(&id).unwrap().len(), 1);
        assert_eq!(events_len(&scratch, &id), before);
    }

    #[test]
    fn u8_t3_select_loci_is_refused() {
        let mut scratch = scratch("u8");
        let t3 = select(&mut scratch, "pj-0123456789abcdef", "ak-1", Tier::T3Sidecar, &KeepJudge);
        assert_eq!(t3.status, "error");
        assert_eq!(t3.error.as_deref(), Some("not_allowed_for_tier:selectLoci"));
        let t0 = select(&mut scratch, "pj-0123456789abcdef", "ak-1", Tier::T0Physical, &KeepJudge);
        assert_eq!(t0.error.as_deref(), Some("not_allowed_for_tier:selectLoci"));
        let proposal: Proposal = serde_json::from_str(
            r#"{"kind":"selectLoci","proposalId":"s","projectId":"pj-0123456789abcdef","askId":"ak-1"}"#,
        )
        .unwrap();
        assert!(gate::check(Tier::T1User, &proposal).is_ok());
        assert!(gate::check(Tier::T2Client, &proposal).is_ok());
        let refused = gate::check(Tier::T3Sidecar, &proposal).unwrap_err();
        assert_eq!(refused.error.as_deref(), Some("not_allowed_for_tier:selectLoci"));
    }

    #[test]
    fn u9_select_loci_opens_nothing_and_appends_no_event() {
        let prod = include_str!("select.rs").split("mod tests").next().unwrap();
        for word in ["evince", "chromium", "ViewerMove", "viewerMove", "append_event", "map_project", "map_pending", "Command::new"] {
            assert!(!prod.contains(word), "{word}");
        }
        let mut scratch = scratch("u9");
        let id = create_cited(&mut scratch, "The refund policy is stated here.\n", "where is the refund policy", Some("refund policy"), None);
        let events = events_len(&scratch, &id);
        let result = select(&mut scratch, &id, "ak-1", Tier::T2Client, &KeepJudge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        assert_eq!(result.detail.unwrap()["locusIds"].as_array().unwrap().len(), 1);
        assert_eq!(events_len(&scratch, &id), events);
    }

    #[test]
    fn relevant_false_drops_the_span_and_a_label_does_not_restore_it() {
        let mut scratch = scratch("drop");
        let path = scratch.root.join("a.txt");
        fs::write(&path, "refund policy lives here\n").unwrap();
        let id = scratch
            .store
            .create(
                Tier::T1User,
                "Research",
                &[AskInput { text: "refund policy".into(), kind: AskKind::Question }],
                &[SourceInput { kind: SourceKind::File, reference: path.display().to_string() }],
                &[],
                1,
            )
            .unwrap();
        map_it(&mut scratch, &id);
        let span_end = "refund policy lives here".chars().count() as u64;
        let labels = json!([{ "sourceId": "sr-1", "spanStart": 0, "spanEnd": span_end, "stance": "supports" }]);
        let result = select_value(&mut scratch, &id, "ak-1", Tier::T1User, labels, &DropJudge);
        assert_eq!(result.status, "ok", "{:?}", result.error);
        assert_eq!(result.detail.unwrap()["locusIds"].as_array().unwrap().len(), 0);
        assert!(scratch.store.stored_locus_ids(&id).unwrap().is_empty());
    }

    #[test]
    fn draft_missing_and_quota_write_nothing() {
        let mut scratch = scratch("edge");
        let draft_wire = json!({
            "kind": "createProject",
            "proposalId": "c",
            "title": "Draft",
            "asks": [{ "text": "refund policy", "kind": "question" }],
            "sources": [{ "path": "/no/such/c10-draft.txt" }],
        });
        let proposal: Proposal = serde_json::from_value(draft_wire).unwrap();
        let created = route_project_with(&proposal, Tier::T3Sidecar, Some(&mut scratch.store), 1, &Silent).unwrap();
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let draft_id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let refused = select(&mut scratch, &draft_id, "ak-1", Tier::T1User, &KeepJudge);
        assert_eq!(refused.error.as_deref(), Some("draft_unconfirmed"));

        let missing = select(&mut scratch, "pj-0123456789abcdef", "ak-1", Tier::T1User, &KeepJudge);
        assert_eq!(missing.error.as_deref(), Some("no_such_project"));

        let path = scratch.root.join("a.txt");
        fs::write(&path, "refund policy lives here\n").unwrap();
        let id = scratch
            .store
            .create(
                Tier::T1User,
                "Research",
                &[AskInput { text: "refund policy".into(), kind: AskKind::Question }],
                &[SourceInput { kind: SourceKind::File, reference: path.display().to_string() }],
                &[],
                1,
            )
            .unwrap();
        map_it(&mut scratch, &id);
        let wrong_ask = select(&mut scratch, &id, "ak-9", Tier::T1User, &KeepJudge);
        assert_eq!(wrong_ask.error.as_deref(), Some("no_such_ask"));
        for n in 0..project::LOCUS_CAP {
            scratch
                .store
                .upsert_locus(
                    &id,
                    LocusInput {
                        source_id: "sr-1".into(),
                        ask_id: Some("ak-1".into()),
                        doc_key: "seed".into(),
                        span_start: n as u64,
                        span_end: n as u64 + 1,
                        words: format!("w{n}"),
                        page: None,
                        line: None,
                        heading_path: None,
                        stance: None,
                        stance_by: None,
                        stance_conf: None,
                        match_kind: None,
                        narration: None,
                        ocr_only: false,
                        view: None,
                    },
                )
                .unwrap();
        }
        let count = scratch.store.stored_locus_ids(&id).unwrap().len();
        let bytes = loci_file_len(&scratch, &id);
        let quota = select(&mut scratch, &id, "ak-1", Tier::T2Client, &KeepJudge);
        assert_eq!(quota.error.as_deref(), Some("quota:loci"));
        assert_eq!(scratch.store.stored_locus_ids(&id).unwrap().len(), count);
        assert_eq!(loci_file_len(&scratch, &id), bytes);
    }

    #[test]
    fn select_loci_rejects_a_path_field() {
        let parsed = serde_json::from_str::<Proposal>(
            r#"{"kind":"selectLoci","proposalId":"s","projectId":"pj-0123456789abcdef","askId":"ak-1","path":"/tmp/x"}"#,
        );
        assert!(parsed.is_err());
    }
}
