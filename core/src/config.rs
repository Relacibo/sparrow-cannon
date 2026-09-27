use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{BoxProfile, Host};

/// Config-Datei: `~/.config/sparrow-cannon/config.toml`.
/// Passwörter gehören hier niemals hinein.
#[derive(Debug, Deserialize, Serialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub boxes: BTreeMap<String, BoxFile>,
    #[serde(default)]
    pub hosts: BTreeMap<String, HostFile>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BoxFile {
    pub base_url: String,
    pub user: String,
}

#[derive(Debug, Deserialize, Serialize)]
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
        base.join("sparrow-cannon").join("config.toml")
    }

    pub fn load_default() -> anyhow::Result<Self> {
        Self::load_from(Self::default_path())
    }

    pub fn load_from(path: PathBuf) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("config nicht lesbar {}: {e}", path.display()))?;
        toml::from_str(&raw).map_err(|e| anyhow::anyhow!("config {} parse: {e}", path.display()))
    }

    /// Schreibt die Config (URL/User — niemals Passwörter!) zurück.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("config serialisieren: {e}"))?;
        std::fs::write(path, body)
            .map_err(|e| anyhow::anyhow!("config {} schreiben: {e}", path.display()))
    }

    /// Fügt eine Box hinzu oder aktualisiert sie (Setup-Modal / CLI).
    pub fn upsert_box(&mut self, id: &str, base_url: &str, user: &str) {
        self.boxes.insert(
            id.to_string(),
            BoxFile {
                base_url: base_url.trim_end_matches('/').to_string(),
                user: user.to_string(),
            },
        );
    }

    /// Eingebauter Fallback (Android hat kein ~/.config): die daheim-Box + pc.
    /// TODO: per tauri-plugin-store ablösen.
    pub fn builtin() -> Self {
        let mut boxes = BTreeMap::new();
        boxes.insert(
            "daheim".into(),
            BoxFile {
                base_url: "http://192.168.178.1:49000".into(),
                user: "fritz8427".into(),
            },
        );
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "pc".into(),
            HostFile {
                mac: "a8:a1:59:db:b3:7e".into(),
                note: "anton-bruckner".into(),
            },
        );
        Self { boxes, hosts }
    }

    pub fn load_default_or_builtin() -> Self {
        Self::load_default().unwrap_or_else(|_| Self::builtin())
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
