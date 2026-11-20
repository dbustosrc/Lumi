use super::HttpArtworkLoader;

use crate::application::settings::AppSettings;
use reqwest::header::AUTHORIZATION;
use tokio::sync::watch;
use windows::Storage::Streams::{DataWriter, IRandomAccessStream, InMemoryRandomAccessStream};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::{AgileReference, Interface};

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

async fn random_access_stream(bytes: &[u8]) -> windows::core::Result<IRandomAccessStream> {
    let stream = InMemoryRandomAccessStream::new()?;
    let output = stream.GetOutputStreamAt(0)?;
    let writer = DataWriter::CreateDataWriter(&output)?;
    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.await?;
    writer.FlushAsync()?.await?;
    writer.DetachStream()?;
    stream.Seek(0)?;
    stream.cast()
}

#[tokio::test(flavor = "current_thread")]
async fn random_access_stream_thumbnail_decodes_png_jpeg_and_webp() {
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
    use std::io::Cursor;

    struct ComApartment;
    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap() };
    let _apartment = ComApartment;

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
        let stream = random_access_stream(bytes.get_ref()).await.unwrap();
        let stream = AgileReference::new(&stream).unwrap();
        let thumbnail =
            super::super::windows::thumbnail::from_random_access_stream(&stream, 4).unwrap();
        assert_eq!(thumbnail.dimensions(), (4, 2));
        assert_eq!(thumbnail.get_pixel(2, 1).0[3], alpha);
    }
}
