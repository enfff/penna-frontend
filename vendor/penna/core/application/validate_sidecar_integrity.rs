use crate::domain::Sidecar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarSource {
    Missing,
    Present(Sidecar),
    Malformed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarIntegrityStatus {
    Ok,
    Missing,
    Mismatch {
        expected_entry_id: String,
        actual_entry_id: String,
    },
    Malformed {
        reason: String,
    },
}

pub struct ValidateSidecarIntegrityUseCase;

impl ValidateSidecarIntegrityUseCase {
    pub fn new() -> Self {
        Self
    }

    pub fn execute(&self, entry_id: &str, source: SidecarSource) -> SidecarIntegrityStatus {
        match source {
            SidecarSource::Missing => SidecarIntegrityStatus::Missing,
            SidecarSource::Malformed(reason) => SidecarIntegrityStatus::Malformed { reason },
            SidecarSource::Present(sidecar) => {
                if sidecar.entry_id == entry_id {
                    SidecarIntegrityStatus::Ok
                } else {
                    SidecarIntegrityStatus::Mismatch {
                        expected_entry_id: entry_id.to_string(),
                        actual_entry_id: sidecar.entry_id,
                    }
                }
            }
        }
    }
}

impl Default for ValidateSidecarIntegrityUseCase {
    fn default() -> Self {
        Self::new()
    }
}
