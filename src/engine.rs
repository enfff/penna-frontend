use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use penna_engine::{CreateEntryRequest, PennaEngine, UpdateEntryRequest};

#[derive(Clone, Debug)]
pub struct EntryRecord {
    pub entry_id: String,
    pub content: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct EntrySummary {
    pub entry_id: String,
    pub tags: Vec<String>,
}

/// Full content snapshot of an entry, taken before deletion so the
/// "Note deleted" toast can offer an undo that restores the original.
#[derive(Clone, Debug)]
pub struct EntrySnapshot {
    pub entry_id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct JournalHandle(pub u64);

/// Whether the connected folder was a brand-new diary or an existing journal.
///
/// A penna journal is, at its core, a git repository. The engine's
/// `connect_journal` opens the repo if `<path>/.git` already exists and
/// initializes a fresh one otherwise, so the presence of `.git` before we
/// connect is the discriminator between the two cases.
///
/// Note: the `.penna/` sidecar directory is *not* a reliable marker — the
/// engine creates it lazily (when the first tag is added), not at init — so
/// a valid new diary has no `.penna/` until tags exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalKind {
    /// No `.git` was present; the engine initialized a new diary.
    New,
    /// A git repository already existed; the engine connected to it.
    Existing,
}

#[derive(Debug)]
pub struct ConnectResult {
    pub journal_handle: JournalHandle,
    pub capabilities: Vec<String>,
    pub current_branch: String,
    pub journal_kind: JournalKind,
}

#[derive(Debug)]
pub struct JournalStatus {
    pub repo_path: String,
    pub branch: String,
    pub head_commit: String,
    pub dirty: bool,
    pub entry_count: usize,
    /// True while MERGE_HEAD exists (ADR 0014): a sync merge is mid-flight.
    pub merge_in_progress: bool,
    /// Entry ids (.md form) whose index stages still conflict, empty
    /// otherwise. Derived from the engine's repo-relative `conflicted_paths`.
    pub conflicted_entry_ids: Vec<String>,
}

/// Result of one sync attempt, mirroring the engine's `SyncReport` with
/// conflict ids converted to the frontend's external `.md` entry id form.
///
/// Statuses: `up_to_date`, `pulled`, `pushed`, `no_remote`, `no_branch`,
/// `diverged` (merge started; `conflicted_entry_ids` lists what needs
/// resolution). Per ADR 0014 a follow-up sync after the last resolution
/// concludes the pending merge and reports `pulled`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncOutcome {
    pub status: String,
    pub branch: Option<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub conflicted_entry_ids: Vec<String>,
}

#[derive(Debug)]
struct SessionState {
    session_id: String,
    repo_path: PathBuf,
}

#[derive(Debug, Default)]
pub struct EngineMock {
    engine: PennaEngine,
    next_handle: u64,
    sessions: BTreeMap<u64, SessionState>,
}

impl EngineMock {
    pub fn connect_journal(&mut self, repo_path: &str) -> Result<ConnectResult, String> {
        let trimmed = repo_path.trim();
        if trimmed.is_empty() {
            return Err("Repository path required".to_string());
        }

        let repo = PathBuf::from(trimmed);
        // A penna journal is a git repository. If `.git` is absent the engine
        // will initialize a new diary; if present it connects to the existing
        // one. (The `.penna/` sidecar is created lazily, so it is not used
        // here as a discriminator.)
        let journal_kind = if repo.join(".git").exists() {
            JournalKind::Existing
        } else {
            JournalKind::New
        };

        let session = self
            .engine
            .connect_journal(trimmed)
            .map_err(Self::format_engine_error)?;

        self.next_handle += 1;
        let handle = JournalHandle(self.next_handle);

        self.sessions.insert(
            handle.0,
            SessionState {
                session_id: session.session_id.clone(),
                repo_path: PathBuf::from(session.repo_path),
            },
        );

        let status = self
            .engine
            .journal_status(&session.session_id)
            .map_err(Self::format_engine_error)?;

        Ok(ConnectResult {
            journal_handle: handle,
            capabilities: vec![
                "list_entries".to_string(),
                "get_entry".to_string(),
                "create_entry".to_string(),
                "update_entry".to_string(),
                "delete_entry".to_string(),
                "sync_journal".to_string(),
                "list_tags".to_string(),
                "add_tag".to_string(),
                "remove_tag".to_string(),
                "sidecar_integrity_status".to_string(),
            ],
            current_branch: status.branch.unwrap_or_else(|| "main".to_string()),
            journal_kind,
        })
    }

    pub fn journal_status(&self, handle: JournalHandle) -> Option<JournalStatus> {
        let session = self.sessions.get(&handle.0)?;
        let status = self.engine.journal_status(&session.session_id).ok()?;
        let entry_count = self.engine.list_entries(&session.session_id).ok()?.len();

        Some(JournalStatus {
            repo_path: status.repo_path,
            branch: status.branch.unwrap_or_else(|| "main".to_string()),
            head_commit: status.head_commit.unwrap_or_default(),
            dirty: status.is_dirty,
            entry_count,
            merge_in_progress: status.merge_in_progress,
            conflicted_entry_ids: Self::conflicted_paths_to_entry_ids(&status.conflicted_paths),
        })
    }

    /// Fetch + push + merge through the engine. On divergence the engine
    /// starts a real git merge (working tree receives conflict markers) and
    /// reports the still-conflicted entries; once everything is resolved and
    /// staged, this same call concludes the merge (ADR 0014).
    pub fn sync_journal(&self, handle: JournalHandle) -> Result<SyncOutcome, String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let report = self
            .engine
            .sync_journal(&session.session_id)
            .map_err(Self::format_engine_error)?;

        Ok(SyncOutcome {
            conflicted_entry_ids: Self::entry_ids_from_stems(&report.conflicts),
            status: report.status,
            branch: report.branch,
            ahead: report.ahead,
            behind: report.behind,
        })
    }

    /// Entry ids with unresolved index conflicts per the latest status.
    pub fn conflicted_entry_ids(&self, handle: JournalHandle) -> Vec<String> {
        self.journal_status(handle)
            .map(|status| status.conflicted_entry_ids)
            .unwrap_or_default()
    }

    pub fn disconnect_journal(&mut self, handle: JournalHandle) -> bool {
        let Some(session) = self.sessions.remove(&handle.0) else {
            return false;
        };

        self.engine.disconnect_journal(&session.session_id).is_ok()
    }

    pub fn list_entries(&self, handle: JournalHandle) -> Vec<EntrySummary> {
        let Some(session) = self.sessions.get(&handle.0) else {
            return Vec::new();
        };

        let mut entries = match self.engine.list_entries(&session.session_id) {
            Ok(entries) => entries,
            Err(_) => return Vec::new(),
        };

        entries.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        entries
            .into_iter()
            .map(|entry| EntrySummary {
                entry_id: Self::to_external_entry_id(&entry.id.0),
                tags: Self::normalize_tags(&entry.tags),
            })
            .collect()
    }

    pub fn list_tags(&self, handle: JournalHandle) -> Vec<String> {
        let Some(session) = self.sessions.get(&handle.0) else {
            return Vec::new();
        };

        match self.engine.list_tags(&session.session_id) {
            Ok(tags) => Self::normalize_tags(&tags),
            Err(_) => Vec::new(),
        }
    }

    pub fn entries_directory(&self, handle: JournalHandle) -> Option<PathBuf> {
        self.sessions.get(&handle.0).map(|s| s.repo_path.clone())
    }

    pub fn reload_entries(&mut self, handle: JournalHandle) -> Result<usize, String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let entries = self
            .engine
            .list_entries(&session.session_id)
            .map_err(Self::format_engine_error)?;

        Ok(entries.len())
    }

    pub fn entries_fingerprint(&self, handle: JournalHandle) -> Result<u64, String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        Self::filesystem_fingerprint(&session.repo_path)
    }

    pub fn get_entry(&self, handle: JournalHandle, entry_id: &str) -> Option<EntryRecord> {
        let session = self.sessions.get(&handle.0)?;
        let internal_id = Self::to_internal_entry_id(entry_id);

        let entry = self
            .engine
            .get_entry(&session.session_id, &internal_id)
            .ok()
            .flatten()?;

        Some(EntryRecord {
            entry_id: Self::to_external_entry_id(&entry.id.0),
            content: Self::compose_markdown_content(&entry.title, &entry.body),
            tags: Self::normalize_tags(&entry.tags),
        })
    }

    /// Create a new entry; the engine assigns the next-free-minute id
    /// (YYYYMMDDHHmm). Returns the created record so the caller can open it.
    /// Blank title is intentional: engine v0.1.1 allows empty titles.
    pub fn create_entry_new(&mut self, handle: JournalHandle) -> Result<EntryRecord, String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let request = CreateEntryRequest {
            title: String::new(),
            body: String::new(),
            tags: Vec::new(),
        };

        let entry = self
            .engine
            .create_entry(&session.session_id, request)
            .map_err(Self::format_engine_error)?;

        Ok(EntryRecord {
            entry_id: Self::to_external_entry_id(&entry.id.0),
            content: String::new(),
            tags: Self::normalize_tags(&entry.tags),
        })
    }

    pub fn update_entry(
        &mut self,
        handle: JournalHandle,
        entry_id: &str,
        content: &str,
        tags: &[String],
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let internal_id = Self::to_internal_entry_id(entry_id);
        // The content is the single source of truth: an empty title (no
        // heading) is stored as-is, so deleting a heading and saving removes
        // it instead of resurrecting the previous one. The engine preserves
        // `created_at` and errors if the entry does not exist.
        let (title, body) = Self::split_markdown_content(content);

        let request = UpdateEntryRequest {
            id: internal_id,
            title,
            body,
            tags: Self::normalize_tags(tags),
        };

        self.engine
            .update_entry(&session.session_id, request)
            .map_err(Self::format_engine_error)?;

        Ok(())
    }

    pub fn delete_entry(&mut self, handle: JournalHandle, entry_id: &str) -> Result<(), String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let internal_id = Self::to_internal_entry_id(entry_id);
        self.engine
            .delete_entry(&session.session_id, &internal_id)
            .map_err(Self::format_engine_error)
    }

    /// Delete an entry, keeping a snapshot of its full content so the caller
    /// can offer an undo that restores it.
    pub fn delete_entry_with_snapshot(
        &mut self,
        handle: JournalHandle,
        entry_id: &str,
    ) -> Result<EntrySnapshot, String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let internal_id = Self::to_internal_entry_id(entry_id);

        let entry = self
            .engine
            .get_entry(&session.session_id, &internal_id)
            .map_err(Self::format_engine_error)?
            .ok_or_else(|| "Entry not found".to_string())?;

        let snapshot = EntrySnapshot {
            entry_id: entry_id.to_string(),
            title: entry.title.clone(),
            body: entry.body.clone(),
            tags: Self::normalize_tags(&entry.tags),
            created_at: entry.created_at.clone(),
        };

        self.engine
            .delete_entry(&session.session_id, &internal_id)
            .map_err(Self::format_engine_error)?;

        Ok(snapshot)
    }

    /// Re-create an entry from a snapshot taken by `delete_entry_with_snapshot`.
    ///
    /// Workaround: engine v0.1.1 has no `restore_entry`/`create_entry_with_id`
    /// in its public API, so the restored note gets a fresh engine-assigned
    /// minute id (content, tags, title identical; original id and
    /// `created_at` are not preserved). Returns the created record so the
    /// caller can open the restored note under its new id.
    /// TODO(engine-team): add
    /// `create_entry_with_id` (id/title/body/tags/created_at/updated_at) so
    /// undo restores the exact entry.
    pub fn restore_entry(
        &mut self,
        handle: JournalHandle,
        snapshot: &EntrySnapshot,
    ) -> Result<EntryRecord, String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        // If the entry reappeared (e.g. Undo pressed twice across a refresh),
        // refuse to silently duplicate it.
        let internal_id = Self::to_internal_entry_id(&snapshot.entry_id);
        if self
            .engine
            .get_entry(&session.session_id, &internal_id)
            .map_err(Self::format_engine_error)?
            .is_some()
        {
            return Err("Entry already exists".to_string());
        }

        let request = CreateEntryRequest {
            title: snapshot.title.clone(),
            body: snapshot.body.clone(),
            tags: Self::normalize_tags(&snapshot.tags),
        };

        let entry = self
            .engine
            .create_entry(&session.session_id, request)
            .map_err(Self::format_engine_error)?;

        Ok(EntryRecord {
            entry_id: Self::to_external_entry_id(&entry.id.0),
            content: Self::compose_markdown_content(&entry.title, &entry.body),
            tags: Self::normalize_tags(&entry.tags),
        })
    }

    pub fn entry_save(
        &mut self,
        handle: JournalHandle,
        entry_id: &str,
        content: &str,
        tags: &[String],
    ) -> Result<(), String> {
        self.update_entry(handle, entry_id, content, tags)
    }

    pub fn add_tag(&mut self, handle: JournalHandle, entry_id: &str, tag: &str) -> Result<Vec<String>, String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let next_tag = tag.trim();
        if next_tag.is_empty() {
            let current = self
                .get_entry(handle, entry_id)
                .map(|entry| entry.tags)
                .unwrap_or_default();
            return Ok(current);
        }

        self.engine
            .add_tag(&session.session_id, next_tag)
            .map_err(Self::format_engine_error)?;

        let mut current = self
            .get_entry(handle, entry_id)
            .ok_or_else(|| "Entry not found".to_string())?
            .tags;

        if !current.iter().any(|existing| existing == next_tag) {
            current.push(next_tag.to_string());
            current = Self::normalize_tags(&current);
            self.update_entry(handle, entry_id, &self.entry_content_or_default(handle, entry_id), &current)?;
        }

        Ok(current)
    }

    pub fn remove_tag(&mut self, handle: JournalHandle, entry_id: &str, tag: &str) -> Result<Vec<String>, String> {
        let mut current = self
            .get_entry(handle, entry_id)
            .ok_or_else(|| "Entry not found".to_string())?
            .tags;

        current.retain(|existing| existing != tag);
        current = Self::normalize_tags(&current);

        self.update_entry(handle, entry_id, &self.entry_content_or_default(handle, entry_id), &current)?;
        Ok(current)
    }

    fn entry_content_or_default(&self, handle: JournalHandle, entry_id: &str) -> String {
        self.get_entry(handle, entry_id)
            .map(|entry| entry.content)
            .unwrap_or_default()
    }

    fn format_engine_error(err: penna_engine::EngineError) -> String {
        err.message()
    }

    fn to_internal_entry_id(entry_id: &str) -> String {
        entry_id.strip_suffix(".md").unwrap_or(entry_id).to_string()
    }

    fn to_external_entry_id(entry_id: &str) -> String {
        if entry_id.ends_with(".md") {
            entry_id.to_string()
        } else {
            format!("{entry_id}.md")
        }
    }

    /// Converts engine conflict stems (bare `YYYYMMDDHHmm` ids) into the
    /// external `.md` form, sorted and deduplicated.
    fn entry_ids_from_stems(stems: &[String]) -> Vec<String> {
        let mut ids: Vec<String> = stems
            .iter()
            .map(|stem| Self::to_external_entry_id(stem.trim()))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Converts the engine's repo-relative conflicted paths (e.g.
    /// `202601011200.md`, possibly nested) into external `.md` entry ids by
    /// file name only; non-entry paths are ignored.
    fn conflicted_paths_to_entry_ids(paths: &[String]) -> Vec<String> {
        let mut ids: Vec<String> = paths
            .iter()
            .filter_map(|path| Path::new(path.trim()).file_name()?.to_str())
            .filter(|name| name.ends_with(".md"))
            .map(|name| name.to_string())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn normalize_tags(tags: &[String]) -> Vec<String> {
        let mut normalized = Vec::new();

        for tag in tags {
            let trimmed = tag.trim();
            if trimmed.is_empty() {
                continue;
            }
            if normalized.iter().any(|existing| existing == trimmed) {
                continue;
            }
            normalized.push(trimmed.to_string());
        }

        normalized.sort_unstable();
        normalized
    }

    fn split_markdown_content(content: &str) -> (String, String) {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return (String::new(), String::new());
        }

        if let Some(first) = lines.first() {
            if let Some(title) = first.strip_prefix("# ") {
                let body_start = if lines.get(1).copied() == Some("") { 2 } else { 1 };
                let body = lines.get(body_start..).unwrap_or(&[]).join("\n");
                return (title.trim().to_string(), body);
            }
        }

        // No heading present: return an empty title so the caller can fall
        // back to the entry's existing title instead of forcing "Untitled".
        (String::new(), content.to_string())
    }

    fn compose_markdown_content(title: &str, body: &str) -> String {
        let trimmed_title = title.trim();
        if trimmed_title.is_empty() {
            return body.to_string();
        }

        if body.trim().is_empty() {
            format!("# {trimmed_title}")
        } else {
            format!("# {trimmed_title}\n\n{body}")
        }
    }

    fn filesystem_fingerprint(repo_path: &Path) -> Result<u64, String> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let dir = fs::read_dir(repo_path)
            .map_err(|e| format!("Unable to read repository directory: {e}"))?;

        for item in dir {
            let item = item.map_err(|e| format!("Unable to read entry metadata: {e}"))?;
            let path = item.path();
            if !path.is_file() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".md") {
                continue;
            }

            let stem = name.strip_suffix(".md").unwrap_or(name);
            if !stem.chars().all(|ch| ch.is_ascii_digit()) || stem.len() != 12 {
                continue;
            }

            name.hash(&mut hasher);

            let metadata = item
                .metadata()
                .map_err(|e| format!("Unable to read file metadata: {e}"))?;
            metadata.len().hash(&mut hasher);

            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                    duration.as_nanos().hash(&mut hasher);
                }
            }

            let sidecar = repo_path.join(".penna").join(format!("{stem}.json"));
            if let Ok(sidecar_metadata) = fs::metadata(sidecar) {
                sidecar_metadata.len().hash(&mut hasher);
                if let Ok(modified) = sidecar_metadata.modified() {
                    if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                        duration.as_nanos().hash(&mut hasher);
                    }
                }
            }
        }

        Ok(hasher.finish())
    }
}

#[cfg(test)]
mod note_data_parsing_tests {
    use super::*;

    #[test]
    fn split_separates_heading_and_body() {
        let (title, body) =
            EngineMock::split_markdown_content("# Morning walk\n\nWent around the lake.");
        assert_eq!(title, "Morning walk");
        assert_eq!(body, "Went around the lake.");
    }

    #[test]
    fn split_accepts_heading_without_blank_line() {
        let (title, body) =
            EngineMock::split_markdown_content("# Morning walk\nWent around the lake.");
        assert_eq!(title, "Morning walk");
        assert_eq!(body, "Went around the lake.");
    }

    #[test]
    fn split_keeps_multiline_body_intact() {
        let content = "# Groceries\n\neggs\nmilk\nbread";
        let (title, body) = EngineMock::split_markdown_content(content);
        assert_eq!(title, "Groceries");
        assert_eq!(body, "eggs\nmilk\nbread");
    }

    #[test]
    fn split_trims_whitespace_from_title() {
        let (title, _) = EngineMock::split_markdown_content("#   Padded title   \n\nBody");
        assert_eq!(title, "Padded title");
    }

    #[test]
    fn split_treats_unheaded_content_as_body() {
        let content = "Just some thoughts, no heading.";
        let (title, body) = EngineMock::split_markdown_content(content);
        assert_eq!(title, "");
        assert_eq!(body, content);
    }

    #[test]
    fn split_treats_empty_content_as_empty_fields() {
        let (title, body) = EngineMock::split_markdown_content("");
        assert_eq!(title, "");
        assert_eq!(body, "");
    }

    #[test]
    fn split_treats_bare_hash_prefix_as_empty_title() {
        // Malformed: a heading with no title text still yields an empty
        // title rather than panicking or swallowing the rest of the note.
        let (title, body) = EngineMock::split_markdown_content("# \n\nBody");
        assert_eq!(title, "");
        assert_eq!(body, "Body");
    }

    #[test]
    fn split_does_not_mistake_subheading_for_title() {
        // "## " does not match the "# " title prefix, so the whole note is
        // body and the caller keeps the previous title.
        let content = "## Deep dive\nDetails here.";
        let (title, body) = EngineMock::split_markdown_content(content);
        assert_eq!(title, "");
        assert_eq!(body, content);
    }

    #[test]
    fn split_ignores_heading_below_first_line() {
        // Only the very first line can be a title; a later heading stays
        // inside the body verbatim.
        let content = "Intro paragraph\n# Not a title";
        let (title, body) = EngineMock::split_markdown_content(content);
        assert_eq!(title, "");
        assert_eq!(body, content);
    }

    #[test]
    fn compose_omits_heading_for_blank_titles() {
        assert_eq!(EngineMock::compose_markdown_content("", "Body only"), "Body only");
        assert_eq!(
            EngineMock::compose_markdown_content("   ", "Body only"),
            "Body only"
        );
    }

    #[test]
    fn compose_omits_separator_for_blank_bodies() {
        assert_eq!(EngineMock::compose_markdown_content("Title", "   "), "# Title");
        assert_eq!(EngineMock::compose_markdown_content("Title", ""), "# Title");
    }

    #[test]
    fn compose_is_identity_for_both_parts_empty() {
        assert_eq!(EngineMock::compose_markdown_content("", ""), "");
    }

    #[test]
    fn compose_split_round_trips_losslessly() {
        for (title, body) in [
            ("Title", "Body"),
            ("Only a title", ""),
            ("", "Only a body"),
            ("", ""),
            ("Multi word title", "line one\nline two"),
            ("Tricky #hash title", "- bullet\n- list"),
        ] {
            let composed = EngineMock::compose_markdown_content(title, body);
            let (parsed_title, parsed_body) = EngineMock::split_markdown_content(&composed);
            assert_eq!(parsed_title, title, "title should survive the round trip");
            assert_eq!(parsed_body, body, "body should survive the round trip");

            let recomposed = EngineMock::compose_markdown_content(&parsed_title, &parsed_body);
            assert_eq!(recomposed, composed, "recomposition should be a fixed point");
        }
    }

    #[test]
    fn normalize_tags_trims_dedupes_sorts_and_skips_blanks() {
        let tags: Vec<String> = vec![
            "  travel ".to_string(),
            String::new(),
            "work".to_string(),
            "   ".to_string(),
            "travel".to_string(),
            " family".to_string(),
        ];
        assert_eq!(
            EngineMock::normalize_tags(&tags),
            vec!["family", "travel", "work"]
        );
    }

    #[test]
    fn normalize_tags_of_garbage_input_is_empty() {
        let garbage: Vec<String> = vec![String::new(), "  ".to_string(), "\t\n".to_string()];
        assert!(EngineMock::normalize_tags(&garbage).is_empty());
        assert!(EngineMock::normalize_tags(&[]).is_empty());
    }
}

#[cfg(test)]
mod entry_id_tests {
    use super::*;

    #[test]
    fn internal_id_strips_extension_once() {
        assert_eq!(
            EngineMock::to_internal_entry_id("202402291230.md"),
            "202402291230"
        );
        assert_eq!(EngineMock::to_internal_entry_id("202402291230"), "202402291230");
    }

    #[test]
    fn external_id_appends_extension_once() {
        assert_eq!(
            EngineMock::to_external_entry_id("202402291230"),
            "202402291230.md"
        );
        assert_eq!(
            EngineMock::to_external_entry_id("202402291230.md"),
            "202402291230.md"
        );
    }

    #[test]
    fn id_normalization_round_trips() {
        for stem in ["202402291230", "202608240000", "199912312359"] {
            let external = EngineMock::to_external_entry_id(stem);
            assert_eq!(external, format!("{stem}.md"));
            assert_eq!(EngineMock::to_internal_entry_id(&external), stem);

            let reexternalized = EngineMock::to_external_entry_id(&external);
            assert_eq!(reexternalized, external);
            assert_eq!(EngineMock::to_internal_entry_id(&reexternalized), stem);
        }
    }
}

#[cfg(test)]
mod sync_surface_tests {
    use super::*;

    #[test]
    fn conflicted_paths_flatten_to_entry_ids() {
        let paths = vec![
            "202601011200.md".to_string(),
            "sub/dir/202601011201.md".to_string(),
            "./202601011202.md".to_string(),
        ];
        assert_eq!(
            EngineMock::conflicted_paths_to_entry_ids(&paths),
            vec![
                "202601011200.md".to_string(),
                "202601011201.md".to_string(),
                "202601011202.md".to_string(),
            ]
        );
    }

    #[test]
    fn conflicted_paths_keep_md_names_and_skip_blank_paths() {
        // Mapping is name-only: the engine scopes conflicted_paths to entry
        // files mid-merge, but blank/malformed entries must not panic.
        let paths = vec![
            ".penna/catalog.json".to_string(),
            "notes/202601011200.md".to_string(),
            String::new(),
        ];
        assert_eq!(
            EngineMock::conflicted_paths_to_entry_ids(&paths),
            vec!["202601011200.md".to_string()]
        );
    }

    #[test]
    fn conflicted_paths_sort_and_dedupe() {
        let paths = vec![
            "202601011201.md".to_string(),
            "202601011200.md".to_string(),
            "202601011201.md".to_string(),
        ];
        assert_eq!(
            EngineMock::conflicted_paths_to_entry_ids(&paths),
            vec![
                "202601011200.md".to_string(),
                "202601011201.md".to_string(),
            ]
        );
    }

    #[test]
    fn conflict_stems_gain_extension_sorted_deduped() {
        let stems = vec![
            " 202601011201 ".to_string(),
            "202601011200".to_string(),
            "202601011201.md".to_string(),
        ];
        assert_eq!(
            EngineMock::entry_ids_from_stems(&stems),
            vec![
                "202601011200.md".to_string(),
                "202601011201.md".to_string(),
            ]
        );
    }

    #[test]
    fn sync_without_connected_journal_errors_gracefully() {
        let engine = EngineMock::default();
        let err = engine.sync_journal(JournalHandle(99)).unwrap_err();
        assert_eq!(err, "Journal handle not found");
        assert!(engine.conflicted_entry_ids(JournalHandle(99)).is_empty());
    }
}
