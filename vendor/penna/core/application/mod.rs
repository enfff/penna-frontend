pub mod create_entry;
pub mod delete_entry;
pub mod list_entries;
pub mod get_entry;
pub mod update_entry;
pub mod markdown_to_document;
pub mod document_to_markdown;
pub mod document_with_sidecar;
pub mod list_tags;
pub mod add_tag;
pub mod remove_tag;
pub mod update_tag;
pub mod sync_journal;
pub mod clone_journal;
pub mod resolve_journal_path;
pub mod pull_journal;
pub mod push_journal;
pub mod validate_sidecar_integrity;

pub use create_entry::{CreateEntryError, CreateEntryInput, CreateEntryUseCase};
pub use delete_entry::DeleteEntryUseCase;
pub use list_entries::ListEntriesUseCase;
pub use get_entry::GetEntryUseCase;
pub use update_entry::{UpdateEntryError, UpdateEntryInput, UpdateEntryUseCase};
pub use markdown_to_document::{
    MarkdownToDocumentError, MarkdownToDocumentInput, MarkdownToDocumentUseCase,
};
pub use document_to_markdown::{
    DocumentToMarkdownError, DocumentToMarkdownUseCase,
};
pub use document_with_sidecar::{
    DocumentWithSidecarError, DocumentWithSidecarInput, DocumentWithSidecarUseCase,
};
pub use list_tags::ListTagsUseCase;
pub use add_tag::{AddTagError, AddTagUseCase};
pub use remove_tag::{RemoveTagError, RemoveTagUseCase};
pub use update_tag::{UpdateTagError, UpdateTagUseCase};
pub use clone_journal::CloneJournalUseCase;
pub use resolve_journal_path::ResolveJournalPathUseCase;
pub use pull_journal::PullJournalUseCase;
pub use push_journal::PushJournalUseCase;
pub use sync_journal::SyncJournalUseCase;
pub use validate_sidecar_integrity::{
    SidecarIntegrityStatus, SidecarSource, ValidateSidecarIntegrityUseCase,
};
