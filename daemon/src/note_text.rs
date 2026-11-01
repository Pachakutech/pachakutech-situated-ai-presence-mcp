//! Note text as stored, and the escapes used when that text leaves the store.
//!
//! NFC is not applied. The daemon stays on the offline crate set and does not
//! take a normalization library.
//!
//! Stored text is the user's words. Control characters are stripped so a note
//! cannot break a JSON line or a TeX field. JSON is emitted with serde.
//! LaTeX and BibTeX pass through [`latex_escape`] and [`bibtex_field`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub const CHAR_CAP: usize = 2000;
pub const LINE_CAP: usize = 40;
const PREVIEW_CHARS: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteVia {
    Typed,
    Voice,
}

impl NoteVia {
    pub fn wire(self) -> &'static str {
        match self {
            NoteVia::Typed => "typed",
            NoteVia::Voice => "voice",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteFlags {
    pub transcribed: bool,
    pub truncated: bool,
    pub normalized: bool,
}

/// Fields carried on a ledger event. Absent fields stay off the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteCarry {
    pub note_id: String,
    pub text: Option<String>,
    pub via: Option<NoteVia>,
    pub replaces: Option<String>,
    pub flags: Option<NoteFlags>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanText {
    pub text: String,
    pub normalized: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteFact {
    pub note_id: String,
    pub text: String,
    pub via: String,
    pub ts_ms: u64,
    pub replaces: Option<String>,
    pub flags: NoteFlags,
    pub tombstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteView {
    pub note_id: String,
    pub text: String,
    pub via: String,
    pub ts_ms: u64,
    pub flags: NoteFlags,
}

pub fn clean(raw: &str) -> Result<CleanText, &'static str> {
    let (stripped, normalized) = strip_controls(raw);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return Err("empty_note");
    }
    let (text, truncated) = apply_caps(trimmed);
    if text.trim().is_empty() {
        return Err("empty_note");
    }
    Ok(CleanText { text, normalized, truncated })
}

/// Plain overlay text. Control characters only. Markup stays as characters.
pub fn overlay_plain(text: &str) -> String {
    strip_controls(text).0
}

pub fn display_note(text: &str, max_chars: usize) -> String {
    let plain = overlay_plain(text);
    let count = plain.chars().count();
    if max_chars == 0 || count <= max_chars {
        return plain;
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = plain.chars().take(keep).collect();
    out.push('…');
    out
}

/// Spreadsheet formula guard for a later export. Prefix, then CSV quoting.
pub fn csv_cell(value: &str) -> String {
    let mut text = value.to_string();
    if let Some(first) = text.chars().next() {
        if matches!(first, '=' | '+' | '-' | '@' | '\t' | '\r') {
            text.insert(0, '\'');
        }
    }
    if text.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

/// LaTeX text. Backslash first, then the specials. Control characters are
/// already gone. The result contains no `\input`, `\include`, `\write`, or
/// `\immediate` command.
pub fn latex_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in strip_controls(text).0.chars() {
        match ch {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '^' => out.push_str("\\^{}"),
            '~' => out.push_str("\\~{}"),
            _ => out.push(ch),
        }
    }
    out
}

/// One BibTeX field value, wrapped in braces. Brace depth stays balanced
/// because a literal brace is a `{\charNNN}` group, which BibTeX counts
/// and which LaTeX prints as that brace.
pub fn bibtex_field(text: &str) -> String {
    let mut inner = String::new();
    for ch in strip_controls(text).0.chars() {
        match ch {
            '\\' => inner.push_str("\\textbackslash{}"),
            '{' => inner.push_str("{\\char123}"),
            '}' => inner.push_str("{\\char125}"),
            '%' => inner.push_str("\\%"),
            '$' => inner.push_str("\\$"),
            '&' => inner.push_str("\\&"),
            '#' => inner.push_str("\\#"),
            '_' => inner.push_str("\\_"),
            '^' => inner.push_str("\\^{}"),
            '~' => inner.push_str("\\~{}"),
            '\n' => inner.push(' '),
            _ => inner.push(ch),
        }
    }
    format!("{{{inner}}}")
}

pub fn note_view_json(view: &NoteView) -> Value {
    json!({
        "noteId": view.note_id,
        "text": view.text,
        "via": view.via,
        "ts": view.ts_ms,
        "flags": {
            "transcribed": view.flags.transcribed,
            "truncated": view.flags.truncated,
            "normalized": view.flags.normalized,
        },
    })
}

/// Latest revision of each chain. A tombstone hides that chain. History is
/// the caller's event list; this function only builds the view.
pub fn visible_notes(facts: &[NoteFact]) -> Vec<NoteView> {
    let mut deleted = HashSet::new();
    let mut notes = Vec::new();
    for fact in facts {
        if fact.tombstone {
            deleted.insert(fact.note_id.as_str());
        } else {
            notes.push(fact);
        }
    }
    let mut by_id: HashMap<&str, &NoteFact> = HashMap::new();
    for note in &notes {
        by_id.insert(note.note_id.as_str(), *note);
    }
    let mut groups: HashMap<String, Vec<&NoteFact>> = HashMap::new();
    let mut order = Vec::new();
    for note in &notes {
        let root = chain_root(note.note_id.as_str(), &by_id);
        if !groups.contains_key(&root) {
            order.push(root.clone());
        }
        groups.entry(root).or_default().push(*note);
    }
    let mut out = Vec::new();
    for root in order {
        let Some(group) = groups.get(&root) else { continue };
        if group.iter().any(|note| deleted.contains(note.note_id.as_str())) {
            continue;
        }
        let Some(latest) = group.last() else { continue };
        out.push(NoteView {
            note_id: latest.note_id.clone(),
            text: latest.text.clone(),
            via: latest.via.clone(),
            ts_ms: latest.ts_ms,
            flags: latest.flags.clone(),
        });
    }
    out
}

fn chain_root<'a>(id: &'a str, by_id: &HashMap<&str, &NoteFact>) -> String {
    let mut current = id;
    let mut guard = 0;
    while guard < 64 {
        let Some(note) = by_id.get(current) else { break };
        let Some(parent) = note.replaces.as_deref() else { break };
        if !by_id.contains_key(parent) || parent == current {
            break;
        }
        current = parent;
        guard += 1;
    }
    current.to_string()
}

/// C0 except newline, tab becomes a space, DEL, and C1. A carriage return
/// is dropped, so `\r\n` becomes `\n`.
fn strip_controls(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for ch in text.chars() {
        if ch == '\n' {
            out.push(ch);
        } else if ch == '\t' {
            out.push(' ');
            changed = true;
        } else if ch.is_control() {
            changed = true;
        } else {
            out.push(ch);
        }
    }
    (out, changed)
}

fn apply_caps(text: &str) -> (String, bool) {
    let mut truncated = false;
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if lines.len() > LINE_CAP {
        lines.truncate(LINE_CAP);
        truncated = true;
    }
    let mut out = lines.join("\n");
    if out.chars().count() > CHAR_CAP {
        out = out.chars().take(CHAR_CAP - 1).collect();
        truncated = true;
    }
    if truncated && !out.ends_with('…') {
        if out.chars().count() >= CHAR_CAP {
            out = out.chars().take(CHAR_CAP - 1).collect();
        }
        out.push('…');
    }
    if out.chars().count() > CHAR_CAP {
        out = out.chars().take(CHAR_CAP - 1).collect();
        out.push('…');
        truncated = true;
    }
    if out.split('\n').count() > LINE_CAP {
        let mut kept: Vec<String> = out.split('\n').take(LINE_CAP).map(str::to_string).collect();
        if let Some(last) = kept.last_mut() {
            if !last.ends_with('…') {
                last.push('…');
            }
        }
        out = kept.join("\n");
        truncated = true;
    }
    (out, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o6_controls_are_stripped_and_the_words_stay() {
        let raw = format!("keep\u{0001} this\tline\u{007F}\u{0085} end\nnext");
        let cleaned = clean(&raw).unwrap();
        assert!(cleaned.normalized);
        assert!(!cleaned.truncated);
        assert_eq!(cleaned.text, "keep this line end\nnext");
        assert!(cleaned.text.contains('\n'));
        let marks = "bid\u{202A}i\u{200B}x";
        assert_eq!(clean(marks).unwrap().text, marks);
    }

    #[test]
    fn o7_carriage_returns_become_newlines() {
        let cleaned = clean("hello  \r\n\r\nworld  ").unwrap();
        assert_eq!(cleaned.text, "hello  \n\nworld");
        assert!(cleaned.normalized);
    }

    #[test]
    fn o8_blank_text_is_empty() {
        assert_eq!(clean("").unwrap_err(), "empty_note");
        assert_eq!(clean("   \n\t  ").unwrap_err(), "empty_note");
        assert_eq!(clean("\r\n\r\n").unwrap_err(), "empty_note");
    }

    #[test]
    fn o9_caps_truncate_on_a_character_boundary() {
        let line = "the quick brown fox jumps over the lazy dog";
        let many_lines = std::iter::repeat(line).take(100).collect::<Vec<_>>().join("\n");
        let cleaned = clean(&many_lines).unwrap();
        assert!(cleaned.truncated);
        assert!(cleaned.text.chars().count() <= CHAR_CAP);
        assert!(cleaned.text.split('\n').count() <= LINE_CAP);
        assert!(cleaned.text.ends_with('…'));
        let mut long = String::new();
        while long.chars().count() < 5000 {
            long.push_str("the quick brown fox jumps over the lazy dog ");
        }
        let one = clean(&long).unwrap();
        assert!(one.truncated);
        assert_eq!(one.text.chars().count(), CHAR_CAP);
        assert!(one.text.ends_with('…'));
        assert_eq!(one.text.split('\n').count(), 1);
    }

    #[test]
    fn o10_json_rejects_a_lone_surrogate_and_bad_utf8() {
        let surrogate = r#"{"kind":"addNote","proposalId":"n","text":"\uD800","via":"typed"}"#;
        assert!(serde_json::from_str::<crate::protocol::Proposal>(surrogate).is_err());
        let bytes = b"{\"kind\":\"addNote\",\"proposalId\":\"n\",\"text\":\"\xff\",\"via\":\"typed\"}";
        assert!(serde_json::from_slice::<crate::protocol::Proposal>(&bytes[..]).is_err());
    }

    #[test]
    fn output_escapes_keep_json_latex_and_bibtex_intact() {
        let raw = "say \"hi\" \\ path\nnext";
        let cleaned = clean(raw).unwrap();
        let line = serde_json::to_string(&cleaned.text).unwrap();
        assert!(!line.contains('\n'));
        let back: String = serde_json::from_str(&line).unwrap();
        assert_eq!(back, cleaned.text);

        let attack = r#"\input{/etc/passwd} \write18{id} \include{x} \immediate\write18{id} 100% $5 & _x^2~ {brace}"#;
        let tex = latex_escape(attack);
        assert!(!tex.contains("\\input"));
        assert!(!tex.contains("\\include"));
        assert!(!tex.contains("\\write"));
        assert!(!tex.contains("\\immediate"));
        assert!(tex.contains("\\textbackslash{}"));
        assert!(tex.contains("\\%"));
        assert!(tex.contains("\\$"));
        assert!(tex.contains("\\&"));
        assert!(tex.contains("\\_"));
        assert!(tex.contains("\\^{}"));
        assert!(tex.contains("\\~{}"));

        let field = bibtex_field(attack);
        assert!(braces_balance(&field), "{field}");
        assert!(field.starts_with('{') && field.ends_with('}'));
        assert!(!field.contains("\\input"));
        assert!(!field.contains("\\write"));
        assert!(!field.contains("\\include"));
        assert!(!field.contains("\\immediate"));
        assert!(!field.contains('\n'));
    }

    #[test]
    fn o12_ordinary_notes_are_kept() {
        let samples = [
            "keep this, but cut the second clause",
            "你好，这是一条正常的笔记",
            "the count is 42",
            "https://example.com/paper",
            "x = y + 1",
            "これは普通のメモです。短くて十分です。",
            "jjjjjjjjjjjjjjjjj",
            "asdfghjklqwrtpzxcvbnmsdfghjklq",
        ];
        for raw in samples {
            let cleaned = clean(raw).unwrap();
            assert_eq!(cleaned.text, raw);
            assert!(!cleaned.normalized);
            assert!(!cleaned.truncated);
        }
    }

    #[test]
    fn o22_formula_cells_are_prefixed_before_quoting() {
        assert_eq!(csv_cell("=cmd"), "'=cmd");
        assert_eq!(csv_cell("+cmd"), "'+cmd");
        assert_eq!(csv_cell("-cmd"), "'-cmd");
        assert_eq!(csv_cell("@cmd"), "'@cmd");
        assert_eq!(csv_cell("\tcmd"), "'\tcmd");
        assert_eq!(csv_cell("\rcmd"), "\"'\rcmd\"");
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
    }

    #[test]
    fn o23_overlay_shows_markup_as_characters() {
        let raw = "<b>cut</b> \\textbf{clause} \u{1b}[31mred\u{0007}";
        let shown = overlay_plain(raw);
        assert_eq!(shown, "<b>cut</b> \\textbf{clause} [31mred");
        assert!(shown.contains("<b>"));
        assert!(shown.contains("\\textbf{"));
        assert!(!shown.contains('\u{1b}'));
        assert!(!shown.contains('\u{0007}'));
        let preview = display_note(&"b".repeat(80), PREVIEW_CHARS);
        assert_eq!(preview.chars().count(), PREVIEW_CHARS);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn revisions_show_the_latest_and_a_tombstone_hides_the_chain() {
        let base = NoteFlags { transcribed: false, truncated: false, normalized: false };
        let facts = vec![
            NoteFact {
                note_id: "nt-1".into(),
                text: "first".into(),
                via: "typed".into(),
                ts_ms: 1,
                replaces: None,
                flags: base.clone(),
                tombstone: false,
            },
            NoteFact {
                note_id: "nt-2".into(),
                text: "second".into(),
                via: "typed".into(),
                ts_ms: 2,
                replaces: Some("nt-1".into()),
                flags: base.clone(),
                tombstone: false,
            },
        ];
        let view = visible_notes(&facts);
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].text, "second");
        let mut gone = facts;
        gone.push(NoteFact {
            note_id: "nt-2".into(),
            text: String::new(),
            via: String::new(),
            ts_ms: 3,
            replaces: None,
            flags: base,
            tombstone: true,
        });
        assert!(visible_notes(&gone).is_empty());
    }

    fn braces_balance(text: &str) -> bool {
        let mut depth = 0i32;
        for ch in text.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        depth == 0
    }
}
