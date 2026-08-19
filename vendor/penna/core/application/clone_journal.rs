use std::path::PathBuf;

use crate::ports::{JournalClone, RepositoryError};

pub struct CloneJournalUseCase<C: JournalClone> {
    cloner: C,
}

impl<C: JournalClone> CloneJournalUseCase<C> {
    pub fn new(cloner: C) -> Self {
        Self { cloner }
    }

    pub fn execute(&self, remote_url: &str, local_path: PathBuf) -> Result<(), RepositoryError> {
        self.cloner.clone_journal(remote_url, &local_path)
    }
}
