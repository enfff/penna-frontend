use crate::ports::{RepositoryError, TagCatalog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateTagError {
    InvalidTag,
    Repository(RepositoryError),
}

pub struct UpdateTagUseCase<T: TagCatalog> {
    tags: T,
}

impl<T: TagCatalog> UpdateTagUseCase<T> {
    pub fn new(tags: T) -> Self {
        Self { tags }
    }

    pub fn execute(&self, old_tag: &str, new_tag: &str) -> Result<Vec<String>, UpdateTagError> {
        let old_normalized = old_tag.trim();
        let new_normalized = new_tag.trim();
        if old_normalized.is_empty() || new_normalized.is_empty() {
            return Err(UpdateTagError::InvalidTag);
        }

        self.tags
            .update_tag(old_normalized, new_normalized)
            .map_err(UpdateTagError::Repository)
    }
}
