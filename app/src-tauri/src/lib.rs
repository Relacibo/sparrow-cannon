use fritz_cannon_core::config::ConfigFile;
use fritz_cannon_core::{pass, BoxProfile, Host};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusRow {
    id: String,
    mac: String,
    note: String,
    state: String,
    ip: Option<String>,
    hostname: Option<String>,
}

/// Config + Passwort laden, Default-Box wählen.
fn setup() -> Result<(BoxProfile, BTreeMap<String, Host>), String> {
    let conf = ConfigFile::load_default().map_err(|e| e.to_string())?;
    let pass = pass::resolve_from_env_or_keyring()
        .ok_or("kein Box-Passwort gefunden (CANNON_PASS / Keyring)")?;
    let profiles = conf.build_with_pass(&pass);
    let box_ = profiles
        .values()
        .next()
        .cloned()
        .ok_or("keine [boxes.*] in der config")?;
    Ok((box_, conf.hosts()))
}

#[tauri::command]
fn get_status() -> Result<Vec<StatusRow>, String> {
    let (box_, hosts) = setup()?;
    Ok(hosts
        .into_iter()
        .map(|(id, h)| match fritz_cannon_core::status(&box_, &h) {
            Ok(s) => StatusRow {
                id,
                mac: h.mac,
                note: h.note,
                state: if s.active { "UP" } else { "DOWN" }.into(),
                ip: s.ip,
                hostname: s.hostname,
            },
            Err(e) => StatusRow {
                id,
                mac: h.mac,
                note: h.note,
                state: "ERR".into(),
                ip: None,
                hostname: Some(e.to_string()),
            },
        })
        .collect())
}

#[tauri::command]
fn wake(host_id: String) -> Result<(), String> {
    let (box_, hosts) = setup()?;
    let h = hosts.get(&host_id).ok_or(format!("host '{host_id}' fehlt"))?;
    fritz_cannon_core::wake(&box_, h).map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_status, wake])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
