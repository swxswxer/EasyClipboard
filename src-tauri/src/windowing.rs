use tauri::{
    window::{Effect, EffectState, EffectsBuilder},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

use crate::{app_state::AppState, error::AppError, native_macos};

const APP_BUNDLE_ID: &str = "com.easyclipboard.desktop";

pub fn toggle_clipboard(app: &AppHandle) -> Result<(), AppError> {
    let window = app
        .get_webview_window("clipboard")
        .ok_or(AppError::ClipboardUnavailable)?;
    if window
        .is_visible()
        .map_err(|_| AppError::ClipboardUnavailable)?
    {
        window.hide().map_err(|_| AppError::ClipboardUnavailable)
    } else {
        show_clipboard(app)
    }
}

pub fn show_clipboard(app: &AppHandle) -> Result<(), AppError> {
    let window = app
        .get_webview_window("clipboard")
        .ok_or(AppError::ClipboardUnavailable)?;

    if let Some(frontmost) = native_macos::frontmost_app() {
        if frontmost.bundle_id.as_deref() != Some(APP_BUNDLE_ID) {
            if let Ok(mut runtime) = app.state::<AppState>().runtime.lock() {
                runtime.target_app = Some(frontmost);
            }
        }
    }

    let cursor = app
        .cursor_position()
        .map_err(|_| AppError::ClipboardUnavailable)?;
    let monitor = window
        .monitor_from_point(cursor.x, cursor.y)
        .map_err(|_| AppError::ClipboardUnavailable)?
        .or_else(|| window.current_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let work = monitor.work_area();
        let horizontal_padding = (40.0 * scale) as u32;
        let vertical_padding = (116.0 * scale) as u32;
        let width = (1040.0 * scale) as u32;
        let height = (466.0 * scale) as u32;
        let width = width.min(work.size.width.saturating_sub(horizontal_padding));
        let height = height.min(work.size.height.saturating_sub(vertical_padding));
        let bottom = if work.size.height < (720.0 * scale) as u32 {
            (36.0 * scale) as i32
        } else {
            (56.0 * scale) as i32
        };
        let x = work.position.x + (work.size.width.saturating_sub(width) / 2) as i32;
        let y = work.position.y + work.size.height as i32 - height as i32 - bottom;
        window
            .set_size(PhysicalSize::new(width, height))
            .map_err(|_| AppError::ClipboardUnavailable)?;
        window
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|_| AppError::ClipboardUnavailable)?;
    }
    window.show().map_err(|_| AppError::ClipboardUnavailable)?;
    window
        .set_focus()
        .map_err(|_| AppError::ClipboardUnavailable)?;
    let _ = window.emit("clipboard://shown", ());
    Ok(())
}

pub fn hide_clipboard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("clipboard") {
        let _ = window.hide();
    }
}

pub fn open_settings(app: &AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let effects = EffectsBuilder::new()
        .effect(Effect::HudWindow)
        .state(EffectState::Active)
        .radius(18.0)
        .build();
    WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App("index.html?window=settings".into()),
    )
    .title("EasyClipboard 设置")
    .inner_size(680.0, 640.0)
    .min_inner_size(620.0, 560.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(true)
    .effects(effects)
    .center()
    .build()
    .map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(())
}

pub fn close_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.close();
    }
}
