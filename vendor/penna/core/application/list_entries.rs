use crate::ports::{EntryRepository, RepositoryError};

pub struct ListEntriesUseCase<R: EntryRepository> {
    repository: R,
}

impl<R: EntryRepository> ListEntriesUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn execute(&self) -> Result<Vec<crate::domain::Entry>, RepositoryError> {
        self.repository.list()
    }
}
