use git2::{build::CheckoutBuilder, Repository, Signature};
use penna_core::domain::{Entry, EntryId};
use penna_core::ports::{
    EntryRepository, JournalClone, JournalPath, JournalSync, RepositoryError, SyncResult,
    TagCatalog,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
enum SyncMode {
    Smart,
    PullOnly,
    PushOnly,
}

pub struct GitJournalCloner;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TagsCatalogFile {
    tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntryTagsSidecar {
    tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryStatus {
    pub branch: Option<String>,
    pub head_commit: Option<String>,
    pub is_dirty: bool,
}

#[derive(Clone)]
pub struct GitEntryRepository {
    repo: Arc<Mutex<Repository>>,
    root: PathBuf,
}

impl std::fmt::Debug for GitEntryRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitEntryRepository")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl GitEntryRepository {
    pub fn new(path: std::path::PathBuf) -> Result<Self, RepositoryError> {
        let repo_path = path.join(".git");

        let repo = if repo_path.exists() {
            Repository::open(&path)
                .map_err(|e| RepositoryError::Storage(format!("Failed to open git repo: {}", e)))?
        } else {
            Repository::init(&path)
                .map_err(|e| RepositoryError::Storage(format!("Failed to init git repo: {}", e)))?
        };

        Ok(Self {
            repo: Arc::new(Mutex::new(repo)),
            root: path,
        })
    }

    pub fn with_existing_repo(repo: Repository) -> Self {
        let root = repo.path().parent().map_or_else(PathBuf::new, PathBuf::from);
        Self {
            repo: Arc::new(Mutex::new(repo)),
            root,
        }
    }

    pub fn repository_path(&self) -> &PathBuf {
        &self.root
    }

    pub fn status(&self) -> Result<RepositoryStatus, RepositoryError> {
        let repo = self.repo.lock().unwrap();

        let branch = match repo.head() {
            Ok(head) if head.is_branch() => head.shorthand().map(ToOwned::to_owned),
            _ => None,
        };

        let head_commit = match repo.head() {
            Ok(head) => head.target().map(|oid| oid.to_string()),
            Err(_) => None,
        };

        let is_dirty = !repo
            .statuses(None)
            .map_err(|e| RepositoryError::Storage(format!("Failed to get repo status: {}", e)))?
            .is_empty();

        Ok(RepositoryStatus {
            branch,
            head_commit,
            is_dirty,
        })
    }

    fn entry_path(&self, id: &str) -> PathBuf {
        PathBuf::from(format!("{}.md", id))
    }

    fn get_head_oid(&self) -> Result<Option<git2::Oid>, RepositoryError> {
        let repo = self.repo.lock().unwrap();
        let head = repo.head();

        match head {
            Ok(head) if head.is_branch() => {
                let commit = head.peel_to_commit().map_err(|e| {
                    RepositoryError::Storage(format!("Failed to get head commit: {}", e))
                })?;
                Ok(Some(commit.id()))
            }
            _ => Ok(None),
        }
    }

    fn read_file_from_commit(
        &self,
        commit_oid: git2::Oid,
        path: &std::path::Path,
    ) -> Result<Option<String>, RepositoryError> {
        let repo = self.repo.lock().unwrap();

        let commit = repo
            .find_commit(commit_oid)
            .map_err(|e| RepositoryError::Storage(format!("Failed to find commit: {}", e)))?;

        let tree = commit
            .tree()
            .map_err(|e| RepositoryError::Storage(format!("Failed to get tree: {}", e)))?;

        match tree.get_path(path) {
            Ok(entry) => {
                let object = entry.to_object(&repo).map_err(|e| {
                    RepositoryError::Storage(format!("Failed to get tree object: {}", e))
                })?;

                let blob = object
                    .into_blob()
                    .map_err(|_| RepositoryError::Storage("Not a blob".to_string()))?;

                let content = String::from_utf8(blob.content().to_vec())
                    .map_err(|e| RepositoryError::Storage(format!("Invalid UTF-8: {}", e)))?;

                Ok(Some(content))
            }
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(RepositoryError::Storage(format!("Failed to get file: {}", e))),
        }
    }

    fn create_signature(&self) -> Result<Signature<'static>, RepositoryError> {
        Signature::now("Penna", "penna@example.com")
            .map_err(|e| RepositoryError::Storage(format!("Failed to create signature: {}", e)))
    }

    fn parse_entry_content(id: &str, content: &str) -> Result<Entry, RepositoryError> {
        let timestamps = None;
        let lines: Vec<&str> = content.lines().collect();

        let (title, body_start) = if lines.first().map(|l| l.starts_with("# ")).unwrap_or(false) {
            (lines[0][2..].to_string(), 1)
        } else {
            ("Untitled".to_string(), 0)
        };

        let mut body_lines = &lines[body_start..];
        if !body_lines.is_empty() && body_lines[0].is_empty() {
            body_lines = &body_lines[1..];
        }
        let body = body_lines.join("\n");

        let (created_at, updated_at) = match timestamps {
            Some((c, u)) => (c, u),
            None => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| {
                        RepositoryError::Storage(format!("Failed to get timestamp: {}", e))
                    })?
                    .as_millis()
                    .to_string();
                (now.clone(), now)
            }
        };

        Ok(Entry {
            id: EntryId(id.to_string()),
            title,
            body,
            tags: Vec::new(),
            created_at,
            updated_at,
        })
    }

    fn format_entry_content(entry: &Entry) -> String {
        format!("# {}\n\n{}", entry.title, entry.body)
    }

    fn tags_file_relative_path() -> &'static Path {
        Path::new(".penna/tags.json")
    }

    fn entry_tags_relative_path(id: &str) -> PathBuf {
        PathBuf::from(format!(".penna/{}.json", id))
    }

    fn tags_file_absolute_path(&self) -> PathBuf {
        self.root.join(Self::tags_file_relative_path())
    }

    fn entry_tags_absolute_path(&self, id: &str) -> PathBuf {
        self.root.join(Self::entry_tags_relative_path(id))
    }

    fn normalize_tags(mut tags: Vec<String>) -> Vec<String> {
        for tag in &mut tags {
            *tag = tag.trim().to_string();
        }
        tags.retain(|t| !t.is_empty());
        tags.sort();
        tags.dedup();
        tags
    }

    fn read_tags_from_disk(&self) -> Result<Vec<String>, RepositoryError> {
        let path = self.tags_file_absolute_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let bytes = std::fs::read(&path).map_err(|e| {
            RepositoryError::Storage(format!("Failed to read tags file {}: {}", path.display(), e))
        })?;

        let parsed: TagsCatalogFile = serde_json::from_slice(&bytes).map_err(|e| {
            RepositoryError::Storage(format!("Failed to parse tags file {}: {}", path.display(), e))
        })?;

        Ok(Self::normalize_tags(parsed.tags))
    }

    fn read_entry_tags_from_disk(&self, id: &str) -> Result<Vec<String>, RepositoryError> {
        let path = self.entry_tags_absolute_path(id);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let bytes = std::fs::read(&path).map_err(|e| {
            RepositoryError::Storage(format!(
                "Failed to read entry tags sidecar {}: {}",
                path.display(),
                e
            ))
        })?;

        let parsed: EntryTagsSidecar = serde_json::from_slice(&bytes).map_err(|e| {
            RepositoryError::Storage(format!(
                "Failed to parse entry tags sidecar {}: {}",
                path.display(),
                e
            ))
        })?;

        Ok(Self::normalize_tags(parsed.tags))
    }

    fn write_entry_tags_sidecar_to_disk(
        &self,
        id: &str,
        tags: Vec<String>,
    ) -> Result<(), RepositoryError> {
        let normalized = Self::normalize_tags(tags);
        let file_path = self.entry_tags_absolute_path(id);

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RepositoryError::Storage(format!(
                    "Failed to create sidecar parent directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        let payload = EntryTagsSidecar { tags: normalized };
        let content = serde_json::to_vec_pretty(&payload).map_err(|e| {
            RepositoryError::Storage(format!("Failed to encode entry tags sidecar: {}", e))
        })?;

        std::fs::write(&file_path, content).map_err(|e| {
            RepositoryError::Storage(format!(
                "Failed to write entry tags sidecar {}: {}",
                file_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    fn list_entry_ids_from_head(&self) -> Result<Vec<String>, RepositoryError> {
        let mut entry_ids: Vec<String> = Vec::new();

        let commit_oid = match self.get_head_oid()? {
            Some(oid) => oid,
            None => return Ok(vec![]),
        };

        let repo = self.repo.lock().unwrap();

        let commit = repo
            .find_commit(commit_oid)
            .map_err(|e| RepositoryError::Storage(format!("Failed to find commit: {}", e)))?;

        let tree = commit
            .tree()
            .map_err(|e| RepositoryError::Storage(format!("Failed to get tree: {}", e)))?;

        for entry in tree.iter() {
            if entry.filemode() == git2::FileMode::Blob as i32 || entry.filemode() == 33188 {
                let path_str = entry.name().unwrap_or("");
                if path_str.ends_with(".md") {
                    let id = path_str[..path_str.len() - 3].to_string();
                    entry_ids.push(id);
                }
            }
        }

        Ok(entry_ids)
    }

    fn write_tags_to_disk_and_commit(
        &self,
        tags: Vec<String>,
        message: &str,
    ) -> Result<Vec<String>, RepositoryError> {
        let normalized = Self::normalize_tags(tags);
        let file_path = self.tags_file_absolute_path();

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RepositoryError::Storage(format!(
                    "Failed to create tags parent directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        let payload = TagsCatalogFile {
            tags: normalized.clone(),
        };

        let content = serde_json::to_vec_pretty(&payload).map_err(|e| {
            RepositoryError::Storage(format!("Failed to encode tags payload: {}", e))
        })?;

        std::fs::write(&file_path, content).map_err(|e| {
            RepositoryError::Storage(format!("Failed to write tags file {}: {}", file_path.display(), e))
        })?;

        let repo = self.repo.lock().unwrap();
        let mut index = repo
            .index()
            .map_err(|e| RepositoryError::Storage(format!("Failed to open git index: {}", e)))?;

        index
            .add_path(Self::tags_file_relative_path())
            .map_err(|e| RepositoryError::Storage(format!("Failed to add tags file to index: {}", e)))?;
        index
            .write()
            .map_err(|e| RepositoryError::Storage(format!("Failed to write git index: {}", e)))?;

        let tree_oid = index
            .write_tree()
            .map_err(|e| RepositoryError::Storage(format!("Failed to write git tree: {}", e)))?;
        let tree = repo
            .find_tree(tree_oid)
            .map_err(|e| RepositoryError::Storage(format!("Failed to find git tree: {}", e)))?;

        let sig = self.create_signature()?;
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.as_ref().map_or_else(Vec::new, |p| vec![p]);

        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(|e| RepositoryError::Storage(format!("Failed to commit tags change: {}", e)))?;

        Ok(normalized)
    }

    fn sync_with_mode(&self, mode: SyncMode) -> Result<SyncResult, RepositoryError> {
        let repo = self.repo.lock().unwrap();

        let mut remote = match repo.find_remote("origin") {
            Ok(remote) => remote,
            Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(SyncResult::NoRemote),
            Err(e) => {
                return Err(RepositoryError::Storage(format!(
                    "Failed to find origin remote: {}",
                    e
                )))
            }
        };

        let branch = match repo.head() {
            Ok(head) if head.is_branch() => head
                .shorthand()
                .map(ToOwned::to_owned)
                .ok_or_else(|| RepositoryError::Storage("failed to resolve current branch".to_string()))?,
            _ => return Ok(SyncResult::NoBranch),
        };

        let local_oid = repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|e| RepositoryError::Storage(format!("Failed to get head commit: {}", e)))?
            .id();

        let fetch_refspec = format!("refs/heads/{0}:refs/remotes/origin/{0}", branch);
        remote
            .fetch(&[&fetch_refspec], None, None)
            .map_err(|e| RepositoryError::Storage(format!("Failed to fetch remote: {}", e)))?;

        let remote_ref_name = format!("refs/remotes/origin/{}", branch);
        let remote_oid = match repo.find_reference(&remote_ref_name) {
            Ok(reference) => reference.target().ok_or_else(|| {
                RepositoryError::Storage("remote tracking reference has no target".to_string())
            })?,
            Err(e) if e.code() == git2::ErrorCode::NotFound => {
                if matches!(mode, SyncMode::PullOnly) {
                    return Ok(SyncResult::UpToDate { branch });
                }

                let push_refspec = format!("refs/heads/{0}:refs/heads/{0}", branch);
                remote.push(&[&push_refspec], None).map_err(|err| {
                    RepositoryError::Storage(format!("Failed to push branch to remote: {}", err))
                })?;
                return Ok(SyncResult::Pushed { branch });
            }
            Err(e) => {
                return Err(RepositoryError::Storage(format!(
                    "Failed to read remote tracking branch: {}",
                    e
                )))
            }
        };

        let (ahead, behind) = repo
            .graph_ahead_behind(local_oid, remote_oid)
            .map_err(|e| RepositoryError::Storage(format!("Failed to compare histories: {}", e)))?;

        if ahead == 0 && behind == 0 {
            return Ok(SyncResult::UpToDate { branch });
        }

        if ahead > 0 && behind > 0 {
            return Ok(SyncResult::Diverged {
                branch,
                ahead,
                behind,
            });
        }

        match mode {
            SyncMode::PullOnly => {
                if behind == 0 {
                    return Ok(SyncResult::UpToDate { branch });
                }

                let local_ref_name = format!("refs/heads/{}", branch);
                let mut local_ref = repo.find_reference(&local_ref_name).map_err(|e| {
                    RepositoryError::Storage(format!("Failed to find local branch reference: {}", e))
                })?;

                local_ref.set_target(remote_oid, "penna pull fast-forward").map_err(|e| {
                    RepositoryError::Storage(format!("Failed to fast-forward branch: {}", e))
                })?;

                repo.set_head(&local_ref_name)
                    .map_err(|e| RepositoryError::Storage(format!("Failed to set HEAD: {}", e)))?;

                repo.checkout_head(Some(CheckoutBuilder::new().force())).map_err(|e| {
                    RepositoryError::Storage(format!("Failed to checkout updated HEAD: {}", e))
                })?;

                Ok(SyncResult::Pulled { branch })
            }
            SyncMode::PushOnly => {
                if ahead == 0 {
                    return Ok(SyncResult::Diverged {
                        branch,
                        ahead,
                        behind,
                    });
                }

                let push_refspec = format!("refs/heads/{0}:refs/heads/{0}", branch);
                remote.push(&[&push_refspec], None).map_err(|e| {
                    RepositoryError::Storage(format!("Failed to push local commits: {}", e))
                })?;

                Ok(SyncResult::Pushed { branch })
            }
            SyncMode::Smart => {
                if behind > 0 {
                    let local_ref_name = format!("refs/heads/{}", branch);
                    let mut local_ref = repo.find_reference(&local_ref_name).map_err(|e| {
                        RepositoryError::Storage(format!("Failed to find local branch reference: {}", e))
                    })?;

                    local_ref.set_target(remote_oid, "penna sync fast-forward").map_err(|e| {
                        RepositoryError::Storage(format!("Failed to fast-forward branch: {}", e))
                    })?;

                    repo.set_head(&local_ref_name)
                        .map_err(|e| RepositoryError::Storage(format!("Failed to set HEAD: {}", e)))?;

                    repo.checkout_head(Some(CheckoutBuilder::new().force())).map_err(|e| {
                        RepositoryError::Storage(format!("Failed to checkout updated HEAD: {}", e))
                    })?;

                    return Ok(SyncResult::Pulled { branch });
                }

                let push_refspec = format!("refs/heads/{0}:refs/heads/{0}", branch);
                remote.push(&[&push_refspec], None).map_err(|e| {
                    RepositoryError::Storage(format!("Failed to push local commits: {}", e))
                })?;

                Ok(SyncResult::Pushed { branch })
            }
        }
    }
}

impl JournalClone for GitJournalCloner {
    fn clone_journal(&self, remote_url: &str, local_path: &PathBuf) -> Result<(), RepositoryError> {
        Repository::clone(remote_url, local_path).map_err(|e| {
            RepositoryError::Storage(format!(
                "Failed to clone repository from {} to {}: {}",
                remote_url,
                local_path.display(),
                e
            ))
        })?;

        Ok(())
    }
}

impl JournalPath for GitEntryRepository {
    fn resolve_path(&self) -> Result<PathBuf, RepositoryError> {
        self.root
            .canonicalize()
            .map_err(|e| RepositoryError::Storage(format!("Failed to canonicalize repo path: {}", e)))
    }
}

impl TagCatalog for GitEntryRepository {
    fn list_tags(&self) -> Result<Vec<String>, RepositoryError> {
        self.read_tags_from_disk()
    }

    fn add_tag(&self, tag: &str) -> Result<Vec<String>, RepositoryError> {
        let mut tags = self.read_tags_from_disk()?;
        if !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_string());
        }
        self.write_tags_to_disk_and_commit(tags, &format!("Add tag {}", tag))
    }

    fn remove_tag(&self, tag: &str) -> Result<Vec<String>, RepositoryError> {
        let mut tags = self.read_tags_from_disk()?;
        tags.retain(|t| t != tag);
        let updated = self.write_tags_to_disk_and_commit(tags, &format!("Remove tag {}", tag))?;

        for id in self.list_entry_ids_from_head()? {
            let mut entry_tags = self.read_entry_tags_from_disk(&id)?;
            entry_tags.retain(|t| t != tag);
            self.write_entry_tags_sidecar_to_disk(&id, entry_tags)?;
        }

        Ok(updated)
    }

    fn update_tag(&self, old_tag: &str, new_tag: &str) -> Result<Vec<String>, RepositoryError> {
        let mut tags = self.read_tags_from_disk()?;
        let Some(position) = tags.iter().position(|t| t == old_tag) else {
            return Err(RepositoryError::NotFound(old_tag.to_string()));
        };

        tags[position] = new_tag.to_string();
        let updated =
            self.write_tags_to_disk_and_commit(tags, &format!("Rename tag {} to {}", old_tag, new_tag))?;

        for id in self.list_entry_ids_from_head()? {
            let mut entry_tags = self.read_entry_tags_from_disk(&id)?;
            for value in &mut entry_tags {
                if value == old_tag {
                    *value = new_tag.to_string();
                }
            }
            self.write_entry_tags_sidecar_to_disk(&id, entry_tags)?;
        }

        Ok(updated)
    }
}

impl EntryRepository for GitEntryRepository {
    fn get(&self, id: &str) -> Result<Option<Entry>, RepositoryError> {
        let entry_path = self.entry_path(id);

        let commit_oid = match self.get_head_oid()? {
            Some(oid) => oid,
            None => return Ok(None),
        };

        let content = self.read_file_from_commit(commit_oid, &entry_path)?;

        match content {
            Some(content) => {
                let mut entry = Self::parse_entry_content(id, &content)?;
                entry.tags = self.read_entry_tags_from_disk(id)?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    fn save(&self, entry: &Entry) -> Result<(), RepositoryError> {
        let entry_path = self.entry_path(&entry.id.0);
        let content = Self::format_entry_content(entry);
        let sig = self.create_signature()?;
        let absolute_entry_path = self.root.join(&entry_path);

        std::fs::write(&absolute_entry_path, content.as_bytes()).map_err(|e| {
            RepositoryError::Storage(format!(
                "Failed to write entry file {}: {}",
                absolute_entry_path.display(),
                e
            ))
        })?;

        self.write_entry_tags_sidecar_to_disk(&entry.id.0, entry.tags.clone())?;

        let repo = self.repo.lock().unwrap();
        let mut index = repo
            .index()
            .map_err(|e| RepositoryError::Storage(format!("Failed to open git index: {}", e)))?;

        index
            .add_path(&entry_path)
            .map_err(|e| RepositoryError::Storage(format!("Failed to add entry to index: {}", e)))?;
        index
            .add_path(&Self::entry_tags_relative_path(&entry.id.0))
            .map_err(|e| RepositoryError::Storage(format!("Failed to add sidecar to index: {}", e)))?;
        index
            .write()
            .map_err(|e| RepositoryError::Storage(format!("Failed to write git index: {}", e)))?;

        let tree_oid = index
            .write_tree()
            .map_err(|e| RepositoryError::Storage(format!("Failed to write git tree: {}", e)))?;
        let tree = repo
            .find_tree(tree_oid)
            .map_err(|e| RepositoryError::Storage(format!("Failed to find git tree: {}", e)))?;

        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.as_ref().map_or_else(Vec::new, |p| vec![p]);

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!(
                "{} entry {}",
                if parents.is_empty() { "Create" } else { "Update" },
                entry.id.0
            ),
            &tree,
            &parents,
        )
        .map_err(|e| RepositoryError::Storage(format!("Failed to commit: {}", e)))?;

        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), RepositoryError> {
        let repo = self.repo.lock().unwrap();
        let entry_path = self.entry_path(id);
        let entry_tags_path = Self::entry_tags_relative_path(id);
        let absolute_entry_path = self.root.join(&entry_path);
        let absolute_entry_tags_path = self.root.join(&entry_tags_path);

        if absolute_entry_path.exists() {
            std::fs::remove_file(&absolute_entry_path).map_err(|e| {
                RepositoryError::Storage(format!(
                    "Failed to remove entry file {}: {}",
                    absolute_entry_path.display(),
                    e
                ))
            })?;
        }

        if absolute_entry_tags_path.exists() {
            std::fs::remove_file(&absolute_entry_tags_path).map_err(|e| {
                RepositoryError::Storage(format!(
                    "Failed to remove sidecar file {}: {}",
                    absolute_entry_tags_path.display(),
                    e
                ))
            })?;
        }

        let sig = self.create_signature()?;
        let mut index = repo
            .index()
            .map_err(|e| RepositoryError::Storage(format!("Failed to open git index: {}", e)))?;

        index
            .remove_path(&entry_path)
            .map_err(|e| RepositoryError::Storage(format!("Failed to remove entry from index: {}", e)))?;

        if self.root.join(&entry_tags_path).exists() {
            index.remove_path(&entry_tags_path).map_err(|e| {
                RepositoryError::Storage(format!("Failed to remove sidecar from index: {}", e))
            })?;
        } else {
            let _ = index.remove_path(&entry_tags_path);
        }

        index
            .write()
            .map_err(|e| RepositoryError::Storage(format!("Failed to write git index: {}", e)))?;

        let tree_oid = index
            .write_tree()
            .map_err(|e| RepositoryError::Storage(format!("Failed to write git tree: {}", e)))?;
        let tree = repo
            .find_tree(tree_oid)
            .map_err(|e| RepositoryError::Storage(format!("Failed to find git tree: {}", e)))?;

        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.as_ref().map_or_else(Vec::new, |p| vec![p]);

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("Delete entry {}", id),
            &tree,
            &parents,
        )
        .map_err(|e| RepositoryError::Storage(format!("Failed to commit: {}", e)))?;

        Ok(())
    }

    fn list(&self) -> Result<Vec<Entry>, RepositoryError> {
        let mut entry_ids: Vec<String> = Vec::new();

        let commit_oid = match self.get_head_oid()? {
            Some(oid) => oid,
            None => return Ok(vec![]),
        };

        {
            let repo = self.repo.lock().unwrap();

            let commit = repo
                .find_commit(commit_oid)
                .map_err(|e| RepositoryError::Storage(format!("Failed to find commit: {}", e)))?;

            let tree = commit
                .tree()
                .map_err(|e| RepositoryError::Storage(format!("Failed to get tree: {}", e)))?;

            for entry in tree.iter() {
                if entry.filemode() == git2::FileMode::Blob as i32 || entry.filemode() == 33188 {
                    let path_str = entry.name().unwrap_or("");
                    if path_str.ends_with(".md") {
                        let id = path_str[..path_str.len() - 3].to_string();
                        entry_ids.push(id);
                    }
                }
            }
        }

        let mut entries = Vec::new();
        for id in entry_ids {
            if let Ok(Some(entry)) = self.get(&id) {
                entries.push(entry);
            }
        }

        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(entries)
    }
}

impl JournalSync for GitEntryRepository {
    fn sync(&self) -> Result<SyncResult, RepositoryError> {
        self.sync_with_mode(SyncMode::Smart)
    }

    fn pull(&self) -> Result<SyncResult, RepositoryError> {
        self.sync_with_mode(SyncMode::PullOnly)
    }

    fn push(&self) -> Result<SyncResult, RepositoryError> {
        self.sync_with_mode(SyncMode::PushOnly)
    }
}
