use sparrow_cannon_core::config::ConfigFile;
use sparrow_cannon_core::{pass, BoxProfile, Host};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

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

/// Desktop: ~/.config/sparrow-cannon/config.toml.
/// Android: <app_data_dir>/config.toml.
fn config_path(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    #[cfg(target_os = "android")]
    {
        app.path()
            .app_data_dir()
            .expect("app_data_dir nicht auflösbar")
            .join("config.toml")
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        ConfigFile::default_path()
    }
}

/// Verzeichnis für den Datei-Fallback der Passwörter (nur Android relevant —
/// dort gibt es keinen Secret Service).
fn secrets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    #[cfg(target_os = "android")]
    {
        app.path()
            .app_data_dir()
            .expect("app_data_dir nicht auflösbar")
            .into()
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        None
    }
}

/// Alte box.txt (Vorgängerversion) entfernen — Werte leben jetzt in der
/// config.toml, das Passwort im Keyring bzw. unter secrets/.
fn migrate_legacy_box_txt(dir: &PathBuf, conf: &mut ConfigFile) {
    let legacy = dir.join("box.txt");
    let Ok(raw) = std::fs::read_to_string(&legacy) else {
        return;
    };
    let mut lines = raw.lines();
    let (Some(url), Some(user)) = (
        lines.next().map(str::trim),
        lines.next().map(str::trim),
    ) else {
        let _ = std::fs::remove_file(&legacy);
        return;
    };
    let legacy_pass = lines.next().map(str::trim).unwrap_or_default();
    if !url.is_empty() && !conf.boxes.is_empty() {
        let id = conf.boxes.keys().next().cloned().unwrap_or_default();
        conf.upsert_box(&id, url, user);
        let _ = conf.save_to(&config_path_static());
        if !legacy_pass.is_empty() {
            // Android-Fallback: Passwort in die neue secrets/<id>.txt übernehmen
            sparrow_cannon_core::pass::file_store(dir, &id, legacy_pass);
        }
        eprintln!("[cannon] legacy box.txt übernommen");
    }
    let _ = std::fs::remove_file(&legacy);
}

fn config_path_static() -> PathBuf {
    ConfigFile::default_path()
}

/// Config + Passwort laden, Default-Box wählen.
fn setup(app: &tauri::AppHandle) -> Result<(BoxProfile, BTreeMap<String, Host>), String> {
    use tauri::Manager;
    let path = config_path(app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| {
        let builtin = ConfigFile::builtin();
        let _ = builtin.save_to(&path);
        builtin
    });
    migrate_legacy_box_txt(
        &app.path()
            .app_data_dir()
            .expect("app_data_dir nicht auflösbar"),
        &mut conf,
    );
    let (box_id, _url, _user) = conf
        .boxes
        .iter()
        .next()
        .map(|(id, b)| (id.clone(), b.base_url.clone(), b.user.clone()))
        .ok_or("keine [boxes.*] in der config")?;
    let pass = pass::resolve(&box_id, secrets_dir(app).as_deref())
        .ok_or("kein Box-Passwort — bitte unten einrichten")?;
    let profiles = conf.build_with_pass(&pass);
    let box_ = profiles
        .get(&box_id)
        .cloned()
        .ok_or("box fehlt nach dem build")?;
    Ok((box_, conf.hosts()))
}

/// Werte zum Vorausfüllen des Setup-Modals.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxInfo {
    base_url: String,
    user: String,
    has_saved: bool,
}

#[tauri::command]
fn get_box_info(app: tauri::AppHandle) -> Result<BoxInfo, String> {
    use tauri::Manager;
    let path = config_path(&app);
    let conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    let (box_id, base_url, user) = conf
        .boxes
        .iter()
        .next()
        .map(|(id, b)| (id.clone(), b.base_url.clone(), b.user.clone()))
        .ok_or_else(|| "keine [boxes.*] in der config".to_string())?;
    let has_saved = pass::resolve(&box_id, secrets_dir(&app).as_deref()).is_some();
    Ok(BoxInfo {
        base_url,
        user,
        has_saved,
    })
}

#[tauri::command]
fn set_box_config(
    base_url: String,
    user: String,
    p: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    if !base_url.starts_with("http") {
        return Err("Box-URL muss mit http:// beginnen".into());
    }
    let path = config_path(&app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    let box_id = conf
        .boxes
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| "daheim".into());
    conf.upsert_box(&box_id, &base_url, &user);
    conf.save_to(&path).map_err(|e| e.to_string())?;
    let stored = pass::store(&box_id, &p, secrets_dir(&app).as_deref());
    eprintln!("[cannon] passwort gespeichert: {stored}");
    Ok(())
}

#[tauri::command]
fn get_status(app: tauri::AppHandle) -> Result<Vec<StatusRow>, String> {
    let t0 = std::time::Instant::now();
    let (box_, hosts) = setup(&app)?;
    Ok(hosts
        .into_iter()
        .map(|(id, h)| match sparrow_cannon_core::status(&box_, &h) {
            Ok(s) => {
                eprintln!("[cannon] status {id}: ok in {:?}", t0.elapsed());
                StatusRow {
                    id,
                    mac: h.mac,
                    note: h.note,
                    state: if s.active { "UP" } else { "DOWN" }.into(),
                    ip: s.ip,
                    hostname: s.hostname,
                }
            }
            Err(e) => {
                eprintln!("[cannon] status {id}: ERR in {:?}: {e:#}", t0.elapsed());
                StatusRow {
                    id,
                    mac: h.mac,
                    note: h.note,
                    state: "ERR".into(),
                    ip: None,
                    hostname: Some(e.to_string()),
                }
            }
        })
        .collect())
}

#[tauri::command]
fn wake(host_id: String, app: tauri::AppHandle) -> Result<(), String> {
    let (box_, hosts) = setup(&app)?;
    let h = hosts.get(&host_id).ok_or(format!("host '{host_id}' fehlt"))?;
    sparrow_cannon_core::wake(&box_, h).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_status,
            wake,
            set_box_config,
            get_box_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
