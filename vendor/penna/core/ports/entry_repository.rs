use crate::domain::{Document, Entry, Sidecar};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    Storage(String),
    NotFound(String),
}

pub trait EntryRepository: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<Entry>, RepositoryError>;
    fn save(&self, entry: &Entry) -> Result<(), RepositoryError>;
    fn delete(&self, id: &str) -> Result<(), RepositoryError>;
    fn list(&self) -> Result<Vec<Entry>, RepositoryError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncResult {
    UpToDate { branch: String },
    NoRemote,
    NoBranch,
    Pulled { branch: String },
    Pushed { branch: String },
    Diverged { branch: String, ahead: usize, behind: usize },
}

pub trait JournalSync: Send + Sync {
    fn sync(&self) -> Result<SyncResult, RepositoryError>;
    fn pull(&self) -> Result<SyncResult, RepositoryError>;
    fn push(&self) -> Result<SyncResult, RepositoryError>;
}

pub trait JournalClone: Send + Sync {
    fn clone_journal(&self, remote_url: &str, local_path: &PathBuf) -> Result<(), RepositoryError>;
}

pub trait JournalPath: Send + Sync {
    fn resolve_path(&self) -> Result<PathBuf, RepositoryError>;
}

pub trait TagCatalog: Send + Sync {
    fn list_tags(&self) -> Result<Vec<String>, RepositoryError>;
    fn add_tag(&self, tag: &str) -> Result<Vec<String>, RepositoryError>;
    fn remove_tag(&self, tag: &str) -> Result<Vec<String>, RepositoryError>;
    fn update_tag(&self, old_tag: &str, new_tag: &str) -> Result<Vec<String>, RepositoryError>;
}

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSystemError {
    Io(String),
    NotFound(String),
}

impl fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileSystemError::Io(msg) => write!(f, "IO error: {}", msg),
            FileSystemError::NotFound(msg) => write!(f, "Not found: {}", msg),
        }
    }
}

impl std::error::Error for FileSystemError {}

pub trait FileSystem: Send + Sync {
    fn read(&self, path: &PathBuf) -> Result<Vec<u8>, FileSystemError>;
    fn write(&self, path: &PathBuf, data: &[u8]) -> Result<(), FileSystemError>;
    fn exists(&self, path: &PathBuf) -> bool;
    fn create_dir_all(&self, path: &PathBuf) -> Result<(), FileSystemError>;
}

pub trait MarkdownImporter: Send + Sync {
    fn import(&self, markdown: &str, frontmatter: &str) -> Result<Document, Box<dyn std::error::Error + Send + Sync>>;
}

pub trait MarkdownExporter: Send + Sync {
    fn export(&self, document: &Document) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
    
    fn export_with_sidecar(
        &self,
        document: &Document,
        sidecar: Option<&Sidecar>,
        include_sidecar: bool,
    ) -> Result<(String, Option<String>), Box<dyn std::error::Error + Send + Sync>>;
}
