use std::{
    path::Path,
    ptr::copy_nonoverlapping,
    slice,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use image::{DynamicImage, ImageFormat, RgbaImage};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::Sender;
use windows::{
    core::w,
    Win32::{
        Foundation::{GlobalFree, HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            DataExchange::{
                AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
                GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
                RegisterClipboardFormatW, RemoveClipboardFormatListener, SetClipboardData,
            },
            LibraryLoader::GetModuleHandleW,
            Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE},
        },
        UI::{
            Shell::{DragQueryFileW, HDROP},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
                GetOpenClipboardWindow, GetWindowThreadProcessId, RegisterClassW, TranslateMessage,
                CS_HREDRAW, CS_VREDRAW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_CLIPBOARDUPDATE, WNDCLASSW,
            },
        },
    },
};

use crate::{
    domain::clipboard::{
        file_title, hash_bytes, hash_files, normalize_image, normalize_single_image_file,
        text_title, WriteReceipt, DECODED_IMAGE_LIMIT, FILE_LIMIT, TEXT_LIMIT,
    },
    error::AppError,
    models::{CapturedClipboard, ClipboardItemDetail, ClipboardKind},
    platform::TargetApplication,
};

const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;
const CF_HDROP: u32 = 15;
const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const DIBV5_HEADER_SIZE: usize = 124;

static CHANGE_SENDER: OnceLock<Mutex<Sender<()>>> = OnceLock::new();

struct ClipboardGuard;

impl ClipboardGuard {
    fn open(owner: Option<HWND>) -> Result<Self, AppError> {
        let mut last_error = None;
        for delay in [0, 10, 20, 40, 80, 120, 160, 200] {
            if delay > 0 {
                thread::sleep(Duration::from_millis(delay));
            }
            match unsafe { OpenClipboard(owner) } {
                Ok(()) => {
                    return Ok(Self);
                }
                Err(error) => last_error = Some(error.code().0),
            }
        }
        let holder = unsafe { GetOpenClipboardWindow() };
        let mut holder_pid = 0;
        if !holder.0.is_null() {
            unsafe { GetWindowThreadProcessId(holder, Some(&mut holder_pid)) };
        }
        log::warn!(
            "windows clipboard open failed error_code={} holder_pid={holder_pid}",
            last_error.unwrap_or_default()
        );
        Err(AppError::ClipboardBusy)
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn change_token() -> i64 {
    unsafe { GetClipboardSequenceNumber() as i64 }
}

pub fn install_clipboard_listener(sender: Sender<()>) -> Result<(), AppError> {
    CHANGE_SENDER
        .set(Mutex::new(sender))
        .map_err(|_| AppError::ClipboardUnavailable)?;
    thread::Builder::new()
        .name("easyclipboard-win32-listener".into())
        .spawn(|| unsafe {
            let class_name = w!("EasyClipboardMessageWindow");
            let Ok(module) = GetModuleHandleW(None) else {
                log::warn!("failed to resolve the EasyClipboard module handle");
                return;
            };
            let instance = HINSTANCE(module.0);
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: class_name,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                log::warn!("failed to register clipboard listener window class");
                return;
            }
            let window = match CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("EasyClipboard clipboard listener"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                None,
            ) {
                Ok(window) => window,
                Err(error) => {
                    log::warn!(
                        "clipboard listener window creation failed error_code={}",
                        error.code().0
                    );
                    return;
                }
            };
            if let Err(error) = AddClipboardFormatListener(window) {
                log::warn!(
                    "clipboard listener registration failed error_code={}",
                    error.code().0
                );
                return;
            }
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let _ = RemoveClipboardFormatListener(window);
        })
        .map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CLIPBOARDUPDATE {
        if let Some(sender) = CHANGE_SENDER.get().and_then(|sender| sender.lock().ok()) {
            let _ = sender.try_send(());
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

pub fn read_capture(source: TargetApplication) -> Result<Option<CapturedClipboard>, AppError> {
    let _guard = ClipboardGuard::open(None)?;
    if clipboard_is_sensitive()? {
        return Ok(None);
    }

    let files = if format_available(CF_HDROP) {
        read_files()?
    } else {
        vec![]
    };
    if !files.is_empty() {
        if files.len() > FILE_LIMIT {
            return Err(AppError::ContentTooLarge);
        }
        if files.len() == 1 {
            let normalized = read_available_image()
                .as_deref()
                .and_then(|bytes| normalize_image(bytes).ok())
                .or_else(|| normalize_single_image_file(&files));
            if let Some((png, width, height)) = normalized {
                let hash = hash_bytes(b"image\0", &png);
                let byte_size = png.len() as u64;
                return Ok(Some(CapturedClipboard {
                    kind: ClipboardKind::Image,
                    title: format!("{} · {width} × {height}", file_title(&files)),
                    content: String::new(),
                    html: None,
                    rtf: None,
                    image_png: Some(png),
                    hash,
                    files,
                    source_name: source.name,
                    source_app_id: source.identifier,
                    byte_size,
                }));
            }
        }
        let byte_size = files
            .iter()
            .filter_map(|file| std::fs::metadata(file).ok())
            .map(|metadata| metadata.len())
            .sum();
        return Ok(Some(CapturedClipboard {
            kind: ClipboardKind::Files,
            title: file_title(&files),
            content: String::new(),
            html: None,
            rtf: None,
            image_png: None,
            hash: hash_files(&files),
            files,
            source_name: source.name,
            source_app_id: source.identifier,
            byte_size,
        }));
    }

    if let Some(image_bytes) = read_available_image() {
        let (png, width, height) = normalize_image(&image_bytes)?;
        let hash = hash_bytes(b"image\0", &png);
        let byte_size = png.len() as u64;
        return Ok(Some(CapturedClipboard {
            kind: ClipboardKind::Image,
            title: format!("图片 · {width} × {height}"),
            content: String::new(),
            html: None,
            rtf: None,
            image_png: Some(png),
            files: vec![],
            source_name: source.name,
            source_app_id: source.identifier,
            byte_size,
            hash,
        }));
    }

    if !format_available(CF_UNICODETEXT) {
        return Ok(None);
    }
    let text = read_unicode_text()?;
    if text.len() > TEXT_LIMIT {
        return Err(AppError::ContentTooLarge);
    }
    let html_format = unsafe { RegisterClipboardFormatW(w!("HTML Format")) };
    let rtf_format = unsafe { RegisterClipboardFormatW(w!("Rich Text Format")) };
    let html = (html_format != 0 && format_available(html_format))
        .then(|| read_global_bytes(html_format).map(|bytes| extract_html(&bytes)))
        .transpose()?;
    let rtf = (rtf_format != 0 && format_available(rtf_format))
        .then(|| read_global_bytes(rtf_format))
        .transpose()?;
    Ok(Some(CapturedClipboard {
        kind: ClipboardKind::Text,
        title: text_title(&text),
        content: text.clone(),
        html,
        rtf,
        image_png: None,
        files: vec![],
        source_name: source.name,
        source_app_id: source.identifier,
        byte_size: text.len() as u64,
        hash: hash_bytes(b"text\0", text.as_bytes()),
    }))
}

fn read_available_image() -> Option<Vec<u8>> {
    let png_format = unsafe { RegisterClipboardFormatW(w!("PNG")) };
    if png_format != 0 && format_available(png_format) {
        match read_global_bytes(png_format) {
            Ok(bytes) => return Some(bytes),
            Err(error) => log::warn!(
                "windows clipboard image read failed format=PNG error_code={}",
                error.code()
            ),
        }
    }
    for format in [CF_DIBV5, CF_DIB] {
        if format_available(format) {
            match read_global_bytes(format).and_then(|bytes| dib_to_png(&bytes)) {
                Ok(bytes) => return Some(bytes),
                Err(error) => log::warn!(
                    "windows clipboard image read failed format={format} error_code={}",
                    error.code()
                ),
            }
        }
    }
    None
}

pub fn write_item(
    app: &AppHandle,
    item: &ClipboardItemDetail,
    image_png: Option<&[u8]>,
    html: Option<&[u8]>,
    rtf: Option<&[u8]>,
) -> Result<WriteReceipt, AppError> {
    if matches!(item.summary.kind, ClipboardKind::Files)
        && item.files.iter().any(|file| !Path::new(file).exists())
    {
        return Err(AppError::FileMissing);
    }
    let owner = app
        .get_webview_window("clipboard")
        .ok_or(AppError::ClipboardWriteFailed)?
        .hwnd()
        .map_err(|error| {
            log::warn!("windows clipboard write failed stage=resolve_owner error={error}");
            AppError::ClipboardWriteFailed
        })?;
    let owner = HWND(owner.0 as *mut core::ffi::c_void);
    let _guard = ClipboardGuard::open(Some(owner))?;
    unsafe { EmptyClipboard() }.map_err(|error| {
        log::warn!(
            "windows clipboard write failed stage=empty error_code={}",
            error.code().0
        );
        AppError::ClipboardWriteFailed
    })?;
    let hash = match item.summary.kind {
        ClipboardKind::Text => {
            set_global_bytes(CF_UNICODETEXT, &encode_unicode_text(&item.content))?;
            if let Some(html) = html {
                let format = unsafe { RegisterClipboardFormatW(w!("HTML Format")) };
                if format != 0 {
                    set_global_bytes(format, &encode_html(html))?;
                }
            }
            if let Some(rtf) = rtf {
                let format = unsafe { RegisterClipboardFormatW(w!("Rich Text Format")) };
                if format != 0 {
                    set_global_bytes(format, rtf)?;
                }
            }
            hash_bytes(b"text\0", item.content.as_bytes())
        }
        ClipboardKind::Image => {
            let png = image_png.ok_or(AppError::NotFound)?;
            let png_format = unsafe { RegisterClipboardFormatW(w!("PNG")) };
            if png_format != 0 {
                set_global_bytes(png_format, png)?;
            }
            let dib = png_to_dibv5(png).map_err(|error| {
                log::warn!(
                    "windows clipboard write failed stage=encode_dib error_code={}",
                    error.code()
                );
                if matches!(&error, AppError::ContentTooLarge) {
                    error
                } else {
                    AppError::ClipboardWriteFailed
                }
            })?;
            set_global_bytes(CF_DIBV5, &dib)?;
            if !item.files.is_empty() && item.files.iter().all(|file| Path::new(file).exists()) {
                set_global_bytes(CF_HDROP, &encode_file_drop(&item.files))?;
            }
            hash_bytes(b"image\0", png)
        }
        ClipboardKind::Files => {
            set_global_bytes(CF_HDROP, &encode_file_drop(&item.files))?;
            hash_files(&item.files)
        }
    };
    Ok(WriteReceipt {
        change_token: change_token(),
        content_hash: hash,
    })
}

fn format_available(format: u32) -> bool {
    unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

fn clipboard_is_sensitive() -> Result<bool, AppError> {
    let excluded =
        unsafe { RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")) };
    if excluded != 0 && format_available(excluded) {
        return Ok(true);
    }
    for name in [
        w!("CanIncludeInClipboardHistory"),
        w!("CanUploadToCloudClipboard"),
    ] {
        let format = unsafe { RegisterClipboardFormatW(name) };
        if format != 0 && format_available(format) {
            let bytes = read_global_bytes(format)?;
            if bytes.get(..4).is_some_and(|value| value == [0, 0, 0, 0]) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn read_global_bytes(format: u32) -> Result<Vec<u8>, AppError> {
    let handle = unsafe { GetClipboardData(format) }.map_err(|_| AppError::ClipboardUnavailable)?;
    let global = HGLOBAL(handle.0);
    let size = unsafe { GlobalSize(global) };
    let pointer = unsafe { GlobalLock(global) };
    if pointer.is_null() || size == 0 {
        return Err(AppError::ClipboardUnavailable);
    }
    let bytes = unsafe { slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec();
    let _ = unsafe { GlobalUnlock(global) };
    Ok(bytes)
}

fn set_global_bytes(format: u32, bytes: &[u8]) -> Result<(), AppError> {
    let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }.map_err(|error| {
        log::warn!(
            "windows clipboard write failed stage=allocate format={format} error_code={}",
            error.code().0
        );
        AppError::ClipboardWriteFailed
    })?;
    let pointer = unsafe { GlobalLock(global) };
    if pointer.is_null() {
        let _ = unsafe { GlobalFree(Some(global)) };
        log::warn!("windows clipboard write failed stage=lock format={format}");
        return Err(AppError::ClipboardWriteFailed);
    }
    unsafe { copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len()) };
    let _ = unsafe { GlobalUnlock(global) };
    if let Err(error) = unsafe { SetClipboardData(format, Some(HANDLE(global.0))) } {
        let _ = unsafe { GlobalFree(Some(global)) };
        log::warn!(
            "windows clipboard write failed stage=set_data format={format} error_code={}",
            error.code().0
        );
        return Err(AppError::ClipboardWriteFailed);
    }
    Ok(())
}

fn read_unicode_text() -> Result<String, AppError> {
    let bytes = read_global_bytes(CF_UNICODETEXT)?;
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    Ok(String::from_utf16_lossy(&units))
}

fn encode_unicode_text(value: &str) -> Vec<u8> {
    value
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn read_files() -> Result<Vec<String>, AppError> {
    let handle =
        unsafe { GetClipboardData(CF_HDROP) }.map_err(|_| AppError::ClipboardUnavailable)?;
    let drop = HDROP(handle.0);
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    let mut files = Vec::with_capacity(count as usize);
    for index in 0..count {
        let length = unsafe { DragQueryFileW(drop, index, None) };
        let mut buffer = vec![0u16; length as usize + 1];
        let written = unsafe { DragQueryFileW(drop, index, Some(&mut buffer)) };
        files.push(String::from_utf16_lossy(&buffer[..written as usize]));
    }
    Ok(files)
}

fn encode_file_drop(files: &[String]) -> Vec<u8> {
    let mut bytes = vec![0u8; 20];
    bytes[0..4].copy_from_slice(&(20u32).to_le_bytes());
    bytes[16..20].copy_from_slice(&(1u32).to_le_bytes());
    for file in files {
        for unit in file.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

fn extract_html(bytes: &[u8]) -> Vec<u8> {
    let header = String::from_utf8_lossy(bytes);
    let offset = |name: &str| {
        header.lines().find_map(|line| {
            line.strip_prefix(name)
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
    };
    match (offset("StartHTML:"), offset("EndHTML:")) {
        (Some(start), Some(end)) if start < end && end <= bytes.len() => bytes[start..end].to_vec(),
        _ => bytes
            .iter()
            .copied()
            .take_while(|byte| *byte != 0)
            .collect(),
    }
}

fn encode_html(html: &[u8]) -> Vec<u8> {
    if html.starts_with(b"Version:") {
        return html.to_vec();
    }
    let prefix = b"Version:1.0\r\nStartHTML:0000000105\r\nEndHTML:";
    let start = 105usize;
    let end = start + html.len();
    let mut output = Vec::new();
    output.extend_from_slice(prefix);
    output.extend_from_slice(
        format!("{end:010}\r\nStartFragment:{start:010}\r\nEndFragment:{end:010}\r\n").as_bytes(),
    );
    debug_assert_eq!(output.len(), start);
    output.extend_from_slice(html);
    output
}

fn dib_to_png(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    if bytes.len() < 40 {
        return Err(AppError::ClipboardUnavailable);
    }
    let read_u32 = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let read_i32 = |offset| i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let header_size = read_u32(0) as usize;
    let width = read_i32(4);
    let signed_height = read_i32(8);
    let bits = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
    let compression = read_u32(16);
    if header_size < 40
        || header_size > bytes.len()
        || width <= 0
        || signed_height == 0
        || !matches!(bits, 24 | 32)
        || !matches!(compression, BI_RGB | BI_BITFIELDS)
    {
        return Err(AppError::ClipboardUnavailable);
    }
    let height = signed_height.unsigned_abs();
    let width = width as u32;
    let extra_masks = usize::from(header_size == 40 && compression == BI_BITFIELDS) * 12;
    let pixel_offset = header_size
        .checked_add(extra_masks)
        .ok_or(AppError::ClipboardUnavailable)?;
    let stride = (width as usize)
        .checked_mul(bits as usize)
        .and_then(|value| value.checked_add(31))
        .map(|value| value / 32)
        .and_then(|value| value.checked_mul(4))
        .ok_or(AppError::ClipboardUnavailable)?;
    let pixel_bytes = stride
        .checked_mul(height as usize)
        .ok_or(AppError::ClipboardUnavailable)?;
    if pixel_bytes > DECODED_IMAGE_LIMIT {
        return Err(AppError::ContentTooLarge);
    }
    if pixel_offset
        .checked_add(pixel_bytes)
        .is_none_or(|end| end > bytes.len())
    {
        return Err(AppError::ClipboardUnavailable);
    }
    let mut rgba = RgbaImage::new(width, height);
    for y in 0..height {
        let source_y = if signed_height > 0 { height - 1 - y } else { y };
        let row = pixel_offset + source_y as usize * stride;
        for x in 0..width {
            let offset = row + x as usize * (bits as usize / 8);
            let blue = bytes[offset];
            let green = bytes[offset + 1];
            let red = bytes[offset + 2];
            let alpha = if bits == 32 && bytes[offset + 3] != 0 {
                bytes[offset + 3]
            } else {
                255
            };
            rgba.put_pixel(x, y, image::Rgba([red, green, blue, alpha]));
        }
    }
    let mut writer = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut writer, ImageFormat::Png)
        .map_err(|_| AppError::ClipboardUnavailable)?;
    Ok(writer.into_inner())
}

fn png_to_dibv5(png: &[u8]) -> Result<Vec<u8>, AppError> {
    let rgba = image::load_from_memory(png)
        .map_err(|_| AppError::ClipboardUnavailable)?
        .to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    if width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(AppError::ContentTooLarge);
    }
    let image_size = (width as usize)
        .checked_mul(height as usize)
        .and_then(|value| value.checked_mul(4))
        .ok_or(AppError::ContentTooLarge)?;
    if image_size > DECODED_IMAGE_LIMIT {
        return Err(AppError::ContentTooLarge);
    }
    let allocation_size = DIBV5_HEADER_SIZE
        .checked_add(image_size)
        .ok_or(AppError::ContentTooLarge)?;
    let mut bytes = vec![0u8; allocation_size];
    bytes[0..4].copy_from_slice(&(DIBV5_HEADER_SIZE as u32).to_le_bytes());
    bytes[4..8].copy_from_slice(&(width as i32).to_le_bytes());
    bytes[8..12].copy_from_slice(&(-(height as i32)).to_le_bytes());
    bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&32u16.to_le_bytes());
    bytes[16..20].copy_from_slice(&BI_BITFIELDS.to_le_bytes());
    bytes[20..24].copy_from_slice(&(image_size as u32).to_le_bytes());
    bytes[40..44].copy_from_slice(&0x00ff0000u32.to_le_bytes());
    bytes[44..48].copy_from_slice(&0x0000ff00u32.to_le_bytes());
    bytes[48..52].copy_from_slice(&0x000000ffu32.to_le_bytes());
    bytes[52..56].copy_from_slice(&0xff000000u32.to_le_bytes());
    bytes[56..60].copy_from_slice(b"sRGB");
    for (index, pixel) in rgba.pixels().enumerate() {
        let offset = DIBV5_HEADER_SIZE + index * 4;
        bytes[offset..offset + 4].copy_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_text_round_trips() {
        let encoded = encode_unicode_text("你好 Windows");
        let units: Vec<u16> = encoded
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        assert_eq!(String::from_utf16_lossy(&units), "你好 Windows");
    }

    #[test]
    fn html_offsets_round_trip() {
        let html = b"<p>Hello</p>";
        assert_eq!(extract_html(&encode_html(html)), html);
    }

    #[test]
    fn png_and_dib_round_trip() {
        let mut png = std::io::Cursor::new(Vec::new());
        DynamicImage::new_rgba8(3, 2)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let dib = png_to_dibv5(&png.into_inner()).unwrap();
        let decoded = dib_to_png(&dib).unwrap();
        let image = image::load_from_memory(&decoded).unwrap();
        assert_eq!((image.width(), image.height()), (3, 2));
    }

    #[test]
    fn file_drop_is_utf16_and_double_null_terminated() {
        let encoded = encode_file_drop(&["C:\\测试\\one.txt".into()]);
        assert_eq!(&encoded[0..4], &20u32.to_le_bytes());
        assert_eq!(&encoded[encoded.len() - 4..], &[0, 0, 0, 0]);
    }
}
