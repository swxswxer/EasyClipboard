use std::{
    path::Path,
    ptr::copy_nonoverlapping,
    slice,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use image::{DynamicImage, ImageFormat, RgbaImage};
use tokio::sync::mpsc::UnboundedSender;
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
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
                TranslateMessage, CS_HREDRAW, CS_VREDRAW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE,
                WINDOW_STYLE, WM_CLIPBOARDUPDATE, WNDCLASSW,
            },
        },
    },
};

use crate::{
    domain::clipboard::{
        file_title, hash_bytes, hash_files, normalize_image, text_title, WriteReceipt, FILE_LIMIT,
        TEXT_LIMIT,
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

static CHANGE_SENDER: OnceLock<Mutex<UnboundedSender<()>>> = OnceLock::new();

struct ClipboardGuard;

impl ClipboardGuard {
    fn open() -> Result<Self, AppError> {
        for _ in 0..10 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err(AppError::ClipboardUnavailable)
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

pub fn install_clipboard_listener(sender: UnboundedSender<()>) -> Result<(), AppError> {
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
                Err(_) => return,
            };
            if AddClipboardFormatListener(window).is_err() {
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
            let _ = sender.send(());
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

pub fn read_capture(source: TargetApplication) -> Result<Option<CapturedClipboard>, AppError> {
    let _guard = ClipboardGuard::open()?;
    if clipboard_is_sensitive()? {
        return Ok(None);
    }

    if format_available(CF_HDROP) {
        let files = read_files()?;
        if !files.is_empty() {
            if files.len() > FILE_LIMIT {
                return Err(AppError::ContentTooLarge);
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
    }

    let png_format = unsafe { RegisterClipboardFormatW(w!("PNG")) };
    let image = if png_format != 0 && format_available(png_format) {
        Some(read_global_bytes(png_format)?)
    } else if format_available(CF_DIBV5) {
        Some(dib_to_png(&read_global_bytes(CF_DIBV5)?)?)
    } else if format_available(CF_DIB) {
        Some(dib_to_png(&read_global_bytes(CF_DIB)?)?)
    } else {
        None
    };
    if let Some(image_bytes) = image {
        let (png, width, height) = normalize_image(&image_bytes)?;
        return Ok(Some(CapturedClipboard {
            kind: ClipboardKind::Image,
            title: format!("图片 · {width} × {height}"),
            content: String::new(),
            html: None,
            rtf: None,
            image_png: Some(png.clone()),
            files: vec![],
            source_name: source.name,
            source_app_id: source.identifier,
            byte_size: png.len() as u64,
            hash: hash_bytes(b"image\0", &png),
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

pub fn write_item(
    item: &ClipboardItemDetail,
    image_png: Option<&[u8]>,
    html: Option<&[u8]>,
    rtf: Option<&[u8]>,
) -> Result<WriteReceipt, AppError> {
    let _guard = ClipboardGuard::open()?;
    unsafe { EmptyClipboard() }.map_err(|_| AppError::ClipboardUnavailable)?;
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
            set_global_bytes(CF_DIBV5, &png_to_dibv5(png)?)?;
            hash_bytes(b"image\0", png)
        }
        ClipboardKind::Files => {
            if item.files.iter().any(|file| !Path::new(file).exists()) {
                return Err(AppError::FileMissing);
            }
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
    let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }
        .map_err(|_| AppError::ClipboardUnavailable)?;
    let pointer = unsafe { GlobalLock(global) };
    if pointer.is_null() {
        let _ = unsafe { GlobalFree(Some(global)) };
        return Err(AppError::ClipboardUnavailable);
    }
    unsafe { copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len()) };
    let _ = unsafe { GlobalUnlock(global) };
    if unsafe { SetClipboardData(format, Some(HANDLE(global.0))) }.is_err() {
        let _ = unsafe { GlobalFree(Some(global)) };
        return Err(AppError::ClipboardUnavailable);
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
    if width <= 0
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
    let stride = (width as usize * bits as usize).div_ceil(32) * 4;
    if pixel_offset + stride * height as usize > bytes.len() {
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
    let image_size = width as usize * height as usize * 4;
    let mut bytes = vec![0u8; DIBV5_HEADER_SIZE + image_size];
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
