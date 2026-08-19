use crate::ports::{JournalSync, RepositoryError, SyncResult};

pub struct PullJournalUseCase<S: JournalSync> {
    sync_port: S,
}

impl<S: JournalSync> PullJournalUseCase<S> {
    pub fn new(sync_port: S) -> Self {
        Self { sync_port }
    }

    pub fn execute(&self) -> Result<SyncResult, RepositoryError> {
        self.sync_port.pull()
    }
}
