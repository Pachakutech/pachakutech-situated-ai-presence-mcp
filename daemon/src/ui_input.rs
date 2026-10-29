//! Physical input. This module is the only constructor of [`T0Witness`].
//!
//! ```compile_fail
//! let _witness = T0Witness { _private: () };
//! ```
//!
//! The private field is the compile failure. This crate is a binary, so
//! Cargo does not execute that doctest. `only_ui_input_constructs_witness`
//! checks the source tree.

use crate::ledger::{Ledger, LedgerError, UserAppend};

/// Nothing outside this module can build one. `append_user` requires it.
pub struct T0Witness {
    _private: (),
}

/// Beside-word control the pointer hit. The word mark is not a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickTarget {
    Check,
    Yes,
    No,
}

/// Tour keys. Ordinary keys are not a variant: the daemon does not watch them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    Ack,
    Stop,
}

pub enum UiEvent {
    Acknowledge { phrase_id: String },
    Uncheck { phrase_id: String },
    Approve { phrase_id: String },
    Discard { phrase_id: String },
    /// Pointer-down on a control square. `ts_ms` is the compositor timestamp.
    Click { phrase_id: String, target: ClickTarget, ts_ms: u64 },
    Hotkey(Hotkey),
}

pub fn apply_ui_event(ledger: &mut Ledger, ev: UiEvent, now_ms: u64) -> Result<u64, LedgerError> {
    let witness = T0Witness { _private: () };
    let append = match ev {
        UiEvent::Acknowledge { phrase_id } => UserAppend::Acknowledged { phrase_id },
        UiEvent::Uncheck { phrase_id } => UserAppend::Cleared { phrase_id },
        UiEvent::Approve { phrase_id } => UserAppend::Approved { phrase_id },
        UiEvent::Discard { phrase_id } => UserAppend::Discarded { phrase_id },
        // Clicks and hotkeys are accepted by the task book, which calls back
        // with one of the variants above. This arm writes nothing.
        UiEvent::Click { .. } | UiEvent::Hotkey(_) => return Err(LedgerError::Schema),
    };
    ledger.append_user(witness, append, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::Tier;
    use crate::ledger::{LedgerKind, ObservedAppend, ViewedTier};
    use crate::protocol::{MatchInfo, MatchStatus, ObservedViewed};
    use crate::highlight::UvBox;

    fn dwell(ledger: &mut Ledger, id: &str) {
        ledger
            .append_observed(
                Tier::T3Sidecar,
                ObservedAppend::Viewed {
                    phrase_id: id.to_string(),
                    task_id: None,
                    viewed: ObservedViewed::Dwelled,
                    uv: Some(UvBox { u: 0.1, v: 0.2, w: 0.3, h: 0.4 }),
                },
                1,
            )
            .unwrap();
    }

    #[test]
    fn ui_ack_uncheck_roundtrip() {
        let mut ledger = Ledger::new();
        dwell(&mut ledger, "one");
        apply_ui_event(&mut ledger, UiEvent::Acknowledge { phrase_id: "one".into() }, 2).unwrap();
        let ack = ledger.events().last().unwrap();
        assert_eq!(ack.kind, LedgerKind::Viewed);
        assert_eq!(ack.viewed, Some(ViewedTier::Acknowledged));
        assert_eq!(ack.by, Tier::T0Physical);
        let view = ledger
            .list_value(&crate::protocol::ListPhraseFlagsBody {
                proposal_id: "p".into(),
                since: None,
                task_id: None,
                doc_key: None,
                decided_by: None,
                limit: None,
                events: false,
                wire_kind: None,
            })
            .unwrap();
        assert_eq!(view["rows"][0]["viewed"], "acknowledged");
        apply_ui_event(&mut ledger, UiEvent::Uncheck { phrase_id: "one".into() }, 3).unwrap();
        let cleared = ledger.events().last().unwrap();
        assert_eq!(cleared.kind, LedgerKind::Cleared);
        assert_eq!(cleared.by, Tier::T0Physical);
        let view = ledger
            .list_value(&crate::protocol::ListPhraseFlagsBody {
                proposal_id: "p".into(),
                since: None,
                task_id: None,
                doc_key: None,
                decided_by: None,
                limit: None,
                events: false,
                wire_kind: None,
            })
            .unwrap();
        assert_eq!(view["rows"][0]["viewed"], "dwelled");
    }

    #[test]
    fn approve_then_discard_latest_wins_history_kept() {
        let mut ledger = Ledger::new();
        ledger
            .append_observed(
                Tier::T3Sidecar,
                ObservedAppend::Located {
                    phrase_id: "one".into(),
                    task_id: None,
                    query: None,
                    doc_key: None,
                    matched: MatchInfo { status: MatchStatus::Located, conf: 1.0, ocr_snippet: None },
                    uv: None,
                },
                1,
            )
            .unwrap();
        apply_ui_event(&mut ledger, UiEvent::Approve { phrase_id: "one".into() }, 2).unwrap();
        apply_ui_event(&mut ledger, UiEvent::Discard { phrase_id: "one".into() }, 3).unwrap();
        assert!(ledger.events().iter().any(|ev| ev.kind == LedgerKind::Approved));
        assert!(ledger.events().iter().any(|ev| ev.kind == LedgerKind::Discarded));
        let view = ledger
            .list_value(&crate::protocol::ListPhraseFlagsBody {
                proposal_id: "p".into(),
                since: None,
                task_id: None,
                doc_key: None,
                decided_by: None,
                limit: None,
                events: false,
                wire_kind: None,
            })
            .unwrap();
        assert_eq!(view["rows"][0]["decision"], "discarded");
        assert_eq!(view["rows"][0]["decisionBy"], "T0");
    }

    #[test]
    fn only_ui_input_constructs_witness() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&root, &mut files);
        for (path, text) in files {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if text.contains("append_user") {
                assert!(
                    name == "ledger.rs" || name == "ui_input.rs",
                    "append_user outside the witness path: {}",
                    path.display()
                );
            }
            if text.contains("T0Witness {") {
                assert_eq!(name, "ui_input.rs", "T0Witness constructed in {}", path.display());
            }
        }
    }

    fn walk(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path, text));
            }
        }
    }
}
