use sparrow_cannon_core::config::ConfigFile;
use sparrow_cannon_core::{pass, BoxProfile, Host};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
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

/// Im Setup-Modal gesetzte Box-Konfiguration; überschreibt Config/Keyring.
#[derive(Clone)]
struct BoxConfig {
    base_url: String,
    user: String,
    pass: String,
}

struct AppState(Mutex<Option<BoxConfig>>);

impl AppState {
    /// Lädt die Box-Konfiguration aus dem App-Datenverzeichnis.
    /// Desktop: 2 Zeilen (url, user) — Passwort lebt im Keyring.
    /// Android (kein Secret Service): 3 Zeilen (url, user, pass), chmod 600.
    /// Legacy 3-Zeilen-Dateien mit Passwort migrieren automatisch in den Keyring.
    fn load(dir: PathBuf) -> Self {
        let file = dir.join("box.txt");
        let mut cfg = std::fs::read_to_string(&file).ok().and_then(|raw| {
            let mut lines = raw.lines();
            let base_url = lines.next()?.trim().to_string();
            let user = lines.next()?.trim().to_string();
            let pass = lines.next().map(|l| l.trim().to_string()).unwrap_or_default();
            (base_url.starts_with("http") && !user.is_empty()).then_some(BoxConfig {
                base_url,
                user,
                pass,
            })
        });

        if let Some(c) = &cfg {
            if !c.pass.is_empty() {
                // Legacy: Passwort in der Datei → Keyring, Datei entschlacken
                if sparrow_cannon_core::pass::store_to_keyring(&c.pass) {
                    let _ = std::fs::write(
                        &file,
                        format!("{}\n{}\n", c.base_url, c.user),
                    );
                    eprintln!("[cannon] legacy-passwort in keyring migriert");
                }
            }
        }
        if let Some(c) = &mut cfg {
            c.pass = String::new(); // Passwort kommt ausschließlich aus dem Keyring
        }
        if cfg.is_some() {
            eprintln!("[cannon] box-config geladen aus {}", file.display());
        }
        AppState(Mutex::new(cfg))
    }

    fn store(&self, dir: &PathBuf, cfg: &BoxConfig) {
        let _ = std::fs::create_dir_all(dir);
        let file = dir.join("box.txt");
        let keyring_ok = sparrow_cannon_core::pass::store_to_keyring(&cfg.pass);
        let body = if keyring_ok {
            format!("{}\n{}\n", cfg.base_url, cfg.user)
        } else {
            // Android-Fallback: Klartext in der Sandbox, chmod 600
            format!("{}\n{}\n{}\n", cfg.base_url, cfg.user, cfg.pass)
        };
        if std::fs::write(&file, body).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
            }
            eprintln!(
                "[cannon] box-config gespeichert ({})",
                if keyring_ok { "keyring" } else { "datei" }
            );
        }
    }
}

/// Config + Passwort laden, Default-Box wählen (Setup-Overrides anwenden).
fn setup(state: &AppState) -> Result<(BoxProfile, BTreeMap<String, Host>), String> {
    let conf = ConfigFile::load_default_or_builtin();
    let cfg = state.0.lock().unwrap().clone();
    let pass = cfg
        .as_ref()
        .filter(|c| !c.pass.is_empty())
        .map(|c| c.pass.clone())
        .or_else(pass::resolve_from_env_or_keyring)
        .ok_or("kein Box-Passwort — bitte unten einrichten")?;
    let profiles = conf.build_with_pass(&pass);
    let mut box_ = profiles
        .values()
        .next()
        .cloned()
        .ok_or("keine [boxes.*] in der config")?;
    if let Some(c) = &cfg {
        box_.base_url = c.base_url.trim_end_matches('/').to_string();
        box_.user = c.user.clone();
    }
    Ok((box_, conf.hosts()))
}

/// Werte zum Vorausfüllen des Setup-Modals (gespeicherte oder config-defaults).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxInfo {
    base_url: String,
    user: String,
    has_saved: bool,
}

#[tauri::command]
fn get_box_info(state: tauri::State<AppState>) -> Result<BoxInfo, String> {
    let conf = ConfigFile::load_default_or_builtin();
    let (default_url, default_user) = conf
        .boxes
        .values()
        .next()
        .map(|b| (b.base_url.clone(), b.user.clone()))
        .ok_or_else(|| "keine [boxes.*] in der config".to_string())?;
    if let Some(c) = state.0.lock().unwrap().as_ref() {
        return Ok(BoxInfo {
            base_url: c.base_url.clone(),
            user: c.user.clone(),
            has_saved: true,
        });
    }
    Ok(BoxInfo {
        base_url: default_url,
        user: default_user,
        has_saved: false,
    })
}

#[tauri::command]
fn set_box_config(
    base_url: String,
    user: String,
    p: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    if !base_url.starts_with("http") {
        return Err("Box-URL muss mit http:// beginnen".into());
    }
    let cfg = BoxConfig {
        base_url: base_url.trim_end_matches('/').to_string(),
        user,
        pass: p,
    };
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app-data-dir: {e}"))?;
    state.store(&dir, &cfg);
    *state.0.lock().unwrap() = Some(cfg);
    Ok(())
}

#[tauri::command]
fn get_status(state: tauri::State<AppState>) -> Result<Vec<StatusRow>, String> {
    let t0 = std::time::Instant::now();
    let (box_, hosts) = setup(&state)?;
    let rows: Vec<StatusRow> = hosts
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
        .collect();
    eprintln!("[cannon] get_status gesamt: {:?}", t0.elapsed());
    Ok(rows)
}

#[tauri::command]
fn wake(host_id: String, state: tauri::State<AppState>) -> Result<(), String> {
    let (box_, hosts) = setup(&state)?;
    let h = hosts.get(&host_id).ok_or(format!("host '{host_id}' fehlt"))?;
    sparrow_cannon_core::wake(&box_, h).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            let dir = app
                .path()
                .app_data_dir()
                .expect("app_data_dir nicht auflösbar");
            app.manage(AppState::load(dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            wake,
            set_box_config,
            get_box_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
