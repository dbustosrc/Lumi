use tokio::sync::watch;

use crate::application::settings::AppSettings;
use crate::domain::media::Artwork;
use crate::domain::ports::{ArtworkLoader, PortError, PortFuture};

/// Artwork is shown at 62 logical px; 2x covers high-DPI screens while keeping
/// memory small.
const ARTWORK_SIZE: u32 = 128;

/// Downloads artwork over HTTP and decodes it into an RGBA thumbnail.
pub struct HttpArtworkLoader {
    client: reqwest::Client,
    settings: watch::Receiver<AppSettings>,
}

impl HttpArtworkLoader {
    pub fn new(client: reqwest::Client, settings: watch::Receiver<AppSettings>) -> Self {
        Self { client, settings }
    }

    /// Music Assistant's image proxy requires the user token; other hosts
    /// must never receive it.
    fn build_request(&self, url: &str) -> Result<reqwest::RequestBuilder, PortError> {
        let settings = self.settings.borrow();
        let target = url::Url::parse(url)
            .map_err(|error| PortError::new("media.error.artworkDownload", error))?;
        let mut server = url::Url::parse(&settings.music_assistant.server_url)
            .map_err(|error| PortError::new("media.error.artworkDownload", error))?;
        // A configured WebSocket endpoint and its HTTP image endpoint share authority.
        let image_scheme = match server.scheme() {
            "ws" => Some("http"),
            "wss" => Some("https"),
            _ => None,
        };
        if let Some(scheme) = image_scheme {
            server.set_scheme(scheme).map_err(|_| {
                PortError::new("media.error.artworkDownload", "invalid server scheme")
            })?;
        }
        let request = self.client.get(target.clone());
        if target.origin() == server.origin() && !settings.music_assistant.token.is_empty() {
            return Ok(request.bearer_auth(&settings.music_assistant.token));
        }
        Ok(request)
    }
}

impl ArtworkLoader for HttpArtworkLoader {
    fn load(&self, url: String) -> PortFuture<Result<Artwork, PortError>> {
        let request = self.build_request(&url);
        Box::pin(async move {
            let response = request?
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|error| PortError::new("media.error.artworkDownload", error))?;
            let bytes = response
                .bytes()
                .await
                .map_err(|error| PortError::new("media.error.artworkDownload", error))?;
            tokio::task::spawn_blocking(move || decode_artwork(&bytes))
                .await
                .map_err(|error| PortError::new("media.error.artworkDecode", error))?
        })
    }
}

#[cfg(test)]
#[path = "../../tests/fixtures/artwork.rs"]
mod tests;

fn decode_artwork(bytes: &[u8]) -> Result<Artwork, PortError> {
    let image = image::load_from_memory(bytes)
        .map_err(|error| PortError::new("media.error.artworkDecode", error))?;
    let thumbnail = image.thumbnail(ARTWORK_SIZE, ARTWORK_SIZE).to_rgba8();
    let (width, height) = thumbnail.dimensions();
    Ok(Artwork::from_rgba(thumbnail.into_raw(), width, height))
}
