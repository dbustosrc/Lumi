use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::imageops::FilterType;
use tokio::sync::{Semaphore, oneshot};

use crate::domain::ports::{PortError, PortFuture, ShelfPlatform};
use crate::domain::shelf::{ShelfPathMetadata, ShelfThumbnail};

const THUMBNAIL_SIDE_PX: u32 = 80;
const MAX_THUMBNAIL_SOURCE_BYTES: u64 = 40 * 1024 * 1024;
// Bound full-resolution decodes, not the number of references in Shelf.
static THUMBNAIL_DECODERS: Semaphore = Semaphore::const_new(2);

/// Concrete OS filesystem, thumbnail decoder and Windows Shell implementation
/// of [`ShelfPlatform`].
#[derive(Default)]
pub struct NativeShelfPlatform;

impl ShelfPlatform for NativeShelfPlatform {
    fn copy_text(&self, text: &str) -> Result<(), PortError> {
        #[cfg(windows)]
        {
            super::windows::shelf_text::copy_text(text)
        }
        #[cfg(not(windows))]
        {
            let _ = text;
            Err(PortError::new(
                "shelf.error.clipboard",
                "Windows is required",
            ))
        }
    }
    fn inspect_path(&self, path: &Path) -> Result<Option<ShelfPathMetadata>, PortError> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(PortError::new("shelf.error.inspectFailed", error)),
        };
        let display_name = extract_display_name(path);
        let is_directory = metadata.is_dir();
        let size_bytes = if is_directory { 0 } else { metadata.len() };
        Ok(Some(ShelfPathMetadata {
            display_name,
            size_bytes,
            is_directory,
        }))
    }

    fn open_path(&self, path: &Path) -> Result<(), PortError> {
        open_os_path(path)
    }

    fn pick_files(&self) -> PortFuture<Result<Vec<PathBuf>, PortError>> {
        Box::pin(async move { spawn_sta_file_picker().await })
    }

    fn load_thumbnail(&self, path: PathBuf) -> PortFuture<Option<ShelfThumbnail>> {
        Box::pin(async move {
            let permit = THUMBNAIL_DECODERS.acquire().await.ok()?;
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                decode_image_thumbnail(&path)
            })
            .await
            .ok()
            .flatten()
        })
    }
}

fn extract_display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn decode_image_thumbnail(path: &Path) -> Option<ShelfThumbnail> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_THUMBNAIL_SOURCE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?;
    let square = decoded.resize_to_fill(THUMBNAIL_SIDE_PX, THUMBNAIL_SIDE_PX, FilterType::Triangle);
    let rgba = square.to_rgba8();
    Some(ShelfThumbnail {
        width: rgba.width(),
        height: rgba.height(),
        rgba: Arc::new(rgba.into_raw()),
    })
}

#[cfg(windows)]
fn open_os_path(path: &Path) -> Result<(), PortError> {
    super::windows::shelf_drag::open_shell_path(path)
}

#[cfg(not(windows))]
fn open_os_path(_path: &Path) -> Result<(), PortError> {
    Err(PortError::new(
        "shelf.error.openFailed",
        "unsupported platform",
    ))
}

#[cfg(windows)]
async fn spawn_sta_file_picker() -> Result<Vec<PathBuf>, PortError> {
    let (tx, rx) = oneshot::channel();
    std::thread::Builder::new()
        .name("lumi-picker".into())
        .spawn(move || {
            let _ = tx.send(super::windows::shelf_drag::pick_files_dialog());
        })
        .map_err(|err| PortError::new("shelf.error.pickerFailed", err))?;
    rx.await
        .map_err(|err| PortError::new("shelf.error.pickerFailed", err))?
}

#[cfg(not(windows))]
async fn spawn_sta_file_picker() -> Result<Vec<PathBuf>, PortError> {
    Ok(Vec::new())
}

#[cfg(test)]
#[path = "../../tests/fixtures/shelf_platform.rs"]
mod tests;
