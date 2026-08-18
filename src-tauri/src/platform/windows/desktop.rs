use std::{mem::size_of, path::Path, thread, time::Duration};

use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND},
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        System::Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
            PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::{
            Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL,
                VK_V,
            },
            WindowsAndMessaging::{
                GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
                GetWindowThreadProcessId, SetForegroundWindow,
            },
        },
    },
};

use crate::{
    error::AppError,
    platform::{ManualPasteReason, PasteOutcome, TargetApplication},
};

pub fn frontmost_application() -> Option<TargetApplication> {
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        if GetWindowThreadProcessId(window, Some(&mut pid)) == 0 || pid == 0 {
            return None;
        }
        let executable = process_path(pid);
        let identifier = executable
            .as_ref()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase);
        let process_name = executable
            .as_ref()
            .and_then(|path| Path::new(path).file_stem())
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        let title_length = GetWindowTextLengthW(window).max(0) as usize;
        let mut title_buffer = vec![0u16; title_length + 1];
        let title_written = GetWindowTextW(window, &mut title_buffer).max(0) as usize;
        let title = String::from_utf16_lossy(&title_buffer[..title_written]);
        Some(TargetApplication {
            pid,
            name: process_name
                .filter(|name| !name.is_empty())
                .or_else(|| (!title.is_empty()).then_some(title))
                .unwrap_or_else(|| "未知应用".into()),
            identifier,
            window_handle: window.0 as isize,
        })
    }
}

pub fn activate_and_paste(target: &TargetApplication) -> Result<PasteOutcome, AppError> {
    unsafe {
        let target_elevated = process_is_elevated(target.pid).unwrap_or(true);
        let current_elevated = token_is_elevated(GetCurrentProcess()).unwrap_or(false);
        if target_elevated && !current_elevated {
            return Ok(PasteOutcome::manual(ManualPasteReason::ElevatedTarget));
        }
        let window = HWND(target.window_handle as *mut core::ffi::c_void);
        if !SetForegroundWindow(window).as_bool() {
            return Ok(PasteOutcome::manual(ManualPasteReason::FocusDenied));
        }
        thread::sleep(Duration::from_millis(50));
        let inputs = [
            keyboard_input(VK_CONTROL, false),
            keyboard_input(VK_V, false),
            keyboard_input(VK_V, true),
            keyboard_input(VK_CONTROL, true),
        ];
        if SendInput(&inputs, size_of::<INPUT>() as i32) != inputs.len() as u32 {
            return Ok(PasteOutcome::manual(ManualPasteReason::InputBlocked));
        }
        Ok(PasteOutcome::pasted())
    }
}

fn keyboard_input(
    key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    up: bool,
) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    }
}

unsafe fn process_path(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT::default(),
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

unsafe fn process_is_elevated(pid: u32) -> Option<bool> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let result = unsafe { token_is_elevated(process) };
    let _ = unsafe { CloseHandle(process) };
    result
}

unsafe fn token_is_elevated(process: HANDLE) -> Option<bool> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    };
    let _ = unsafe { CloseHandle(token) };
    result.ok()?;
    Some(elevation.TokenIsElevated != 0)
}
