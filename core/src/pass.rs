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
