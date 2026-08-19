use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntryId(pub String);

impl Deref for EntryId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for EntryId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: EntryId,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub content: Vec<Node>,
    pub schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub node_type: String,
    pub content: Option<Vec<Node>>,
    pub marks: Option<Vec<Mark>>,
    pub text: Option<String>,
    pub attrs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub mark_type: String,
    pub attrs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    pub schema_version: u32,
    pub entry_id: String,
    pub generated_at: String,
    pub blocks: Vec<Block>,
    pub attachments: Option<Vec<Attachment>>,
    pub revisions: Option<Vec<Revision>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub anchor: String,
    pub block_type: String,
    pub comment: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub widget: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub path: String,
    pub mime_type: String,
    pub alt_text: Option<String>,
    pub inline_anchor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub revision_id: String,
    pub timestamp: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    InvalidFormat(String),
    MissingContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    EmptyTitle,
}

impl Entry {
    pub fn new(
        id: EntryId,
        title: String,
        body: String,
        tags: Vec<String>,
        created_at: String,
        updated_at: String,
    ) -> Result<Self, DomainError> {
        if title.trim().is_empty() {
            return Err(DomainError::EmptyTitle);
        }

        Ok(Self {
            id,
            title,
            body,
            tags,
            created_at,
            updated_at,
        })
    }
}
