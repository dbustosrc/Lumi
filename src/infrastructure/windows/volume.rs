use std::sync::Arc;

use tokio::sync::Notify;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator,
    IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, eMultimedia, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::{GUID, PCWSTR, Result, implement};

pub(super) struct VolumeMonitor {
    devices: IMMDeviceEnumerator,
    listener: IMMNotificationClient,
    endpoint: Option<Endpoint>,
}

impl VolumeMonitor {
    pub fn new(device_changed: Arc<Notify>) -> Result<Self> {
        let devices: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let listener: IMMNotificationClient = DeviceChanged(device_changed).into();
        unsafe { devices.RegisterEndpointNotificationCallback(&listener)? };
        Ok(Self {
            devices,
            listener,
            endpoint: None,
        })
    }

    pub fn bind_default(&mut self, volume_changed: Arc<Notify>) -> Result<()> {
        self.endpoint = None;
        let volume: IAudioEndpointVolume = unsafe {
            self.devices
                .GetDefaultAudioEndpoint(eRender, eMultimedia)?
                .Activate(CLSCTX_ALL, None)?
        };
        let listener: IAudioEndpointVolumeCallback = VolumeChanged(volume_changed).into();
        unsafe { volume.RegisterControlChangeNotify(&listener)? };
        self.endpoint = Some(Endpoint { volume, listener });
        Ok(())
    }

    pub fn read(&self) -> Option<(f32, bool)> {
        let volume = &self.endpoint.as_ref()?.volume;
        unsafe {
            Some((
                volume.GetMasterVolumeLevelScalar().ok()?,
                volume.GetMute().ok()?.as_bool(),
            ))
        }
    }

    pub fn set(&self, level: f32) -> Result<()> {
        let endpoint = self.endpoint.as_ref().ok_or_else(|| {
            windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL)
        })?;
        unsafe {
            endpoint
                .volume
                .SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), &GUID::zeroed())?;
            endpoint.volume.SetMute(level <= 0.0, &GUID::zeroed())
        }
    }
}

impl Drop for VolumeMonitor {
    fn drop(&mut self) {
        let _ = unsafe {
            self.devices
                .UnregisterEndpointNotificationCallback(&self.listener)
        };
    }
}

struct Endpoint {
    volume: IAudioEndpointVolume,
    listener: IAudioEndpointVolumeCallback,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = unsafe { self.volume.UnregisterControlChangeNotify(&self.listener) };
    }
}

#[implement(IMMNotificationClient)]
struct DeviceChanged(Arc<Notify>);

impl IMMNotificationClient_Impl for DeviceChanged_Impl {
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> Result<()> {
        if flow == eRender && role == eMultimedia {
            self.0.notify_one();
        }
        Ok(())
    }

    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeChanged(Arc<Notify>);

impl IAudioEndpointVolumeCallback_Impl for VolumeChanged_Impl {
    fn OnNotify(&self, _: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        self.0.notify_one();
        Ok(())
    }
}
