use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::protocol::QueueState;
use crate::domain::media::{PlaybackSnapshot, PlaybackStatus};

const TRANSFER_COOLDOWN: Duration = Duration::from_secs(5);

/// Keeps every known queue and decides which one the notch follows: the
/// configured one, Lumi's local Sendspin player, or whichever queue is active.
pub struct QueueTracker {
    queues: HashMap<String, QueueState>,
    preferred: Option<String>,
    local_player_id: String,
    followed: Option<String>,
    last_published: Option<PlaybackSnapshot>,
    last_transfer_at: Option<Instant>,
}

impl QueueTracker {
    pub fn new(preferred_queue_id: &str, local_player_id: String) -> Self {
        let preferred = Some(preferred_queue_id.trim().to_owned()).filter(|id| !id.is_empty());
        Self {
            queues: HashMap::new(),
            preferred,
            local_player_id,
            followed: None,
            last_published: None,
            last_transfer_at: None,
        }
    }

    pub fn followed_queue_id(&self) -> Option<&str> {
        self.followed.as_deref()
    }

    pub fn upsert(&mut self, queue: QueueState) -> Option<PlaybackSnapshot> {
        self.queues.insert(queue.queue_id.clone(), queue);
        self.refresh()
    }

    pub fn update_elapsed(
        &mut self,
        queue_id: &str,
        elapsed: Duration,
    ) -> Option<PlaybackSnapshot> {
        let queue = self.queues.get_mut(queue_id)?;
        queue.elapsed = elapsed;
        self.refresh()
    }

    /// When no explicit `queue_id` is configured and another queue is playing
    /// silently while Lumi's local audio player is online, returns `(source, target)`.
    pub fn take_auto_transfer(&mut self, now: Instant) -> Option<(String, String)> {
        if self.preferred.is_some() || !self.is_local_ready_and_idle() {
            return None;
        }
        if self
            .last_transfer_at
            .is_some_and(|at| now.saturating_duration_since(at) < TRANSFER_COOLDOWN)
        {
            return None;
        }
        let source = self
            .queues
            .values()
            .find(|q| q.queue_id != self.local_player_id && q.status == PlaybackStatus::Playing)
            .map(|q| q.queue_id.clone())?;
        self.last_transfer_at = Some(now);
        Some((source, self.local_player_id.clone()))
    }

    /// When the user presses Play on a paused remote queue while `queue_id` is
    /// automatic, transfers playback onto Lumi's local PC audio player.
    pub fn take_play_transfer(&mut self, now: Instant) -> Option<(String, String)> {
        if self.preferred.is_some() || !self.queues.contains_key(&self.local_player_id) {
            return None;
        }
        let followed_id = self.followed.as_ref()?;
        if followed_id == &self.local_player_id {
            return None;
        }
        let followed = self.queues.get(followed_id)?;
        if followed.status == PlaybackStatus::Playing || followed.track.is_none() {
            return None;
        }
        self.last_transfer_at = Some(now);
        Some((followed_id.clone(), self.local_player_id.clone()))
    }

    fn is_local_ready_and_idle(&self) -> bool {
        self.queues
            .get(&self.local_player_id)
            .is_some_and(|local| local.status != PlaybackStatus::Playing)
    }

    /// Returns a snapshot only when the followed playback actually changed.
    fn refresh(&mut self) -> Option<PlaybackSnapshot> {
        self.followed = self.choose_followed();
        let snapshot = self.snapshot();
        let is_same = self.last_published.as_ref().is_some_and(|last| {
            last.track == snapshot.track
                && last.status == snapshot.status
                && last.position == snapshot.position
        });
        if is_same {
            return None;
        }
        self.last_published = Some(snapshot.clone());
        Some(snapshot)
    }

    fn choose_followed(&self) -> Option<String> {
        if self.preferred.is_some() {
            return self.preferred.clone();
        }
        if self.is_queue_playing(&self.local_player_id) {
            return Some(self.local_player_id.clone());
        }
        if self
            .followed
            .as_ref()
            .is_some_and(|id| self.is_queue_playing(id))
        {
            return self.followed.clone();
        }
        if let Some(playing) = self.queues.keys().find(|id| self.is_queue_playing(id)) {
            return Some(playing.clone());
        }
        if self.has_track(&self.local_player_id) {
            return Some(self.local_player_id.clone());
        }
        self.followed
            .clone()
            .filter(|id| self.has_track(id))
            .or_else(|| self.first_active_with_track())
    }

    fn is_queue_playing(&self, id: &str) -> bool {
        self.queues
            .get(id)
            .is_some_and(|queue| queue.status == PlaybackStatus::Playing)
    }

    fn has_track(&self, id: &str) -> bool {
        self.queues
            .get(id)
            .is_some_and(|queue| queue.track.is_some())
    }

    fn first_active_with_track(&self) -> Option<String> {
        self.queues
            .values()
            .find(|queue| queue.is_active && queue.track.is_some())
            .or_else(|| self.queues.values().find(|queue| queue.track.is_some()))
            .map(|queue| queue.queue_id.clone())
    }

    fn snapshot(&self) -> PlaybackSnapshot {
        let queue = self.followed.as_ref().and_then(|id| self.queues.get(id));
        match queue {
            Some(queue) => PlaybackSnapshot {
                track: queue.track.clone(),
                status: queue.status,
                position: queue.elapsed,
                sampled_at: Instant::now(),
            },
            None => PlaybackSnapshot::idle(),
        }
    }
}
