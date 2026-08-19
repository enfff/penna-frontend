use std::path::PathBuf;

use crate::ports::{JournalPath, RepositoryError};

pub struct ResolveJournalPathUseCase<P: JournalPath> {
    path_port: P,
}

impl<P: JournalPath> ResolveJournalPathUseCase<P> {
    pub fn new(path_port: P) -> Self {
        Self { path_port }
    }

    pub fn execute(&self) -> Result<PathBuf, RepositoryError> {
        self.path_port.resolve_path()
    }
}
