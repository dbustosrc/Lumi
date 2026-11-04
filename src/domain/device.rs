/// Battery and power-supply state of the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PowerStatus {
    pub has_battery: bool,
    pub is_plugged_in: bool,
    pub percent: u8,
}

/// A wireless audio device (headphones, earbuds, speakers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadphoneStatus {
    pub name: String,
    pub battery_percent: Option<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiState {
    Disconnected,
    Connected(String),
}
