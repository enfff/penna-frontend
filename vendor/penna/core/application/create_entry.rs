use crate::domain::{DomainError, Entry, EntryId};
use crate::ports::{EntryRepository, RepositoryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateEntryInput {
    pub id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateEntryError {
    Domain(DomainError),
    Repository(RepositoryError),
}

pub struct CreateEntryUseCase<R: EntryRepository> {
    repository: R,
}

impl<R: EntryRepository> CreateEntryUseCase<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn execute(&self, input: CreateEntryInput) -> Result<Entry, CreateEntryError> {
        let entry = Entry::new(
            EntryId(input.id),
            input.title,
            input.body,
            input.tags,
            input.created_at,
            input.updated_at,
        )
        .map_err(CreateEntryError::Domain)?;

        self.repository
            .save(&entry)
            .map_err(CreateEntryError::Repository)?;

        Ok(entry)
    }
}
