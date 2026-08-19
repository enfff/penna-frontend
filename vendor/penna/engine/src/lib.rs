use chrono::{Duration, Local};
use penna_adapters_git::{GitEntryRepository, GitJournalCloner};
use penna_core::application::{
    AddTagError, AddTagUseCase, CloneJournalUseCase, CreateEntryError, CreateEntryInput,
    CreateEntryUseCase, DeleteEntryUseCase, GetEntryUseCase, ListEntriesUseCase,
    ListTagsUseCase, PullJournalUseCase, PushJournalUseCase, RemoveTagError,
    RemoveTagUseCase, ResolveJournalPathUseCase, SidecarIntegrityStatus, SidecarSource,
    SyncJournalUseCase, UpdateEntryError, UpdateEntryInput, UpdateEntryUseCase,
    UpdateTagError, UpdateTagUseCase, ValidateSidecarIntegrityUseCase,
};
use penna_core::domain::{Entry, Sidecar};
use penna_core::ports::{EntryRepository, RepositoryError, SyncResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug)]
pub enum EngineError {
    Io(String),
    NotConnected(String),
    Repo(RepositoryError),
    Create(CreateEntryError),
    Update(UpdateEntryError),
    AddTag(AddTagError),
    RemoveTag(RemoveTagError),
    UpdateTag(UpdateTagError),
    IdCollision(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineErrorDto {
    pub code: String,
    pub message: String,
}

impl EngineError {
    pub fn code(&self) -> &'static str {
        match self {
            EngineError::Io(_) => "IO",
            EngineError::NotConnected(_) => "NOT_CONNECTED",
            EngineError::Repo(_) => "REPO",
            EngineError::Create(CreateEntryError::Domain(_)) => "VALIDATION",
            EngineError::Create(CreateEntryError::Repository(_)) => "REPO",
            EngineError::Update(UpdateEntryError::Domain(_)) => "VALIDATION",
            EngineError::Update(UpdateEntryError::Repository(_)) => "REPO",
            EngineError::AddTag(AddTagError::InvalidTag) => "VALIDATION",
            EngineError::AddTag(AddTagError::Repository(_)) => "REPO",
            EngineError::RemoveTag(RemoveTagError::InvalidTag) => "VALIDATION",
            EngineError::RemoveTag(RemoveTagError::Repository(_)) => "REPO",
            EngineError::UpdateTag(UpdateTagError::InvalidTag) => "VALIDATION",
            EngineError::UpdateTag(UpdateTagError::Repository(_)) => "REPO",
            EngineError::IdCollision(_) => "CONFLICT",
        }
    }

    pub fn message(&self) -> String {
        match self {
            EngineError::Io(msg) => msg.clone(),
            EngineError::NotConnected(msg) => msg.clone(),
            EngineError::Repo(err) => format!("{:?}", err),
            EngineError::Create(err) => format!("{:?}", err),
            EngineError::Update(err) => format!("{:?}", err),
            EngineError::AddTag(err) => format!("{:?}", err),
            EngineError::RemoveTag(err) => format!("{:?}", err),
            EngineError::UpdateTag(err) => format!("{:?}", err),
            EngineError::IdCollision(msg) => msg.clone(),
        }
    }

    pub fn to_dto(&self) -> EngineErrorDto {
        EngineErrorDto {
            code: self.code().to_string(),
            message: self.message(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalSession {
    pub session_id: String,
    pub repo_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalStatus {
    pub session_id: String,
    pub repo_path: String,
    pub branch: Option<String>,
    pub head_commit: Option<String>,
    pub is_dirty: bool,
    pub connected_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloneJournalRequest {
    pub remote_url: String,
    pub local_parent_dir: String,
    pub directory_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveJournalPathResponse {
    pub repo_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEntryRequest {
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateEntryRequest {
    pub id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEntryApiRequest {
    pub session_id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryDto {
    pub id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarIntegrityReport {
    pub status: String,
    pub expected_entry_id: Option<String>,
    pub actual_entry_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncReport {
    pub status: String,
    pub branch: Option<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
}

impl From<SyncResult> for SyncReport {
    fn from(value: SyncResult) -> Self {
        match value {
            SyncResult::UpToDate { branch } => Self {
                status: "up_to_date".to_string(),
                branch: Some(branch),
                ahead: None,
                behind: None,
            },
            SyncResult::NoRemote => Self {
                status: "no_remote".to_string(),
                branch: None,
                ahead: None,
                behind: None,
            },
            SyncResult::NoBranch => Self {
                status: "no_branch".to_string(),
                branch: None,
                ahead: None,
                behind: None,
            },
            SyncResult::Pulled { branch } => Self {
                status: "pulled".to_string(),
                branch: Some(branch),
                ahead: None,
                behind: None,
            },
            SyncResult::Pushed { branch } => Self {
                status: "pushed".to_string(),
                branch: Some(branch),
                ahead: None,
                behind: None,
            },
            SyncResult::Diverged {
                branch,
                ahead,
                behind,
            } => Self {
                status: "diverged".to_string(),
                branch: Some(branch),
                ahead: Some(ahead),
                behind: Some(behind),
            },
        }
    }
}

impl From<SidecarIntegrityStatus> for SidecarIntegrityReport {
    fn from(value: SidecarIntegrityStatus) -> Self {
        match value {
            SidecarIntegrityStatus::Ok => Self {
                status: "ok".to_string(),
                expected_entry_id: None,
                actual_entry_id: None,
                reason: None,
            },
            SidecarIntegrityStatus::Missing => Self {
                status: "missing".to_string(),
                expected_entry_id: None,
                actual_entry_id: None,
                reason: None,
            },
            SidecarIntegrityStatus::Mismatch {
                expected_entry_id,
                actual_entry_id,
            } => Self {
                status: "mismatch".to_string(),
                expected_entry_id: Some(expected_entry_id),
                actual_entry_id: Some(actual_entry_id),
                reason: None,
            },
            SidecarIntegrityStatus::Malformed { reason } => Self {
                status: "malformed".to_string(),
                expected_entry_id: None,
                actual_entry_id: None,
                reason: Some(reason),
            },
        }
    }
}

impl From<Entry> for EntryDto {
    fn from(value: Entry) -> Self {
        Self {
            id: value.id.0,
            title: value.title,
            body: value.body,
            tags: value.tags,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
struct SessionState {
    repo: GitEntryRepository,
    repo_path: PathBuf,
    connected_at: String,
}

#[derive(Debug)]
pub struct PennaEngine {
    sessions: Mutex<HashMap<String, SessionState>>,
}

impl Default for PennaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PennaEngine {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn connect_journal<P: AsRef<Path>>(&self, repo_path: P) -> Result<JournalSession, EngineError> {
        let repo_path = repo_path.as_ref().to_path_buf();
        fs::create_dir_all(&repo_path)
            .map_err(|e| EngineError::Io(format!("failed to create repo directory: {}", e)))?;

        let repo = GitEntryRepository::new(repo_path.clone()).map_err(EngineError::Repo)?;
        self.register_session(repo_path, repo)
    }

    pub fn clone_journal(&self, request: CloneJournalRequest) -> Result<JournalSession, EngineError> {
        let parent_dir = PathBuf::from(&request.local_parent_dir);
        fs::create_dir_all(&parent_dir)
            .map_err(|e| EngineError::Io(format!("failed to create parent directory: {}", e)))?;

        let repo_path = parent_dir.join(&request.directory_name);
        let use_case = CloneJournalUseCase::new(GitJournalCloner);
        use_case
            .execute(&request.remote_url, repo_path.clone())
            .map_err(EngineError::Repo)?;

        let repo = GitEntryRepository::new(repo_path.clone()).map_err(EngineError::Repo)?;
        self.register_session(repo_path, repo)
    }

    pub fn resolve_journal_path(
        &self,
        session_id: &str,
    ) -> Result<ResolveJournalPathResponse, EngineError> {
        let state = self.session(session_id)?;
        let use_case = ResolveJournalPathUseCase::new(state.repo.clone());
        let repo_path = use_case.execute().map_err(EngineError::Repo)?;

        Ok(ResolveJournalPathResponse {
            repo_path: repo_path.to_string_lossy().to_string(),
        })
    }

    pub fn pull_journal(&self, session_id: &str) -> Result<SyncReport, EngineError> {
        let state = self.session(session_id)?;
        let use_case = PullJournalUseCase::new(state.repo.clone());
        let result = use_case.execute().map_err(EngineError::Repo)?;
        Ok(result.into())
    }

    pub fn push_journal(&self, session_id: &str) -> Result<SyncReport, EngineError> {
        let state = self.session(session_id)?;
        let use_case = PushJournalUseCase::new(state.repo.clone());
        let result = use_case.execute().map_err(EngineError::Repo)?;
        Ok(result.into())
    }

    fn register_session(
        &self,
        repo_path: PathBuf,
        repo: GitEntryRepository,
    ) -> Result<JournalSession, EngineError> {
        let session_id = Self::new_session_id();
        let connected_at = Local::now().to_rfc3339();

        let state = SessionState {
            repo,
            repo_path: repo_path.clone(),
            connected_at,
        };

        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(session_id.clone(), state);

        Ok(JournalSession {
            session_id,
            repo_path: repo_path.to_string_lossy().to_string(),
        })
    }

    pub fn disconnect_journal(&self, session_id: &str) -> Result<(), EngineError> {
        let mut sessions = self.sessions.lock().unwrap();
        let removed = sessions.remove(session_id);
        if removed.is_none() {
            return Err(EngineError::NotConnected(session_id.to_string()));
        }
        Ok(())
    }

    pub fn journal_status(&self, session_id: &str) -> Result<JournalStatus, EngineError> {
        let state = self.session(session_id)?;
        let status = state.repo.status().map_err(EngineError::Repo)?;

        Ok(JournalStatus {
            session_id: session_id.to_string(),
            repo_path: state.repo_path.to_string_lossy().to_string(),
            branch: status.branch,
            head_commit: status.head_commit,
            is_dirty: status.is_dirty,
            connected_at: state.connected_at,
        })
    }

    pub fn list_entries(&self, session_id: &str) -> Result<Vec<Entry>, EngineError> {
        let state = self.session(session_id)?;
        let use_case = ListEntriesUseCase::new(state.repo.clone());
        use_case.execute().map_err(EngineError::Repo)
    }

    pub fn get_entry(&self, session_id: &str, id: &str) -> Result<Option<Entry>, EngineError> {
        let state = self.session(session_id)?;
        let use_case = GetEntryUseCase::new(state.repo.clone());
        use_case.execute(id).map_err(EngineError::Repo)
    }

    pub fn create_entry(&self, session_id: &str, request: CreateEntryRequest) -> Result<Entry, EngineError> {
        let state = self.session(session_id)?;
        let id = Self::next_available_entry_id(&state.repo)?;

        let now = Local::now().to_rfc3339();
        let use_case = CreateEntryUseCase::new(state.repo.clone());
        use_case
            .execute(CreateEntryInput {
                id,
                title: request.title,
                body: request.body,
                tags: request.tags,
                created_at: now.clone(),
                updated_at: now,
            })
            .map_err(EngineError::Create)
    }

    pub fn create_entry_with_id(
        &self,
        session_id: &str,
        id: &str,
        request: CreateEntryRequest,
    ) -> Result<Entry, EngineError> {
        let state = self.session(session_id)?;

        if state.repo.get(id).map_err(EngineError::Repo)?.is_some() {
            return Err(EngineError::IdCollision(id.to_string()));
        }

        let now = Local::now().to_rfc3339();
        let use_case = CreateEntryUseCase::new(state.repo.clone());
        use_case
            .execute(CreateEntryInput {
                id: id.to_string(),
                title: request.title,
                body: request.body,
                tags: request.tags,
                created_at: now.clone(),
                updated_at: now,
            })
            .map_err(EngineError::Create)
    }

    pub fn restore_entry(
        &self,
        session_id: &str,
        id: &str,
        title: &str,
        body: &str,
        tags: &[String],
        created_at: &str,
    ) -> Result<Entry, EngineError> {
        let state = self.session(session_id)?;

        let created_at = if created_at.trim().is_empty() {
            Local::now().to_rfc3339()
        } else {
            created_at.to_string()
        };

        let use_case = CreateEntryUseCase::new(state.repo.clone());
        use_case
            .execute(CreateEntryInput {
                id: id.to_string(),
                title: title.to_string(),
                body: body.to_string(),
                tags: tags.to_vec(),
                created_at,
                updated_at: Local::now().to_rfc3339(),
            })
            .map_err(EngineError::Create)
    }

    pub fn create_entry_api(&self, request: CreateEntryApiRequest) -> Result<EntryDto, EngineError> {
        let entry = self.create_entry(
            &request.session_id,
            CreateEntryRequest {
                title: request.title,
                body: request.body,
                tags: request.tags,
            },
        )?;
        Ok(entry.into())
    }

    pub fn update_entry(&self, session_id: &str, request: UpdateEntryRequest) -> Result<Entry, EngineError> {
        let state = self.session(session_id)?;
        let existing = state
            .repo
            .get(&request.id)
            .map_err(EngineError::Repo)?
            .ok_or_else(|| EngineError::Repo(RepositoryError::NotFound(request.id.clone())))?;

        let use_case = UpdateEntryUseCase::new(state.repo.clone());
        use_case
            .execute(UpdateEntryInput {
                id: request.id,
                title: request.title,
                body: request.body,
                tags: request.tags,
                created_at: existing.created_at,
                updated_at: Local::now().to_rfc3339(),
            })
            .map_err(EngineError::Update)
    }

    pub fn delete_entry(&self, session_id: &str, id: &str) -> Result<(), EngineError> {
        let state = self.session(session_id)?;
        let use_case = DeleteEntryUseCase::new(state.repo.clone());
        use_case.execute(id).map_err(EngineError::Repo)
    }

    pub fn sync_journal(&self, session_id: &str) -> Result<SyncReport, EngineError> {
        let state = self.session(session_id)?;
        let use_case = SyncJournalUseCase::new(state.repo.clone());
        let result = use_case.execute().map_err(EngineError::Repo)?;
        Ok(result.into())
    }

    pub fn list_tags(&self, session_id: &str) -> Result<Vec<String>, EngineError> {
        let state = self.session(session_id)?;
        let use_case = ListTagsUseCase::new(state.repo.clone());
        use_case.execute().map_err(EngineError::Repo)
    }

    pub fn add_tag(&self, session_id: &str, tag: &str) -> Result<Vec<String>, EngineError> {
        let state = self.session(session_id)?;
        let use_case = AddTagUseCase::new(state.repo.clone());
        use_case.execute(tag).map_err(EngineError::AddTag)
    }

    pub fn remove_tag(&self, session_id: &str, tag: &str) -> Result<Vec<String>, EngineError> {
        let state = self.session(session_id)?;
        let use_case = RemoveTagUseCase::new(state.repo.clone());
        use_case.execute(tag).map_err(EngineError::RemoveTag)
    }

    pub fn update_tag(
        &self,
        session_id: &str,
        old_tag: &str,
        new_tag: &str,
    ) -> Result<Vec<String>, EngineError> {
        let state = self.session(session_id)?;
        let use_case = UpdateTagUseCase::new(state.repo.clone());
        use_case
            .execute(old_tag, new_tag)
            .map_err(EngineError::UpdateTag)
    }

    pub fn sidecar_integrity_status(
        &self,
        entry_id: &str,
        sidecar_json: Option<&str>,
    ) -> SidecarIntegrityReport {
        let source = match sidecar_json {
            None => SidecarSource::Missing,
            Some(json) => match serde_json::from_str::<Sidecar>(json) {
                Ok(sidecar) => SidecarSource::Present(sidecar),
                Err(err) => SidecarSource::Malformed(err.to_string()),
            },
        };

        let use_case = ValidateSidecarIntegrityUseCase::new();
        use_case.execute(entry_id, source).into()
    }

    fn session(&self, session_id: &str) -> Result<SessionState, EngineError> {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| EngineError::NotConnected(session_id.to_string()))
    }

    fn new_session_id() -> String {
        let now = Local::now();
        format!("session-{}", now.timestamp_nanos_opt().unwrap_or_default())
    }

    fn next_available_entry_id(repo: &GitEntryRepository) -> Result<String, EngineError> {
        let mut candidate = Local::now();
        for _ in 0..(24 * 60) {
            let id = candidate.format("%Y%m%d%H%M").to_string();
            if repo.get(&id).map_err(EngineError::Repo)?.is_none() {
                return Ok(id);
            }
            candidate += Duration::minutes(1);
        }

        Err(EngineError::IdCollision(
            "no available minute slot in next 24h".to_string(),
        ))
    }
}
