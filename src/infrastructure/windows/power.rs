use windows::Foundation::EventHandler;
use windows::System::Power::{BatteryStatus, PowerManager, PowerSupplyStatus};
use windows::core::IInspectable;

use crate::application::events::{AppEvent, EventSender};
use crate::domain::device::PowerStatus;

/// Subscribes to WinRT power notifications: Windows pushes changes, so no
/// polling is needed. Registrations live for the whole process.
pub fn watch_power(events: EventSender) -> windows::core::Result<()> {
    publish_power(&events);
    let handler = EventHandler::<IInspectable>::new(move |_, _| {
        publish_power(&events);
        Ok(())
    });
    PowerManager::PowerSupplyStatusChanged(&handler)?;
    PowerManager::RemainingChargePercentChanged(&handler)?;
    PowerManager::BatteryStatusChanged(&handler)?;
    Ok(())
}

fn publish_power(events: &EventSender) {
    match read_power() {
        Ok(status) => {
            if events.send(AppEvent::Power(status)).is_err() {
                tracing::warn!(event = "log.events.receiverClosed");
            }
        }
        Err(error) => tracing::error!(event = "log.power.readFailed", error = %error),
    }
}

fn read_power() -> windows::core::Result<PowerStatus> {
    let battery = PowerManager::BatteryStatus()?;
    let supply = PowerManager::PowerSupplyStatus()?;
    let percent = PowerManager::RemainingChargePercent()?;
    Ok(PowerStatus {
        has_battery: battery != BatteryStatus::NotPresent,
        is_plugged_in: supply != PowerSupplyStatus::NotPresent,
        percent: percent.clamp(0, 100) as u8,
    })
}
