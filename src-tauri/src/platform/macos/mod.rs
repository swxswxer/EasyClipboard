mod clipboard;
mod desktop;
mod permissions;

pub use clipboard::{change_token, read_capture, write_item};
pub use desktop::{activate_and_paste, frontmost_application, open_excluded_app_picker};
pub use permissions::{
    open_paste_automation_settings, paste_automation_ready, request_paste_automation,
};

use tauri::{
    utils::config::WindowEffectsConfig,
    window::{Effect, EffectState, EffectsBuilder},
};

use crate::models::Settings;

pub const PLATFORM_NAME: &str = "macos";
pub const DEFAULT_SHORTCUT: &str = "Command+Shift+V";
pub const SUPPORTS_APP_EXCLUSIONS: bool = true;
pub const RECORDING_STARTS_AUTOMATICALLY: bool = false;

pub fn install_clipboard_listener(
    _sender: tokio::sync::mpsc::Sender<()>,
) -> Result<(), crate::error::AppError> {
    Ok(())
}

pub fn window_effects() -> WindowEffectsConfig {
    EffectsBuilder::new()
        .effect(Effect::HudWindow)
        .state(EffectState::Active)
        .radius(18.0)
        .build()
}

pub fn source_is_excluded(settings: &Settings, identifier: Option<&str>) -> bool {
    identifier.is_some_and(|identifier| {
        settings
            .excluded_apps
            .iter()
            .any(|app| app.identifier == identifier)
    })
}

pub fn sanitize_settings(settings: Settings) -> Settings {
    settings
}

pub fn is_own_identifier(identifier: Option<&str>) -> bool {
    identifier == Some("com.easyclipboard.desktop")
}
