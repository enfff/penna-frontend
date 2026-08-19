use crate::domain::{DomainError, Entry, EntryId};
use crate::ports::{EntryRepository, RepositoryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEntryError {
    Domain(DomainError),
    Repository(RepositoryError),
}

pub struct UpdateEntryInput {
    pub id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct UpdateEntryUseCase<R: EntryRepository> {
    repository: R,
}

impl<R: EntryRepository> UpdateEntryUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn execute(&self, input: UpdateEntryInput) -> Result<Entry, UpdateEntryError> {
        let entry = Entry::new(
            EntryId(input.id),
            input.title,
            input.body,
            input.tags,
            input.created_at,
            input.updated_at,
        )
        .map_err(UpdateEntryError::Domain)?;

        self.repository
            .save(&entry)
            .map_err(UpdateEntryError::Repository)?;

        Ok(entry)
    }
}
