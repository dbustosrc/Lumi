use std::path::PathBuf;

use super::presenter::ShelfEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: String,
    pub sender: String,
    pub pin: String,
    pub files: Vec<String>,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub outgoing: Vec<ShelfEntry>,
    pub received: Vec<ShelfEntry>,
    pub devices: Vec<Device>,
    pub request: Option<Request>,
    pub visible: bool,
    pub status: Option<&'static str>,
    pub received_status: Option<&'static str>,
    pub sending: bool,
    pub pin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tray {
    Outgoing,
    Received,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Browse,
    Stage(Vec<PathBuf>),
    Remove(Tray, uuid::Uuid),
    Clear(Tray),
    Discover(bool),
    Send(String),
    Decide { id: String, accept: bool },
}
