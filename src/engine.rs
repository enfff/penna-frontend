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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct JournalHandle(pub u64);

#[derive(Clone, Copy, Debug)]
pub enum SyncAction {
    Downloaded,
    Updated,
}

#[derive(Debug)]
pub struct ConnectResult {
    pub journal_handle: JournalHandle,
    pub capabilities: Vec<String>,
    pub current_branch: String,
    pub sync_action: SyncAction,
}

#[derive(Debug)]
pub struct JournalStatus {
    pub repo_path: String,
    pub branch: String,
    pub head_commit: String,
    pub dirty: bool,
    pub entry_count: usize,
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
        let had_git_repo = repo.join(".git").exists();

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
            sync_action: if had_git_repo {
                SyncAction::Updated
            } else {
                SyncAction::Downloaded
            },
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
        })
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

        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries
            .into_iter()
            .map(|entry| EntrySummary {
                entry_id: Self::to_external_entry_id(&entry.id),
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
            entry_id: Self::to_external_entry_id(&entry.id),
            content: Self::compose_markdown_content(&entry.title, &entry.body),
            tags: Self::normalize_tags(&entry.tags),
        })
    }

    pub fn create_entry(
        &mut self,
        handle: JournalHandle,
        _entry_id: &str,
        content: &str,
        tags: &[String],
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let (title, body) = Self::split_markdown_content(content);
        let request = CreateEntryRequest {
            title,
            body,
            tags: Self::normalize_tags(tags),
        };

        self.engine
            .create_entry(&session.session_id, request)
            .map_err(Self::format_engine_error)?;

        Ok(())
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
        let existing = self
            .engine
            .get_entry(&session.session_id, &internal_id)
            .map_err(Self::format_engine_error)?
            .ok_or_else(|| "Entry not found".to_string())?;

        let (mut title, body) = Self::split_markdown_content(content);
        if title.trim().is_empty() {
            title = existing.title;
        }

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
            return ("Untitled".to_string(), String::new());
        }

        if let Some(first) = lines.first() {
            if let Some(title) = first.strip_prefix("# ") {
                let body_start = if lines.get(1).copied() == Some("") { 2 } else { 1 };
                let body = lines.get(body_start..).unwrap_or(&[]).join("\n");
                return (title.trim().to_string(), body);
            }
        }

        ("Untitled".to_string(), content.to_string())
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
