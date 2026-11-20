use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use image::{ImageFormat, RgbaImage, imageops::FilterType};
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::{Interface, PCWSTR};

#[derive(Clone, Copy)]
enum Size {
    Fit(u32),
    Square(u32),
}

pub fn from_file(path: &Path, side: u32) -> Result<RgbaImage, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut signature = [0; 16];
    file.read_exact(&mut signature)
        .map_err(|error| error.to_string())?;
    match image::guess_format(&signature).map_err(|error| error.to_string())? {
        ImageFormat::Jpeg | ImageFormat::Png => with_factory(|factory| unsafe {
            let wide: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
            let decoder = factory.CreateDecoderFromFilename(
                PCWSTR(wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )?;
            scale_frame(factory, &decoder, Size::Square(side))
        }),
        ImageFormat::WebP => {
            let reader = image::ImageReader::open(path).map_err(|error| error.to_string())?;
            let image = reader
                .with_guessed_format()
                .map_err(|error| error.to_string())?
                .decode()
                .map_err(|error| error.to_string())?;
            Ok(image
                .resize_to_fill(side, side, FilterType::Triangle)
                .to_rgba8())
        }
        _ => Err("unsupported image format".into()),
    }
}

pub fn from_bytes(bytes: &[u8], side: u32) -> Result<RgbaImage, String> {
    match image::guess_format(bytes).map_err(|error| error.to_string())? {
        ImageFormat::Jpeg | ImageFormat::Png => with_factory(|factory| unsafe {
            let stream = factory.CreateStream()?;
            stream.InitializeFromMemory(bytes)?;
            let decoder = factory.CreateDecoderFromStream(
                &stream,
                std::ptr::null(),
                WICDecodeMetadataCacheOnDemand,
            )?;
            scale_frame(factory, &decoder, Size::Fit(side))
        }),
        ImageFormat::WebP => image::load_from_memory_with_format(bytes, ImageFormat::WebP)
            .map(|image| image.thumbnail(side, side).to_rgba8())
            .map_err(|error| error.to_string()),
        _ => Err("unsupported image format".into()),
    }
}

fn with_factory(
    decode: impl FnOnce(&IWICImagingFactory) -> windows::core::Result<RgbaImage>,
) -> Result<RgbaImage, String> {
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|error| error.to_string())?;
        let _apartment = Apartment;
        let factory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| error.to_string())?;
        decode(&factory).map_err(|error| error.to_string())
    }
}

fn scale_frame(
    factory: &IWICImagingFactory,
    decoder: &IWICBitmapDecoder,
    size: Size,
) -> windows::core::Result<RgbaImage> {
    unsafe {
        let frame = decoder.GetFrame(0)?;
        let (mut width, mut height) = (0, 0);
        frame.GetSize(&mut width, &mut height)?;
        let (source, target_width, target_height): (IWICBitmapSource, u32, u32) = match size {
            Size::Fit(side) => {
                let ratio = f64::from(side) / f64::from(width.max(height).max(1));
                let fit = |n| (f64::from(n) * ratio).round().max(1.) as u32;
                (frame.cast()?, fit(width), fit(height))
            }
            Size::Square(side) => {
                let edge = width.min(height);
                let region = WICRect {
                    X: ((width - edge) / 2) as i32,
                    Y: ((height - edge) / 2) as i32,
                    Width: edge as i32,
                    Height: edge as i32,
                };
                let clip = factory.CreateBitmapClipper()?;
                clip.Initialize(&frame, &region)?;
                (clip.cast()?, side, side)
            }
        };
        let scaler = factory.CreateBitmapScaler()?;
        scaler.Initialize(
            &source,
            target_width,
            target_height,
            WICBitmapInterpolationModeFant,
        )?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &scaler,
            &GUID_WICPixelFormat32bppRGBA,
            WICBitmapDitherTypeNone,
            None,
            0.,
            WICBitmapPaletteTypeCustom,
        )?;
        let stride = target_width * 4;
        let mut pixels = vec![0; (stride * target_height) as usize];
        converter.CopyPixels(std::ptr::null(), stride, &mut pixels)?;
        RgbaImage::from_raw(target_width, target_height, pixels)
            .ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))
    }
}
