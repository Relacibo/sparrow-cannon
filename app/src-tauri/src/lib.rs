use fritz_cannon_core::config::ConfigFile;
use fritz_cannon_core::{pass, BoxProfile, Host};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;

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

/// Laufzeit-Passwort (z. B. per UI gesetzt, wenn kein Keyring existiert).
struct AppState(Mutex<Option<String>>);

/// Config + Passwort laden, Default-Box wählen.
fn setup(state: &AppState) -> Result<(BoxProfile, BTreeMap<String, Host>), String> {
    let conf = ConfigFile::load_default_or_builtin();
    let pass = state
        .0
        .lock()
        .unwrap()
        .clone()
        .or_else(pass::resolve_from_env_or_keyring)
        .ok_or("kein Box-Passwort — bitte unten eingeben")?;
    let profiles = conf.build_with_pass(&pass);
    let box_ = profiles
        .values()
        .next()
        .cloned()
        .ok_or("keine [boxes.*] in der config")?;
    Ok((box_, conf.hosts()))
}

#[tauri::command]
fn set_password(p: String, state: tauri::State<AppState>) -> Result<(), String> {
    *state.0.lock().unwrap() = Some(p);
    Ok(())
}

#[tauri::command]
fn get_status(state: tauri::State<AppState>) -> Result<Vec<StatusRow>, String> {
    let (box_, hosts) = setup(&state)?;
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
fn wake(host_id: String, state: tauri::State<AppState>) -> Result<(), String> {
    let (box_, hosts) = setup(&state)?;
    let h = hosts.get(&host_id).ok_or(format!("host '{host_id}' fehlt"))?;
    fritz_cannon_core::wake(&box_, h).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![get_status, wake, set_password])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
