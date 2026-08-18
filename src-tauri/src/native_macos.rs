use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use objc2_foundation::{NSString, NSURL};

#[derive(Clone, Debug)]
pub struct FrontmostApp {
    pub pid: i32,
    pub name: String,
    pub bundle_id: Option<String>,
}

pub fn frontmost_app() -> Option<FrontmostApp> {
    autoreleasepool(|_| {
        let application = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        Some(FrontmostApp {
            pid: application.processIdentifier(),
            name: application
                .localizedName()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "未知应用".into()),
            bundle_id: application
                .bundleIdentifier()
                .map(|value| value.to_string()),
        })
    })
}

pub fn activate_application(pid: i32) -> bool {
    autoreleasepool(|_| {
        NSRunningApplication::runningApplicationWithProcessIdentifier(pid).is_some_and(
            |application| application.activateWithOptions(NSApplicationActivationOptions::empty()),
        )
    })
}

pub fn send_command_v() -> bool {
    let Ok(down_source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return false;
    };
    let Ok(up_source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return false;
    };
    let Ok(down) = CGEvent::new_keyboard_event(down_source, 9, true) else {
        return false;
    };
    let Ok(up) = CGEvent::new_keyboard_event(up_source, 9, false) else {
        return false;
    };
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);
    up.post(CGEventTapLocation::HID);
    true
}

pub fn accessibility_trusted(prompt: bool) -> bool {
    if prompt {
        macos_accessibility_client::accessibility::application_is_trusted_with_prompt()
    } else {
        macos_accessibility_client::accessibility::application_is_trusted()
    }
}

pub fn open_accessibility_settings() -> bool {
    autoreleasepool(|_| {
        let value = NSString::from_str(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        );
        NSURL::URLWithString(&value).is_some_and(|url| NSWorkspace::sharedWorkspace().openURL(&url))
    })
}
