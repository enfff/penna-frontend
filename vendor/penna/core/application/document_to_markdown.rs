use crate::domain::Document;
use crate::ports::MarkdownExporter;
use std::error::Error;

#[derive(Debug)]
pub enum DocumentToMarkdownError {
    Export(Box<dyn Error + Send + Sync>),
}

pub struct DocumentToMarkdownUseCase<E: MarkdownExporter> {
    exporter: E,
}

impl<E: MarkdownExporter> DocumentToMarkdownUseCase<E> {
    pub fn new(exporter: E) -> Self {
        Self { exporter }
    }

    pub fn execute(&self, document: &Document) -> Result<String, DocumentToMarkdownError> {
        self.exporter
            .export(document)
            .map_err(DocumentToMarkdownError::Export)
    }
}
