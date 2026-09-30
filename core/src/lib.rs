pub mod actions;
pub mod config;
pub mod digest;
pub mod keys;
pub mod pass;
pub mod ping;
pub mod ssh;
pub mod tr064;
pub mod widgets;

use anyhow::Context;

/// Eine erreichbare FRITZ!Box (im LAN oder über WireGuard — gleiche URL).
/// `pass` wird zur Laufzeit vom Frontend aufgelöst (Keyring/Env/Prompt),
/// nie aus der Config-Datei gelesen.
#[derive(Debug, Clone)]
pub struct BoxProfile {
    pub name: String,
    pub base_url: String,
    pub user: String,
    pub pass: String,
}

/// Ein steuerbarer Rechner im Heimnetz.
#[derive(Debug, Clone)]
pub struct Host {
    pub id: String,
    pub mac: String,
    pub note: String,
    /// SSH-Ziel (alias oder user@host) — leer = kein SSH-Provider.
    pub ssh: String,
}

/// Live-Zustand eines Hosts laut Box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostStatus {
    pub active: bool,
    pub ip: Option<String>,
    pub hostname: Option<String>,
}

/// Wakes einen Host über die Box. 💥
pub fn wake(box_: &BoxProfile, host: &Host) -> anyhow::Result<()> {
    tr064::wake_on_lan(box_, &host.mac).with_context(|| format!("wake {} ({})", host.id, host.mac))
}

/// Fragt den Zustand eines Hosts bei der Box ab.
pub fn status(box_: &BoxProfile, host: &Host) -> anyhow::Result<HostStatus> {
    tr064::host_status(box_, &host.mac).with_context(|| format!("status {}", host.id))
}
