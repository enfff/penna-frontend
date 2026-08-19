use crate::ports::{RepositoryError, TagCatalog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddTagError {
    InvalidTag,
    Repository(RepositoryError),
}

pub struct AddTagUseCase<T: TagCatalog> {
    tags: T,
}

impl<T: TagCatalog> AddTagUseCase<T> {
    pub fn new(tags: T) -> Self {
        Self { tags }
    }

    pub fn execute(&self, tag: &str) -> Result<Vec<String>, AddTagError> {
        let normalized = tag.trim();
        if normalized.is_empty() {
            return Err(AddTagError::InvalidTag);
        }

        self.tags
            .add_tag(normalized)
            .map_err(AddTagError::Repository)
    }
}
