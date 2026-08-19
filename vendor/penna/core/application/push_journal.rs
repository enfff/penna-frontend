use crate::ports::{JournalSync, RepositoryError, SyncResult};

pub struct PushJournalUseCase<S: JournalSync> {
    sync_port: S,
}

impl<S: JournalSync> PushJournalUseCase<S> {
    pub fn new(sync_port: S) -> Self {
        Self { sync_port }
    }

    pub fn execute(&self) -> Result<SyncResult, RepositoryError> {
        self.sync_port.push()
    }
}
