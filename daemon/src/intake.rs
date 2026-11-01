//! Source checks for project intake. A failure that names one source is a
//! `failed(reason)` record. `schema`, `url_not_allowed`, and
//! `path_not_in_granted_root` write nothing. This module does not map text
//! and does not open a window.

use crate::protocol::ProjectSourceIn;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MAX_BYTES: u64 = 50 * 1024 * 1024;
/// Cloudflare serves a 403 challenge to curl's default User-Agent.
pub const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const REF_CAP: usize = 2000;
const TEXT_CAP: usize = 500;
const SOURCE_CAP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckError {
    Schema,
    UrlNotAllowed,
    PathNotInGrantedRoot,
    QuotaSources,
}

impl CheckError {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::UrlNotAllowed => "url_not_allowed",
            Self::PathNotInGrantedRoot => "path_not_in_granted_root",
            Self::QuotaSources => "quota:sources",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    T1,
    T2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Url { url: String, claim: Option<String>, quote: Option<String> },
    Path { path: String, claim: Option<String>, quote: Option<String> },
    Dir { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceClass {
    Web,
    File,
    Dir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub class: SourceClass,
    pub reference: String,
    pub reason: Option<String>,
    pub claim: Option<String>,
    pub quote: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchReply {
    Response { status: u16, content_length: Option<u64>, location: Option<String>, challenge: bool },
    /// No HTTP status. Scripted probes use this for a host that never answered.
    Unreachable,
    /// Transport failure. `class` is `unreachable`, `dns`, `timeout`, or `spawn`.
    Down { class: &'static str, curl_exit: Option<i32>, stderr: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfReply {
    Text(String),
    NoText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageBody {
    pub status: u16,
    pub content_type: Option<String>,
    pub location: Option<String>,
    pub challenge: bool,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyReply {
    Page(PageBody),
    Unreachable,
    Oversize,
    /// `class` is `unreachable`, `dns`, `timeout`, or `spawn`.
    Down { class: &'static str, curl_exit: Option<i32>, stderr: Option<String> },
}

pub trait Probe {
    fn fetch(&self, url: &str) -> FetchReply;
    fn pdf_first_page(&self, path: &Path) -> PdfReply;
    /// The page body. Intake checks use [`Probe::fetch`] and discard the body.
    fn fetch_body(&self, url: &str) -> BodyReply {
        let _ = url;
        BodyReply::Unreachable
    }
}

pub struct LiveProbe;

impl Probe for LiveProbe {
    fn fetch(&self, url: &str) -> FetchReply {
        let output = match Command::new("curl").args(curl_args("-", "/dev/null")).arg("--url").arg(url).output() {
            Ok(output) => output,
            Err(_) => return down_fetch("spawn", None, None),
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let (status, content_length, location, challenge) = parse_curl_headers(&text);
        let code = output.status.code();
        let over = code == Some(63) || content_length.is_some_and(|len| len > MAX_BYTES);
        if status == 0 && !over {
            let fail = transport_fail(code, &output.stderr);
            return down_fetch(fail.class, fail.curl_exit, fail.stderr);
        }
        FetchReply::Response {
            status: if status == 0 { 200 } else { status },
            content_length: if over { Some(content_length.unwrap_or(MAX_BYTES + 1)) } else { content_length },
            location,
            challenge,
        }
    }

    fn pdf_first_page(&self, path: &Path) -> PdfReply {
        let output = match Command::new("pdftotext").args(["-f", "1", "-l", "1", "-q"]).arg(path).arg("-").output() {
            Ok(output) => output,
            Err(_) => return PdfReply::NoText,
        };
        let text = String::from_utf8_lossy(&output.stdout);
        if output.status.success() && !text.trim().is_empty() {
            PdfReply::Text(text.into_owned())
        } else {
            PdfReply::NoText
        }
    }

    fn fetch_body(&self, url: &str) -> BodyReply {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir();
        let headers_path = dir.join(format!("presence-curl-h-{}-{stamp}", std::process::id()));
        let body_path = dir.join(format!("presence-curl-b-{}-{stamp}", std::process::id()));
        let _headers = TempFile(&headers_path);
        let _body = TempFile(&body_path);
        let output = Command::new("curl")
            .args(curl_args(headers_path.to_str().unwrap_or(""), body_path.to_str().unwrap_or("")))
            .arg("--url")
            .arg(url)
            .output();
        let Ok(output) = output else {
            return down_body("spawn", None, None);
        };
        let header_text = std::fs::read_to_string(&headers_path).unwrap_or_default();
        let (status, content_length, location, content_type, challenge) = parse_curl_body_headers(&header_text);
        let code = output.status.code();
        let over = code == Some(63) || content_length.is_some_and(|len| len > MAX_BYTES);
        if over {
            return BodyReply::Oversize;
        }
        if status == 0 {
            let fail = transport_fail(code, &output.stderr);
            return down_body(fail.class, fail.curl_exit, fail.stderr);
        }
        let len = std::fs::metadata(&body_path).map(|meta| meta.len()).unwrap_or(0);
        if len > MAX_BYTES {
            return BodyReply::Oversize;
        }
        let body = std::fs::read(&body_path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
        BodyReply::Page(PageBody { status, content_type, location, challenge, body })
    }
}

struct TempFile<'a>(&'a Path);

impl Drop for TempFile<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
}

/// Shape only. No filesystem and no network. T3 drafts use this.
pub fn shape_draft(sources: &[ProjectSourceIn]) -> Result<Vec<Prepared>, CheckError> {
    Ok(parse_items(sources)?
        .into_iter()
        .map(|item| match item {
            Item::Url { url, claim, quote } => Prepared { class: SourceClass::Web, reference: url, reason: None, claim, quote },
            Item::Path { path, claim, quote } => Prepared { class: SourceClass::File, reference: path, reason: None, claim, quote },
            Item::Dir { path } => Prepared { class: SourceClass::Dir, reference: path, reason: None, claim: None, quote: None },
        })
        .collect())
}

pub fn check_wire(
    role: Role,
    sources: &[ProjectSourceIn],
    granted_roots: &[String],
    allow_private: bool,
    already: usize,
    probe: &dyn Probe,
) -> Result<Vec<Prepared>, CheckError> {
    check_items(role, &parse_items(sources)?, granted_roots, allow_private, already, probe)
}

pub fn check_items(
    role: Role,
    items: &[Item],
    granted_roots: &[String],
    allow_private: bool,
    already: usize,
    probe: &dyn Probe,
) -> Result<Vec<Prepared>, CheckError> {
    let candidates = expand(role, items, granted_roots, allow_private, already)?;
    let mut prepared = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let reason = if let Some(reason) = candidate.pre_fail {
            Some(reason.to_string())
        } else if candidate.web {
            fetch_url(&candidate.reference, allow_private, probe)?
        } else if candidate.pdf {
            match probe.pdf_first_page(Path::new(&candidate.reference)) {
                PdfReply::Text(text) if !text.trim().is_empty() => None,
                _ => Some("no-text-layer".to_string()),
            }
        } else {
            None
        };
        prepared.push(Prepared {
            class: if candidate.web { SourceClass::Web } else { SourceClass::File },
            reference: candidate.reference,
            reason,
            claim: candidate.claim,
            quote: candidate.quote,
        });
    }
    Ok(prepared)
}

/// A directory the user granted. Stored as its canonical path.
pub fn canon_dir(raw: &str) -> Result<String, CheckError> {
    if raw.is_empty() || raw.chars().count() > REF_CAP {
        return Err(CheckError::Schema);
    }
    let path = std::fs::canonicalize(raw).map_err(|_| CheckError::Schema)?;
    let meta = std::fs::metadata(&path).map_err(|_| CheckError::Schema)?;
    if !meta.is_dir() {
        return Err(CheckError::Schema);
    }
    let text = path.to_string_lossy().into_owned();
    if text.chars().count() > REF_CAP {
        return Err(CheckError::Schema);
    }
    Ok(text)
}

struct Candidate {
    web: bool,
    reference: String,
    claim: Option<String>,
    quote: Option<String>,
    pre_fail: Option<&'static str>,
    pdf: bool,
}

fn expand(role: Role, items: &[Item], roots: &[String], allow_private: bool, already: usize) -> Result<Vec<Candidate>, CheckError> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Dir { path } => {
                if role == Role::T2 {
                    return Err(CheckError::Schema);
                }
                for file in snapshot_dir(path)? {
                    let text = file.to_string_lossy().into_owned();
                    out.push(file_candidate(Role::T1, &text, roots, None, None)?);
                    if already + out.len() > SOURCE_CAP {
                        return Err(CheckError::QuotaSources);
                    }
                }
            }
            Item::Path { path, claim, quote } => {
                out.push(file_candidate(role, path, roots, claim.clone(), quote.clone())?);
            }
            Item::Url { url, claim, quote } => {
                if !url_allowed(url, allow_private) {
                    return Err(CheckError::UrlNotAllowed);
                }
                out.push(Candidate {
                    web: true,
                    reference: url.clone(),
                    claim: claim.clone(),
                    quote: quote.clone(),
                    pre_fail: None,
                    pdf: false,
                });
            }
        }
        if already + out.len() > SOURCE_CAP {
            return Err(CheckError::QuotaSources);
        }
    }
    Ok(out)
}

fn snapshot_dir(dir: &str) -> Result<Vec<PathBuf>, CheckError> {
    let dir_path = Path::new(dir);
    let meta = std::fs::symlink_metadata(dir_path).map_err(|_| CheckError::Schema)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(CheckError::Schema);
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir_path).map_err(|_| CheckError::Schema)? {
        let entry = entry.map_err(|_| CheckError::Schema)?;
        let kind = entry.file_type().map_err(|_| CheckError::Schema)?;
        if kind.is_symlink() || !kind.is_file() {
            continue;
        }
        files.push(entry.path());
    }
    files.sort();
    Ok(files)
}

fn file_candidate(role: Role, path: &str, roots: &[String], claim: Option<String>, quote: Option<String>) -> Result<Candidate, CheckError> {
    let miss = match role {
        Role::T1 => CheckError::Schema,
        Role::T2 => CheckError::PathNotInGrantedRoot,
    };
    let canon = std::fs::canonicalize(path).map_err(|_| miss)?;
    let meta = std::fs::symlink_metadata(&canon).map_err(|_| miss)?;
    if !meta.is_file() {
        return Err(if meta.is_dir() && role == Role::T1 { CheckError::Schema } else { miss });
    }
    if role == Role::T2 && !roots.iter().any(|root| under_root(root, &canon)) {
        return Err(CheckError::PathNotInGrantedRoot);
    }
    let reference = canon.to_string_lossy().into_owned();
    if reference.chars().count() > REF_CAP {
        return Err(CheckError::Schema);
    }
    let pdf = canon.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));
    Ok(Candidate {
        web: false,
        reference,
        claim,
        quote,
        pre_fail: if meta.len() > MAX_BYTES { Some("oversize") } else { None },
        pdf,
    })
}

fn under_root(root: &str, file: &Path) -> bool {
    let Ok(root) = std::fs::canonicalize(root) else { return false };
    file.starts_with(&root) && file != root
}

fn curl_args(header_dest: &str, body_dest: &str) -> Vec<String> {
    vec![
        "-sS".to_string(),
        "-D".to_string(),
        header_dest.to_string(),
        "-o".to_string(),
        body_dest.to_string(),
        "--max-redirs".to_string(),
        "0".to_string(),
        "--max-filesize".to_string(),
        MAX_BYTES.to_string(),
        "--max-time".to_string(),
        "20".to_string(),
        "--proto".to_string(),
        "=http,https".to_string(),
        "-A".to_string(),
        BROWSER_USER_AGENT.to_string(),
    ]
}

struct TransportFail {
    class: &'static str,
    curl_exit: Option<i32>,
    stderr: Option<String>,
}

fn transport_fail(code: Option<i32>, stderr: &[u8]) -> TransportFail {
    let note = String::from_utf8_lossy(stderr).trim().to_string();
    let stderr = if note.is_empty() { None } else { Some(note) };
    let class = match code {
        Some(6) => "dns",
        Some(28) => "timeout",
        _ => "unreachable",
    };
    TransportFail { class, curl_exit: code, stderr }
}

fn down_fetch(class: &'static str, curl_exit: Option<i32>, stderr: Option<String>) -> FetchReply {
    FetchReply::Down { class, curl_exit, stderr }
}

fn down_body(class: &'static str, curl_exit: Option<i32>, stderr: Option<String>) -> BodyReply {
    BodyReply::Down { class, curl_exit, stderr }
}

pub(crate) fn http_reason(status: u16, challenge: bool) -> String {
    let mut reason = format!("http-{status}");
    if challenge {
        reason.push_str(" challenge");
    }
    if reason.len() > 64 {
        reason.truncate(64);
    }
    reason
}

fn clip_stderr(class: &str, note: &str) -> Option<String> {
    if !matches!(class, "dns" | "timeout" | "spawn" | "unreachable") {
        return None;
    }
    let mut clipped = String::new();
    for ch in note.chars() {
        if ch == '\n' || ch == '\r' {
            break;
        }
        if clipped.chars().count() == 120 {
            break;
        }
        clipped.push(ch);
    }
    let clipped = clipped.trim().to_string();
    if clipped.is_empty() { None } else { Some(clipped) }
}

pub(crate) fn intake_log_line(url: &str, class: &str, status: Option<u16>, curl_exit: Option<i32>, stderr: Option<&str>) -> String {
    let mut line = format!("[intake] {url} {class}");
    if let Some(status) = status {
        line.push_str(&format!(" status={status}"));
    }
    if let Some(code) = curl_exit {
        line.push_str(&format!(" curl={code}"));
    }
    if let Some(note) = stderr.and_then(|note| clip_stderr(class, note)) {
        line.push(' ');
        line.push_str(&note);
    }
    line
}

pub(crate) fn log_fetch_failure(url: &str, class: &str, status: Option<u16>, curl_exit: Option<i32>, stderr: Option<&str>) {
    eprintln!("{}", intake_log_line(url, class, status, curl_exit, stderr));
}

fn fail_fetch(url: &str, class: &str, status: Option<u16>, curl_exit: Option<i32>, stderr: Option<&str>) -> String {
    log_fetch_failure(url, class, status, curl_exit, stderr);
    class.to_string()
}

fn fetch_url(start: &str, allow_private: bool, probe: &dyn Probe) -> Result<Option<String>, CheckError> {
    if !url_allowed(start, allow_private) {
        return Err(CheckError::UrlNotAllowed);
    }
    let mut current = start.to_string();
    for hop in 0..=3 {
        match probe.fetch(&current) {
            FetchReply::Unreachable => return Ok(Some(fail_fetch(&current, "unreachable", None, None, None))),
            FetchReply::Down { class, curl_exit, stderr } => {
                return Ok(Some(fail_fetch(&current, class, None, curl_exit, stderr.as_deref())));
            }
            FetchReply::Response { status, content_length, location, challenge } => {
                if content_length.is_some_and(|len| len > MAX_BYTES) {
                    return Ok(Some(fail_fetch(&current, "oversize", Some(status), None, None)));
                }
                if (300..400).contains(&status) {
                    if hop == 3 {
                        return Ok(Some(fail_fetch(&current, "unreachable", Some(status), None, None)));
                    }
                    let next = resolve_redirect(&current, location.as_deref().unwrap_or(""))?;
                    if !url_allowed(&next, allow_private) {
                        return Err(CheckError::UrlNotAllowed);
                    }
                    current = next;
                    continue;
                }
                if (200..300).contains(&status) {
                    return Ok(None);
                }
                let reason = http_reason(status, challenge);
                log_fetch_failure(&current, &reason, Some(status), None, None);
                return Ok(Some(reason));
            }
        }
    }
    Ok(Some(fail_fetch(&current, "unreachable", None, None, None)))
}

pub(crate) fn resolve_redirect(current: &str, location: &str) -> Result<String, CheckError> {
    let location = location.trim();
    if location.is_empty() || location.chars().count() > REF_CAP {
        return Err(CheckError::UrlNotAllowed);
    }
    if location.contains("://") {
        return Ok(location.to_string());
    }
    if let Some(rest) = location.strip_prefix("//") {
        let scheme = current.split("://").next().unwrap_or("https");
        return Ok(format!("{scheme}://{rest}"));
    }
    let scheme_end = current.find("://").ok_or(CheckError::UrlNotAllowed)? + 3;
    let after = &current[scheme_end..];
    let origin_len = after.find(['/', '?', '#']).map(|index| scheme_end + index).unwrap_or(current.len());
    let origin = &current[..origin_len];
    if location.starts_with('/') {
        return Ok(format!("{origin}{location}"));
    }
    let path = current[origin_len..].split(['?', '#']).next().unwrap_or("");
    let prefix = match path.rfind('/') {
        Some(index) => &path[..=index],
        None => "/",
    };
    Ok(format!("{origin}{prefix}{location}"))
}

pub(crate) fn url_allowed(raw: &str, allow_private: bool) -> bool {
    if raw.is_empty() || raw.chars().count() > REF_CAP {
        return false;
    }
    if raw.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return false;
    }
    let Some((scheme, rest)) = raw.split_once("://") else { return false };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    if rest.is_empty() {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let Some(host) = host_from_authority(authority) else { return false };
    host_ok(&host, allow_private)
}

fn host_from_authority(authority: &str) -> Option<String> {
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        let host = &rest[..end];
        let after = &rest[end + 1..];
        if !(after.is_empty() || (after.starts_with(':') && after[1..].chars().all(|ch| ch.is_ascii_digit()))) {
            return None;
        }
        host.parse::<std::net::Ipv6Addr>().ok()?;
        return Some(host.to_string());
    }
    if let Some((host, port)) = authority.rsplit_once(':') {
        if port.is_empty() || !port.chars().all(|ch| ch.is_ascii_digit()) || host.is_empty() {
            return None;
        }
        return Some(host.to_string());
    }
    Some(authority.to_string())
}

fn host_ok(host: &str, allow_private: bool) -> bool {
    let bare = host.trim_end_matches('.');
    if bare.is_empty() {
        return false;
    }
    if bare.eq_ignore_ascii_case("localhost") {
        return allow_private;
    }
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return allow_private || !blocked_ip(ip);
    }
    if bare.split('.').all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit())) {
        return false;
    }
    true
}

fn blocked_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let oct = v4.octets();
            oct[0] == 127
                || v4.is_unspecified()
                || oct[0] == 10
                || (oct[0] == 172 && (oct[1] & 0xf0) == 16)
                || (oct[0] == 192 && oct[1] == 168)
                || (oct[0] == 169 && oct[1] == 254)
        }
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return blocked_ip(std::net::IpAddr::V4(v4));
            }
            let seg = v6.segments()[0];
            v6.is_loopback() || v6.is_unspecified() || (seg & 0xfe00) == 0xfc00 || (seg & 0xffc0) == 0xfe80
        }
    }
}

fn parse_items(sources: &[ProjectSourceIn]) -> Result<Vec<Item>, CheckError> {
    let mut out = Vec::with_capacity(sources.len());
    for source in sources {
        let claim = optional_text(source.claim.as_deref())?;
        let quote = optional_text(source.quote.as_deref())?;
        match (&source.url, &source.path, &source.dir) {
            (Some(url), None, None) => {
                if url.is_empty() || url.chars().count() > REF_CAP {
                    return Err(CheckError::Schema);
                }
                out.push(Item::Url { url: url.clone(), claim, quote });
            }
            (None, Some(path), None) => {
                if path.is_empty() || path.chars().count() > REF_CAP {
                    return Err(CheckError::Schema);
                }
                out.push(Item::Path { path: path.clone(), claim, quote });
            }
            (None, None, Some(dir)) => {
                if claim.is_some() || quote.is_some() || dir.is_empty() || dir.chars().count() > REF_CAP {
                    return Err(CheckError::Schema);
                }
                out.push(Item::Dir { path: dir.clone() });
            }
            _ => return Err(CheckError::Schema),
        }
    }
    Ok(out)
}

fn optional_text(value: Option<&str>) -> Result<Option<String>, CheckError> {
    match value {
        None => Ok(None),
        Some(text) if text.is_empty() => Ok(None),
        Some(text) if text.chars().count() > TEXT_CAP => Err(CheckError::Schema),
        Some(text) => Ok(Some(text.to_string())),
    }
}

fn header_is_challenge(name: &str, value: &str) -> bool {
    name.eq_ignore_ascii_case("cf-mitigated") && value.to_ascii_lowercase().contains("challenge")
}

fn parse_curl_body_headers(text: &str) -> (u16, Option<u64>, Option<String>, Option<String>, bool) {
    let mut status = 0u16;
    let mut length = None;
    let mut location = None;
    let mut content_type = None;
    let mut challenge = false;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("HTTP/") {
            status = rest.split_whitespace().nth(1).and_then(|code| code.parse().ok()).unwrap_or(0);
            length = None;
            location = None;
            content_type = None;
            challenge = false;
        } else if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok();
            } else if name.eq_ignore_ascii_case("location") {
                location = Some(value.trim().to_string());
            } else if name.eq_ignore_ascii_case("content-type") {
                content_type = Some(value.trim().to_string());
            } else if header_is_challenge(name, value) {
                challenge = true;
            }
        }
    }
    (status, length, location, content_type, challenge)
}

fn parse_curl_headers(text: &str) -> (u16, Option<u64>, Option<String>, bool) {
    let (status, length, location, _, challenge) = parse_curl_body_headers(text);
    (status, length, location, challenge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::Tier;
    use crate::project::{route_project_with, ProjectStore, SourceStatus};
    use crate::protocol::Proposal;
    use crate::task::route_task;
    use crate::ui_input::{Hotkey, UiEvent};
    use crate::ledger::Ledger;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::fs;
    use std::os::unix::fs::symlink;

    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Temp {
        let dir = std::env::temp_dir().join(format!("presence-c8-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }

    struct ScriptProbe {
        http: HashMap<String, FetchReply>,
        pdf: HashMap<String, PdfReply>,
        hits: Cell<u32>,
    }

    impl Probe for ScriptProbe {
        fn fetch(&self, url: &str) -> FetchReply {
            self.hits.set(self.hits.get() + 1);
            self.http.get(url).cloned().unwrap_or(FetchReply::Unreachable)
        }

        fn pdf_first_page(&self, path: &Path) -> PdfReply {
            self.hits.set(self.hits.get() + 1);
            self.pdf.get(&path.to_string_lossy().to_string()).cloned().unwrap_or(PdfReply::NoText)
        }
    }

    fn http_ok(url: &str) -> (String, FetchReply) {
        (
            url.to_string(),
            FetchReply::Response { status: 200, content_length: Some(4), location: None, challenge: false },
        )
    }

    fn probe_ok(urls: &[&str]) -> ScriptProbe {
        ScriptProbe { http: urls.iter().copied().map(http_ok).collect(), pdf: HashMap::new(), hits: Cell::new(0) }
    }

    fn parse(json: &str) -> Proposal {
        serde_json::from_str(json).unwrap_or_else(|err| panic!("parse {json}: {err}"))
    }

    fn create_json(title: &str, extra: &str, sources: &str) -> String {
        format!(r#"{{"kind":"createProject","proposalId":"c","title":"{title}"{extra},"asks":[],"sources":[{sources}]}}"#)
    }

    fn route(store: &mut ProjectStore, tier: Tier, json: &str, probe: &dyn Probe) -> crate::protocol::ProposalResult {
        route_project_with(&parse(json), tier, Some(store), 1, probe).unwrap()
    }

    fn project_count(root: &Path) -> usize {
        fs::read_dir(root).unwrap().filter(|entry| entry.as_ref().unwrap().path().is_dir()).count()
    }

    #[test]
    fn l1_t2_path_outside_a_root_writes_nothing() {
        let tmp = scratch("l1");
        let root = tmp.0.join("store");
        let granted = tmp.0.join("granted");
        let outside = tmp.0.join("outside.txt");
        let inside = granted.join("inside.txt");
        fs::create_dir(&granted).unwrap();
        fs::write(&outside, b"no").unwrap();
        fs::write(&inside, b"yes").unwrap();
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&[]);
        let bare = create_json("Bare", "", &format!(r#"{{"path":"{}"}}"#, outside.display()));
        let denied = route(&mut store, Tier::T2Client, &bare, &probe);
        assert_eq!(denied.error.as_deref(), Some("path_not_in_granted_root"));
        assert_eq!(project_count(&root), 0);

        let opened = create_json("Open", &format!(r#","grantedRoots":["{}"]"#, granted.display()), r#"{"url":"https://example.com/a"}"#);
        let probe = probe_ok(&["https://example.com/a"]);
        let created = route(&mut store, Tier::T1User, &opened, &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        assert_eq!(store.get(&id).unwrap().sources.len(), 1);
        let add_out = format!(
            r#"{{"kind":"addSources","proposalId":"a","projectId":"{id}","sources":[{{"path":"{}"}}]}}"#,
            outside.display()
        );
        let again = route(&mut store, Tier::T2Client, &add_out, &probe);
        assert_eq!(again.error.as_deref(), Some("path_not_in_granted_root"));
        assert_eq!(store.get(&id).unwrap().sources.len(), 1);

        let add_in = format!(
            r#"{{"kind":"addSources","proposalId":"b","projectId":"{id}","sources":[{{"path":"{}"}}]}}"#,
            inside.display()
        );
        let accepted = route(&mut store, Tier::T2Client, &add_in, &probe);
        assert_eq!(accepted.status, "ok", "{:?}", accepted.error);
        let sources = &store.get(&id).unwrap().sources;
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[1].status, SourceStatus::Pending);
        assert!(sources[1].reference.ends_with("inside.txt"));
        assert!(sources[1].doc_key.is_none());
    }

    #[test]
    fn l2_refused_urls_write_nothing() {
        let tmp = scratch("l2");
        let root = tmp.0.join("store");
        let mut store = ProjectStore::open(&root).unwrap();
        let urls = [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/plain,hi",
            "https://user:secret@example.com/a",
            "http://localhost/a",
            "http://LOCALHOST/a",
            "http://127.0.0.1/a",
            "http://10.1.2.3/a",
            "http://172.16.5.5/a",
            "http://192.168.1.9/a",
            "http://169.254.1.1/a",
            "http://0.0.0.0/a",
            "http://[::1]/a",
            "http://[fc00::1]/a",
            "http://[fe80::1]/a",
        ];
        let probe = probe_ok(&[]);
        for url in urls {
            let body = create_json("Bad", "", &format!(r#"{{"url":"{url}"}}"#));
            let result = route(&mut store, Tier::T2Client, &body, &probe);
            assert_eq!(result.error.as_deref(), Some("url_not_allowed"), "{url}");
        }
        assert_eq!(project_count(&root), 0);
        assert_eq!(probe.hits.get(), 0);

        let mut http = HashMap::new();
        http.insert(
            "https://example.com/go".into(),
            FetchReply::Response { status: 302, content_length: None, location: Some("http://127.0.0.1/hidden".into()), challenge: false },
        );
        http.insert(http_ok("https://example.com/a").0, http_ok("https://example.com/a").1);
        http.insert(
            "https://example.com/a".into(),
            FetchReply::Response { status: 302, content_length: None, location: Some("https://example.com/b".into()), challenge: false },
        );
        http.insert(
            "https://example.com/b".into(),
            FetchReply::Response { status: 302, content_length: None, location: Some("https://example.com/c".into()), challenge: false },
        );
        http.insert(
            "https://example.com/c".into(),
            FetchReply::Response { status: 302, content_length: None, location: Some("https://example.com/d".into()), challenge: false },
        );
        http.insert(http_ok("https://example.com/d").0, http_ok("https://example.com/d").1);
        let probe = ScriptProbe { http, pdf: HashMap::new(), hits: Cell::new(0) };
        let hopped = route(
            &mut store,
            Tier::T2Client,
            &create_json("Hop", "", r#"{"url":"https://example.com/go"}"#),
            &probe,
        );
        assert_eq!(hopped.error.as_deref(), Some("url_not_allowed"));
        assert_eq!(project_count(&root), 0);
        let landed = route(
            &mut store,
            Tier::T2Client,
            &create_json("Land", "", r#"{"url":"https://example.com/a"}"#),
            &probe,
        );
        assert_eq!(landed.status, "ok", "{:?}", landed.error);
        let id = landed.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        assert_eq!(store.get(&id).unwrap().sources[0].status, SourceStatus::Pending);
        assert_eq!(store.get(&id).unwrap().sources[0].reference, "https://example.com/a");
    }

    #[test]
    fn l3_t2_dir_or_granted_roots_are_schema() {
        let tmp = scratch("l3");
        let root = tmp.0.join("store");
        let dir = tmp.0.join("docs");
        fs::create_dir(&dir).unwrap();
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&[]);
        let with_dir = create_json("Dir", "", &format!(r#"{{"dir":"{}"}}"#, dir.display()));
        let denied_dir = route(&mut store, Tier::T2Client, &with_dir, &probe);
        assert_eq!(denied_dir.error.as_deref(), Some("schema"));
        let with_roots = create_json("Roots", &format!(r#","grantedRoots":["{}"]"#, dir.display()), r#"{"url":"https://example.com/a"}"#);
        let denied_roots = route(&mut store, Tier::T2Client, &with_roots, &probe);
        assert_eq!(denied_roots.error.as_deref(), Some("schema"));
        assert_eq!(project_count(&root), 0);
        assert_eq!(probe.hits.get(), 0);
    }

    #[test]
    fn l4_directory_snapshot_is_taken_once() {
        let tmp = scratch("l4");
        let root = tmp.0.join("store");
        let docs = tmp.0.join("docs");
        fs::create_dir(&docs).unwrap();
        fs::write(docs.join("a.txt"), b"a").unwrap();
        fs::write(docs.join("b.txt"), b"b").unwrap();
        fs::create_dir(docs.join("nested")).unwrap();
        fs::write(docs.join("nested").join("hidden.txt"), b"h").unwrap();
        symlink("a.txt", docs.join("link.txt")).unwrap();
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&[]);
        let body = create_json("Snap", "", &format!(r#"{{"dir":"{}"}}"#, docs.display()));
        let created = route(&mut store, Tier::T1User, &body, &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let names: Vec<_> = store.get(&id).unwrap().sources.iter().map(|source| source.reference.clone()).collect();
        assert_eq!(names.len(), 2);
        assert!(names[0].ends_with("a.txt"));
        assert!(names[1].ends_with("b.txt"));
        drop(store);
        fs::write(docs.join("later.txt"), b"later").unwrap();
        let store = ProjectStore::open(&root).unwrap();
        let reloaded: Vec<_> = store.get(&id).unwrap().sources.iter().map(|source| source.reference.clone()).collect();
        assert_eq!(reloaded, names);
        assert!(reloaded.iter().all(|path| !path.contains("later.txt") && !path.contains("link.txt") && !path.contains("hidden.txt")));

        let many = tmp.0.join("many");
        fs::create_dir(&many).unwrap();
        for index in 0..501 {
            fs::write(many.join(format!("f{index:04}.txt")), b"x").unwrap();
        }
        let mut store = ProjectStore::open(&root).unwrap();
        let overflow = create_json("Cap", "", &format!(r#"{{"dir":"{}"}}"#, many.display()));
        let capped = route(&mut store, Tier::T1User, &overflow, &probe);
        assert_eq!(capped.error.as_deref(), Some("quota:sources"));
        assert!(store.get(&id).unwrap().sources.len() == 2);
    }

    #[test]
    fn l5_draft_stays_unchecked_until_confirm() {
        let tmp = scratch("l5");
        let root = tmp.0.join("store");
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&["https://example.com/ok"]);
        let good = create_json("Good", "", r#"{"url":"https://example.com/ok"}"#);
        let draft = route_project_with(&parse(&good), Tier::T3Sidecar, Some(&mut store), 1, &probe).unwrap();
        assert_eq!(draft.status, "ok", "{:?}", draft.error);
        assert_eq!(draft.detail.as_ref().unwrap()["draft"], true);
        let good_id = draft.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        assert_eq!(probe.hits.get(), 0);
        assert!(store.get(&good_id).unwrap().draft);
        assert_eq!(store.get(&good_id).unwrap().sources[0].status, SourceStatus::Pending);
        assert!(store.get(&good_id).unwrap().sources[0].doc_key.is_none());

        let local = create_json("Local", "", r#"{"url":"http://127.0.0.1/secret"}"#);
        let local_draft = route_project_with(&parse(&local), Tier::T3Sidecar, Some(&mut store), 2, &probe).unwrap();
        let local_id = local_draft.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        assert!(store.get(&local_id).unwrap().draft);
        assert_eq!(store.get(&local_id).unwrap().sources[0].status, SourceStatus::Pending);
        assert_eq!(probe.hits.get(), 0);

        let mut book = crate::task::TaskBook::new();
        let mut ledger = Ledger::new();
        let task = route_task(
            &parse(r#"{"kind":"createTask","proposalId":"t","presenceId":"p","mode":"explain","docKey":"doc-a","items":[{"query":"the line","scroll":true}]}"#),
            Tier::T2Client,
            &mut book,
            &mut ledger,
            true,
            3,
        )
        .unwrap();
        let task_id = task.detail.unwrap()["taskId"].as_str().unwrap().to_string();
        let state = |book: &mut crate::task::TaskBook, ledger: &mut Ledger| {
            let query = format!(r#"{{"kind":"queryTask","proposalId":"q","taskId":"{task_id}"}}"#);
            route_task(&parse(&query), Tier::T1User, book, ledger, true, 4).unwrap().detail.unwrap()["state"].as_str().unwrap().to_string()
        };
        assert_eq!(state(&mut book, &mut ledger), "awaitingStart");
        assert!(!book.handle_ui(&mut ledger, UiEvent::Hotkey(Hotkey::ConfirmProject), 5));
        assert_eq!(state(&mut book, &mut ledger), "awaitingStart");
        assert!(store.get(&good_id).unwrap().draft);
        assert!(book.handle_ui(&mut ledger, UiEvent::Hotkey(Hotkey::Start), 6));
        assert_eq!(state(&mut book, &mut ledger), "active");
        assert!(store.get(&good_id).unwrap().draft);
        assert!(store.get(&local_id).unwrap().draft);

        let confirm_local = format!(r#"{{"kind":"confirmProject","proposalId":"k","projectId":"{local_id}"}}"#);
        let refused = route(&mut store, Tier::T1User, &confirm_local, &probe);
        assert_eq!(refused.error.as_deref(), Some("url_not_allowed"));
        assert!(store.get(&local_id).unwrap().draft);
        let t2_confirm = route(&mut store, Tier::T2Client, &confirm_local, &probe);
        assert_eq!(t2_confirm.error.as_deref(), Some("not_allowed_for_tier:confirmProject"));

        assert!(store.confirm_hotkey(Hotkey::ConfirmProject, &probe, 7));
        assert!(!store.get(&good_id).unwrap().draft);
        assert!(store.get(&local_id).unwrap().draft);
        assert!(probe.hits.get() > 0);
        assert_eq!(store.get(&good_id).unwrap().sources[0].status, SourceStatus::Pending);
        assert!(store.get(&good_id).unwrap().sources[0].doc_key.is_none());

        let again = format!(r#"{{"kind":"confirmProject","proposalId":"k2","projectId":"{good_id}"}}"#);
        let active = route(&mut store, Tier::T1User, &again, &probe);
        assert_eq!(active.error.as_deref(), Some("schema"));
        assert!(!store.confirm_hotkey(Hotkey::Start, &probe, 8));
        assert!(store.get(&local_id).unwrap().draft);
    }

    #[test]
    fn l6_mixed_list_keeps_order_and_empty_doc_key() {
        let tmp = scratch("l6");
        let root = tmp.0.join("store");
        let file = tmp.0.join("paper.txt");
        let docs = tmp.0.join("docs");
        fs::write(&file, b"paper").unwrap();
        fs::create_dir(&docs).unwrap();
        fs::write(docs.join("b.txt"), b"b").unwrap();
        fs::write(docs.join("a.txt"), b"a").unwrap();
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&["https://example.com/a"]);
        let sources = format!(
            r#"{{"url":"https://example.com/a","claim":"the claim","quote":"the quote"}},{{"path":"{}"}},{{"dir":"{}"}}"#,
            file.display(),
            docs.display()
        );
        let created = route(&mut store, Tier::T1User, &create_json("Mix", "", &sources), &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let got = &store.get(&id).unwrap().sources;
        assert_eq!(got.len(), 4);
        assert_eq!(got[0].reference, "https://example.com/a");
        assert_eq!(got[0].claim.as_deref(), Some("the claim"));
        assert_eq!(got[0].quote.as_deref(), Some("the quote"));
        assert!(got[1].reference.ends_with("paper.txt"));
        assert!(got[2].reference.ends_with("a.txt"));
        assert!(got[3].reference.ends_with("b.txt"));
        assert!(got.iter().all(|source| source.doc_key.is_none() && source.status == SourceStatus::Pending));
    }

    #[test]
    fn l7_per_source_failures_keep_the_next_record() {
        let tmp = scratch("l7");
        let root = tmp.0.join("store");
        let big = tmp.0.join("big.bin");
        let pdf = tmp.0.join("scan.pdf");
        let next = tmp.0.join("next.txt");
        let file = fs::OpenOptions::new().write(true).create(true).open(&big).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        fs::write(&pdf, b"%PDF-1.4\n").unwrap();
        fs::write(&next, b"next").unwrap();
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&[]);
        let sources = format!(
            r#"{{"path":"{}"}},{{"path":"{}"}},{{"url":"https://example.com/missing"}},{{"path":"{}"}}"#,
            big.display(),
            pdf.display(),
            next.display()
        );
        let created = route(&mut store, Tier::T1User, &create_json("Fail", "", &sources), &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let got = &store.get(&id).unwrap().sources;
        assert_eq!(got.len(), 4);
        assert_eq!(got[0].status, SourceStatus::Failed);
        assert_eq!(got[0].reason.as_deref(), Some("oversize"));
        assert_eq!(got[1].status, SourceStatus::Failed);
        assert_eq!(got[1].reason.as_deref(), Some("no-text-layer"));
        assert_eq!(got[2].status, SourceStatus::Failed);
        assert_eq!(got[2].reason.as_deref(), Some("unreachable"));
        assert_eq!(got[3].status, SourceStatus::Pending);
        assert!(got[3].reason.is_none());
        assert!(got[3].reference.ends_with("next.txt"));
        assert!(got.iter().all(|source| source.doc_key.is_none()));
    }

    #[test]
    fn l7b_private_network_flag_is_t1_only() {
        let tmp = scratch("l7b");
        let root = tmp.0.join("store");
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&["http://127.0.0.1/a"]);
        let from_t2 = create_json("Priv", r#","allowPrivateNetwork":true"#, r#"{"url":"http://127.0.0.1/a"}"#);
        let denied = route(&mut store, Tier::T2Client, &from_t2, &probe);
        assert_eq!(denied.error.as_deref(), Some("schema"));
        assert_eq!(project_count(&root), 0);
        let from_t1 = create_json("Loop", r#","allowPrivateNetwork":true"#, r#"{"url":"http://127.0.0.1/a"}"#);
        let created = route(&mut store, Tier::T1User, &from_t1, &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let project = store.get(&id).unwrap();
        assert!(project.allow_private_network);
        assert_eq!(project.sources[0].status, SourceStatus::Pending);
        assert_eq!(project.sources[0].reference, "http://127.0.0.1/a");
        let plain = create_json("No", "", r#"{"url":"http://127.0.0.1/a"}"#);
        let blocked = route(&mut store, Tier::T1User, &plain, &probe);
        assert_eq!(blocked.error.as_deref(), Some("url_not_allowed"));
        assert_eq!(project_count(&root), 1);
    }

    #[test]
    fn l8_add_sources_uses_the_same_rules() {
        let tmp = scratch("l8");
        let root = tmp.0.join("store");
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&["https://example.com/a", "https://example.com/b"]);
        let draft = route_project_with(
            &parse(&create_json("Draft", "", r#"{"url":"https://example.com/a"}"#)),
            Tier::T3Sidecar,
            Some(&mut store),
            1,
            &probe,
        )
        .unwrap();
        let draft_id = draft.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let add = format!(
            r#"{{"kind":"addSources","proposalId":"a","projectId":"{draft_id}","sources":[{{"url":"https://example.com/b"}}]}}"#
        );
        let rejected = route(&mut store, Tier::T1User, &add, &probe);
        assert_eq!(rejected.error.as_deref(), Some("draft_unconfirmed"));
        assert_eq!(store.get(&draft_id).unwrap().sources.len(), 1);
        assert_eq!(probe.hits.get(), 0);

        let granted = tmp.0.join("root");
        fs::create_dir(&granted).unwrap();
        let opened = create_json("Live", &format!(r#","grantedRoots":["{}"]"#, granted.display()), r#"{"url":"https://example.com/a"}"#);
        let created = route(&mut store, Tier::T1User, &opened, &probe);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let added = format!(
            r#"{{"kind":"addSources","proposalId":"b","projectId":"{id}","sources":[{{"url":"https://example.com/b"}},{{"url":"https://example.com/missing"}}]}}"#
        );
        let mixed = route(&mut store, Tier::T2Client, &added, &probe);
        assert_eq!(mixed.status, "ok", "{:?}", mixed.error);
        let sources = &store.get(&id).unwrap().sources;
        assert_eq!(sources.len(), 3);
        assert_eq!(sources[1].status, SourceStatus::Pending);
        assert_eq!(sources[2].status, SourceStatus::Failed);
        assert_eq!(sources[2].reason.as_deref(), Some("unreachable"));
        let dir = tmp.0.join("docs");
        fs::create_dir(&dir).unwrap();
        let bad_dir = format!(r#"{{"kind":"addSources","proposalId":"d","projectId":"{id}","sources":[{{"dir":"{}"}}]}}"#, dir.display());
        let schema = route(&mut store, Tier::T2Client, &bad_dir, &probe);
        assert_eq!(schema.error.as_deref(), Some("schema"));
        assert_eq!(store.get(&id).unwrap().sources.len(), 3);
        let t3 = route(&mut store, Tier::T3Sidecar, &added, &probe);
        assert_eq!(t3.error.as_deref(), Some("not_allowed_for_tier:addSources"));
    }

    fn scripted(status: u16, challenge: bool) -> ScriptProbe {
        let mut http = HashMap::new();
        http.insert(
            "https://example.com/poem".to_string(),
            FetchReply::Response { status, content_length: Some(12), location: None, challenge },
        );
        ScriptProbe { http, pdf: HashMap::new(), hits: Cell::new(0) }
    }

    #[test]
    fn f1_http_403_stores_the_status_and_keeps_the_next_record() {
        let tmp = scratch("f1");
        let root = tmp.0.join("store");
        let next = tmp.0.join("next.txt");
        fs::write(&next, b"next").unwrap();
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = scripted(403, false);
        let sources = format!(r#"{{"url":"https://example.com/poem"}},{{"path":"{}"}}"#, next.display());
        let created = route(&mut store, Tier::T1User, &create_json("Poem", "", &sources), &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let got = &store.get(&id).unwrap().sources;
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].status, SourceStatus::Failed);
        assert_eq!(got[0].reason.as_deref(), Some("http-403"));
        assert_eq!(got[1].status, SourceStatus::Pending);
        assert!(got[1].reason.is_none());
        assert!(got[1].reference.ends_with("next.txt"));
    }

    #[test]
    fn f2_a_cloudflare_challenge_names_the_class() {
        let tmp = scratch("f2");
        let root = tmp.0.join("store");
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = scripted(403, true);
        let created = route(&mut store, Tier::T1User, &create_json("Poem", "", r#"{"url":"https://example.com/poem"}"#), &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let source = &store.get(&id).unwrap().sources[0];
        assert_eq!(source.status, SourceStatus::Failed);
        assert_eq!(source.reason.as_deref(), Some("http-403 challenge"));
        let (status, _, _, challenge) = parse_curl_headers("HTTP/2 403 \r\ncf-mitigated: challenge\r\nserver: cloudflare\r\n");
        assert_eq!(status, 403);
        assert!(challenge);
        let (status, _, _, challenge) = parse_curl_headers("HTTP/2 403 \r\nCF-Mitigated: Challenge\r\n");
        assert_eq!(status, 403);
        assert!(challenge);
        let (status, _, _, challenge) = parse_curl_headers("HTTP/2 403 \r\ncf-mitigated: challenge\r\n\r\nHTTP/2 200 \r\n");
        assert_eq!(status, 200);
        assert!(!challenge);
        let (_, _, _, content_type, challenge) = parse_curl_body_headers("HTTP/2 403 \r\ncontent-type: text/html\r\ncf-mitigated: challenge\r\n");
        assert_eq!(content_type.as_deref(), Some("text/html"));
        assert!(challenge);
    }

    #[test]
    fn f3_a_probe_miss_stays_unreachable() {
        let tmp = scratch("f3");
        let root = tmp.0.join("store");
        let next = tmp.0.join("next.txt");
        fs::write(&next, b"next").unwrap();
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = probe_ok(&[]);
        let sources = format!(r#"{{"url":"https://example.com/missing"}},{{"path":"{}"}}"#, next.display());
        let created = route(&mut store, Tier::T1User, &create_json("Miss", "", &sources), &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let got = &store.get(&id).unwrap().sources;
        assert_eq!(got[0].status, SourceStatus::Failed);
        assert_eq!(got[0].reason.as_deref(), Some("unreachable"));
        assert_eq!(got[1].status, SourceStatus::Pending);
    }

    #[test]
    fn f4_status_200_stays_pending() {
        let tmp = scratch("f4");
        let root = tmp.0.join("store");
        let mut store = ProjectStore::open(&root).unwrap();
        let probe = scripted(200, false);
        let created = route(&mut store, Tier::T1User, &create_json("Ok", "", r#"{"url":"https://example.com/poem"}"#), &probe);
        assert_eq!(created.status, "ok", "{:?}", created.error);
        let id = created.detail.unwrap()["projectId"].as_str().unwrap().to_string();
        let source = &store.get(&id).unwrap().sources[0];
        assert_eq!(source.status, SourceStatus::Pending);
        assert!(source.reason.is_none());
    }

    #[test]
    fn f5_both_curls_share_a_browser_user_agent() {
        let args = curl_args("-", "/dev/null");
        assert!(args.windows(2).any(|pair| pair[0] == "-A" && pair[1] == BROWSER_USER_AGENT));
        let proto = args.windows(2).find(|pair| pair[0] == "--proto").unwrap();
        assert_eq!(proto[1], "=http,https");
        assert!(!args.iter().any(|arg| arg == "-f" || arg == "--fail"));
        let prod = include_str!("intake.rs").split("mod tests").next().unwrap();
        assert_eq!(prod.matches(".args(curl_args(").count(), 2);
    }

    #[test]
    fn f7_log_line_names_the_challenge_and_skips_the_body() {
        let body = "<html><title>Just a moment...</title></html>";
        let line = intake_log_line(
            "https://www.poetryfoundation.org/poems/47311/the-waste-land",
            "http-403 challenge",
            Some(403),
            Some(0),
            Some(body),
        );
        assert!(line.starts_with("[intake] "));
        assert!(line.contains("https://www.poetryfoundation.org/poems/47311/the-waste-land"));
        assert!(line.contains("http-403 challenge"));
        assert!(line.contains("status=403"));
        assert!(!line.contains("Just a moment"));
        assert!(!line.contains("<html>"));
        let note = format!("could not resolve host {}\nsecond line", "x".repeat(200));
        let dns = intake_log_line("https://example.com/missing", "dns", None, Some(6), Some(&note));
        assert!(dns.contains("[intake]"));
        assert!(dns.contains("dns"));
        assert!(dns.contains("curl=6"));
        assert!(dns.contains("could not resolve host "));
        assert!(!dns.contains('\n'));
        assert!(!dns.contains(&"x".repeat(121)));
        assert!(!dns.contains("second line"));
    }
}
