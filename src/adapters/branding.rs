//! Embedded identity assets, decoded once and shared by native surfaces.
use std::sync::{Arc, OnceLock};

pub struct IconBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

pub fn app_icon() -> &'static IconBitmap {
    static ICON: OnceLock<IconBitmap> = OnceLock::new();
    ICON.get_or_init(|| decode(include_bytes!("../../assets/branding/lumi-256.png"), 256))
}

pub fn tray_icon() -> &'static IconBitmap {
    static ICON: OnceLock<IconBitmap> = OnceLock::new();
    ICON.get_or_init(|| decode(include_bytes!("../../assets/branding/lumi-32.png"), 32))
}

fn decode(bytes: &[u8], size: u32) -> IconBitmap {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("embedded Lumi icon must be a valid PNG")
        .into_rgba8();
    assert_eq!(
        image.dimensions(),
        (size, size),
        "invalid embedded icon size"
    );
    IconBitmap {
        width: size,
        height: size,
        rgba: Arc::new(image.into_raw()),
    }
}

#[cfg(test)]
#[path = "../../tests/fixtures/branding.rs"]
mod tests;
