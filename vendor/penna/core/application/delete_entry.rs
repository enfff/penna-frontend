use crate::ports::{EntryRepository, RepositoryError};

pub struct DeleteEntryUseCase<R: EntryRepository> {
    repository: R,
}

impl<R: EntryRepository> DeleteEntryUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn execute(&self, id: &str) -> Result<(), RepositoryError> {
        self.repository.delete(id)
    }
}