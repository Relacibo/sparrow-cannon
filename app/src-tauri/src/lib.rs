use sparrow_cannon_core::config::ConfigFile;
use sparrow_cannon_core::widgets::{self, Widget, WidgetState};
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
    let path = config_path(app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    {
        use tauri::Manager;
        let data_dir = app
            .path()
            .app_data_dir()
            .expect("app_data_dir nicht auflösbar");
        migrate_legacy_box_txt(&data_dir, &mut conf);
    }
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

fn load_conf(app: &tauri::AppHandle) -> ConfigFile {
    let path = config_path(app);
    ConfigFile::load_from(path).unwrap_or_else(|_| ConfigFile::builtin())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConnInfo {
    pub id: String,
    pub dest: String,
    pub note: String,
    pub ok: bool,
    pub detail: String,
}

/// Alle SSH-Verbindungen mit Connect-Test.
#[tauri::command]
fn get_ssh_connections(app: tauri::AppHandle) -> Result<Vec<SshConnInfo>, String> {
    let conf = load_conf(&app);
    let mut out = Vec::new();
    for (id, c) in &conf.connections.ssh {
        let r = std::process::Command::new("timeout")
            .args([
                "8",
                "ssh",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=4",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "-o",
                "LogLevel=ERROR",
                &c.dest,
                "echo ok",
            ])
            .output();
        let (ok, detail) = match r {
            Ok(o) if o.status.success() => (true, "verbunden".into()),
            Ok(o) => (
                false,
                String::from_utf8_lossy(&o.stderr).trim().chars().take(120).collect(),
            ),
            Err(e) => (false, e.to_string()),
        };
        out.push(SshConnInfo {
            id: id.clone(),
            dest: c.dest.clone(),
            note: c.note.clone(),
            ok,
            detail,
        });
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxConnInfo {
    pub id: String,
    pub base_url: String,
    pub user: String,
    pub has_secret: bool,
}

#[tauri::command]
fn get_box_connections(app: tauri::AppHandle) -> Result<Vec<BoxConnInfo>, String> {
    let conf = load_conf(&app);
    Ok(conf
        .boxes
        .iter()
        .map(|(id, b)| BoxConnInfo {
            id: id.clone(),
            base_url: b.base_url.clone(),
            user: b.user.clone(),
            has_secret: pass::resolve(id, secrets_dir(&app).as_deref()).is_some(),
        })
        .collect())
}

#[tauri::command]
fn upsert_ssh_conn(
    id: String,
    dest: String,
    note: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let path = config_path(&app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    conf.connections.ssh.insert(
        id,
        sparrow_cannon_core::config::SshConn {
            dest: dest.trim_end_matches('/').to_string(),
            note,
        },
    );
    conf.save_to(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_ssh_conn(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let path = config_path(&app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    conf.connections.ssh.remove(&id);
    conf.save_to(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn upsert_box_conn(
    id: String,
    base_url: String,
    user: String,
    pass: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let path = config_path(&app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    conf.boxes.insert(
        id.clone(),
        sparrow_cannon_core::config::BoxFile {
            base_url: base_url.trim_end_matches('/').to_string(),
            user,
        },
    );
    conf.save_to(&path).map_err(|e| e.to_string())?;
    if !pass.is_empty() {
        sparrow_cannon_core::pass::store(&id, &pass, secrets_dir(&app).as_deref());
    }
    Ok(())
}

#[tauri::command]
fn remove_box_conn(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let path = config_path(&app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    conf.boxes.remove(&id);
    conf.save_to(&path).map_err(|e| e.to_string())?;
    sparrow_cannon_core::pass::delete(&id);
    Ok(())
}

#[tauri::command]
fn get_widgets(app: tauri::AppHandle) -> Result<Vec<WidgetState>, String> {
    let conf = load_conf(&app);
    let ctx = widgets::Ctx::from_config(&conf, secrets_dir(&app));
    Ok(widgets::widget_states(&conf.widgets, &ctx))
}

#[tauri::command]
fn fire_widget(id: String, index: usize, app: tauri::AppHandle) -> Result<String, String> {
    let conf = load_conf(&app);
    let ctx = widgets::Ctx::from_config(&conf, secrets_dir(&app));
    let w = conf
        .widgets
        .iter()
        .find(|w| w.id == id)
        .cloned()
        .ok_or(format!("widget '{id}' fehlt"))?;
    widgets::fire(&w, index, &ctx).map_err(|e| e.to_string())
}

/// Param-Schema der Registry für den "+"-Dialog.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodDef {
    pub kind: String,
    pub fields: Vec<FieldDefUi>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDefUi {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub required: bool,
}

#[tauri::command]
fn get_methods() -> Vec<MethodDef> {
    widgets::field_defs()
        .into_iter()
        .map(|(kind, fields)| MethodDef {
            kind: kind.to_string(),
            fields: fields
                .into_iter()
                .map(|f| FieldDefUi {
                    key: f.key.to_string(),
                    label: f.label.to_string(),
                    kind: match &f.kind {
                        sparrow_cannon_core::widgets::FieldKind::Text => "text".into(),
                        sparrow_cannon_core::widgets::FieldKind::Mac => "mac".into(),
                        sparrow_cannon_core::widgets::FieldKind::SshConn => "ssh-conn".into(),
                    },
                    required: f.required,
                })
                .collect(),
        })
        .collect()
}

#[tauri::command]
fn add_widget(widget: Widget, app: tauri::AppHandle) -> Result<(), String> {
    let path = config_path(&app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    if conf.widgets.iter().any(|w| w.id == widget.id) {
        return Err(format!("widget-id '{}' existiert schon", widget.id));
    }
    if widget.action.is_none() && widget.status.is_none() {
        return Err("widget braucht action oder status".into());
    }
    conf.widgets.push(widget);
    conf.save_to(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_widget(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let path = config_path(&app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    conf.widgets.retain(|w| w.id != id);
    conf.save_to(&path).map_err(|e| e.to_string())
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
        // Hosts ohne MAC sind reine SSH-Hosts — die Fritzbox kennt sie nicht.
        .filter(|(_, h)| !h.mac.is_empty())
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
            get_box_info,
            get_widgets,
            fire_widget,
            get_methods,
            add_widget,
            remove_widget,
            get_ssh_connections,
            get_box_connections,
            upsert_ssh_conn,
            remove_ssh_conn,
            upsert_box_conn,
            remove_box_conn
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
