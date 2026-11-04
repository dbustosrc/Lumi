//! Event-driven Bluetooth audio device connection and battery watcher.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use windows::Devices::Enumeration::{
    DeviceInformation, DeviceInformationKind, DeviceInformationUpdate, DeviceWatcher,
};
use windows::Foundation::{IPropertyValue, TypedEventHandler};
use windows::core::{Array, HSTRING, IInspectable, Interface};
use windows_collections::{IIterable, IMapView};

use crate::application::events::{AppEvent, EventSender};
use crate::domain::device::HeadphoneStatus;

const BLUETOOTH_PAIRED_AQS: &str = "System.Devices.Aep.ProtocolId:=\"{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}\" \
     AND System.Devices.Aep.IsPaired:=System.StructuredQueryType.Boolean#True";
const PROP_IS_CONNECTED: &str = "System.Devices.Aep.IsConnected";
const PROP_CATEGORY: &str = "System.Devices.Aep.Category";
const PROP_BATTERY: &str = "{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2";

#[derive(Clone)]
struct TrackedEndpoint {
    name: String,
    is_audio: bool,
    is_connected: bool,
    battery_percent: Option<u8>,
}

#[derive(Default)]
struct EndpointRegistry {
    endpoints: HashMap<String, TrackedEndpoint>,
    has_completed_initial_scan: bool,
}

/// Keeps the WinRT [`DeviceWatcher`] alive for the lifetime of the application.
pub struct HeadphonesWatcher {
    _watcher: DeviceWatcher,
}

/// Starts listening for paired Bluetooth audio devices connecting to Windows.
pub fn watch_headphones(events: EventSender) -> windows::core::Result<HeadphonesWatcher> {
    let properties = IIterable::<HSTRING>::from(vec![
        HSTRING::from(PROP_IS_CONNECTED),
        HSTRING::from(PROP_CATEGORY),
        HSTRING::from(PROP_BATTERY),
    ]);
    let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
        &HSTRING::from(BLUETOOTH_PAIRED_AQS),
        &properties,
        DeviceInformationKind::AssociationEndpoint,
    )?;
    let registry = Arc::new(Mutex::new(EndpointRegistry::default()));
    register_added_handler(&watcher, Arc::clone(&registry), events.clone())?;
    register_updated_handler(&watcher, Arc::clone(&registry), events.clone())?;
    register_removed_handler(&watcher, Arc::clone(&registry), events.clone())?;
    register_completed_handler(&watcher, registry, events)?;
    watcher.Start()?;
    Ok(HeadphonesWatcher { _watcher: watcher })
}

fn register_added_handler(
    watcher: &DeviceWatcher,
    registry: Arc<Mutex<EndpointRegistry>>,
    events: EventSender,
) -> windows::core::Result<()> {
    watcher.Added(&TypedEventHandler::<DeviceWatcher, DeviceInformation>::new(
        move |_, info_ref| {
            if let Some(info) = info_ref.as_ref() {
                handle_device_added(&registry, &events, info);
            }
            Ok(())
        },
    ))?;
    Ok(())
}

fn register_updated_handler(
    watcher: &DeviceWatcher,
    registry: Arc<Mutex<EndpointRegistry>>,
    events: EventSender,
) -> windows::core::Result<()> {
    watcher.Updated(
        &TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(move |_, update_ref| {
            if let Some(update) = update_ref.as_ref() {
                handle_device_updated(&registry, &events, update);
            }
            Ok(())
        }),
    )?;
    Ok(())
}

fn register_removed_handler(
    watcher: &DeviceWatcher,
    registry: Arc<Mutex<EndpointRegistry>>,
    events: EventSender,
) -> windows::core::Result<()> {
    watcher.Removed(
        &TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(move |_, update_ref| {
            if let Some(update) = update_ref.as_ref() {
                handle_device_removed(&registry, &events, update);
            }
            Ok(())
        }),
    )?;
    Ok(())
}

fn register_completed_handler(
    watcher: &DeviceWatcher,
    registry: Arc<Mutex<EndpointRegistry>>,
    events: EventSender,
) -> windows::core::Result<()> {
    watcher.EnumerationCompleted(&TypedEventHandler::<DeviceWatcher, IInspectable>::new(
        move |_, _| {
            let status = registry.lock().ok().map(|mut guard| {
                guard.has_completed_initial_scan = true;
                active_audio_headphone(&guard.endpoints)
            });
            if let Some(current) = status {
                emit_state(&events, current, false);
            }
            Ok(())
        },
    ))?;
    Ok(())
}

fn handle_device_added(
    registry: &Mutex<EndpointRegistry>,
    events: &EventSender,
    info: &DeviceInformation,
) {
    let (Ok(id), Ok(name), Ok(props)) = (info.Id(), info.Name(), info.Properties()) else {
        return;
    };
    let endpoint = TrackedEndpoint {
        name: name.to_string_lossy(),
        is_audio: has_audio_category(&props),
        is_connected: read_bool_property(&props, PROP_IS_CONNECTED).unwrap_or(false),
        battery_percent: read_battery_property(&props),
    };
    let Ok(mut guard) = registry.lock() else {
        return;
    };
    let is_newly_connected =
        guard.has_completed_initial_scan && endpoint.is_audio && endpoint.is_connected;
    let status = HeadphoneStatus {
        name: endpoint.name.clone(),
        battery_percent: endpoint.battery_percent,
    };
    guard.endpoints.insert(id.to_string_lossy(), endpoint);
    drop(guard);
    if is_newly_connected {
        emit_state(events, Some(status), true);
    }
}

fn handle_device_updated(
    registry: &Mutex<EndpointRegistry>,
    events: &EventSender,
    update: &DeviceInformationUpdate,
) {
    let (Ok(id), Ok(props)) = (update.Id(), update.Properties()) else {
        return;
    };
    let Ok(mut guard) = registry.lock() else {
        return;
    };
    let is_ready = guard.has_completed_initial_scan;
    let Some(entry) = guard.endpoints.get_mut(&id.to_string_lossy()) else {
        return;
    };
    let was_connected = entry.is_connected;
    let prev_battery = entry.battery_percent;
    apply_endpoint_updates(entry, &props);
    if !is_ready || !entry.is_audio {
        return;
    }
    if !was_connected && entry.is_connected {
        let status = HeadphoneStatus {
            name: entry.name.clone(),
            battery_percent: entry.battery_percent,
        };
        drop(guard);
        emit_state(events, Some(status), true);
    } else if was_connected != entry.is_connected || prev_battery != entry.battery_percent {
        let status = active_audio_headphone(&guard.endpoints);
        drop(guard);
        emit_state(events, status, false);
    }
}

fn apply_endpoint_updates(entry: &mut TrackedEndpoint, props: &IMapView<HSTRING, IInspectable>) {
    if let Some(is_connected) = read_bool_property(props, PROP_IS_CONNECTED) {
        entry.is_connected = is_connected;
    }
    if let Some(percent) = read_battery_property(props) {
        entry.battery_percent = Some(percent);
    }
    if has_audio_category(props) {
        entry.is_audio = true;
    }
}

fn handle_device_removed(
    registry: &Mutex<EndpointRegistry>,
    events: &EventSender,
    update: &DeviceInformationUpdate,
) {
    let Ok(id) = update.Id() else { return };
    let Ok(mut guard) = registry.lock() else {
        return;
    };
    let removed = guard.endpoints.remove(&id.to_string_lossy());
    if guard.has_completed_initial_scan && removed.is_some_and(|ep| ep.is_audio && ep.is_connected)
    {
        let status = active_audio_headphone(&guard.endpoints);
        drop(guard);
        emit_state(events, status, false);
    }
}

fn active_audio_headphone(endpoints: &HashMap<String, TrackedEndpoint>) -> Option<HeadphoneStatus> {
    endpoints
        .values()
        .filter(|ep| ep.is_audio && ep.is_connected)
        .max_by(|left, right| {
            left.battery_percent
                .is_some()
                .cmp(&right.battery_percent.is_some())
                .then_with(|| right.name.cmp(&left.name))
        })
        .map(|ep| HeadphoneStatus {
            name: ep.name.clone(),
            battery_percent: ep.battery_percent,
        })
}

fn emit_state(events: &EventSender, status: Option<HeadphoneStatus>, is_newly_connected: bool) {
    if events
        .send(AppEvent::HeadphonesState {
            status,
            is_newly_connected,
        })
        .is_err()
    {
        tracing::warn!(event = "log.events.receiverClosed");
    }
}

fn read_bool_property(props: &IMapView<HSTRING, IInspectable>, key: &str) -> Option<bool> {
    let value = props.Lookup(&HSTRING::from(key)).ok()?;
    value.cast::<IPropertyValue>().ok()?.GetBoolean().ok()
}

fn read_battery_property(props: &IMapView<HSTRING, IInspectable>) -> Option<u8> {
    let value = props.Lookup(&HSTRING::from(PROP_BATTERY)).ok()?;
    let prop = value.cast::<IPropertyValue>().ok()?;
    let percent = prop
        .GetUInt8()
        .ok()
        .or_else(|| prop.GetInt32().ok().and_then(|num| u8::try_from(num).ok()))?;
    (percent <= 100).then_some(percent)
}

fn has_audio_category(props: &IMapView<HSTRING, IInspectable>) -> bool {
    let Ok(value) = props.Lookup(&HSTRING::from(PROP_CATEGORY)) else {
        return false;
    };
    let Ok(prop) = value.cast::<IPropertyValue>() else {
        return false;
    };
    if let Ok(single) = prop.GetString() {
        return is_audio_token(&single.to_string_lossy());
    }
    let mut items = Array::<HSTRING>::new();
    if prop.GetStringArray(&mut items).is_ok() {
        return items
            .iter()
            .any(|item| is_audio_token(&item.to_string_lossy()));
    }
    false
}

fn is_audio_token(category: &str) -> bool {
    let lower = category.to_ascii_lowercase();
    lower.contains("audio") || lower.contains("headphone") || lower.contains("headset")
}
