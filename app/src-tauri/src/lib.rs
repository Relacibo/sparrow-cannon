use fritz_cannon_core::config::ConfigFile;
use fritz_cannon_core::{pass, BoxProfile, Host};
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

/// Zur Laufzeit gesetzte Credentials (Setup-Formular), überschreiben Config/Keyring.
#[derive(Clone)]
struct Credentials {
    user: String,
    pass: String,
}

struct AppState(Mutex<Option<Credentials>>);

impl AppState {
    /// Lädt gespeicherte Credentials aus dem App-Datenverzeichnis (2-Zeilen-Format:
    /// user \n pass, chmod 600). Kein Cloud-Gedöns.
    fn load(dir: PathBuf) -> Self {
        let file = dir.join("credentials.txt");
        let creds = std::fs::read_to_string(&file)
            .ok()
            .and_then(|raw| {
                let mut lines = raw.lines();
                let user = lines.next()?.trim().to_string();
                let pass = lines.next()?.trim().to_string();
                (!user.is_empty() && !pass.is_empty()).then_some(Credentials { user, pass })
            });
        if creds.is_some() {
            eprintln!("[cannon] credentials geladen aus {}", file.display());
        }
        AppState(Mutex::new(creds))
    }

    fn store(&self, dir: &PathBuf, creds: &Credentials) {
        let _ = std::fs::create_dir_all(dir);
        let file = dir.join("credentials.txt");
        if std::fs::write(&file, format!("{}\n{}\n", creds.user, creds.pass)).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
            }
            eprintln!("[cannon] credentials gespeichert in {}", file.display());
        }
    }
}

/// Config + Passwort laden, Default-Box wählen.
fn setup(state: &AppState) -> Result<(BoxProfile, BTreeMap<String, Host>), String> {
    let conf = ConfigFile::load_default_or_builtin();
    let creds = state.0.lock().unwrap().clone();
    let pass = creds
        .as_ref()
        .map(|c| c.pass.clone())
        .or_else(pass::resolve_from_env_or_keyring)
        .ok_or("kein Box-Passwort — bitte unten eingeben")?;
    let profiles = conf.build_with_pass(&pass);
    let mut box_ = profiles
        .values()
        .next()
        .cloned()
        .ok_or("keine [boxes.*] in der config")?;
    if let Some(c) = &creds {
        box_.user = c.user.clone();
    }
    Ok((box_, conf.hosts()))
}

/// Default-User der ersten Box (Prefill für das Setup-Feld).
#[tauri::command]
fn get_box_user() -> Result<String, String> {
    let conf = ConfigFile::load_default_or_builtin();
    conf.boxes
        .values()
        .next()
        .map(|b| b.user.clone())
        .ok_or_else(|| "keine [boxes.*] in der config".to_string())
}

#[tauri::command]
fn set_credentials(
    user: String,
    p: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    let creds = Credentials { user, pass: p };
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app-data-dir: {e}"))?;
    state.store(&dir, &creds);
    *state.0.lock().unwrap() = Some(creds);
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
            set_credentials,
            get_box_user
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
