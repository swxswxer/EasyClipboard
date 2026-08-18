use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct TargetApplication {
    pub pid: u32,
    pub name: String,
    pub identifier: Option<String>,
    #[cfg(target_os = "windows")]
    pub window_handle: isize,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManualPasteReason {
    ElevatedTarget,
    FocusDenied,
    InputBlocked,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PasteMode {
    Pasted,
    ManualRequired,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PasteOutcome {
    pub mode: PasteMode,
    pub reason: Option<ManualPasteReason>,
}

impl PasteOutcome {
    pub fn pasted() -> Self {
        Self {
            mode: PasteMode::Pasted,
            reason: None,
        }
    }

    #[cfg(target_os = "windows")]
    pub fn manual(reason: ManualPasteReason) -> Self {
        Self {
            mode: PasteMode::ManualRequired,
            reason: Some(reason),
        }
    }
}
