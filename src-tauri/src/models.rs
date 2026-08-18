use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardKind {
    Text,
    Image,
    Files,
}

impl ClipboardKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Files => "files",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardItemSummary {
    pub id: String,
    pub kind: ClipboardKind,
    pub title: String,
    pub source_name: String,
    pub source_app_id: Option<String>,
    pub copied_at: String,
    pub byte_size: u64,
    pub pinned: bool,
    pub group_id: Option<String>,
    pub retained: bool,
    pub missing_files: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardItemDetail {
    #[serde(flatten)]
    pub summary: ClipboardItemSummary,
    pub content: String,
    pub preview_data_url: Option<String>,
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardPage {
    pub items: Vec<ClipboardItemSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedApp {
    pub name: String,
    #[serde(alias = "bundleId")]
    pub identifier: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub shortcut: String,
    pub launch_at_login: bool,
    pub recording_paused: bool,
    pub max_items: u32,
    pub retention_days: u32,
    pub excluded_apps: Vec<ExcludedApp>,
}

impl Default for Settings {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let excluded_apps = vec![
            ExcludedApp {
                name: "EasyClipboard".into(),
                identifier: "com.easyclipboard.desktop".into(),
            },
            ExcludedApp {
                name: "1Password".into(),
                identifier: "com.1password.1password".into(),
            },
            ExcludedApp {
                name: "Bitwarden".into(),
                identifier: "com.bitwarden.desktop".into(),
            },
            ExcludedApp {
                name: "KeePassXC".into(),
                identifier: "org.keepassxc.keepassxc".into(),
            },
            ExcludedApp {
                name: "Passwords".into(),
                identifier: "com.apple.Passwords".into(),
            },
        ];
        #[cfg(target_os = "windows")]
        let excluded_apps = vec![];
        Self {
            shortcut: crate::platform::DEFAULT_SHORTCUT.into(),
            launch_at_login: false,
            recording_paused: false,
            max_items: 500,
            retention_days: 30,
            excluded_apps,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopCapabilities {
    pub platform: String,
    pub clipboard_access: String,
    pub paste_automation: String,
    pub supports_app_exclusions: bool,
}

#[derive(Clone, Debug)]
pub struct CapturedClipboard {
    pub kind: ClipboardKind,
    pub title: String,
    pub content: String,
    pub html: Option<Vec<u8>>,
    pub rtf: Option<Vec<u8>>,
    pub image_png: Option<Vec<u8>>,
    pub files: Vec<String>,
    pub source_name: String,
    pub source_app_id: Option<String>,
    pub byte_size: u64,
    pub hash: String,
}

#[cfg(test)]
mod tests {
    use super::{ExcludedApp, Settings};

    #[test]
    fn legacy_auto_paste_setting_is_ignored() {
        let legacy = r#"{
            "shortcut":"Command+Shift+V",
            "autoPasteEnabled":false,
            "launchAtLogin":false,
            "recordingPaused":false,
            "maxItems":500,
            "retentionDays":30,
            "excludedApps":[]
        }"#;
        let settings: Settings = serde_json::from_str(legacy).unwrap();
        assert_eq!(settings.shortcut, crate::platform::DEFAULT_SHORTCUT);
        assert_eq!(
            serde_json::to_value(settings).unwrap()["autoPasteEnabled"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn legacy_bundle_id_deserializes_as_a_platform_identifier() {
        let app: ExcludedApp =
            serde_json::from_str(r#"{"name":"Legacy App","bundleId":"com.example.legacy"}"#)
                .unwrap();
        assert_eq!(app.identifier, "com.example.legacy");
    }
}
