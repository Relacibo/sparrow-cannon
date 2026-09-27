pub mod tr064;

use std::collections::BTreeMap;

/// Eine erreichbare FRITZ!Box (im LAN oder über WireGuard — gleiche URL).
#[derive(Debug, Clone)]
pub struct BoxProfile {
    pub name: String,
    pub base_url: String,
    pub user: String,
}

/// Ein steuerbarer Rechner im Heimnetz.
#[derive(Debug, Clone)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub mac: String,
    pub note: String,
}

/// Live-Zustand eines Hosts laut Box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostStatus {
    pub active: bool,
    pub ip: Option<String>,
}

/// Config: mehrere Hosts von Anfang an.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub boxes: Vec<BoxProfile>,
    pub hosts: Vec<Host>,
    /// id -> Host, komfortabler Zugriff
    pub by_id: BTreeMap<String, Host>,
}

impl Config {
    pub fn host(&self, id: &str) -> Option<&Host> {
        self.by_id.get(id)
    }
}

/// Wakes einen Host über die Box. 💥
pub fn wake(box_: &BoxProfile, host: &Host) -> anyhow::Result<()> {
    tr064::wake_on_lan(box_, &host.mac)
}

/// Fragt den Zustand eines Hosts bei der Box ab.
pub fn status(box_: &BoxProfile, host: &Host) -> anyhow::Result<HostStatus> {
    tr064::host_status(box_, &host.mac)
}
