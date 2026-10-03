//! Secret-Store: alles in einer Datei — `<config-dir>/secrets.toml` (chmod 600).
//!
//! Format: flache `id = "wert"`-Map. Private Keys (PEM) bewusst NICHT hier —
//! die liegen als eigene Dateien (z. B. secrets/device.key), der SSH-Pfad
//! erwartet Keyfile-Pfade.
//! Resolve-Reihenfolge: env CANNON_PASS → secrets.toml.
//! Legacy (Keyring, secrets/<id>.txt) wird bewusst nicht mehr gelesen
//! (hard cut — bestaunte Passwörter einmalig neu eintragen).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn secrets_file(dir: &Path) -> PathBuf {
    dir.join("secrets.toml")
}

fn load(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(secrets_file(dir))
        .ok()
        .and_then(|raw| toml::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, map: &BTreeMap<String, String>) -> std::io::Result<()> {
    let body = toml::to_string_pretty(map).map_err(std::io::Error::other)?;
    let path = secrets_file(dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Atomic (tmp + rename): Parallel-Leser (Scheduler/Sync) sehen nie
    // halbe Dateien.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Ein Secret lesen (ohne env-Shortcut — für `${secret:id}`-Expansion).
pub fn lookup(id: &str, dir: Option<&Path>) -> Option<String> {
    let dir = dir?;
    load(dir).remove(id).filter(|v| !v.is_empty())
}

/// Komfort-Auflösung: env CANNON_PASS → secrets.toml.
pub fn resolve(id: &str, dir: Option<&Path>) -> Option<String> {
    if let Ok(p) = std::env::var("CANNON_PASS")
        && !p.is_empty()
    {
        return Some(p);
    }
    lookup(id, dir)
}

/// Secret speichern. Liefert den genutzten Weg (fürs Logging).
pub fn store(id: &str, pass: &str, dir: Option<&Path>) -> &'static str {
    match dir {
        Some(d) => {
            let mut map = load(d);
            map.insert(id.to_string(), pass.to_string());
            match save(d, &map) {
                Ok(()) => "datei",
                Err(_) => "nirgends",
            }
        }
        None => "nirgends",
    }
}

pub fn delete(id: &str, dir: Option<&Path>) {
    if let Some(d) = dir {
        let mut map = load(d);
        if map.remove(id).is_some() {
            let _ = save(d, &map);
        }
    }
}

/// IDs aller Secrets (für UI/CLI-Liste — niemals die Werte).
pub fn ids(dir: Option<&Path>) -> Vec<String> {
    dir.map(|d| load(d).into_keys().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "cannon-pass-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn store_lookup_roundtrip() {
        let dir = tmpdir();
        assert_eq!(lookup("x", Some(&dir)), None);
        assert_eq!(store("x", "geheim", Some(&dir)), "datei");
        assert_eq!(lookup("x", Some(&dir)).as_deref(), Some("geheim"));
        assert_eq!(ids(Some(&dir)), vec!["x".to_string()]);
        delete("x", Some(&dir));
        assert_eq!(lookup("x", Some(&dir)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_erzeugt_0600_datei() {
        let dir = tmpdir();
        store("x", "geheim", Some(&dir));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(secrets_file(&dir))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ohne_dir_geht_nichts() {
        assert_eq!(store("x", "y", None), "nirgends");
        assert_eq!(lookup("x", None), None);
        assert!(ids(None).is_empty());
    }

    #[test]
    fn leerer_wert_gilt_nicht() {
        let dir = tmpdir();
        store("leer", "", Some(&dir));
        assert_eq!(lookup("leer", Some(&dir)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
