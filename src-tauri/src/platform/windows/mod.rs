mod clipboard;
mod desktop;
mod permissions;

pub use clipboard::{change_token, install_clipboard_listener, read_capture, write_item};
pub use desktop::{activate_and_paste, frontmost_application};
pub use permissions::{
    open_paste_automation_settings, paste_automation_ready, request_paste_automation,
};

use tauri::{
    utils::config::WindowEffectsConfig,
    window::{Effect, EffectsBuilder},
};

use crate::{
    error::AppError,
    models::{ExcludedApp, Settings},
};

pub const PLATFORM_NAME: &str = "windows";
pub const DEFAULT_SHORTCUT: &str = "Control+Shift+V";
pub const SUPPORTS_APP_EXCLUSIONS: bool = false;
pub const RECORDING_STARTS_AUTOMATICALLY: bool = true;

pub fn window_effects() -> WindowEffectsConfig {
    EffectsBuilder::new().effect(Effect::Acrylic).build()
}

pub fn source_is_excluded(_settings: &Settings, _identifier: Option<&str>) -> bool {
    false
}

pub fn sanitize_settings(mut settings: Settings) -> Settings {
    settings.excluded_apps.clear();
    settings
}

pub fn is_own_identifier(identifier: Option<&str>) -> bool {
    identifier.is_some_and(|value| value.eq_ignore_ascii_case("easyclipboard.exe"))
}

pub async fn open_excluded_app_picker() -> Result<Option<ExcludedApp>, AppError> {
    Ok(None)
}
