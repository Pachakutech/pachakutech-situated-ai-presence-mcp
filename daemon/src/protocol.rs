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
        description: String,
        #[serde(rename = "durationSeconds")]
        duration_seconds: u32,
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
    /// Speak `text`: local TTS -> Rhubarb -> audio + synced jaw/visemes. Cancels any current utterance.
    #[serde(rename = "avatarSpeak")]
    AvatarSpeak {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        text: String,
    },
    /// Interrupt speech: stops audio and face together and cancels pending synthesis.
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
}

/// Applied on the overlay thread after the socket pump. Not a GPU command.
#[derive(Debug, Clone)]
pub enum AvatarCommand {
    Rest,
    Face { jaw_open: f32, morph_index: Option<u32>, morph_weight: f32 },
    Walk { x: f32, z: f32 },
    Speak { text: String },
    StopSpeech,
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
            | Proposal::AvatarStop { proposal_id }
            | Proposal::AvatarWalk { proposal_id, .. } => proposal_id,
        }
    }
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
