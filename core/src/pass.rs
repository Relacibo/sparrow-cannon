//! Passwort-Auflösung ohne Interaktion: env → Keyring (secret-tool).
//! Interaktive Prompts gehören in die Frontends (cli: rpassword,
//! app später: tauri secure store — hier schlägt das dann fehl).

pub fn resolve_from_env_or_keyring() -> Option<String> {
    if let Ok(p) = std::env::var("CANNON_PASS") {
        if !p.is_empty() {
            return Some(p);
        }
    }
    let out = std::process::Command::new("secret-tool")
        .args(["lookup", "service", "sparrow-cannon", "username", "fritzbox"])
        .output()
        .ok()?;
    if out.status.success() {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !p.is_empty() {
            return Some(p);
        }
    }
    None
}

/// Schreibt das Passwort in den Secret Service. Gibt false zurück, wenn
/// kein Keyring existiert (z. B. Android) — der Aufrufer fällt dann auf
/// die Datei-Persistenz zurück.
pub fn store_to_keyring(pass: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let Ok(mut child) = Command::new("secret-tool")
        .args([
            "store",
            "--label=sparrow-cannon",
            "service",
            "sparrow-cannon",
            "username",
            "fritzbox",
        ])
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
