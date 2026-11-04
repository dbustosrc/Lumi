//! Infrastructure layer: concrete adapters for the ports (network clients,
//! storage, audio capture and Windows integrations).

pub mod artwork;
pub mod logging;
pub mod lrclib;
pub mod microphone;
pub mod music_assistant;
pub mod paths;
pub mod settings_store;
pub mod shelf_platform;
pub mod shelf_store;
pub mod startup;
pub mod task_store;
pub mod water_store;
pub mod wyoming;

#[cfg(windows)]
pub mod windows;
