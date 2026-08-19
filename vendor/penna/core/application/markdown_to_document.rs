use crate::domain::{Document, DocumentError};
use crate::ports::MarkdownImporter;
use std::error::Error;

#[derive(Debug, PartialEq, Eq)]
pub struct MarkdownToDocumentInput {
    pub markdown_body: String,
    pub frontmatter: Option<String>,
}

#[derive(Debug)]
pub enum MarkdownToDocumentError {
    Document(DocumentError),
    Import(Box<dyn Error + Send + Sync>),
}

pub struct MarkdownToDocumentUseCase<I: MarkdownImporter> {
    importer: I,
}

impl<I: MarkdownImporter> MarkdownToDocumentUseCase<I> {
    pub fn new(importer: I) -> Self {
        Self { importer }
    }

    pub fn execute(&self, input: MarkdownToDocumentInput) -> Result<Document, MarkdownToDocumentError> {
        let frontmatter = input.frontmatter.unwrap_or_default();

        self.importer
            .import(&input.markdown_body, &frontmatter)
            .map_err(MarkdownToDocumentError::Import)
    }
}
