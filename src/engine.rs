use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

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
struct JournalState {
    repo_path: PathBuf,
    entries_dir: PathBuf,
    branch: String,
    head_commit: String,
    dirty: bool,
    entries: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct EngineMock {
    next_handle: u64,
    journals: BTreeMap<u64, JournalState>,
}

impl EngineMock {
    pub fn connect_journal(&mut self, repo_path: &str) -> Result<ConnectResult, String> {
        let trimmed = repo_path.trim();
        if trimmed.is_empty() {
            return Err("Repository path required".to_string());
        }

        let repo = PathBuf::from(trimmed);
        fs::create_dir_all(&repo).map_err(|e| format!("Unable to prepare repository path: {e}"))?;

        let marker = repo.join(".penna_connected");
        let sync_action = if marker.exists() {
            SyncAction::Updated
        } else {
            SyncAction::Downloaded
        };

        fs::write(&marker, b"connected\n").map_err(|e| format!("Unable to write marker file: {e}"))?;

        let entries_dir = if repo.join("entries").is_dir() {
            repo.join("entries")
        } else {
            repo.clone()
        };

        if !entries_dir.exists() {
            fs::create_dir_all(&entries_dir)
                .map_err(|e| format!("Unable to prepare entries directory: {e}"))?;
        }
        let entries = Self::load_entries(&entries_dir)?;

        self.next_handle += 1;
        let handle = JournalHandle(self.next_handle);

        self.journals.insert(
            handle.0,
            JournalState {
                repo_path: repo.clone(),
                entries_dir,
                branch: "main".to_string(),
                head_commit: "mock-head".to_string(),
                dirty: false,
                entries,
            },
        );

        Ok(ConnectResult {
            journal_handle: handle,
            capabilities: vec![
                "list_entries".to_string(),
                "get_entry".to_string(),
                "create_entry".to_string(),
                "update_entry".to_string(),
                "delete_entry".to_string(),
            ],
            current_branch: "main".to_string(),
            sync_action,
        })
    }

    pub fn journal_status(&self, handle: JournalHandle) -> Option<JournalStatus> {
        let state = self.journals.get(&handle.0)?;
        Some(JournalStatus {
            repo_path: state.repo_path.to_string_lossy().to_string(),
            branch: state.branch.clone(),
            head_commit: state.head_commit.clone(),
            dirty: state.dirty,
            entry_count: state.entries.len(),
        })
    }

    pub fn disconnect_journal(&mut self, handle: JournalHandle) -> bool {
        self.journals.remove(&handle.0).is_some()
    }

    pub fn list_entries(&self, handle: JournalHandle) -> Vec<String> {
        self.journals
            .get(&handle.0)
            .map(|state| state.entries.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn entries_directory(&self, handle: JournalHandle) -> Option<PathBuf> {
        self.journals
            .get(&handle.0)
            .map(|state| state.entries_dir.clone())
    }

    pub fn reload_entries(&mut self, handle: JournalHandle) -> Result<usize, String> {
        let state = self
            .journals
            .get_mut(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let entries = Self::load_entries(&state.entries_dir)?;
        let count = entries.len();
        state.entries = entries;
        Ok(count)
    }

    pub fn entries_fingerprint(&self, handle: JournalHandle) -> Result<u64, String> {
        let state = self
            .journals
            .get(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let dir = fs::read_dir(&state.entries_dir)
            .map_err(|e| format!("Unable to read entries directory: {e}"))?;

        for item in dir {
            let item = item.map_err(|e| format!("Unable to read entry metadata: {e}"))?;
            let path = item.path();
            if !path.is_file() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            if !Self::is_valid_entry_id(name) {
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
        }

        Ok(hasher.finish())
    }

    pub fn get_entry(&self, handle: JournalHandle, entry_id: &str) -> Option<String> {
        self.journals
            .get(&handle.0)
            .and_then(|state| state.entries.get(entry_id).cloned())
    }

    pub fn create_entry(&mut self, handle: JournalHandle, entry_id: &str, content: &str) -> Result<(), String> {
        Self::validate_entry_id(entry_id)?;

        let state = self
            .journals
            .get_mut(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        if state.entries.contains_key(entry_id) {
            return Err("Entry already exists".to_string());
        }

        state.entries.insert(entry_id.to_string(), content.to_string());
        state.dirty = true;
        Self::write_entry_file(&state.entries_dir, entry_id, content)
    }

    pub fn update_entry(&mut self, handle: JournalHandle, entry_id: &str, content: &str) -> Result<(), String> {
        Self::validate_entry_id(entry_id)?;

        let state = self
            .journals
            .get_mut(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        if !state.entries.contains_key(entry_id) {
            return Err("Entry not found".to_string());
        }

        state.entries.insert(entry_id.to_string(), content.to_string());
        state.dirty = true;
        Self::write_entry_file(&state.entries_dir, entry_id, content)
    }

    pub fn delete_entry(&mut self, handle: JournalHandle, entry_id: &str) -> Result<(), String> {
        let state = self
            .journals
            .get_mut(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        if state.entries.remove(entry_id).is_none() {
            return Err("Entry not found".to_string());
        }

        let path = state.entries_dir.join(entry_id);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Unable to delete entry file: {e}"))?;
        }
        state.dirty = true;
        Ok(())
    }

    pub fn entry_save(&mut self, handle: JournalHandle, entry_id: &str, content: &str) -> Result<(), String> {
        Self::validate_entry_id(entry_id)?;

        let state = self
            .journals
            .get_mut(&handle.0)
            .ok_or_else(|| "Journal handle not found".to_string())?;

        state.entries.insert(entry_id.to_string(), content.to_string());

        Self::write_entry_file(&state.entries_dir, entry_id, content)?;
        state.dirty = false;
        state.head_commit = "mock-head-saved".to_string();
        Ok(())
    }

    fn load_entries(entries_dir: &Path) -> Result<BTreeMap<String, String>, String> {
        let mut entries = BTreeMap::new();
        let dir = fs::read_dir(entries_dir)
            .map_err(|e| format!("Unable to read entries directory: {e}"))?;

        for entry in dir {
            let entry = entry.map_err(|e| format!("Unable to read entry metadata: {e}"))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            if !Self::is_valid_entry_id(name) {
                continue;
            }

            let content = fs::read_to_string(&path).unwrap_or_default();
            entries.insert(name.to_string(), content);
        }

        Ok(entries)
    }

    fn write_entry_file(entries_dir: &Path, entry_id: &str, content: &str) -> Result<(), String> {
        fs::create_dir_all(entries_dir)
            .map_err(|e| format!("Unable to prepare entries directory: {e}"))?;

        let file_path = entries_dir.join(entry_id);
        fs::write(&file_path, content).map_err(|e| format!("Unable to write entry file: {e}"))
    }

    fn validate_entry_id(entry_id: &str) -> Result<(), String> {
        if Self::is_valid_entry_id(entry_id) {
            Ok(())
        } else {
            Err("Entry id must match YYYYMMDDHHmm.md".to_string())
        }
    }

    fn is_valid_entry_id(entry_id: &str) -> bool {
        if !entry_id.ends_with(".md") {
            return false;
        }

        let stem = &entry_id[..entry_id.len() - 3];
        if stem.len() != 12 {
            return false;
        }

        stem.bytes().all(|c| c.is_ascii_digit())
    }
}
