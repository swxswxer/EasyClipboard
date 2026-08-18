use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::GlobalShortcutExt as _;

use crate::{
    app_state::AppState,
    clipboard,
    error::AppError,
    models::{ClipboardItemDetail, ClipboardPage, ExcludedApp, Group, PermissionState, Settings},
    native_macos, windowing,
};

fn validate_paste_target(
    accessibility_granted: bool,
    target: Option<native_macos::FrontmostApp>,
) -> Result<native_macos::FrontmostApp, AppError> {
    if !accessibility_granted {
        return Err(AppError::PermissionDenied);
    }
    target.ok_or(AppError::ClipboardUnavailable)
}

fn clipboard_permission(state: &AppState) -> String {
    state
        .runtime
        .lock()
        .map(|runtime| {
            if runtime.clipboard_started {
                "authorized"
            } else {
                "not_requested"
            }
        })
        .unwrap_or("denied")
        .to_owned()
}

#[tauri::command]
pub async fn list_items(
    state: State<'_, AppState>,
    query: String,
    group_id: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
) -> Result<ClipboardPage, AppError> {
    state
        .database
        .list_items(query, group_id, cursor, limit.unwrap_or(100))
        .await
}

#[tauri::command]
pub async fn get_item(
    state: State<'_, AppState>,
    id: String,
) -> Result<ClipboardItemDetail, AppError> {
    state.database.get_item(id).await
}

#[tauri::command]
pub async fn paste_item(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    let target = state
        .runtime
        .lock()
        .map_err(|_| AppError::ClipboardUnavailable)?
        .target_app
        .clone();
    let target = validate_paste_target(native_macos::accessibility_trusted(false), target)?;
    let item = state.database.get_item(id.clone()).await?;
    let (html, rtf) = state.database.text_representations(id.clone()).await?;
    let image = if matches!(item.summary.kind, crate::models::ClipboardKind::Image) {
        Some(state.database.original_image(id.clone()).await?)
    } else {
        None
    };
    let receipt = clipboard::write_item(&item, image.as_deref(), html.as_deref(), rtf.as_deref())?;
    {
        let mut runtime = state
            .runtime
            .lock()
            .map_err(|_| AppError::ClipboardUnavailable)?;
        runtime.last_change_count = receipt.change_count;
        runtime.suppress_change_count = Some(receipt.change_count);
        runtime.suppress_until = Instant::now() + Duration::from_secs(1);
        runtime.expected_hash = Some(receipt.content_hash);
    }
    windowing::hide_clipboard(&app);
    if !native_macos::activate_application(target.pid) {
        let _ = windowing::show_clipboard(&app);
        return Err(AppError::ClipboardUnavailable);
    }
    tokio::time::sleep(Duration::from_millis(85)).await;
    if !native_macos::send_command_v() {
        let _ = windowing::show_clipboard(&app);
        return Err(AppError::ClipboardUnavailable);
    }
    state.database.touch_item(id).await?;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_item(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    state.database.delete_item(id).await?;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn clear_recent(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state.database.clear_recent().await?;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_all_data(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state.database.delete_all_data().await?;
    if app.autolaunch().is_enabled().unwrap_or(false) {
        let _ = app.autolaunch().disable();
    }
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    pinned: bool,
) -> Result<(), AppError> {
    state.database.set_pinned(id, pinned).await?;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn list_groups(state: State<'_, AppState>) -> Result<Vec<Group>, AppError> {
    state.database.list_groups().await
}

#[tauri::command]
pub async fn create_group(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<Group, AppError> {
    let group = state.database.create_group(name).await?;
    emit_changed(&app);
    Ok(group)
}

#[tauri::command]
pub async fn rename_group(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), AppError> {
    state.database.rename_group(id, name).await?;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_group(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    state.database.delete_group(id).await?;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn move_item(
    app: AppHandle,
    state: State<'_, AppState>,
    item_id: String,
    group_id: Option<String>,
) -> Result<(), AppError> {
    state.database.move_item(item_id, group_id).await?;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    state.database.get_settings().await
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, AppError> {
    if settings.launch_at_login {
        app.autolaunch()
            .enable()
            .map_err(|error| AppError::Storage(error.to_string()))?;
    } else {
        app.autolaunch()
            .disable()
            .map_err(|error| AppError::Storage(error.to_string()))?;
    }
    let result = state.database.save_settings(settings).await?;
    let _ = app.emit("settings://changed", &result);
    Ok(result)
}

#[tauri::command]
pub async fn set_global_shortcut(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<Settings, AppError> {
    let mut settings = state.database.get_settings().await?;
    let previous = settings.shortcut.clone();
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| AppError::Storage(error.to_string()))?;
    if app.global_shortcut().register(shortcut.as_str()).is_err() {
        let _ = app.global_shortcut().register(previous.as_str());
        return Err(AppError::ShortcutConflict);
    }
    settings.shortcut = shortcut;
    state.database.save_settings(settings).await
}

#[tauri::command]
pub fn get_permission_state(state: State<'_, AppState>) -> PermissionState {
    PermissionState {
        clipboard: clipboard_permission(&state),
        accessibility: native_macos::accessibility_trusted(false),
    }
}

#[tauri::command]
pub fn start_recording(state: State<'_, AppState>) -> Result<PermissionState, AppError> {
    if !native_macos::accessibility_trusted(false) {
        return Err(AppError::PermissionDenied);
    }
    let source = native_macos::frontmost_app().unwrap_or(native_macos::FrontmostApp {
        pid: 0,
        name: "EasyClipboard".into(),
        bundle_id: Some("com.easyclipboard.desktop".into()),
    });
    let _ = clipboard::read_capture(source);
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| AppError::ClipboardUnavailable)?;
    runtime.clipboard_started = true;
    runtime.last_change_count = clipboard::change_count();
    Ok(PermissionState {
        clipboard: "authorized".into(),
        accessibility: native_macos::accessibility_trusted(false),
    })
}

#[tauri::command]
pub fn request_accessibility(state: State<'_, AppState>) -> PermissionState {
    PermissionState {
        clipboard: clipboard_permission(&state),
        accessibility: native_macos::accessibility_trusted(true),
    }
}

#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), AppError> {
    if native_macos::open_accessibility_settings() {
        Ok(())
    } else {
        Err(AppError::PermissionDenied)
    }
}

#[tauri::command]
pub async fn pick_excluded_app() -> Result<Option<ExcludedApp>, AppError> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("macOS 应用", &["app"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = handle.path();
    let plist_path = path.join("Contents/Info.plist");
    let value = plist::Value::from_file(&plist_path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let dictionary = value
        .as_dictionary()
        .ok_or_else(|| AppError::Storage("invalid application Info.plist".into()))?;
    let bundle_id = dictionary
        .get("CFBundleIdentifier")
        .and_then(plist::Value::as_string)
        .ok_or_else(|| AppError::Storage("missing bundle identifier".into()))?;
    let name = dictionary
        .get("CFBundleDisplayName")
        .or_else(|| dictionary.get("CFBundleName"))
        .and_then(plist::Value::as_string)
        .map(str::to_owned)
        .or_else(|| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "macOS 应用".into());
    Ok(Some(ExcludedApp {
        name,
        bundle_id: bundle_id.into(),
    }))
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    windowing::hide_clipboard(&app);
}

#[tauri::command]
pub fn close_settings(app: AppHandle) {
    windowing::close_settings(&app);
}

fn emit_changed(app: &AppHandle) {
    let _ = app.emit("clipboard://changed", ());
}

#[cfg(test)]
mod tests {
    use super::validate_paste_target;
    use crate::{error::AppError, native_macos::FrontmostApp};

    fn target() -> FrontmostApp {
        FrontmostApp {
            pid: 42,
            name: "Target".into(),
            bundle_id: Some("com.example.target".into()),
        }
    }

    #[test]
    fn paste_preconditions_reject_missing_accessibility_before_any_paste_work() {
        assert!(matches!(
            validate_paste_target(false, Some(target())),
            Err(AppError::PermissionDenied)
        ));
    }

    #[test]
    fn paste_preconditions_require_a_target_application() {
        assert!(matches!(
            validate_paste_target(true, None),
            Err(AppError::ClipboardUnavailable)
        ));
    }
}
