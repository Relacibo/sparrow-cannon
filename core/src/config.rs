use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::{BoxProfile, Host};

/// Config-Datei: `~/.config/fritz-cannon/config.toml`.
/// Passwörter gehören hier niemals hinein.
#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub boxes: BTreeMap<String, BoxFile>,
    #[serde(default)]
    pub hosts: BTreeMap<String, HostFile>,
}

#[derive(Debug, Deserialize)]
pub struct BoxFile {
    pub base_url: String,
    pub user: String,
}

#[derive(Debug, Deserialize)]
pub struct HostFile {
    pub mac: String,
    #[serde(default)]
    pub note: String,
}

impl ConfigFile {
    pub fn default_path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|_| PathBuf::from(".config"));
        base.join("fritz-cannon").join("config.toml")
    }

    pub fn load_default() -> anyhow::Result<Self> {
        let path = Self::default_path();
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("config nicht lesbar {}: {e}", path.display()))?;
        toml::from_str(&raw).map_err(|e| anyhow::anyhow!("config {} parse: {e}", path.display()))
    }

    /// Baut die Runtime-Config; `pass` wird auf alle Boxen angewendet
    /// (in der Praxis gibt es genau eine).
    pub fn build_with_pass(&self, pass: &str) -> BTreeMap<String, BoxProfile> {
        self.boxes
            .iter()
            .map(|(name, b)| {
                (
                    name.clone(),
                    BoxProfile {
                        name: name.clone(),
                        base_url: b.base_url.trim_end_matches('/').to_string(),
                        user: b.user.clone(),
                        pass: pass.to_string(),
                    },
                )
            })
            .collect()
    }

    pub fn hosts(&self) -> BTreeMap<String, Host> {
        self.hosts
            .iter()
            .map(|(id, h)| {
                (
                    id.clone(),
                    Host {
                        id: id.clone(),
                        mac: h.mac.clone(),
                        note: h.note.clone(),
                    },
                )
            })
            .collect()
    }
}
