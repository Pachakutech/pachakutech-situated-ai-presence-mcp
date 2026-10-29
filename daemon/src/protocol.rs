//! The wire format between the MCP Binding (Node) and this daemon. Matches
//! the sketch in docs/architecture.md exactly: small, typed, newline-
//! delimited JSON over a Unix socket. Nothing dense — no buffers, no
//! pixels — crosses this boundary. Only proposals and results do.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum Proposal {
    #[serde(rename = "highlightRegion")]
    HighlightRegion {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "presenceId")]
        presence_id: String,
        description: String,
        /// Full-screen UV. Origin is the top left. `v` grows downward.
        u: f32,
        v: f32,
        w: f32,
        h: f32,
        #[serde(rename = "durationSeconds")]
        duration_seconds: u32,
        /// The tint stays up for this presence's utterance, then drops.
        /// `durationSeconds` remains the safety cap if speech never finishes.
        #[serde(rename = "untilSpeech", default)]
        until_speech: bool,
    },
    #[serde(rename = "spawnPresence")]
    SpawnPresence {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "presenceId")]
        presence_id: String,
        #[serde(rename = "sourceContext")]
        source_context: String,
        #[serde(rename = "styleHint")]
        style_hint: Option<String>,
        #[serde(rename = "artifactId")]
        artifact_id: Option<String>,
    },
    #[serde(rename = "animatePresence")]
    AnimatePresence {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "presenceId")]
        presence_id: String,
        text: String,
    },
    #[serde(rename = "retirePresence")]
    RetirePresence {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "presenceId")]
        presence_id: String,
    },
    #[serde(rename = "addArtifact")]
    AddArtifact {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "artifactId")]
        artifact_id: String,
        description: String,
        #[serde(rename = "sourceUri")]
        source_uri: Option<String>,
    },
    #[serde(rename = "retireArtifact")]
    RetireArtifact {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "artifactId")]
        artifact_id: String,
    },
    /// Ask the process to leave the overlay loop after this response is written.
    #[serde(rename = "shutdown")]
    Shutdown {
        #[serde(rename = "proposalId")]
        proposal_id: String,
    },
    /// Drop demo motion and return to the bind pose, jaw closed.
    #[serde(rename = "avatarRest")]
    AvatarRest {
        #[serde(rename = "proposalId")]
        proposal_id: String,
    },
    /// Set the jaw and, optionally, one morph-target weight. Turns demo motion off.
    #[serde(rename = "avatarFace")]
    AvatarFace {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "jawOpen")]
        jaw_open: f32,
        #[serde(rename = "morphIndex")]
        morph_index: Option<u32>,
        #[serde(rename = "morphWeight")]
        morph_weight: Option<f32>,
    },
    /// Stop one presence's speech (audio and mouth) and return its face to neutral. Socket/CLI
    /// only; the MCP contract has no such tool.
    #[serde(rename = "stopPresence")]
    StopPresence {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "presenceId")]
        presence_id: String,
    },
    /// DEBUG: speak on the default avatar without a presence. The product path is
    /// `animatePresence` with a presenceId.
    #[serde(rename = "avatarSpeak")]
    AvatarSpeak {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        text: String,
    },
    /// DEBUG: interrupt speech on the default avatar. Product path: `stopPresence`.
    #[serde(rename = "avatarStop")]
    AvatarStop {
        #[serde(rename = "proposalId")]
        proposal_id: String,
    },
    /// Walk the root toward a point on the ground plane. No click-to-walk.
    #[serde(rename = "avatarWalk")]
    AvatarWalk {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        x: f32,
        z: f32,
    },
    /// Latest coherent screen-text message. Does not tint and does not speak.
    #[serde(rename = "queryScreenText")]
    QueryScreenText {
        #[serde(rename = "proposalId")]
        proposal_id: String,
    },
    /// Which sentence of the current utterance is playing. Does not tint or speak.
    #[serde(rename = "querySpeech")]
    QuerySpeech {
        #[serde(rename = "proposalId")]
        proposal_id: String,
    },
    /// Drop a speech-held tint for the moment a page jumps. The box stays.
    /// The voice is not stopped. The next mark raises the tint again.
    #[serde(rename = "concealHighlight")]
    ConcealHighlight {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "presenceId")]
        presence_id: String,
    },
    /// Read the derived phrase view, or the event log when `events` is set.
    #[serde(rename = "listPhraseFlags")]
    ListPhraseFlags(ListPhraseFlagsBody),
    /// Own connection. Later lines on it are ledger events, not proposal replies.
    #[serde(rename = "subscribeLedger")]
    SubscribeLedger(SubscribeLedgerBody),
    /// Sidecar observation. Cannot carry an acknowledgement or a decision.
    #[serde(rename = "observePhrase")]
    ObservePhrase(ObservePhraseBody),
    /// Open a task. T1 stores origin `user`. T2 stores origin `agent`.
    #[serde(rename = "createTask")]
    CreateTask(CreateTaskBody),
    /// Sidecar draft. Draws nothing and moves nothing until the user confirms.
    #[serde(rename = "draftTask")]
    DraftTask(CreateTaskBody),
    #[serde(rename = "cancelTask")]
    CancelTask(CancelTaskBody),
    #[serde(rename = "queryTask")]
    QueryTask(QueryTaskBody),
    /// Report a locate result for an item the task already holds.
    #[serde(rename = "resolveItem")]
    ResolveItem(ResolveItemBody),
    /// Runner facts: the item was entered, narration finished, or the viewer
    /// took the page. None of these is a phrase decision.
    #[serde(rename = "tourSignal")]
    TourSignal(TourSignalBody),
    /// Index one file. The client does not send `docKey`; the daemon hashes the bytes.
    #[serde(rename = "buildDocMap")]
    BuildDocMap(BuildDocMapBody),
    /// Passages from a map. This is not a move and not a decision.
    #[serde(rename = "queryDocMap")]
    QueryDocMap(QueryDocMapBody),
}

/// Applied on the overlay thread after the socket pump. Not a GPU command.
#[derive(Debug, Clone)]
pub enum AvatarCommand {
    Rest,
    Face { jaw_open: f32, morph_index: Option<u32>, morph_weight: f32 },
    Walk { x: f32, z: f32 },
    /// `presence_id: None` is the debug path (avatarSpeak/avatarStop).
    Speak { presence_id: Option<String>, text: String },
    StopSpeech { presence_id: Option<String> },
    /// A presence was granted the avatar body: show it, neutral pose.
    Bind { presence_id: String },
    /// The owning presence was retired: stop speech and hide the body.
    Unbind { presence_id: String },
}

impl Proposal {
    pub fn proposal_id(&self) -> &str {
        match self {
            Proposal::HighlightRegion { proposal_id, .. }
            | Proposal::SpawnPresence { proposal_id, .. }
            | Proposal::AnimatePresence { proposal_id, .. }
            | Proposal::RetirePresence { proposal_id, .. }
            | Proposal::AddArtifact { proposal_id, .. }
            | Proposal::RetireArtifact { proposal_id, .. }
            | Proposal::Shutdown { proposal_id }
            | Proposal::AvatarRest { proposal_id }
            | Proposal::AvatarFace { proposal_id, .. }
            | Proposal::AvatarSpeak { proposal_id, .. }
            | Proposal::StopPresence { proposal_id, .. }
            | Proposal::AvatarStop { proposal_id }
            | Proposal::AvatarWalk { proposal_id, .. }
            | Proposal::QueryScreenText { proposal_id }
            | Proposal::QuerySpeech { proposal_id }
            | Proposal::ConcealHighlight { proposal_id, .. } => proposal_id,
            Proposal::ListPhraseFlags(body) => &body.proposal_id,
            Proposal::SubscribeLedger(body) => &body.proposal_id,
            Proposal::ObservePhrase(body) => &body.proposal_id,
            Proposal::CreateTask(body) | Proposal::DraftTask(body) => &body.proposal_id,
            Proposal::CancelTask(body) => &body.proposal_id,
            Proposal::QueryTask(body) => &body.proposal_id,
            Proposal::ResolveItem(body) => &body.proposal_id,
            Proposal::TourSignal(body) => &body.proposal_id,
            Proposal::BuildDocMap(body) => &body.proposal_id,
            Proposal::QueryDocMap(body) => &body.proposal_id,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Proposal::HighlightRegion { .. } => "highlightRegion",
            Proposal::SpawnPresence { .. } => "spawnPresence",
            Proposal::AnimatePresence { .. } => "animatePresence",
            Proposal::RetirePresence { .. } => "retirePresence",
            Proposal::AddArtifact { .. } => "addArtifact",
            Proposal::RetireArtifact { .. } => "retireArtifact",
            Proposal::Shutdown { .. } => "shutdown",
            Proposal::AvatarRest { .. } => "avatarRest",
            Proposal::AvatarFace { .. } => "avatarFace",
            Proposal::StopPresence { .. } => "stopPresence",
            Proposal::AvatarSpeak { .. } => "avatarSpeak",
            Proposal::AvatarStop { .. } => "avatarStop",
            Proposal::AvatarWalk { .. } => "avatarWalk",
            Proposal::QueryScreenText { .. } => "queryScreenText",
            Proposal::QuerySpeech { .. } => "querySpeech",
            Proposal::ConcealHighlight { .. } => "concealHighlight",
            Proposal::ListPhraseFlags(_) => "listPhraseFlags",
            Proposal::SubscribeLedger(_) => "subscribeLedger",
            Proposal::ObservePhrase(_) => "observePhrase",
            Proposal::CreateTask(_) => "createTask",
            Proposal::DraftTask(_) => "draftTask",
            Proposal::CancelTask(_) => "cancelTask",
            Proposal::QueryTask(_) => "queryTask",
            Proposal::ResolveItem(_) => "resolveItem",
            Proposal::TourSignal(_) => "tourSignal",
            Proposal::BuildDocMap(_) => "buildDocMap",
            Proposal::QueryDocMap(_) => "queryDocMap",
        }
    }
}

/// `notFound` is camelCase on the wire. The other statuses are single words.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MatchStatus {
    #[serde(rename = "located")]
    Located,
    #[serde(rename = "ambiguous")]
    Ambiguous,
    #[serde(rename = "notFound")]
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchInfo {
    pub status: MatchStatus,
    pub conf: f32,
    #[serde(rename = "ocrSnippet", default)]
    pub ocr_snippet: Option<String>,
}

/// What a socket observer may say. `acknowledged` is not a variant.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ObservedViewed {
    #[serde(rename = "shown")]
    Shown,
    #[serde(rename = "dwelled")]
    Dwelled,
}

/// Closed body. `kind` is accepted so the enum tag is not a schema error.
/// Any other unknown field, including a decision, fails the parse.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservePhraseBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(rename = "phraseId", default)]
    pub phrase_id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(rename = "docKey", default)]
    pub doc_key: Option<String>,
    #[serde(rename = "taskId", default)]
    pub task_id: Option<String>,
    #[serde(rename = "match")]
    pub match_info: MatchInfo,
    #[serde(default)]
    pub u: Option<f32>,
    #[serde(default)]
    pub v: Option<f32>,
    #[serde(default)]
    pub w: Option<f32>,
    #[serde(default)]
    pub h: Option<f32>,
    #[serde(default)]
    pub viewed: Option<ObservedViewed>,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListPhraseFlagsBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(rename = "taskId", default)]
    pub task_id: Option<String>,
    #[serde(rename = "docKey", default)]
    pub doc_key: Option<String>,
    #[serde(rename = "decidedBy", default)]
    pub decided_by: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub events: bool,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscribeLedgerBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskMode {
    #[serde(rename = "explain")]
    Explain,
    #[serde(rename = "select")]
    Select,
    #[serde(rename = "approve")]
    Approve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPace {
    #[serde(rename = "flow")]
    Flow,
    #[serde(rename = "wait")]
    Wait,
}

/// What the client may ask to draw. Approve ignores this field: any value is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientControls {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "check")]
    Check,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskItemBody {
    #[serde(rename = "phraseId", default)]
    pub phrase_id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub narration: Option<String>,
    #[serde(default)]
    pub scroll: bool,
    #[serde(default)]
    pub page: Option<u32>,
}

/// Closed body for `createTask` and `draftTask`. `kind` is accepted so the
/// enum tag is not a schema error. A decision field fails the parse.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(rename = "presenceId")]
    pub presence_id: String,
    pub mode: TaskMode,
    #[serde(default)]
    pub pace: Option<TaskPace>,
    #[serde(default)]
    pub controls: Option<ClientControls>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(rename = "ttlSeconds", default)]
    pub ttl_seconds: Option<u32>,
    #[serde(rename = "itemTimeoutSeconds", default)]
    pub item_timeout_seconds: Option<u32>,
    #[serde(rename = "docKey", default)]
    pub doc_key: Option<String>,
    pub items: Vec<TaskItemBody>,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelTaskBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryTaskBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

/// Closed body. `viewed` and a decision field fail the parse.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveItemBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(rename = "phraseId")]
    pub phrase_id: String,
    #[serde(rename = "match")]
    pub match_info: MatchInfo,
    #[serde(default)]
    pub u: Option<f32>,
    #[serde(default)]
    pub v: Option<f32>,
    #[serde(default)]
    pub w: Option<f32>,
    #[serde(default)]
    pub h: Option<f32>,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TourSignalKind {
    #[serde(rename = "begin")]
    Begin,
    #[serde(rename = "narrationDone")]
    NarrationDone,
    #[serde(rename = "preempt")]
    Preempt,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TourSignalBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(rename = "taskId")]
    pub task_id: String,
    pub signal: TourSignalKind,
    /// Why the viewer took the page. Absent on begin and narrationDone.
    #[serde(default)]
    pub reason: Option<PreemptReason>,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

/// A file the user named. `url` parses so the handler can answer `schema`.
/// Any other kind, including a window class, does not parse.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind")]
pub enum DocSource {
    #[serde(rename = "file")]
    File { path: String },
    #[serde(rename = "html")]
    Html { path: String },
    #[serde(rename = "pdf")]
    Pdf { path: String },
    #[serde(rename = "url")]
    Url {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        url: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildDocMapBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    pub source: DocSource,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryDocMapBody {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    #[serde(rename = "docKey")]
    pub doc_key: String,
    pub query: String,
    #[serde(default)]
    pub k: Option<u32>,
    #[serde(default, rename = "kind")]
    pub wire_kind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum PreemptReason {
    #[serde(rename = "page")]
    Page,
    #[serde(rename = "pointer")]
    Pointer,
}

#[derive(Debug, Serialize)]
pub struct ProposalResult {
    #[serde(rename = "proposalId")]
    pub proposal_id: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ProposalResult {
    pub fn ok(proposal_id: &str, detail: Option<serde_json::Value>) -> Self {
        Self { proposal_id: proposal_id.to_string(), status: "ok", detail, error: None }
    }

    pub fn err(proposal_id: &str, message: impl Into<String>) -> Self {
        Self {
            proposal_id: proposal_id.to_string(),
            status: "error",
            detail: None,
            error: Some(message.into()),
        }
    }
}
