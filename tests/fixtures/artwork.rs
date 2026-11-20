use super::HttpArtworkLoader;

use crate::application::settings::AppSettings;
use reqwest::header::AUTHORIZATION;
use tokio::sync::watch;

fn loader(server_url: &str) -> HttpArtworkLoader {
    let mut settings = AppSettings::default();
    settings.music_assistant.server_url = server_url.into();
    settings.music_assistant.token = "secret-token".into();
    let (_sender, receiver) = watch::channel(settings);
    HttpArtworkLoader::new(reqwest::Client::new(), receiver)
}

fn authorization_header(server_url: &str, image_url: &str) -> Option<String> {
    let request = loader(server_url)
        .build_request(image_url)
        .unwrap()
        .build()
        .unwrap();
    request
        .headers()
        .get(AUTHORIZATION)
        .map(|value| value.to_str().unwrap().to_owned())
}

#[test]
fn music_assistant_token_is_sent_only_to_the_exact_origin() {
    assert_eq!(
        authorization_header(
            "https://music.example.test:8443",
            "https://music.example.test:8443/image/1"
        ),
        Some("Bearer secret-token".into())
    );
    assert_eq!(
        authorization_header(
            "https://music.example.test:8443",
            "https://music.example.test.evil.test:8443/image/1"
        ),
        None
    );
    assert_eq!(
        authorization_header(
            "https://music.example.test:8443",
            "https://music.example.test:8444/image/1"
        ),
        None
    );
    assert_eq!(
        authorization_header(
            "https://music.example.test:8443",
            "http://music.example.test:8443/image/1"
        ),
        None
    );
}

#[test]
fn websocket_origins_normalize_only_to_their_secure_http_counterpart() {
    assert_eq!(
        authorization_header(
            "ws://music.example.test:8095",
            "http://music.example.test:8095/image/1"
        ),
        Some("Bearer secret-token".into())
    );
    assert_eq!(
        authorization_header(
            "wss://music.example.test:8095",
            "https://music.example.test:8095/image/1"
        ),
        Some("Bearer secret-token".into())
    );
    assert_eq!(
        authorization_header(
            "wss://music.example.test:8095",
            "http://music.example.test:8095/image/1"
        ),
        None
    );
}

#[test]
fn thumbnail_decodes_png_jpeg_and_webp_to_bounded_rgba() {
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
    use std::io::Cursor;

    let cases = [
        (
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 4, Rgba([24, 48, 72, 64]))),
            ImageFormat::Png,
            64,
        ),
        (
            DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 4, Rgb([24, 48, 72]))),
            ImageFormat::Jpeg,
            255,
        ),
        (
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 4, Rgba([24, 48, 72, 64]))),
            ImageFormat::WebP,
            64,
        ),
    ];

    for (image, format, alpha) in cases {
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        let thumbnail = super::super::windows::thumbnail::from_bytes(bytes.get_ref(), 4).unwrap();
        assert_eq!(thumbnail.dimensions(), (4, 2));
        assert_eq!(thumbnail.get_pixel(2, 1).0[3], alpha);
    }
}
