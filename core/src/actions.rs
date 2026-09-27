//! Actions: SSH-Kommandos mit optionalem Status-Check.
//! Karte = Action (+ Status). Status ist nur eine schreibgeschützte Action.

use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ActionFile {
    /// Ziel-Host (id aus [hosts.*]) — braucht dort `ssh = <ziel>`.
    pub host: String,
    /// Auszuführendes Kommando (Button). Leer = reine Status-Karte.
    #[serde(default)]
    pub run: String,
    /// Status-Check-Kommando. Leer = Karte ohne Status (IDLE).
    #[serde(default)]
    pub check: String,
    /// Optional: Ausgabe muss das enthalten, sonst FAIL.
    #[serde(default)]
    pub check_ok: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub id: String,
    pub host: String,
    /// OK | FAIL | ERR | IDLE
    pub state: String,
    pub output: String,
    pub has_run: bool,
}

fn ssh(dest: &str, cmd: &str) -> anyhow::Result<String> {
    crate::ssh::exec(dest, cmd)
}

/// Führt die Action aus und liefert die Ausgabe.
pub fn run(dest: &str, a: &ActionFile) -> anyhow::Result<String> {
    if a.run.is_empty() {
        anyhow::bail!("aktion ohne run-kommando");
    }
    ssh(dest, &a.run)
}

/// Status-Check einer Action (schreibgeschützt).
pub fn check(dest: &str, id: &str, a: &ActionFile) -> ActionResult {
    let base = |state: &str, output: &str| ActionResult {
        id: id.to_string(),
        host: a.host.clone(),
        state: state.to_string(),
        output: output.chars().take(300).collect(),
        has_run: !a.run.is_empty(),
    };
    if a.check.is_empty() {
        return base("IDLE", "");
    }
    match ssh(dest, &a.check) {
        Ok(out) => {
            let ok = a.check_ok.is_empty() || out.contains(&a.check_ok);
            base(if ok { "OK" } else { "FAIL" }, &out)
        }
        Err(e) => base("ERR", &e.to_string()),
    }
}
