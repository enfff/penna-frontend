use crate::ports::{RepositoryError, TagCatalog};

pub struct ListTagsUseCase<T: TagCatalog> {
    tags: T,
}

impl<T: TagCatalog> ListTagsUseCase<T> {
    pub fn new(tags: T) -> Self {
        Self { tags }
    }

    pub fn execute(&self) -> Result<Vec<String>, RepositoryError> {
        self.tags.list_tags()
    }
}
