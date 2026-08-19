use crate::ports::{RepositoryError, TagCatalog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveTagError {
    InvalidTag,
    Repository(RepositoryError),
}

pub struct RemoveTagUseCase<T: TagCatalog> {
    tags: T,
}

impl<T: TagCatalog> RemoveTagUseCase<T> {
    pub fn new(tags: T) -> Self {
        Self { tags }
    }

    pub fn execute(&self, tag: &str) -> Result<Vec<String>, RemoveTagError> {
        let normalized = tag.trim();
        if normalized.is_empty() {
            return Err(RemoveTagError::InvalidTag);
        }

        self.tags
            .remove_tag(normalized)
            .map_err(RemoveTagError::Repository)
    }
}
