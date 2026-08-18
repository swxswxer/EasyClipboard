use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

use crate::{
    error::AppError,
    models::ExcludedApp,
    platform::{PasteOutcome, TargetApplication},
};

pub fn frontmost_application() -> Option<TargetApplication> {
    autoreleasepool(|_| {
        let application = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        Some(TargetApplication {
            pid: application.processIdentifier() as u32,
            name: application
                .localizedName()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "未知应用".into()),
            identifier: application
                .bundleIdentifier()
                .map(|value| value.to_string()),
        })
    })
}

pub fn activate_and_paste(target: &TargetApplication) -> Result<PasteOutcome, AppError> {
    let activated = autoreleasepool(|_| {
        NSRunningApplication::runningApplicationWithProcessIdentifier(target.pid as i32)
            .is_some_and(|application| {
                application.activateWithOptions(NSApplicationActivationOptions::empty())
            })
    });
    if !activated {
        return Err(AppError::ClipboardUnavailable);
    }

    let down_source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| AppError::ClipboardUnavailable)?;
    let up_source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| AppError::ClipboardUnavailable)?;
    let down = CGEvent::new_keyboard_event(down_source, 9, true)
        .map_err(|_| AppError::ClipboardUnavailable)?;
    let up = CGEvent::new_keyboard_event(up_source, 9, false)
        .map_err(|_| AppError::ClipboardUnavailable)?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);
    up.post(CGEventTapLocation::HID);
    Ok(PasteOutcome::pasted())
}

pub async fn open_excluded_app_picker() -> Result<Option<ExcludedApp>, AppError> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("macOS 应用", &["app"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = handle.path();
    let value = plist::Value::from_file(path.join("Contents/Info.plist"))
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let dictionary = value
        .as_dictionary()
        .ok_or_else(|| AppError::Storage("invalid application Info.plist".into()))?;
    let identifier = dictionary
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
        identifier: identifier.into(),
    }))
}
