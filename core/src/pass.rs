//! Passwort-Auflösung & -Speicherung.
//!
//! Reihenfolge: env CANNON_PASS → Keyring (Secret Service, adressiert per
//! Box-ID) → Datei-Fallback (Android hat keinen Secret Service).
//! Legacy-Items unter username=fritzbox migrieren sich beim Lookup selbst.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SERVICE: &str = "sparrow-cannon";
const LEGACY_USERNAME: &str = "fritzbox";

fn secret_tool(args: &[&str]) -> Option<std::process::Output> {
    Command::new("secret-tool")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
}

fn secret_tool_store(pass: &str, username: &str) -> bool {
    let label = format!("--label={SERVICE} {username}");
    let Ok(mut child) = Command::new("secret-tool")
        .args(["store", &label, "service", SERVICE, "username", username])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let mut stdin = match child.stdin.take() {
        Some(s) => s,
        None => return false,
    };
    let wrote = stdin.write_all(pass.as_bytes()).is_ok();
    drop(stdin);
    if !wrote {
        return false;
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn keyring_lookup_username(username: &str) -> Option<String> {
    let out = secret_tool(&["lookup", "service", SERVICE, "username", username])?;
    let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!val.is_empty()).then_some(val)
}

/// Passwort zur Box-ID aus dem Keyring; migriert Legacy-Items automatisch.
pub fn keyring_lookup(box_id: &str) -> Option<String> {
    if let Some(v) = keyring_lookup_username(box_id) {
        return Some(v);
    }
    // Legacy: früher hieß das Item pauschal "fritzbox"
    let legacy = keyring_lookup_username(LEGACY_USERNAME)?;
    secret_tool_store(&legacy, box_id);
    let _ = Command::new("secret-tool")
        .args(["clear", "service", SERVICE, "username", LEGACY_USERNAME])
        .status();
    Some(legacy)
}

/// Passwort zur Box-ID im Keyring speichern. false, wenn kein Keyring da ist.
pub fn keyring_store(box_id: &str, pass: &str) -> bool {
    secret_tool_store(pass, box_id)
}

fn secrets_file(dir: &Path, box_id: &str) -> PathBuf {
    dir.join("secrets").join(format!("{box_id}.txt"))
}

/// Datei-Fallback (Android): secrets/<box_id>.txt, chmod 600.
pub fn file_lookup(dir: &Path, box_id: &str) -> Option<String> {
    let raw = std::fs::read_to_string(secrets_file(dir, box_id)).ok()?;
    let val = raw.trim().to_string();
    (!val.is_empty()).then_some(val)
}

pub fn file_store(dir: &Path, box_id: &str, pass: &str) -> bool {
    let path = secrets_file(dir, box_id);
    let Some(parent) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    if std::fs::write(&path, format!("{pass}\n")).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        true
    } else {
        false
    }
}

/// Keyring-Eintrag einer Box löschen.
pub fn delete(box_id: &str) {
    let _ = Command::new("secret-tool")
        .args(["clear", "service", SERVICE, "username", box_id])
        .status();
}

/// Komfort-Auflösung für Frontends: env → keyring(box_id) → datei-fallback.
pub fn resolve(box_id: &str, secrets_dir: Option<&Path>) -> Option<String> {
    if let Ok(p) = std::env::var("CANNON_PASS")
        && !p.is_empty()
    {
        return Some(p);
    }
    if let Some(p) = keyring_lookup(box_id) {
        return Some(p);
    }
    secrets_dir.and_then(|d| file_lookup(d, box_id))
}

/// Speichert bevorzugt im Keyring, sonst in der Datei. Liefert den genutzten
/// Weg zurück (fürs Logging).
pub fn store(box_id: &str, pass: &str, secrets_dir: Option<&Path>) -> &'static str {
    if keyring_store(box_id, pass) {
        "keyring"
    } else if secrets_dir
        .map(|d| file_store(d, box_id, pass))
        .unwrap_or(false)
    {
        "datei"
    } else {
        "nirgends"
    }
}
