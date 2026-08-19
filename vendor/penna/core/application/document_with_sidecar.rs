use crate::domain::{Document, Sidecar};
use crate::ports::MarkdownExporter;
use std::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentWithSidecarInput {
    pub document: Document,
    pub sidecar: Option<Sidecar>,
    pub include_sidecar: bool,
}

#[derive(Debug)]
pub enum DocumentWithSidecarError {
    Export(Box<dyn Error + Send + Sync>),
}

pub struct DocumentWithSidecarUseCase<E: MarkdownExporter> {
    exporter: E,
}

impl<E: MarkdownExporter> DocumentWithSidecarUseCase<E> {
    pub fn new(exporter: E) -> Self {
        Self { exporter }
    }

    pub fn execute(
        &self,
        input: DocumentWithSidecarInput,
    ) -> Result<(String, Option<String>), DocumentWithSidecarError> {
        self.exporter
            .export_with_sidecar(&input.document, input.sidecar.as_ref(), input.include_sidecar)
            .map_err(DocumentWithSidecarError::Export)
    }
}
