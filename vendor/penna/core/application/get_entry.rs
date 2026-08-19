use crate::domain::Entry;
use crate::ports::{EntryRepository, RepositoryError};

pub struct GetEntryUseCase<R: EntryRepository> {
    repository: R,
}

impl<R: EntryRepository> GetEntryUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn execute(&self, id: &str) -> Result<Option<Entry>, RepositoryError> {
        self.repository.get(id)
    }
}
