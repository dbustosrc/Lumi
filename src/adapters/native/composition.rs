//! Native visual tree: the DXGI content and the clipped host backdrop share one target.
use std::{
    cell::RefCell,
    mem::size_of,
    rc::{Rc, Weak},
};
use windows::Graphics::{IGeometrySource2D, IGeometrySource2D_Impl};
use windows::System::DispatcherQueueController;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{
    CompositionPath, CompositionPathGeometry, Compositor, SpriteVisual,
};
use windows::Win32::Foundation::{E_NOTIMPL, HWND};
use windows::Win32::Graphics::Direct2D::{ID2D1Factory, ID2D1Geometry};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute};
use windows::Win32::Graphics::Dxgi::IDXGISwapChain1;
use windows::Win32::System::WinRT::Composition::{ICompositorDesktopInterop, ICompositorInterop};
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    IGeometrySource2DInterop, IGeometrySource2DInterop_Impl,
};
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
};
use windows::core::{BOOL, Interface, Ref, Result, implement};
use windows_numerics::Vector2;

thread_local! {
    static RUNTIME: RefCell<Weak<Runtime>> = RefCell::new(Weak::new());
}

pub struct Runtime {
    compositor: Compositor,
    queue: DispatcherQueueController,
}

impl Runtime {
    pub fn acquire() -> Result<Rc<Self>> {
        RUNTIME.with(|cached| {
            if let Some(runtime) = cached.borrow().upgrade() {
                return Ok(runtime);
            }
            let queue = unsafe {
                CreateDispatcherQueueController(DispatcherQueueOptions {
                    dwSize: size_of::<DispatcherQueueOptions>() as u32,
                    threadType: DQTYPE_THREAD_CURRENT,
                    apartmentType: DQTAT_COM_NONE,
                })?
            };
            let runtime = Rc::new(Self {
                compositor: Compositor::new()?,
                queue,
            });
            *cached.borrow_mut() = Rc::downgrade(&runtime);
            Ok(runtime)
        })
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.compositor.Close();
        if let Ok(shutdown) = self.queue.ShutdownQueueAsync() {
            // AsyncStatus::Started is zero in the Windows ABI.
            while shutdown.Status().is_ok_and(|status| status.0 == 0) {
                let mut message = MSG::default();
                unsafe {
                    while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
}

pub struct WindowComposition {
    target: DesktopWindowTarget,
    content: SpriteVisual,
    backdrop: SpriteVisual,
    path: CompositionPathGeometry,
    enabled: bool,
    hwnd: HWND,
    runtime: Rc<Runtime>,
}

impl WindowComposition {
    pub fn check_glass(hwnd: HWND) -> Result<()> {
        let enabled = BOOL(1);
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_HOSTBACKDROPBRUSH,
                (&enabled as *const BOOL).cast(),
                size_of::<BOOL>() as u32,
            )
        }
    }

    pub fn new(runtime: Rc<Runtime>, hwnd: HWND, swap: &IDXGISwapChain1) -> Result<Self> {
        let compositor = &runtime.compositor;
        let interop: ICompositorInterop = compositor.cast()?;
        let surface = unsafe { interop.CreateCompositionSurfaceForSwapChain(swap)? };
        let content = compositor.CreateSpriteVisual()?;
        content.SetBrush(&compositor.CreateSurfaceBrushWithSurface(&surface)?)?;
        let backdrop = compositor.CreateSpriteVisual()?;
        backdrop.SetIsVisible(false)?;
        let path = compositor.CreatePathGeometry()?;
        backdrop.SetClip(&compositor.CreateGeometricClipWithGeometry(&path)?)?;
        let root = compositor.CreateContainerVisual()?;
        root.Children()?.InsertAtBottom(&backdrop)?;
        root.Children()?.InsertAtTop(&content)?;
        let interop: ICompositorDesktopInterop = compositor.cast()?;
        let target = unsafe { interop.CreateDesktopWindowTarget(hwnd, false)? };
        target.SetRoot(&root)?;
        Ok(Self {
            runtime,
            target,
            content,
            backdrop,
            path,
            enabled: false,
            hwnd,
        })
    }

    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        let size = Vector2 {
            X: width as f32,
            Y: height as f32,
        };
        self.content.SetSize(size)?;
        self.backdrop.SetSize(size)
    }

    pub fn glass_enabled(&self) -> bool {
        self.enabled
    }
    pub fn set_glass(&mut self, enabled: bool) -> Result<bool> {
        if enabled == self.enabled {
            return Ok(false);
        }
        let value = BOOL::from(enabled);
        unsafe {
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_USE_HOSTBACKDROPBRUSH,
                (&value as *const BOOL).cast(),
                size_of::<BOOL>() as u32,
            )?;
        }
        if enabled {
            self.backdrop
                .SetBrush(&self.runtime.compositor.CreateHostBackdropBrush()?)?;
        }
        self.backdrop.SetIsVisible(enabled)?;
        self.enabled = enabled;
        Ok(true)
    }

    pub fn clip(&self, geometry: ID2D1Geometry) -> Result<()> {
        if self.enabled {
            let source: IGeometrySource2D = GeometrySource(geometry).into();
            self.path.SetPath(&CompositionPath::Create(&source)?)?;
        }
        Ok(())
    }
}

impl Drop for WindowComposition {
    fn drop(&mut self) {
        let _ = self.target.Close();
    }
}

#[implement(IGeometrySource2D, IGeometrySource2DInterop)]
struct GeometrySource(ID2D1Geometry);

impl IGeometrySource2D_Impl for GeometrySource_Impl {}

impl IGeometrySource2DInterop_Impl for GeometrySource_Impl {
    fn GetGeometry(&self) -> Result<ID2D1Geometry> {
        Ok(self.0.clone())
    }

    fn TryGetGeometryUsingFactory(&self, _factory: Ref<'_, ID2D1Factory>) -> Result<ID2D1Geometry> {
        // Composition copies the source geometry when it uses a different D2D factory.
        Err(E_NOTIMPL.into())
    }
}
