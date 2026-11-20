//! One shared native graphics device; window surfaces own no application state.
use super::composition::{Runtime, WindowComposition};
use super::model::{Rect, SURFACE_WIDTH, Shell};
use super::scene::{Paint, PathSegment};
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use windows::Win32::Foundation::{COLORREF, HMODULE, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetWindowRect, IsWindowVisible, SW_HIDE, SW_SHOWNOACTIVATE,
    ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Interface, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

pub struct GraphicsDevice {
    hardware: OnceCell<HardwareDevice>,
    factory: ID2D1Factory1,
    write: IDWriteFactory,
    round_stroke: ID2D1StrokeStyle,
}
struct HardwareDevice {
    d3d: ID3D11Device,
    dxgi: IDXGIFactory2,
    d2d: ID2D1Device,
    composition: Rc<Runtime>,
}
impl GraphicsDevice {
    pub fn new() -> Result<Rc<Self>> {
        unsafe {
            let factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let round_stroke = factory
                .CreateStrokeStyle(
                    &D2D1_STROKE_STYLE_PROPERTIES1 {
                        startCap: D2D1_CAP_STYLE_ROUND,
                        endCap: D2D1_CAP_STYLE_ROUND,
                        dashCap: D2D1_CAP_STYLE_ROUND,
                        lineJoin: D2D1_LINE_JOIN_ROUND,
                        miterLimit: 2.,
                        dashStyle: D2D1_DASH_STYLE_SOLID,
                        dashOffset: 0.,
                        transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
                    },
                    None,
                )?
                .into();
            Ok(Rc::new(Self {
                hardware: OnceCell::new(),
                factory,
                write,
                round_stroke,
            }))
        }
    }
    fn hardware(&self) -> Result<&HardwareDevice> {
        if let Some(hardware) = self.hardware.get() {
            return Ok(hardware);
        }
        unsafe {
            let mut d3d = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let d3d = d3d.ok_or_else(windows::core::Error::from_thread)?;
            let dxgi_device: IDXGIDevice = d3d.cast()?;
            let dxgi = dxgi_device.GetAdapter()?.GetParent::<IDXGIFactory2>()?;
            let d2d = self.factory.CreateDevice(&dxgi_device)?;
            let composition = Runtime::acquire()?;
            let _ = self.hardware.set(HardwareDevice {
                d3d,
                dxgi,
                d2d,
                composition,
            });
            Ok(self.hardware.get().expect("hardware initialized"))
        }
    }
}

struct SoftwarePresentation {
    target: ID2D1DCRenderTarget,
    dib: WindowBitmap,
    window: Option<LayeredWindow>,
}

struct LayeredWindow(HWND);

impl LayeredWindow {
    fn new(owner: HWND) -> Result<Self> {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                w!("STATIC"),
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                Some(owner),
                None,
                None,
                None,
            )?;
            Ok(Self(hwnd))
        }
    }
    fn show(&self, visible: bool) {
        unsafe {
            let _ = ShowWindow(self.0, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }
}

impl Drop for LayeredWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

impl SoftwarePresentation {
    fn bind(&mut self, width: u32, height: u32) -> Result<()> {
        let bitmap = WindowBitmap::new(width, height)?;
        unsafe {
            self.target.BindDC(
                bitmap.dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                },
            )?;
        }
        self.dib = bitmap;
        Ok(())
    }
    #[cfg(test)]
    fn pixels(&self) -> &[u8] {
        self.dib.pixels()
    }
}

struct WindowBitmap {
    dc: HDC,
    bitmap: HBITMAP,
    original: HGDIOBJ,
    #[cfg(test)]
    pixels: *const u8,
    #[cfg(test)]
    length: usize,
}

impl WindowBitmap {
    fn new(width: u32, height: u32) -> Result<Self> {
        unsafe {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = std::ptr::null_mut();
            let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut pixels, None, 0)?;
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                let error = windows::core::Error::from_thread();
                let _ = DeleteObject(bitmap.into());
                return Err(error);
            }
            let original = SelectObject(dc, bitmap.into());
            if original.is_invalid() {
                let error = windows::core::Error::from_thread();
                let _ = DeleteDC(dc);
                let _ = DeleteObject(bitmap.into());
                return Err(error);
            }
            Ok(Self {
                dc,
                bitmap,
                original,
                #[cfg(test)]
                pixels: pixels.cast(),
                #[cfg(test)]
                length: width as usize * height as usize * 4,
            })
        }
    }
    #[cfg(test)]
    fn pixels(&self) -> &[u8] {
        unsafe {
            let _ = GdiFlush();
            std::slice::from_raw_parts(self.pixels, self.length)
        }
    }
}

impl Drop for WindowBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.dc, self.original);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct TextKey {
    text: String,
    size: u32,
    width: u32,
    height: u32,
    weight: u16,
    center: bool,
    wrap: bool,
}
pub struct Surface {
    pub device: Rc<GraphicsDevice>,
    context: ID2D1DeviceContext,
    svg_context: ID2D1DeviceContext5,
    vectors: HashMap<(String, u32, u32, u32), ID2D1SvgDocument>,
    used_vectors: HashSet<(String, u32, u32, u32)>,
    swap: Option<IDXGISwapChain1>,
    composition: Option<WindowComposition>,
    software: Option<SoftwarePresentation>,
    outgoing_window: Option<LayeredWindow>,
    hwnd: HWND,
    bitmap: Option<ID2D1Bitmap1>,
    motion_blur: Option<(ID2D1Bitmap1, ID2D1Effect)>,
    brush: ID2D1SolidColorBrush,
    gradients: HashMap<(u32, u32), ID2D1LinearGradientBrush>,
    width: u32,
    height: u32,
    scale: f32,
    layouts: HashMap<TextKey, IDWriteTextLayout>,
    used_layouts: HashSet<TextKey>,
    images: HashMap<(usize, u32, u32), ID2D1Bitmap1>,
    used_images: HashSet<(usize, u32, u32)>,
    shell_mask: Option<ID2D1Geometry>,
    shell_shape: Option<(Shell, bool)>,
    glass_mask: Option<(Shell, bool, f32)>,
}
impl Surface {
    pub fn new(
        device: Rc<GraphicsDevice>,
        hwnd: HWND,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self> {
        unsafe {
            let hardware = device.hardware()?;
            let description = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                ..Default::default()
            };
            let swap =
                hardware
                    .dxgi
                    .CreateSwapChainForComposition(&hardware.d3d, &description, None)?;
            let context = hardware
                .d2d
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let composition = WindowComposition::new(hardware.composition.clone(), hwnd, &swap)?;
            composition.resize(width, height)?;
            let mut surface = Self::from_context(device, context, hwnd, width, height, scale)?;
            surface.swap = Some(swap);
            surface.composition = Some(composition);
            surface.bind_target()?;
            Ok(surface)
        }
    }
    pub fn new_software(
        device: Rc<GraphicsDevice>,
        hwnd: HWND,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self> {
        unsafe {
            let target = device
                .factory
                .CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96. * scale,
                    dpiY: 96. * scale,
                    ..Default::default()
                })?;
            let dib = WindowBitmap::new(width, height)?;
            target.BindDC(
                dib.dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                },
            )?;
            let mut surface =
                Self::from_context(device, target.cast()?, hwnd, width, height, scale)?;
            surface.software = Some(SoftwarePresentation {
                target,
                dib,
                window: Some(LayeredWindow::new(hwnd)?),
            });
            Ok(surface)
        }
    }
    fn from_context(
        device: Rc<GraphicsDevice>,
        context: ID2D1DeviceContext,
        hwnd: HWND,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self> {
        unsafe {
            let brush = context.CreateSolidColorBrush(&color(0xffffff, 1.), None)?;
            context.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            Ok(Self {
                device,
                svg_context: context.cast()?,
                vectors: HashMap::new(),
                used_vectors: HashSet::new(),
                context,
                swap: None,
                composition: None,
                software: None,
                outgoing_window: None,
                hwnd,
                bitmap: None,
                motion_blur: None,
                brush,
                gradients: HashMap::new(),
                width,
                height,
                scale,
                layouts: HashMap::new(),
                used_layouts: HashSet::new(),
                images: HashMap::new(),
                used_images: HashSet::new(),
                shell_mask: None,
                shell_shape: None,
                glass_mask: None,
            })
        }
    }
    pub fn is_hardware(&self) -> bool {
        self.software.is_none()
    }
    pub fn promote(&mut self) -> Result<()> {
        let next = Self::new(
            self.device.clone(),
            self.hwnd,
            self.width,
            self.height,
            self.scale,
        )?;
        let mut previous = std::mem::replace(self, next);
        self.outgoing_window = previous
            .software
            .as_mut()
            .and_then(|software| software.window.take());
        Ok(())
    }
    pub fn set_software_visibility(&self, visible: bool) {
        if let Some(window) = self
            .software
            .as_ref()
            .and_then(|software| software.window.as_ref())
        {
            window.show(visible);
        }
        if !visible && let Some(window) = &self.outgoing_window {
            window.show(false);
        }
    }
    fn bind_target(&mut self) -> Result<()> {
        unsafe {
            let buffer: IDXGISurface =
                self.swap.as_ref().expect("hardware surface").GetBuffer(0)?;
            let properties = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96. * self.scale,
                dpiY: 96. * self.scale,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            };
            let bitmap = self
                .context
                .CreateBitmapFromDxgiSurface(&buffer, Some(&properties))?;
            self.context.SetTarget(&bitmap);
            self.context.SetDpi(96. * self.scale, 96. * self.scale);
            self.context
                .SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            self.bitmap = Some(bitmap);
            Ok(())
        }
    }
    pub fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<()> {
        unsafe {
            if self.width == width && self.height == height && self.scale == scale {
                return Ok(());
            }
            if let Some(software) = &mut self.software {
                software.bind(width, height)?;
                software.target.SetDpi(96. * scale, 96. * scale);
                self.width = width;
                self.height = height;
                self.scale = scale;
                return Ok(());
            }
            self.context.SetTarget(None);
            self.bitmap = None;
            self.motion_blur = None;
            self.swap
                .as_ref()
                .expect("hardware surface")
                .ResizeBuffers(
                    2,
                    width,
                    height,
                    DXGI_FORMAT_UNKNOWN,
                    DXGI_SWAP_CHAIN_FLAG(0),
                )?;
            self.width = width;
            self.height = height;
            self.scale = scale;
            self.composition
                .as_ref()
                .expect("bound composition")
                .resize(width, height)?;
            self.bind_target()
        }
    }
    pub fn recreate(&mut self, device: Rc<GraphicsDevice>) -> Result<()> {
        if self.software.is_some() {
            *self = Self::new_software(device, self.hwnd, self.width, self.height, self.scale)?;
            return Ok(());
        }
        unsafe {
            self.context.SetTarget(None);
        }
        self.bitmap = None;
        self.composition = None;
        let next = Self::new(device, self.hwnd, self.width, self.height, self.scale)?;
        *self = next;
        Ok(())
    }
    pub fn check_glass(&self) -> Result<()> {
        WindowComposition::check_glass(self.hwnd)
    }
    pub fn set_glass(&mut self, enabled: bool) -> Result<()> {
        if let Some(composition) = &mut self.composition
            && composition.set_glass(enabled)?
        {
            self.glass_mask = None;
        }
        Ok(())
    }
    fn sync_glass_clip(&mut self, shell: Shell, floating: bool) -> Result<()> {
        let key = (shell, floating, self.scale);
        let Some(composition) = &self.composition else {
            return Ok(());
        };
        if !composition.glass_enabled() || self.glass_mask == Some(key) {
            return Ok(());
        }
        if let Some(mask) = &self.shell_mask {
            let geometry = unsafe {
                self.device.factory.CreateTransformedGeometry(
                    mask,
                    &Matrix3x2 {
                        M11: self.scale,
                        M12: 0.,
                        M21: 0.,
                        M22: self.scale,
                        M31: 0.,
                        M32: 0.,
                    },
                )?
            };
            composition.clip(geometry.into())?;
            self.glass_mask = Some(key);
        }
        Ok(())
    }
    pub fn begin(&mut self, background: Option<u32>) {
        unsafe {
            self.used_layouts.clear();
            self.used_vectors.clear();
            self.used_images.clear();
            if let Some(software) = &self.software {
                software.target.BeginDraw();
            } else {
                self.context.BeginDraw();
            }
            self.context.SetTransform(&Matrix3x2::identity());
            self.context.Clear(Some(&background.map_or(
                D2D1_COLOR_F {
                    r: 0.,
                    g: 0.,
                    b: 0.,
                    a: 0.,
                },
                |c| color(c, 1.),
            )));
        }
    }
    pub fn end(&mut self) -> Result<()> {
        unsafe {
            if let Some(software) = &self.software {
                software.target.EndDraw(None, None)?;
                let window = software
                    .window
                    .as_ref()
                    .expect("layered presentation window");
                let mut owner_bounds = RECT::default();
                GetWindowRect(self.hwnd, &mut owner_bounds)?;
                let origin = POINT {
                    x: owner_bounds.left,
                    y: owner_bounds.top,
                };
                let size = SIZE {
                    cx: self.width as i32,
                    cy: self.height as i32,
                };
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                    ..Default::default()
                };
                UpdateLayeredWindow(
                    window.0,
                    None,
                    Some(&origin),
                    Some(&size),
                    Some(software.dib.dc),
                    Some(&POINT::default()),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                )?;
                if IsWindowVisible(self.hwnd).as_bool() && !IsWindowVisible(window.0).as_bool() {
                    window.show(true);
                }
            } else {
                self.context.EndDraw(None, None)?;
                self.swap
                    .as_ref()
                    .expect("hardware surface")
                    .Present(1, DXGI_PRESENT(0))
                    .ok()?;
                self.outgoing_window = None;
            }
            self.layouts
                .retain(|key, _| self.used_layouts.contains(key));
            self.images.retain(|key, _| self.used_images.contains(key));
            self.vectors
                .retain(|key, _| self.used_vectors.contains(key));
            Ok(())
        }
    }
    pub fn styled_shell(&mut self, s: Shell, floating: bool, opacity: f32) -> Result<()> {
        if self.shell_shape != Some((s, floating)) {
            self.shell_mask = Some(self.shell_geometry(s, floating)?);
            self.shell_shape = Some((s, floating));
        }
        self.tint(0, opacity);
        if let Some(mask) = &self.shell_mask {
            unsafe { self.context.FillGeometry(mask, &self.brush, None) };
        }
        self.sync_glass_clip(s, floating)
    }

    fn shell_geometry(&self, s: Shell, floating: bool) -> Result<ID2D1Geometry> {
        unsafe {
            let width = s.width.max(40.);
            let height = s.height.max(4.);
            let (ear, radius) = s.corners(floating);
            let x = (SURFACE_WIDTH - width) / 2.;
            if floating {
                let body = rounded(Rect::new(x, 0., width, height), radius);
                return self
                    .device
                    .factory
                    .CreateRoundedRectangleGeometry(&body)?
                    .cast();
            }
            let path = self.device.factory.CreatePathGeometry()?;
            let sink = path.Open()?;
            sink.BeginFigure(point(x - ear, 0.), D2D1_FIGURE_BEGIN_FILLED);
            let arc = |end: Vector2, r: f32, sweep| D2D1_ARC_SEGMENT {
                point: end,
                size: D2D_SIZE_F {
                    width: r,
                    height: r,
                },
                rotationAngle: 0.,
                sweepDirection: sweep,
                arcSize: D2D1_ARC_SIZE_SMALL,
            };
            sink.AddArc(&arc(point(x, ear), ear, D2D1_SWEEP_DIRECTION_CLOCKWISE));
            sink.AddLine(point(x, height - radius));
            sink.AddArc(&arc(
                point(x + radius, height),
                radius,
                D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
            ));
            sink.AddLine(point(x + width - radius, height));
            sink.AddArc(&arc(
                point(x + width, height - radius),
                radius,
                D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
            ));
            sink.AddLine(point(x + width, ear));
            sink.AddArc(&arc(
                point(x + width + ear, 0.),
                ear,
                D2D1_SWEEP_DIRECTION_CLOCKWISE,
            ));
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            path.cast()
        }
    }
    pub fn clip_shell(&self) -> Result<()> {
        unsafe {
            let mut parameters = D2D1_LAYER_PARAMETERS1 {
                contentBounds: D2D_RECT_F {
                    left: 0.,
                    top: 0.,
                    right: self.width as f32 / self.scale,
                    bottom: self.height as f32 / self.scale,
                },
                geometricMask: std::mem::ManuallyDrop::new(self.shell_mask.clone()),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2::identity(),
                opacity: 1.,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
            };
            self.context.PushLayer(&parameters, None);
            std::mem::ManuallyDrop::drop(&mut parameters.geometricMask);
            Ok(())
        }
    }
    pub fn unclip_shell(&self) {
        unsafe {
            self.context.PopLayer();
        }
    }
    fn tint(&self, value: u32, opacity: f32) {
        unsafe {
            self.brush.SetColor(&color(value, opacity));
        }
    }
    pub fn prepare_motion_blur(&mut self, commands: &[Paint], deviation: f32) -> Result<()> {
        if deviation <= 0. {
            self.motion_blur = None;
            return Ok(());
        }
        unsafe {
            if self.motion_blur.is_none() {
                let bitmap = self.context.CreateBitmap(
                    D2D_SIZE_U {
                        width: self.width,
                        height: self.height,
                    },
                    None,
                    0,
                    &D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96. * self.scale,
                        dpiY: 96. * self.scale,
                        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
                        ..Default::default()
                    },
                )?;
                let effect = self.context.CreateEffect(&CLSID_D2D1GaussianBlur)?;
                effect.SetInput(0, &bitmap, true);
                effect.SetValue(
                    D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32,
                    D2D1_PROPERTY_TYPE_ENUM,
                    &D2D1_BORDER_MODE_HARD.0.to_ne_bytes(),
                )?;
                self.motion_blur = Some((bitmap, effect));
            }
            let (bitmap, effect) = self.motion_blur.as_ref().expect("blur target initialized");
            effect.SetValue(
                D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,
                D2D1_PROPERTY_TYPE_FLOAT,
                &deviation.to_ne_bytes(),
            )?;
            let target = self.context.GetTarget()?;
            self.context.SetTarget(bitmap);
            self.context.Clear(None);
            let result = self.paint(commands, 1., (0., 0.));
            self.context.SetTarget(&target);
            result
        }
    }

    pub fn draw_motion_blur(&self, opacity: f32, offset: (f32, f32)) -> Result<()> {
        if let Some((_, effect)) = &self.motion_blur {
            unsafe {
                let image = effect.GetOutput()?;
                self.opacity_layer(opacity);
                self.context.DrawImage(
                    &image,
                    Some(&point(offset.0, offset.1)),
                    None,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    D2D1_COMPOSITE_MODE_SOURCE_OVER,
                );
                self.context.PopLayer();
            }
        }
        Ok(())
    }
    pub fn paint(&mut self, commands: &[Paint], opacity: f32, offset: (f32, f32)) -> Result<()> {
        unsafe {
            let scene_opacity = opacity;
            let outer_layer = opacity < 0.999;
            let opacity = 1.;
            let mut scope_layer = false;
            let base_transform = Matrix3x2 {
                M11: 1.,
                M12: 0.,
                M21: 0.,
                M22: 1.,
                M31: offset.0,
                M32: offset.1,
            };
            let mut current_transform = base_transform;
            let mut transform_stack: Vec<Matrix3x2> = Vec::new();
            self.context.SetTransform(&current_transform);
            if outer_layer {
                self.opacity_layer(scene_opacity);
            }
            for command in commands {
                match command {
                    Paint::Round(r, radius, c) => {
                        self.tint(*c, opacity);
                        self.context
                            .FillRoundedRectangle(&rounded(*r, *radius), &self.brush);
                    }
                    Paint::AlphaRound(r, radius, color, alpha) => {
                        self.tint(*color, *alpha);
                        self.context
                            .FillRoundedRectangle(&rounded(*r, *radius), &self.brush);
                    }
                    Paint::AlphaBorder(r, radius, color, width, alpha) => {
                        self.tint(*color, *alpha);
                        self.context.DrawRoundedRectangle(
                            &rounded(*r, *radius),
                            &self.brush,
                            *width,
                            None,
                        );
                    }
                    Paint::GradientRound(r, radius, top, bottom) => {
                        let key = (*top, *bottom);
                        if !self.gradients.contains_key(&key) {
                            let target: &ID2D1RenderTarget = &self.context;
                            let stops = target.CreateGradientStopCollection(
                                &[
                                    D2D1_GRADIENT_STOP {
                                        position: 0.,
                                        color: color(*top, 1.),
                                    },
                                    D2D1_GRADIENT_STOP {
                                        position: 1.,
                                        color: color(*bottom, 1.),
                                    },
                                ],
                                D2D1_GAMMA_2_2,
                                D2D1_EXTEND_MODE_CLAMP,
                            )?;
                            let gradient = self.context.CreateLinearGradientBrush(
                                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                                    startPoint: point(r.x, r.y),
                                    endPoint: point(r.x, r.y + r.h),
                                },
                                None,
                                &stops,
                            )?;
                            self.gradients.insert(key, gradient);
                        }
                        let brush = &self.gradients[&key];
                        brush.SetStartPoint(point(r.x, r.y));
                        brush.SetEndPoint(point(r.x, r.y + r.h));
                        brush.SetOpacity(opacity);
                        self.context
                            .FillRoundedRectangle(&rounded(*r, *radius), brush);
                    }
                    Paint::Border(r, radius, c, width) => {
                        self.tint(*c, opacity);
                        self.context.DrawRoundedRectangle(
                            &rounded(*r, *radius),
                            &self.brush,
                            *width,
                            None,
                        );
                    }
                    Paint::Line(x, y, x2, y2, c, width) => {
                        self.tint(*c, opacity);
                        self.context.DrawLine(
                            point(*x, *y),
                            point(*x2, *y2),
                            &self.brush,
                            *width,
                            &self.device.round_stroke,
                        );
                    }
                    Paint::Text(r, text, size, c, weight, center) => {
                        self.text(*r, text, *size, *c, *weight, *center, opacity)?
                    }
                    Paint::Icon(r, text, c) => self.vector_icon(*r, text, *c, opacity)?,
                    Paint::Strike(r, text, size, color) => {
                        let width = self.text_width(*r, text, *size, 400)?.min(r.w);
                        self.tint(*color, opacity);
                        self.context.DrawLine(
                            point(r.x, r.y + if r.h < 20. { 8. } else { 13. }),
                            point(r.x + width, r.y + if r.h < 20. { 8. } else { 13. }),
                            &self.brush,
                            1.2,
                            None,
                        );
                    }
                    Paint::Paragraph(r, text, size, color) => {
                        let layout = self.layout(*r, text, *size, 400, false, true)?;
                        self.tint(*color, opacity);
                        self.context.DrawTextLayout(
                            point(r.x, r.y),
                            &layout,
                            &self.brush,
                            D2D1_DRAW_TEXT_OPTIONS_CLIP,
                        );
                    }
                    Paint::TextRun(r, spans, spacing) => {
                        let mut x = r.x;
                        for span in spans {
                            let height = if span.bottom { span.size * 1.3 } else { r.h };
                            let at = Rect::new(
                                x,
                                r.y + if span.bottom { r.h - height } else { 0. },
                                (r.x + r.w - x).max(0.),
                                height,
                            );
                            let width = self.text_width(at, &span.text, span.size, span.weight)?;
                            self.text(
                                at,
                                &span.text,
                                span.size,
                                span.color,
                                span.weight,
                                false,
                                opacity,
                            )?;
                            x += width + spacing;
                        }
                    }
                    Paint::ButtonContent(r, label, glyph) => {
                        let glyph_w = if glyph.is_empty() { 0. } else { 12. };
                        let gap = if !label.is_empty() && glyph_w > 0. {
                            4.
                        } else {
                            0.
                        };
                        let label_w = if label.is_empty() {
                            0.
                        } else {
                            self.text_width(*r, label, 10.5, 600)?
                                .min((r.w - 12. - glyph_w - gap).max(0.))
                        };
                        let x = r.x + (r.w - glyph_w - gap - label_w) / 2.;
                        if glyph_w > 0. {
                            self.vector_icon(
                                Rect::new(x, r.y + (r.h - 12.) / 2., 12., 12.),
                                glyph,
                                0xffffff,
                                opacity,
                            )?;
                        }
                        if label_w > 0. {
                            self.text(
                                Rect::new(x + glyph_w + gap, r.y, label_w + 0.5, r.h),
                                label,
                                10.5,
                                0xffffff,
                                600,
                                false,
                                opacity,
                            )?;
                        }
                    }
                    Paint::ProgressRing(r, value, tint, open) => {
                        if !open {
                            self.tint(0x3c2024, opacity);
                            self.context.DrawEllipse(
                                &D2D1_ELLIPSE {
                                    point: point(r.x + r.w / 2., r.y + r.h / 2.),
                                    radiusX: r.w / 2. - r.w * 3.5 / 72.,
                                    radiusY: r.h / 2. - r.w * 3.5 / 72.,
                                },
                                &self.brush,
                                r.w * 7. / 72.,
                                None,
                            );
                        }
                        let draw_arc = |this: &mut Self, amount: f32, color: u32| -> Result<()> {
                            if amount <= 0. {
                                return Ok(());
                            }
                            let start = if *open { 135_f32 } else { -90_f32 };
                            let sweep = if *open { 270. } else { 359.5 };
                            let end = (start + sweep * amount).to_radians();
                            let begin = start.to_radians();
                            let center = point(r.x + r.w / 2., r.y + r.h / 2.);
                            let radius = r.w * 0.43;
                            let path = this.device.factory.CreatePathGeometry()?;
                            let sink = path.Open()?;
                            sink.BeginFigure(
                                point(
                                    center.X + radius * begin.cos(),
                                    center.Y + radius * begin.sin(),
                                ),
                                D2D1_FIGURE_BEGIN_HOLLOW,
                            );
                            sink.AddArc(&D2D1_ARC_SEGMENT {
                                point: point(
                                    center.X + radius * end.cos(),
                                    center.Y + radius * end.sin(),
                                ),
                                size: D2D_SIZE_F {
                                    width: radius,
                                    height: radius,
                                },
                                rotationAngle: 0.,
                                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                                arcSize: if sweep * amount > 180. {
                                    D2D1_ARC_SIZE_LARGE
                                } else {
                                    D2D1_ARC_SIZE_SMALL
                                },
                            });
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                            sink.Close()?;
                            this.tint(color, opacity);
                            this.context
                                .DrawGeometry(&path, &this.brush, r.w * 0.08, None);
                            Ok(())
                        };
                        if *open {
                            draw_arc(self, 1., 0x102d45)?;
                        }
                        draw_arc(self, *value, *tint)?;
                    }
                    Paint::Ring(r, progress, c, width, track) => {
                        self.ring(*r, *progress, *c, *width, *track, opacity)?
                    }
                    Paint::Bitmap(r, w, h, pixels) | Paint::RoundedBitmap(r, w, h, pixels, _) => {
                        let key = (std::sync::Arc::as_ptr(pixels) as usize, *w, *h);
                        self.used_images.insert(key);
                        if !self.images.contains_key(&key) {
                            let mut bgra = Vec::with_capacity(pixels.len());
                            for p in pixels.chunks_exact(4) {
                                let a = u16::from(p[3]);
                                bgra.extend_from_slice(&[
                                    ((u16::from(p[2]) * a + 127) / 255) as u8,
                                    ((u16::from(p[1]) * a + 127) / 255) as u8,
                                    ((u16::from(p[0]) * a + 127) / 255) as u8,
                                    p[3],
                                ]);
                            }
                            if bgra.len() != (*w as usize) * (*h as usize) * 4 {
                                return Err(windows::core::Error::from_hresult(
                                    windows::Win32::Foundation::E_INVALIDARG,
                                ));
                            }
                            let props = D2D1_BITMAP_PROPERTIES1 {
                                pixelFormat: D2D1_PIXEL_FORMAT {
                                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                                },
                                dpiX: 96.,
                                dpiY: 96.,
                                ..Default::default()
                            };
                            self.images.insert(
                                key,
                                self.context.CreateBitmap(
                                    D2D_SIZE_U {
                                        width: *w,
                                        height: *h,
                                    },
                                    Some(bgra.as_ptr().cast()),
                                    w * 4,
                                    &props,
                                )?,
                            );
                        }
                        let round = if let Paint::RoundedBitmap(_, _, _, _, radius) = command {
                            Some(*radius)
                        } else {
                            None
                        };
                        if let Some(radius) = round {
                            let mask = self
                                .device
                                .factory
                                .CreateRoundedRectangleGeometry(&rounded(*r, radius))?;
                            let mut params = D2D1_LAYER_PARAMETERS1 {
                                contentBounds: rect(*r),
                                geometricMask: std::mem::ManuallyDrop::new(Some(mask.cast()?)),
                                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                                maskTransform: Matrix3x2::identity(),
                                opacity: 1.,
                                opacityBrush: std::mem::ManuallyDrop::new(None),
                                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                            };
                            self.context.PushLayer(&params, None);
                            std::mem::ManuallyDrop::drop(&mut params.geometricMask);
                        }
                        let source = if round.is_some() {
                            let scale = (r.w / *w as f32).max(r.h / *h as f32);
                            let sw = r.w / scale;
                            let sh = r.h / scale;
                            Some(rect(Rect::new(
                                (*w as f32 - sw) / 2.,
                                (*h as f32 - sh) / 2.,
                                sw,
                                sh,
                            )))
                        } else {
                            None
                        };
                        self.context.DrawBitmap(
                            &self.images[&key],
                            Some(&rect(*r)),
                            opacity,
                            D2D1_INTERPOLATION_MODE_LINEAR,
                            source.as_ref().map(|rect| rect as *const D2D_RECT_F),
                            None,
                        );
                        if round.is_some() {
                            self.context.PopLayer();
                        }
                    }
                    Paint::PushTransform {
                        x,
                        y,
                        angle,
                        scale_x,
                        scale_y,
                    } => {
                        transform_stack.push(current_transform);
                        let (sin, cos) = angle.sin_cos();
                        let local = Matrix3x2 {
                            M11: cos * *scale_x,
                            M12: sin * *scale_y,
                            M21: -sin * *scale_x,
                            M22: cos * *scale_y,
                            M31: *x,
                            M32: *y,
                        };
                        current_transform = mul_matrix(local, current_transform);
                        self.context.SetTransform(&current_transform);
                    }
                    Paint::PopTransform => {
                        if let Some(previous) = transform_stack.pop() {
                            current_transform = previous;
                            self.context.SetTransform(&current_transform);
                        }
                    }
                    Paint::Ellipse {
                        center,
                        radius,
                        color: tint_color,
                        alpha,
                        stroke,
                    } => {
                        let ellipse = D2D1_ELLIPSE {
                            point: point(center.0, center.1),
                            radiusX: radius.0,
                            radiusY: radius.1,
                        };
                        self.tint(*tint_color, *alpha * opacity);
                        if let Some(width) = stroke {
                            self.context.DrawEllipse(
                                &ellipse,
                                &self.brush,
                                *width,
                                &self.device.round_stroke,
                            );
                        } else {
                            self.context.FillEllipse(&ellipse, &self.brush);
                        }
                    }
                    Paint::VectorPath {
                        start,
                        segments,
                        closed,
                        fill,
                        stroke,
                    } => {
                        let path = self.device.factory.CreatePathGeometry()?;
                        let sink = path.Open()?;
                        sink.BeginFigure(
                            point(start.0, start.1),
                            if fill.is_some() {
                                D2D1_FIGURE_BEGIN_FILLED
                            } else {
                                D2D1_FIGURE_BEGIN_HOLLOW
                            },
                        );
                        for segment in segments {
                            match *segment {
                                PathSegment::Line(x, y) => sink.AddLine(point(x, y)),
                                PathSegment::Quad(cx, cy, x, y) => {
                                    sink.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT {
                                        point1: point(cx, cy),
                                        point2: point(x, y),
                                    });
                                }
                                PathSegment::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                                    sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                                        point1: point(c1x, c1y),
                                        point2: point(c2x, c2y),
                                        point3: point(x, y),
                                    });
                                }
                            }
                        }
                        sink.EndFigure(if *closed {
                            D2D1_FIGURE_END_CLOSED
                        } else {
                            D2D1_FIGURE_END_OPEN
                        });
                        sink.Close()?;
                        if let Some((fill_color, fill_alpha)) = fill {
                            self.tint(*fill_color, *fill_alpha * opacity);
                            self.context.FillGeometry(&path, &self.brush, None);
                        }
                        if let Some((stroke_color, stroke_width, stroke_alpha)) = stroke {
                            self.tint(*stroke_color, *stroke_alpha * opacity);
                            self.context.DrawGeometry(
                                &path,
                                &self.brush,
                                *stroke_width,
                                &self.device.round_stroke,
                            );
                        }
                    }
                    Paint::Clip(r) => self
                        .context
                        .PushAxisAlignedClip(&rect(*r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE),
                    Paint::Unclip => self.context.PopAxisAlignedClip(),
                    Paint::Opacity(value) => {
                        if scope_layer {
                            self.context.PopLayer();
                            scope_layer = false;
                        }
                        if *value < 0.999 {
                            self.opacity_layer(*value);
                            scope_layer = true;
                        }
                    }
                }
            }
            if scope_layer {
                self.context.PopLayer();
            }
            if outer_layer {
                self.context.PopLayer();
            }
            self.context.SetTransform(&Matrix3x2::identity());
            Ok(())
        }
    }
    fn opacity_layer(&self, opacity: f32) {
        unsafe {
            self.context.PushLayer(
                &D2D1_LAYER_PARAMETERS1 {
                    contentBounds: D2D_RECT_F {
                        left: -16384.,
                        top: -16384.,
                        right: 16384.,
                        bottom: 16384.,
                    },
                    geometricMask: std::mem::ManuallyDrop::new(None),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                },
                None,
            );
        }
    }
    fn layout(
        &mut self,
        r: Rect,
        value: &str,
        size: f32,
        weight: u16,
        center: bool,
        wrap: bool,
    ) -> Result<IDWriteTextLayout> {
        unsafe {
            let key = TextKey {
                text: value.into(),
                size: size.to_bits(),
                width: r.w.to_bits(),
                height: r.h.to_bits(),
                weight,
                center,
                wrap,
            };
            self.used_layouts.insert(key.clone());
            if !self.layouts.contains_key(&key) {
                let format = self.device.write.CreateTextFormat(
                    w!("Segoe UI Variable Text"),
                    None,
                    DWRITE_FONT_WEIGHT(i32::from(weight)),
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!(""),
                )?;
                format.SetTextAlignment(if center {
                    DWRITE_TEXT_ALIGNMENT_CENTER
                } else {
                    DWRITE_TEXT_ALIGNMENT_LEADING
                })?;
                format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                format.SetWordWrapping(if wrap {
                    DWRITE_WORD_WRAPPING_WRAP
                } else {
                    DWRITE_WORD_WRAPPING_NO_WRAP
                })?;
                let trimming = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    ..Default::default()
                };
                let ellipsis = self.device.write.CreateEllipsisTrimmingSign(&format)?;
                format.SetTrimming(&trimming, &ellipsis)?;
                let layout = self.device.write.CreateTextLayout(
                    &value.encode_utf16().collect::<Vec<_>>(),
                    &format,
                    r.w,
                    r.h,
                )?;
                self.layouts.insert(key.clone(), layout);
            }
            Ok(self.layouts[&key].clone())
        }
    }
    fn text_width(&mut self, r: Rect, value: &str, size: f32, weight: u16) -> Result<f32> {
        let layout = self.layout(r, value, size, weight, false, false)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe {
            layout.GetMetrics(&mut metrics)?;
        }
        Ok(metrics.widthIncludingTrailingWhitespace)
    }
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        r: Rect,
        value: &str,
        size: f32,
        c: u32,
        weight: u16,
        center: bool,
        opacity: f32,
    ) -> Result<()> {
        unsafe {
            if value.is_empty() || r.w <= 0. || r.h <= 0. {
                return Ok(());
            }
            let layout = self.layout(r, value, size, weight, center, false)?;
            self.tint(c, opacity);
            let target: &ID2D1RenderTarget = &self.context;
            target.DrawTextLayout(
                point(r.x, r.y),
                &layout,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
            Ok(())
        }
    }
    fn vector_icon(&mut self, r: Rect, name: &str, tint: u32, opacity: f32) -> Result<()> {
        unsafe {
            if r.w <= 0. || r.h <= 0. || opacity <= 0. {
                return Ok(());
            }
            let key = (name.to_owned(), tint, r.w.to_bits(), r.h.to_bits());
            self.used_vectors.insert(key.clone());
            if !self.vectors.contains_key(&key) {
                let svg = super::icons::svg(name, tint, r.w, r.h);
                let stream = windows::Win32::UI::Shell::SHCreateMemStream(Some(svg.as_bytes()))
                    .ok_or_else(|| {
                        windows::core::Error::from_hresult(
                            windows::Win32::Foundation::E_OUTOFMEMORY,
                        )
                    })?;
                self.vectors.insert(
                    key.clone(),
                    self.svg_context.CreateSvgDocument(
                        &stream,
                        D2D_SIZE_F {
                            width: r.w,
                            height: r.h,
                        },
                    )?,
                );
            }
            let mut transform = Matrix3x2::identity();
            self.context.GetTransform(&mut transform);
            let at = mul_matrix(
                Matrix3x2 {
                    M11: 1.,
                    M12: 0.,
                    M21: 0.,
                    M22: 1.,
                    M31: r.x,
                    M32: r.y,
                },
                transform,
            );
            self.context.SetTransform(&at);
            let translucent = opacity < 0.999;
            if translucent {
                self.context.PushLayer(
                    &D2D1_LAYER_PARAMETERS1 {
                        contentBounds: rect(Rect::new(0., 0., r.w, r.h)),
                        geometricMask: std::mem::ManuallyDrop::new(None),
                        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        maskTransform: Matrix3x2::identity(),
                        opacity,
                        opacityBrush: std::mem::ManuallyDrop::new(None),
                        layerOptions: D2D1_LAYER_OPTIONS1_NONE,
                    },
                    None,
                );
            }
            self.svg_context.DrawSvgDocument(&self.vectors[&key]);
            if translucent {
                self.context.PopLayer();
            }
            self.context.SetTransform(&transform);
            Ok(())
        }
    }
    fn ring(
        &self,
        r: Rect,
        progress: f32,
        c: u32,
        width: f32,
        track: u32,
        opacity: f32,
    ) -> Result<()> {
        unsafe {
            let center = point(r.x + r.w / 2., r.y + r.h / 2.);
            let radius = (r.w.min(r.h) - width) / 2.;
            let ellipse = D2D1_ELLIPSE {
                point: center,
                radiusX: radius,
                radiusY: radius,
            };
            self.tint(track, opacity);
            self.context.DrawEllipse(&ellipse, &self.brush, width, None);
            self.tint(c, opacity);
            if progress >= 0.999 {
                self.context.DrawEllipse(&ellipse, &self.brush, width, None);
            } else if progress > 0.0001 {
                let angle = progress * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let path = self.device.factory.CreatePathGeometry()?;
                let sink = path.Open()?;
                sink.BeginFigure(point(center.X, center.Y - radius), D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: point(
                        center.X + radius * angle.cos(),
                        center.Y + radius * angle.sin(),
                    ),
                    size: D2D_SIZE_F {
                        width: radius,
                        height: radius,
                    },
                    rotationAngle: 0.,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: if progress > 0.5 {
                        D2D1_ARC_SIZE_LARGE
                    } else {
                        D2D1_ARC_SIZE_SMALL
                    },
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                self.context.DrawGeometry(&path, &self.brush, width, None);
            }
            Ok(())
        }
    }
}
fn mul_matrix(a: Matrix3x2, b: Matrix3x2) -> Matrix3x2 {
    Matrix3x2 {
        M11: a.M11 * b.M11 + a.M12 * b.M21,
        M12: a.M11 * b.M12 + a.M12 * b.M22,
        M21: a.M21 * b.M11 + a.M22 * b.M21,
        M22: a.M21 * b.M12 + a.M22 * b.M22,
        M31: a.M31 * b.M11 + a.M32 * b.M21 + b.M31,
        M32: a.M31 * b.M12 + a.M32 * b.M22 + b.M32,
    }
}
fn color(value: u32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((value >> 16) & 255) as f32 / 255.,
        g: ((value >> 8) & 255) as f32 / 255.,
        b: (value & 255) as f32 / 255.,
        a,
    }
}
fn point(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}
fn rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}
fn rounded(r: Rect, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: rect(r),
        radiusX: radius,
        radiusY: radius,
    }
}

#[cfg(test)]
#[path = "../../../tests/fixtures/native_render.rs"]
mod tests;
