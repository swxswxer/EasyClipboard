use std::path::Path;

use image::ImageFormat;
use sha2::{Digest, Sha256};

use crate::error::AppError;

pub const TEXT_LIMIT: usize = 2 * 1024 * 1024;
pub const IMAGE_LIMIT: usize = 25 * 1024 * 1024;
pub const FILE_LIMIT: usize = 100;

#[derive(Clone, Debug)]
pub struct WriteReceipt {
    pub change_token: i64,
    pub content_hash: String,
}

pub fn text_title(value: &str) -> String {
    let compact = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("空白文本");
    let mut title: String = compact.chars().take(72).collect();
    if compact.chars().count() > 72 {
        title.push('…');
    }
    title
}

pub fn file_title(files: &[String]) -> String {
    let first = Path::new(&files[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("文件");
    if files.len() == 1 {
        first.to_owned()
    } else {
        format!("{} 等 {} 个文件", first, files.len())
    }
}

pub fn hash_bytes(prefix: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prefix);
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn hash_files(files: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"files\0");
    for file in files {
        hasher.update(file.as_bytes());
        hasher.update(b"\0");
    }
    hex::encode(hasher.finalize())
}

pub fn normalize_image(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), AppError> {
    if bytes.len() > IMAGE_LIMIT {
        return Err(AppError::ContentTooLarge);
    }
    let decoded = image::load_from_memory(bytes).map_err(|_| AppError::ClipboardUnavailable)?;
    let (width, height) = (decoded.width(), decoded.height());
    let mut writer = std::io::Cursor::new(Vec::new());
    decoded
        .write_to(&mut writer, ImageFormat::Png)
        .map_err(|_| AppError::ClipboardUnavailable)?;
    let png = writer.into_inner();
    if png.len() > IMAGE_LIMIT {
        return Err(AppError::ContentTooLarge);
    }
    Ok((png, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_bounded_and_readable() {
        assert_eq!(text_title("\n  hello world\nnext"), "hello world");
        let long = "好".repeat(90);
        assert_eq!(text_title(&long).chars().count(), 73);
        assert_eq!(
            file_title(&["/tmp/one.txt".into(), "/tmp/two.txt".into()]),
            "one.txt 等 2 个文件"
        );
    }

    #[test]
    fn hashes_include_the_clipboard_kind() {
        assert_ne!(
            hash_bytes(b"text\0", b"same"),
            hash_bytes(b"image\0", b"same")
        );
        assert_eq!(hash_files(&["one".into()]), hash_files(&["one".into()]));
    }
}
