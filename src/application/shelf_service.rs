use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use super::events::{AppEvent, EventSender};
use super::presenter::ShelfEntry;
use crate::domain::ports::{PortError, ShelfPlatform, ShelfRepository};
use crate::domain::shelf::{ShelfItem, ShelfThumbnail, is_same_shelf_path};

/// Orchestrates the zero-copy temporary file shelf, metadata refresh,
/// persistence and asynchronous image thumbnail loading.
pub struct ShelfService {
    repository: Arc<dyn ShelfRepository>,
    platform: Arc<dyn ShelfPlatform>,
    events: EventSender,
    items: Vec<ShelfItem>,
    thumbnails: HashMap<Uuid, ShelfThumbnail>,
}

impl ShelfService {
    pub fn load(
        repository: Arc<dyn ShelfRepository>,
        platform: Arc<dyn ShelfPlatform>,
        events: EventSender,
    ) -> Result<Self, PortError> {
        let saved = repository.load()?;
        let (items, was_pruned) = prune_and_refresh_saved(saved, platform.as_ref());
        if was_pruned {
            repository.save(&items)?;
        }
        let service = Self {
            repository,
            platform,
            events,
            items,
            thumbnails: HashMap::new(),
        };
        service.spawn_all_thumbnails();
        Ok(service)
    }

    pub fn entries(&self) -> Vec<ShelfEntry> {
        self.items
            .iter()
            .map(|item| ShelfEntry {
                item: item.clone(),
                badge: item.badge(),
                size_label: item.formatted_size(),
                thumbnail: self.thumbnails.get(&item.id).cloned(),
            })
            .collect()
    }

    pub fn add_text(&mut self, text: String) -> Result<bool, PortError> {
        let item = ShelfItem::new_text(text, Utc::now()).ok_or_else(|| {
            PortError::new("shelf.error.invalidText", "empty text or embedded NUL")
        })?;
        let mut items = self.items.clone();
        items.insert(0, item);
        self.repository.save(&items)?;
        self.items = items;
        Ok(true)
    }

    pub fn add_paths(&mut self, paths: &[PathBuf]) -> Result<bool, PortError> {
        let previous = self.items.clone();
        let mut has_changed = false;
        for path in paths.iter().rev() {
            if self.upsert_single_path(path) {
                has_changed = true;
            }
        }
        if has_changed && let Err(error) = self.repository.save(&self.items) {
            self.items = previous;
            return Err(error);
        }
        Ok(has_changed)
    }

    pub fn remove(&mut self, id: Uuid) -> Result<bool, PortError> {
        let next: Vec<_> = self
            .items
            .iter()
            .filter(|item| item.id != id)
            .cloned()
            .collect();
        let was_removed = self.items.len() != next.len();
        if was_removed {
            self.repository.save(&next)?;
            self.items = next;
            self.thumbnails.remove(&id);
        }
        Ok(was_removed)
    }

    pub fn clear(&mut self) -> Result<bool, PortError> {
        if self.items.is_empty() {
            return Ok(false);
        }
        self.repository.save(&[])?;
        self.items.clear();
        self.thumbnails.clear();
        Ok(true)
    }

    /// Opens an item via the OS Shell. If the underlying file was deleted on
    /// disk, prunes the stale shelf card and returns `(true, Err(_))`.
    pub fn open_item(&mut self, id: Uuid) -> (bool, Result<(), PortError>) {
        if let Some(text) = self
            .items
            .iter()
            .find(|item| item.id == id)
            .and_then(ShelfItem::text)
        {
            return (false, self.platform.copy_text(text));
        }
        let Some(path) = self.find_path(id) else {
            return (false, Ok(()));
        };
        match self.platform.inspect_path(&path) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return match self.remove(id) {
                    Ok(removed) => (
                        removed,
                        Err(PortError::new(
                            "shelf.error.inspectFailed",
                            "file no longer exists",
                        )),
                    ),
                    Err(error) => (false, Err(error)),
                };
            }
            Err(error) => return (false, Err(error)),
        }
        (false, self.platform.open_path(&path))
    }

    pub fn on_thumbnail_loaded(&mut self, id: Uuid, thumbnail: Option<ShelfThumbnail>) -> bool {
        let Some(thumbnail) = thumbnail else {
            return false;
        };
        if !self.items.iter().any(|item| item.id == id) {
            return false;
        }
        self.thumbnails.insert(id, thumbnail);
        true
    }

    pub fn request_file_picker(&self) {
        let future = self.platform.pick_files();
        let events = self.events.clone();
        tokio::spawn(async move {
            match future.await {
                Ok(paths) if !paths.is_empty() => {
                    let _ = events.send(AppEvent::FilesPicked(paths));
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(event = "log.shelf.pickerFailed", error = %error);
                }
            }
        });
    }

    fn find_path(&self, id: Uuid) -> Option<PathBuf> {
        self.items
            .iter()
            .find(|item| item.id == id)
            .and_then(ShelfItem::path)
            .map(Path::to_path_buf)
    }

    fn upsert_single_path(&mut self, path: &Path) -> bool {
        let metadata = match self.platform.inspect_path(path) {
            Ok(Some(metadata)) => metadata,
            Ok(None) => return false,
            Err(error) => {
                tracing::warn!(event = "log.shelf.inspectRejected", reason = %error);
                return false;
            }
        };
        if let Some(index) = self.items.iter().position(|it| {
            it.path()
                .is_some_and(|existing| is_same_shelf_path(existing, path))
        }) {
            let mut existing = self.items.remove(index).with_refreshed_metadata(metadata);
            existing.added_at = Utc::now();
            self.spawn_thumbnail_if_image(&existing);
            self.items.insert(0, existing);
            return true;
        }
        let item = ShelfItem::new(path.to_path_buf(), metadata, Utc::now());
        self.spawn_thumbnail_if_image(&item);
        self.items.insert(0, item);
        true
    }

    fn spawn_all_thumbnails(&self) {
        for item in &self.items {
            self.spawn_thumbnail_if_image(item);
        }
    }

    fn spawn_thumbnail_if_image(&self, item: &ShelfItem) {
        if !item.is_decodable_image() {
            return;
        }
        let id = item.id;
        let Some(path) = item.path() else { return };
        let future = self.platform.load_thumbnail(path.to_path_buf());
        let events = self.events.clone();
        tokio::spawn(async move {
            let thumbnail = future.await;
            let _ = events.send(AppEvent::ShelfThumbnailLoaded { id, thumbnail });
        });
    }
}

fn prune_and_refresh_saved(
    saved: Vec<ShelfItem>,
    platform: &dyn ShelfPlatform,
) -> (Vec<ShelfItem>, bool) {
    let initial_len = saved.len();
    let mut kept = Vec::with_capacity(initial_len);
    for item in saved {
        let Some(path) = item.path() else {
            kept.push(item);
            continue;
        };
        match platform.inspect_path(path) {
            Ok(Some(metadata)) => kept.push(item.with_refreshed_metadata(metadata)),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(event = "log.shelf.inspectRejected", reason = %error);
                kept.push(item);
            }
        }
    }
    let was_pruned = kept.len() != initial_len;
    (kept, was_pruned)
}

#[cfg(test)]
#[path = "../../tests/fixtures/shelf.rs"]
mod tests;
