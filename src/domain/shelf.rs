use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Visual category of a file or folder pinned to the notch shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShelfItemKind {
    Text,
    Document,
    Image,
    Audio,
    Video,
    Archive,
    Code,
    Folder,
}

/// Filesystem attributes inspected when an item is added or reloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfPathMetadata {
    pub display_name: String,
    pub size_bytes: u64,
    pub is_directory: bool,
}

/// Decoded RGBA8 square thumbnail for image files on the shelf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfThumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

/// Zero-copy reference to a file or folder held temporarily on the notch shelf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ShelfContent {
    File(PathBuf),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShelfItem {
    pub id: Uuid,
    pub content: ShelfContent,
    pub display_name: String,
    pub size_bytes: u64,
    pub is_directory: bool,
    pub kind: ShelfItemKind,
    pub added_at: DateTime<Utc>,
}

impl ShelfItem {
    pub fn new(path: PathBuf, metadata: ShelfPathMetadata, added_at: DateTime<Utc>) -> Self {
        let kind = classify_shelf_item(&path, metadata.is_directory);
        Self {
            id: Uuid::new_v4(),
            content: ShelfContent::File(path),
            display_name: metadata.display_name,
            size_bytes: metadata.size_bytes,
            is_directory: metadata.is_directory,
            kind,
            added_at,
        }
    }

    pub fn with_refreshed_metadata(mut self, metadata: ShelfPathMetadata) -> Self {
        if let Some(path) = self.path() {
            self.kind = classify_shelf_item(path, metadata.is_directory);
        }
        self.display_name = metadata.display_name;
        self.size_bytes = metadata.size_bytes;
        self.is_directory = metadata.is_directory;
        self
    }

    pub fn badge(&self) -> String {
        self.path().map_or_else(
            || "TXT".into(),
            |path| extension_badge(path, self.is_directory),
        )
    }

    pub fn formatted_size(&self) -> String {
        format_byte_size(self.size_bytes)
    }

    pub fn is_decodable_image(&self) -> bool {
        !self.is_directory && self.path().is_some_and(is_decodable_image_path)
    }

    pub fn new_text(text: String, added_at: DateTime<Utc>) -> Option<Self> {
        if text.trim().is_empty() || text.contains('\0') {
            return None;
        }
        Some(Self {
            id: Uuid::new_v4(),
            display_name: text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or_default()
                .trim()
                .into(),
            size_bytes: text.len() as u64,
            content: ShelfContent::Text(text),
            is_directory: false,
            kind: ShelfItemKind::Text,
            added_at,
        })
    }
    pub fn path(&self) -> Option<&Path> {
        match &self.content {
            ShelfContent::File(path) => Some(path),
            ShelfContent::Text(_) => None,
        }
    }
    pub fn text(&self) -> Option<&str> {
        match &self.content {
            ShelfContent::Text(text) => Some(text),
            ShelfContent::File(_) => None,
        }
    }
}

/// Compares two shelf paths using case-insensitive normalization on Windows.
pub fn is_same_shelf_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

/// Classifies a shelf item by directory flag or file extension.
pub fn classify_shelf_item(path: &Path, is_directory: bool) -> ShelfItemKind {
    if is_directory {
        return ShelfItemKind::Folder;
    }
    let ext = lower_extension(path);
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg" | "ico" | "avif" => {
            ShelfItemKind::Image
        }
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" => ShelfItemKind::Audio,
        "mp4" | "mkv" | "webm" | "mov" | "avi" => ShelfItemKind::Video,
        "zip" | "7z" | "rar" | "tar" | "gz" | "xz" | "bz2" => ShelfItemKind::Archive,
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "json" | "toml" | "yaml" | "yml" | "html"
        | "css" | "slint" | "sql" | "sh" | "ps1" | "c" | "cpp" | "go" => ShelfItemKind::Code,
        _ => ShelfItemKind::Document,
    }
}

/// Returns whether the built-in image decoder can rasterize a preview thumbnail.
pub fn is_decodable_image_path(path: &Path) -> bool {
    matches!(
        lower_extension(path).as_str(),
        "png" | "jpg" | "jpeg" | "webp"
    )
}

/// Builds a short uppercase badge (`PDF`, `ZIP`, `MP3`) for non-folder items.
pub fn extension_badge(path: &Path, is_directory: bool) -> String {
    if is_directory {
        return String::new();
    }
    let raw = lower_extension(path);
    let cleaned: String = raw
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(4)
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    if cleaned.is_empty() {
        "FILE".to_owned()
    } else {
        cleaned
    }
}

/// Formats a byte count as a compact, human-readable size.
pub fn format_byte_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let value = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if value < MB {
        format_scaled_unit(value / KB, "KB")
    } else if value < GB {
        format_scaled_unit(value / MB, "MB")
    } else {
        format_scaled_unit(value / GB, "GB")
    }
}

fn format_scaled_unit(scaled: f64, unit: &str) -> String {
    if scaled >= 10.0 {
        format!("{scaled:.0} {unit}")
    } else {
        format!("{scaled:.1} {unit}")
    }
}

fn lower_extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map_or_else(String::new, str::to_ascii_lowercase)
}
