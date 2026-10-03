// Die command-wrapper ummanteln bodies absichtlich als sofort-aufgerufene closure
// (erhält return/?-semantik beim spawn_blocking). Das triggerd redundant_closure_call.
#![allow(clippy::redundant_closure_call)]
use serde::Serialize;
use sparrow_cannon_core::config::ConfigFile;
use sparrow_cannon_core::widgets::{self, Widget, WidgetState};
use sparrow_cannon_core::{BoxProfile, Host, pass};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

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
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
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

/// Hintergrund-Ergebnisse: Widget-Statusse (alle 10s bzw. per Intervall)
/// und SSH-Verbindungstests (alle 60s) — das Frontend liest nur noch Cache.
struct SchedResults(StdMutex<BTreeMap<String, (String, String)>>);
struct ConnTests(StdMutex<BTreeMap<String, (bool, String)>>);

fn spawn_scheduler(app: tauri::AppHandle) {
    use tauri::Manager;
    std::thread::spawn(move || {
        let mut last: BTreeMap<String, Instant> = BTreeMap::new();
        let mut pass_cache: Option<(Instant, BTreeMap<String, sparrow_cannon_core::BoxProfile>)> =
            None;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let conf = load_conf(&app);

            // pass-cache: secret-tool nicht jede sekunde spawnen (60s ttl)
            let refresh_pass = pass_cache
                .as_ref()
                .map(|(t, _)| t.elapsed() >= Duration::from_secs(60))
                .unwrap_or(true);
            if refresh_pass {
                let mut boxes = BTreeMap::new();
                for (id, b) in &conf.boxes {
                    if let Some(pass) =
                        sparrow_cannon_core::pass::resolve(id, secrets_dir(&app).as_deref())
                    {
                        boxes.insert(
                            id.clone(),
                            sparrow_cannon_core::BoxProfile {
                                name: id.clone(),
                                base_url: b.base_url.trim_end_matches('/').to_string(),
                                user: b.user.clone(),
                                pass,
                            },
                        );
                    }
                }
                pass_cache = Some((Instant::now(), boxes));
            }
            let ctx = widgets::Ctx {
                boxes: pass_cache
                    .as_ref()
                    .map(|(_, b)| b.clone())
                    .unwrap_or_default(),
                ssh: widgets::Ctx::from_config(&conf, secrets_dir(&app)).ssh,
                secrets_dir: secrets_dir(&app),
            };

            // widget-statusse (kill-switch: CANNON_NO_CHECKS=1)
            let no_checks = std::env::var("CANNON_NO_CHECKS").is_ok();
            if no_checks {
                continue;
            }
            for w in &conf.widgets {
                if w.disabled || w.status_paused {
                    continue;
                }
                let Some(op) = &w.status else { continue };
                // trigger.kind ist historisch — nur interval_secs zählt (0 = default 10s)
                let interval = if w.trigger.interval_secs > 0 {
                    w.trigger.interval_secs
                } else {
                    10
                };
                let now = Instant::now();
                let due = last
                    .get(&w.id)
                    .map(|t| now.duration_since(*t) >= Duration::from_secs(interval))
                    .unwrap_or(true);
                if !due {
                    continue;
                }
                last.insert(w.id.clone(), now);
                let app2 = app.clone();
                let op = op.clone();
                let id = w.id.clone();
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let res = widgets::eval_status(&op, &ctx2);
                    if let Some(st) = app2.try_state::<SchedResults>() {
                        st.0.lock().unwrap().insert(id, res);
                    }
                });
            }
        }
    });
}

/// Verzeichnis für secrets.toml — immer das Config-Verzeichnis
/// (Desktop: ~/.config/sparrow-cannon, Android: app_data_dir).
fn secrets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    config_path(app).parent().map(Path::to_path_buf)
}

/// Alte box.txt (Vorgängerversion) entfernen — Werte leben jetzt in der
/// config.toml, das Passwort im Keyring bzw. unter secrets/.
fn migrate_legacy_box_txt(dir: &Path, conf: &mut ConfigFile) {
    // legacy: per-connection ssh-key → device-key
    let legacy_key = dir.join("secrets").join("ssh-pc.key");
    let device_key = dir.join("secrets").join("device.key");
    if legacy_key.exists() && !device_key.exists() {
        let _ = std::fs::rename(&legacy_key, &device_key);
        tracing::info!("legacy ssh-key → device-key migriert");
    }

    let legacy = dir.join("box.txt");
    let Ok(raw) = std::fs::read_to_string(&legacy) else {
        return;
    };
    let mut lines = raw.lines();
    let (Some(url), Some(user)) = (lines.next().map(str::trim), lines.next().map(str::trim)) else {
        let _ = std::fs::remove_file(&legacy);
        return;
    };
    let legacy_pass = lines.next().map(str::trim).unwrap_or_default();
    if !url.is_empty() && !conf.boxes.is_empty() {
        let id = conf.boxes.keys().next().cloned().unwrap_or_default();
        conf.upsert_box(&id, url, user);
        let _ = conf.save_to(&config_path_static());
        if !legacy_pass.is_empty() {
            // Legacy: Passwort in die neue secrets.toml übernehmen
            sparrow_cannon_core::pass::store(&id, legacy_pass, Some(dir));
        }
        tracing::info!("legacy box.txt übernommen");
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
async fn get_ssh_connections(app: tauri::AppHandle) -> Result<Vec<SshConnInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<SshConnInfo>, String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<Vec<SshConnInfo>, String> {
            use tauri::Manager;
            let conf = load_conf(&app);
            let tests = app.state::<ConnTests>();
            let cached = tests.0.lock().unwrap().clone();
            let mut out = Vec::new();
            for (id, c) in &conf.connections.ssh {
                // nur cache — die tests laufen on-demand (test_ssh_connections)
                let (ok, detail) = cached
                    .get(id)
                    .cloned()
                    .unwrap_or((false, "test läuft…".into()));
                out.push(SshConnInfo {
                    id: id.clone(),
                    dest: c.dest.clone(),
                    note: c.note.clone(),
                    ok,
                    detail,
                });
            }
            Ok(out)
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command get_ssh_connections: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxConnInfo {
    pub id: String,
    pub base_url: String,
    pub user: String,
    pub has_secret: bool,
}

fn get_box_connections_impl(app: &tauri::AppHandle) -> Result<Vec<BoxConnInfo>, String> {
    let conf = load_conf(app);
    Ok(conf
        .boxes
        .iter()
        .map(|(id, b)| BoxConnInfo {
            id: id.clone(),
            base_url: b.base_url.clone(),
            user: b.user.clone(),
            has_secret: pass::resolve(id, secrets_dir(app).as_deref()).is_some(),
        })
        .collect())
}

#[tauri::command]
async fn get_box_connections(app: tauri::AppHandle) -> Result<Vec<BoxConnInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t = std::time::Instant::now();
        let r = get_box_connections_impl(&app);
        if t.elapsed() > std::time::Duration::from_millis(300) {
            tracing::warn!("command get_box_connections: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Lokaler SSH-Pubkey (zum Verteilen auf Zielsysteme).
/// Android: Device-Key aus secrets/ (wird bei Bedarf generiert).
#[tauri::command]
async fn get_pubkey(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<String, String> {
            #[cfg(target_os = "android")]
            {
                ensure_device_key_impl(&app)
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = &app;
                let home = std::env::var("HOME").map_err(|_| "kein HOME")?;
                let pub_path = std::path::Path::new(&home).join(".ssh/id_ed25519.pub");
                std::fs::read_to_string(&pub_path)
                    .map(|s| s.trim().to_string())
                    .map_err(|_| format!("{} nicht lesbar — erst ssh-keygen?", pub_path.display()))
            }
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command get_pubkey: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Plattform fürs UI (plattformspezifische Optionen ein/aus).
#[tauri::command]
fn get_platform() -> String {
    #[cfg(target_os = "android")]
    {
        "android".into()
    }
    #[cfg(not(target_os = "android"))]
    {
        "desktop".into()
    }
}

#[tauri::command]
fn upsert_ssh_conn(
    id: String,
    dest: String,
    user: String,
    note: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let path = config_path(&app);
    let mut conf = ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
    conf.connections.ssh.insert(
        id.clone(),
        sparrow_cannon_core::config::SshConn {
            dest: dest.trim_end_matches('/').to_string(),
            user,
            note,
        },
    );
    conf.save_to(&path).map_err(|e| e.to_string())
}

/// Zieht die Config von der Sync-Quelle und merged sie Joplin-artig
/// (base-hash pro Item, Konflikt-Stash als deaktivierte Karte).
/// Quelle: [sync].conn, sonst — wenn vorhanden — die einzige ssh-Verbindung.
/// Die verwendete Quelle wird in [sync] gemerkt.
#[tauri::command]
async fn sync_from_remote(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<String, String> {
            let path = config_path(&app);
            let conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());

            let conn_id = match &conf.sync {
                Some(s) => s.conn.clone(),
                None => {
                    if conf.connections.ssh.len() == 1 {
                        conf.connections.ssh.keys().next().unwrap().clone()
                    } else {
                        return Err("keine sync-quelle: erst eine ssh-verbindung anlegen".into());
                    }
                }
            };
            let remote_path = conf
                .sync
                .as_ref()
                .map(|s| s.path.clone())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "~/.config/sparrow-cannon/config.toml".into());

            let ctx = sparrow_cannon_core::widgets::Ctx::from_config(&conf, secrets_dir(&app));
            let target = ctx
                .ssh
                .get(&conn_id)
                .ok_or_else(|| format!("ssh-verbindung '{conn_id}' fehlt"))?;
            let raw = sparrow_cannon_core::widgets::ssh_exec(target, &format!("cat {remote_path}"))
                .map_err(|e| format!("sync-quelle '{conn_id}': {e}"))?;
            let remote: ConfigFile =
                toml::from_str(&raw).map_err(|e| format!("remote config parse: {e}"))?;

            let state = load_sync_state(&app);
            let outcome = sparrow_cannon_core::sync::merge(conf, remote, &state);
            let mut conf = outcome.conf;
            conf.sync = Some(sparrow_cannon_core::config::SyncFile {
                conn: conn_id.clone(),
                path: String::new(),
            });
            conf.save_to(&path).map_err(|e| e.to_string())?;
            save_sync_state(&app, &outcome.state)?;

            let mut msg = outcome.report.summary();
            if !outcome.report.conflicts.is_empty() {
                msg.push_str(" — konflikt-karten sind deaktiviert markiert");
            }

            // secrets.toml (sibling der remote config) — union-merge,
            // remote gewinnt pro ID. Best effort: fehlt die Datei auf der
            // Quelle (älterer Stand), scheitert der Sync nicht daran.
            let secrets_msg = match std::path::Path::new(&remote_path)
                .parent()
                .map(|p| p.join("secrets.toml"))
                .map(|sp| {
                    sparrow_cannon_core::widgets::ssh_exec(target, &format!("cat {}", sp.display()))
                }) {
                Some(pulled) => match pulled {
                    Ok(raw) => match sparrow_cannon_core::pass::merge_remote_raw(
                        secrets_dir(&app).as_deref(),
                        &raw,
                    ) {
                        Ok((taken, total)) => format!(", secrets: {taken} neu ({total} gesamt)"),
                        Err(e) => format!(", secrets: {e}"),
                    },
                    Err(_) => ", secrets: keine datei auf der quelle".to_string(),
                },
                None => String::new(),
            };
            Ok(format!("sync von {conn_id} ✅ {msg}{secrets_msg}"))
        })();
        if t.elapsed() > std::time::Duration::from_secs(2) {
            tracing::warn!("command sync_from_remote: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

fn sync_state_path(app: &tauri::AppHandle) -> PathBuf {
    config_path(app).with_file_name("sync_state.toml")
}

fn load_sync_state(app: &tauri::AppHandle) -> sparrow_cannon_core::sync::SyncState {
    std::fs::read_to_string(sync_state_path(app))
        .ok()
        .and_then(|raw| toml::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_sync_state(
    app: &tauri::AppHandle,
    state: &sparrow_cannon_core::sync::SyncState,
) -> Result<(), String> {
    let body = toml::to_string_pretty(state).map_err(|e| e.to_string())?;
    let p = sync_state_path(app);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(p, body).map_err(|e| e.to_string())
}

/// Stellt den globalen Device-SSH-Key sicher (generiert bei Bedarf).
/// Rückgabe: pubkey zum Verteilen.
fn ensure_device_key_impl(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .expect("app_data_dir nicht auflösbar");
    let key_path = dir.join("secrets").join("device.key");
    if let Ok(existing) = std::fs::read_to_string(&key_path) {
        return sparrow_cannon_core::keys::public_line(&existing).map_err(|e| format!("{e}"));
    }
    let (priv_pem, pub_line) = sparrow_cannon_core::keys::generate_ed25519()
        .map_err(|e| format!("{e} (desktop: system-keys nutzen)"))?;
    std::fs::create_dir_all(key_path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&key_path, priv_pem).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
    }
    tracing::info!("device-ssh-key generiert: {}", key_path.display());
    Ok(pub_line)
}

#[tauri::command]
async fn ensure_device_key(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || ensure_device_key_impl(&app))
        .await
        .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
async fn get_secret_ids(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        Ok(sparrow_cannon_core::pass::ids(secrets_dir(&app).as_deref()))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
async fn set_secret(id: String, value: String, app: tauri::AppHandle) -> Result<(), String> {
    if id.trim().is_empty() || value.is_empty() {
        return Err("id und wert nötig".into());
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        match sparrow_cannon_core::pass::store(id.trim(), &value, secrets_dir(&app).as_deref()) {
            "datei" => Ok(()),
            _ => Err("secret konnte nicht gespeichert werden".into()),
        }
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
async fn remove_secret(id: String, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        sparrow_cannon_core::pass::delete(&id, secrets_dir(&app).as_deref());
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
async fn remove_ssh_conn(id: String, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<(), String> {
            let path = config_path(&app);
            let mut conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
            conf.connections.ssh.remove(&id);
            conf.save_to(&path).map_err(|e| e.to_string())
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command remove_ssh_conn: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
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
async fn remove_box_conn(id: String, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<(), String> {
            let path = config_path(&app);
            let mut conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
            conf.boxes.remove(&id);
            conf.save_to(&path).map_err(|e| e.to_string())?;
            sparrow_cannon_core::pass::delete(&id, secrets_dir(&app).as_deref());
            Ok(())
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command remove_box_conn: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// JS-Fehler landen im journal (statt still im WebView-Console).
#[tauri::command]
fn js_log(msg: String) {
    tracing::warn!("[js] {msg}");
}

/// SSH-Verbindungen on-demand testen (löst Hintergrund-Threads aus).
/// Läuft über denselben Pfad wie die Widgets (Android: device-key + russh).
#[tauri::command]
async fn test_ssh_connections(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<(), String> {
            use tauri::Manager;
            let conf = load_conf(&app);
            let ctx = sparrow_cannon_core::widgets::Ctx::from_config(&conf, secrets_dir(&app));
            for id in conf.connections.ssh.keys() {
                let Some(target) = ctx.ssh.get(id).cloned() else {
                    continue;
                };
                let app2 = app.clone();
                let id = id.clone();
                std::thread::spawn(move || {
                    let res = match sparrow_cannon_core::widgets::ssh_exec(&target, "echo ok") {
                        Ok(_) => (true, "verbunden".into()),
                        Err(e) => (false, e.to_string()),
                    };
                    if let Some(st) = app2.try_state::<ConnTests>() {
                        st.0.lock().unwrap().insert(id, res);
                    }
                });
            }
            Ok(())
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command test_ssh_connections: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Periodische Status-Abfrage eines Widgets pausieren/starten.
#[tauri::command]
async fn set_status_paused(id: String, paused: bool, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<(), String> {
            let path = config_path(&app);
            let mut conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
            if let Some(w) = conf.widgets.iter_mut().find(|w| w.id == id) {
                w.status_paused = paused;
            }
            conf.save_to(&path).map_err(|e| e.to_string())
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command set_status_paused: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Bestehendes Widget überschreiben (bearbeiten).
#[tauri::command]
async fn update_widget(widget: Widget, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<(), String> {
            let path = config_path(&app);
            let mut conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
            let Some(i) = conf.widgets.iter().position(|w| w.id == widget.id) else {
                return Err(format!("widget '{}' fehlt", widget.id));
            };
            if widget.actions.is_empty() && widget.status.is_none() {
                return Err("widget braucht action oder status".into());
            }
            conf.widgets[i] = widget;
            conf.save_to(&path).map_err(|e| e.to_string())
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command update_widget: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Widget aktivieren/deaktivieren.
#[tauri::command]
async fn set_widget_enabled(
    id: String,
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<(), String> {
            let path = config_path(&app);
            let mut conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
            if let Some(w) = conf.widgets.iter_mut().find(|w| w.id == id) {
                w.disabled = !enabled;
            }
            conf.save_to(&path).map_err(|e| e.to_string())
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command set_widget_enabled: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

fn get_widgets_impl(app: &tauri::AppHandle) -> Result<Vec<WidgetState>, String> {
    use tauri::Manager;
    let conf = load_conf(app);
    // Nur Cache — kein Inline-Eval: Der bremst den Appstart um Sekunden
    // (ICMP/TCP/SSH-Timeouts). Fehlende Ergebnisse kommen als PEND und
    // liefert der Scheduler nach; das Frontend pollt solange zügig.
    let sched = app.state::<SchedResults>();
    let sched_map = sched.0.lock().unwrap().clone();
    Ok(widgets::widget_states_cached(&conf.widgets, &sched_map))
}

#[tauri::command]
async fn get_widgets(app: tauri::AppHandle) -> Result<Vec<WidgetState>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t = std::time::Instant::now();
        let r = get_widgets_impl(&app);
        // Cache-Lesen: sollte im Millisekundenbereich liegen.
        if t.elapsed() > std::time::Duration::from_millis(300) {
            tracing::warn!("command get_widgets: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

fn fire_widget_impl(app: &tauri::AppHandle, id: &str, index: usize) -> Result<String, String> {
    use tauri::Manager;
    let conf = load_conf(app);
    let ctx = widgets::Ctx::from_config(&conf, secrets_dir(app));
    let w = conf
        .widgets
        .iter()
        .find(|w| w.id == id)
        .cloned()
        .ok_or(format!("widget '{id}' fehlt"))?;
    match widgets::fire(&w, index, &ctx) {
        Ok(out) => {
            tracing::info!("widget {id}[{index}] gefeuert");
            // Status sofort auffrischen — der Scheduler würde sonst bis zum
            // nächsten Intervall den alten Stand zeigen (get_widgets ist
            // cache-only).
            if !w.status_paused
                && !w.disabled
                && let Some(op) = w.status.as_ref()
            {
                let res = widgets::eval_status(op, &ctx);
                if let Some(st) = app.try_state::<SchedResults>() {
                    st.0.lock().unwrap().insert(id.to_string(), res);
                }
            }
            Ok(out)
        }
        Err(e) => {
            tracing::warn!("widget {id}[{index}] fehlgeschlagen: {e:#}");
            Err(e.to_string())
        }
    }
}

#[tauri::command]
async fn fire_widget(id: String, index: usize, app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t = std::time::Instant::now();
        let r = fire_widget_impl(&app, &id, index);
        if t.elapsed() > std::time::Duration::from_millis(300) {
            tracing::warn!("command fire_widget: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
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
async fn get_methods() -> Vec<MethodDef> {
    tauri::async_runtime::spawn_blocking(move || -> Vec<MethodDef> {
        let t = std::time::Instant::now();
        let r = (|| -> Vec<MethodDef> {
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
                                sparrow_cannon_core::widgets::FieldKind::SshConn => {
                                    "ssh-conn".into()
                                }
                                sparrow_cannon_core::widgets::FieldKind::BoxConn => {
                                    "box-conn".into()
                                }
                            },
                            required: f.required,
                        })
                        .collect(),
                })
                .collect()
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command get_methods: {:?}", t.elapsed());
        }
        r
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn add_widget(widget: Widget, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<(), String> {
            let path = config_path(&app);
            let mut conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
            if conf.widgets.iter().any(|w| w.id == widget.id) {
                return Err(format!("widget-id '{}' existiert schon", widget.id));
            }
            if widget.actions.is_empty() && widget.status.is_none() {
                return Err("widget braucht action oder status".into());
            }
            conf.widgets.push(widget);
            conf.save_to(&path).map_err(|e| e.to_string())
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command add_widget: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
async fn remove_widget(id: String, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<(), String> {
            let path = config_path(&app);
            let mut conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
            conf.widgets.retain(|w| w.id != id);
            conf.save_to(&path).map_err(|e| e.to_string())
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command remove_widget: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
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
async fn get_box_info(app: tauri::AppHandle) -> Result<BoxInfo, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<BoxInfo, String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<BoxInfo, String> {
            let path = config_path(&app);
            let conf =
                ConfigFile::load_from(path.clone()).unwrap_or_else(|_| ConfigFile::builtin());
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
        })();
        if t.elapsed() > std::time::Duration::from_millis(20) {
            tracing::warn!("command get_box_info: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[tauri::command]
fn set_box_config(
    base_url: String,
    user: String,
    p: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
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
    tracing::info!("passwort gespeichert: {stored}");
    Ok(())
}

fn get_status_impl(app: &tauri::AppHandle) -> Result<Vec<StatusRow>, String> {
    let t0 = std::time::Instant::now();
    let (box_, hosts) = setup(app)?;
    Ok(hosts
        .into_iter()
        // Hosts ohne MAC sind reine SSH-Hosts — die Fritzbox kennt sie nicht.
        .filter(|(_, h)| !h.mac.is_empty())
        .map(|(id, h)| match sparrow_cannon_core::status(&box_, &h) {
            Ok(s) => {
                tracing::debug!("status {id}: ok in {:?}", t0.elapsed());
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
                tracing::warn!("status {id} fehlgeschlagen in {:?}: {e:#}", t0.elapsed());
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
async fn get_status(app: tauri::AppHandle) -> Result<Vec<StatusRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t = std::time::Instant::now();
        let r = get_status_impl(&app);
        // Netzwerk-Call: zu langsam fuer die 20ms-Performance-Warnung, aber ein
        // echter Hang (>>2s) bleibt sichtbar. Vorher 300ms — spammt den Log
        // bei jedem 10s-Poll zu.
        if t.elapsed() > std::time::Duration::from_secs(2) {
            tracing::warn!("command get_status: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FritzDeviceUi {
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub active: bool,
}

/// Geräte der Box — Formular-Vorschläge für MAC- und SSH-Ziel-Felder.
#[tauri::command]
async fn get_fritz_devices(app: tauri::AppHandle) -> Result<Vec<FritzDeviceUi>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<FritzDeviceUi>, String> {
        let t = std::time::Instant::now();
        let r = (|| -> Result<Vec<FritzDeviceUi>, String> {
            let (box_, _) = setup(&app)?;
            sparrow_cannon_core::tr064::host_entries(&box_)
                .map(|v| {
                    v.into_iter()
                        .map(|d| FritzDeviceUi {
                            name: d.name,
                            mac: d.mac,
                            ip: d.ip,
                            active: d.active,
                        })
                        .collect()
                })
                .map_err(|e| e.to_string())
        })();
        // TR-064-HTTP-Call: dauerhaft >20ms, nur echte Haenger loggen.
        if t.elapsed() > std::time::Duration::from_secs(2) {
            tracing::warn!("command get_fritz_devices: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

fn wake_impl(app: &tauri::AppHandle, host_id: &str) -> Result<(), String> {
    let (box_, hosts) = setup(app)?;
    let h = hosts
        .get(host_id)
        .ok_or(format!("host '{host_id}' fehlt"))?;
    sparrow_cannon_core::wake(&box_, h).map_err(|e| e.to_string())
}

#[tauri::command]
async fn wake(host_id: String, app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let t = std::time::Instant::now();
        let r = wake_impl(&app, &host_id);
        if t.elapsed() > std::time::Duration::from_millis(300) {
            tracing::warn!("command wake: {:?}", t.elapsed());
        }
        r
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tracing_subscriber::EnvFilter;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    tauri::Builder::default()
        .manage(SchedResults(StdMutex::new(BTreeMap::new())))
        .manage(ConnTests(StdMutex::new(BTreeMap::new())))
        .setup(|app| {
            let handle = app.handle().clone();
            spawn_scheduler(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_fritz_devices,
            get_status,
            wake,
            set_box_config,
            get_box_info,
            get_widgets,
            test_ssh_connections,
            set_widget_enabled,
            set_status_paused,
            fire_widget,
            get_methods,
            js_log,
            add_widget,
            update_widget,
            remove_widget,
            get_ssh_connections,
            get_box_connections,
            get_pubkey,
            get_platform,
            upsert_ssh_conn,
            ensure_device_key,
            remove_ssh_conn,
            get_secret_ids,
            set_secret,
            remove_secret,
            upsert_box_conn,
            remove_box_conn,
            sync_from_remote
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
