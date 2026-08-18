use objc2::rc::autoreleasepool;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};

pub fn paste_automation_ready() -> bool {
    macos_accessibility_client::accessibility::application_is_trusted()
}

pub fn request_paste_automation() -> bool {
    macos_accessibility_client::accessibility::application_is_trusted_with_prompt()
}

pub fn open_paste_automation_settings() -> bool {
    autoreleasepool(|_| {
        let value = NSString::from_str(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        );
        NSURL::URLWithString(&value).is_some_and(|url| NSWorkspace::sharedWorkspace().openURL(&url))
    })
}
