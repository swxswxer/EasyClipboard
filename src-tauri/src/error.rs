use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("requested item was not found")]
    NotFound,
    #[error("one or more referenced files no longer exist")]
    FileMissing,
    #[error("required system permission is not granted")]
    PermissionDenied,
    #[error("the shortcut is already registered by another application")]
    ShortcutConflict,
    #[error("clipboard content exceeds the configured MVP limit")]
    ContentTooLarge,
    #[error("the system clipboard is unavailable")]
    ClipboardUnavailable,
    #[cfg(target_os = "windows")]
    #[error("the system clipboard is currently in use by another application")]
    ClipboardBusy,
    #[cfg(target_os = "windows")]
    #[error("failed to write data to the system clipboard")]
    ClipboardWriteFailed,
    #[error("no target application is available for paste")]
    PasteTargetMissing,
    #[error("local storage operation failed: {0}")]
    Storage(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::FileMissing => "file_missing",
            Self::PermissionDenied => "permission_denied",
            Self::ShortcutConflict => "shortcut_conflict",
            Self::ContentTooLarge => "content_too_large",
            Self::ClipboardUnavailable => "clipboard_unavailable",
            #[cfg(target_os = "windows")]
            Self::ClipboardBusy => "clipboard_busy",
            #[cfg(target_os = "windows")]
            Self::ClipboardWriteFailed => "clipboard_write_failed",
            Self::PasteTargetMissing => "paste_target_missing",
            Self::Storage(_) => "storage_error",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializedError<'a> {
    code: &'a str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        SerializedError {
            code: self.code(),
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

impl From<tokio_rusqlite::Error> for AppError {
    fn from(value: tokio_rusqlite::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<tokio_rusqlite::rusqlite::Error> for AppError {
    fn from(value: tokio_rusqlite::rusqlite::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Storage(value.to_string())
    }
}
