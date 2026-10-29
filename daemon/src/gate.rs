//! Per-endpoint allow-list. Tier comes from the listener that accepted the
//! connection. A proposal body cannot name a tier.

use crate::protocol::{Proposal, ProposalResult};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Tier {
    #[serde(rename = "T0")]
    T0Physical,
    #[serde(rename = "T1")]
    T1User,
    #[serde(rename = "T2")]
    T2Client,
    #[serde(rename = "T3")]
    T3Sidecar,
}

impl Tier {
    pub fn wire_name(self) -> &'static str {
        match self {
            Tier::T0Physical => "T0",
            Tier::T1User => "T1",
            Tier::T2Client => "T2",
            Tier::T3Sidecar => "T3",
        }
    }
}

/// Every kind that exists today is T2 only. Give a new variant its own arm.
/// Do not add it to the T2 arm, and do not write a wildcard. C2's
/// `observePhrase` is T3 only.
pub fn allowed(tier: Tier, proposal: &Proposal) -> bool {
    match proposal {
        Proposal::HighlightRegion { .. }
        | Proposal::SpawnPresence { .. }
        | Proposal::AnimatePresence { .. }
        | Proposal::RetirePresence { .. }
        | Proposal::AddArtifact { .. }
        | Proposal::RetireArtifact { .. }
        | Proposal::Shutdown { .. }
        | Proposal::AvatarRest { .. }
        | Proposal::AvatarFace { .. }
        | Proposal::StopPresence { .. }
        | Proposal::AvatarSpeak { .. }
        | Proposal::AvatarStop { .. }
        | Proposal::AvatarWalk { .. }
        | Proposal::QueryScreenText { .. }
        | Proposal::QuerySpeech { .. }
        | Proposal::ConcealHighlight { .. } => matches!(tier, Tier::T2Client),
    }
}

pub fn check(tier: Tier, proposal: &Proposal) -> Result<(), ProposalResult> {
    if allowed(tier, proposal) {
        Ok(())
    } else {
        Err(ProposalResult::err(
            proposal.proposal_id(),
            format!("not_allowed_for_tier:{}", proposal.kind_name()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<(Proposal, &'static str)> {
        const LINES: &[(&str, &str)] = &[
            (
                r#"{"kind":"highlightRegion","proposalId":"9","presenceId":"p-a","description":"the banner","u":0.1,"v":0.2,"w":0.3,"h":0.4,"durationSeconds":12}"#,
                "highlightRegion",
            ),
            (
                r#"{"kind":"spawnPresence","proposalId":"1","presenceId":"p-a","sourceContext":"x","styleHint":null,"artifactId":null}"#,
                "spawnPresence",
            ),
            (
                r#"{"kind":"animatePresence","proposalId":"2","presenceId":"p-a","text":"Hello"}"#,
                "animatePresence",
            ),
            (
                r#"{"kind":"retirePresence","proposalId":"3","presenceId":"p-a"}"#,
                "retirePresence",
            ),
            (
                r#"{"kind":"addArtifact","proposalId":"4","artifactId":"a-1","description":"cloud","sourceUri":null}"#,
                "addArtifact",
            ),
            (
                r#"{"kind":"retireArtifact","proposalId":"5","artifactId":"a-1"}"#,
                "retireArtifact",
            ),
            (r#"{"kind":"shutdown","proposalId":"6"}"#, "shutdown"),
            (r#"{"kind":"avatarRest","proposalId":"7"}"#, "avatarRest"),
            (
                r#"{"kind":"avatarFace","proposalId":"8","jawOpen":0.4,"morphIndex":null,"morphWeight":null}"#,
                "avatarFace",
            ),
            (
                r#"{"kind":"stopPresence","proposalId":"s","presenceId":"p-a"}"#,
                "stopPresence",
            ),
            (r#"{"kind":"avatarSpeak","proposalId":"10","text":"Hello"}"#, "avatarSpeak"),
            (r#"{"kind":"avatarStop","proposalId":"11"}"#, "avatarStop"),
            (r#"{"kind":"avatarWalk","proposalId":"12","x":1.0,"z":2.0}"#, "avatarWalk"),
            (r#"{"kind":"queryScreenText","proposalId":"13"}"#, "queryScreenText"),
            (r#"{"kind":"querySpeech","proposalId":"14"}"#, "querySpeech"),
            (
                r#"{"kind":"concealHighlight","proposalId":"15","presenceId":"p-a"}"#,
                "concealHighlight",
            ),
        ];
        LINES
            .iter()
            .map(|(json, kind)| {
                let proposal: Proposal = serde_json::from_str(json).unwrap_or_else(|e| panic!("{kind}: {e}"));
                assert_eq!(proposal.kind_name(), *kind);
                (proposal, *kind)
            })
            .collect()
    }

    #[test]
    fn t2_allows_every_existing_kind() {
        for (proposal, kind) in samples() {
            assert!(allowed(Tier::T2Client, &proposal), "{kind}");
            assert!(check(Tier::T2Client, &proposal).is_ok(), "{kind}");
        }
    }

    #[test]
    fn t1_and_t3_reject_every_existing_kind() {
        for (proposal, kind) in samples() {
            for tier in [Tier::T1User, Tier::T3Sidecar] {
                assert!(!allowed(tier, &proposal), "{kind} on {tier:?}");
                let message = check(tier, &proposal).unwrap_err().error.expect("error string");
                assert!(message.starts_with("not_allowed_for_tier:"), "{message}");
                assert!(message.contains(kind), "{message} does not name {kind}");
            }
        }
    }

    #[test]
    fn t0_has_no_socket_kinds() {
        for (proposal, kind) in samples() {
            assert!(!allowed(Tier::T0Physical, &proposal), "{kind}");
            let message = check(Tier::T0Physical, &proposal).unwrap_err().error.expect("error string");
            assert!(message.starts_with("not_allowed_for_tier:"), "{message}");
            assert!(message.contains(kind), "{message}");
        }
    }

    #[test]
    fn allow_list_table_matches_doc() {
        let documented = [
            "highlightRegion",
            "spawnPresence",
            "animatePresence",
            "retirePresence",
            "addArtifact",
            "retireArtifact",
            "queryScreenText",
            "querySpeech",
            "concealHighlight",
            "stopPresence",
            "shutdown",
            "avatarRest",
            "avatarFace",
            "avatarSpeak",
            "avatarStop",
            "avatarWalk",
        ];
        let got: Vec<&str> = samples().iter().map(|(_, kind)| *kind).collect();
        let mut got_sorted = got.clone();
        let mut documented_sorted = documented.to_vec();
        got_sorted.sort_unstable();
        documented_sorted.sort_unstable();
        assert_eq!(got_sorted, documented_sorted);
        assert_eq!(serde_json::to_string(&Tier::T0Physical).unwrap(), "\"T0\"");
        assert_eq!(serde_json::to_string(&Tier::T1User).unwrap(), "\"T1\"");
        assert_eq!(serde_json::to_string(&Tier::T2Client).unwrap(), "\"T2\"");
        assert_eq!(serde_json::to_string(&Tier::T3Sidecar).unwrap(), "\"T3\"");
        for (proposal, kind) in samples() {
            assert!(documented.contains(&kind), "{kind}");
            assert_eq!(allowed(Tier::T2Client, &proposal), true, "{kind}");
            assert_eq!(allowed(Tier::T1User, &proposal), false, "{kind}");
            assert_eq!(allowed(Tier::T3Sidecar, &proposal), false, "{kind}");
        }
    }

    #[test]
    fn body_cannot_claim_tier() {
        let proposal: Proposal = serde_json::from_str(
            r#"{"kind":"highlightRegion","proposalId":"9","presenceId":"p-a","description":"the banner","u":0.1,"v":0.2,"w":0.3,"h":0.4,"durationSeconds":12,"tier":"T0"}"#,
        )
        .expect("extra tier field still parses");
        assert!(matches!(proposal, Proposal::HighlightRegion { .. }));
        assert_eq!(proposal.kind_name(), "highlightRegion");
        assert!(check(Tier::T2Client, &proposal).is_ok());
        let rejected = check(Tier::T3Sidecar, &proposal).unwrap_err();
        assert_eq!(rejected.proposal_id, "9");
        assert_eq!(rejected.error.as_deref(), Some("not_allowed_for_tier:highlightRegion"));
    }

    #[test]
    #[ignore = "enabled by C2"]
    fn observe_phrase_rejected_on_mcp() {
        panic!("C2 enables observe_phrase_rejected_on_mcp");
    }

    #[test]
    #[ignore = "enabled by C2"]
    fn create_task_rejected_on_sidecar() {
        panic!("C2 enables create_task_rejected_on_sidecar");
    }

    #[test]
    #[ignore = "enabled by C3"]
    fn draft_task_rejected_on_mcp() {
        panic!("C3 enables draft_task_rejected_on_mcp");
    }
}
