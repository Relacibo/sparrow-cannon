//! Widgets: das Herz der Kanone. Ein Widget = optionale Action + optionaler
//! Status + Trigger. Methoden sind Registry-Einträge mit Param-Schema —
//! daraus bauen CLI und UI ihre Formulare, ohne pro Provider UI-Code.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::process::Command;

use crate::config::ConfigFile;
use crate::{pass, tr064, BoxProfile};

/// Ein Parameterfeld im Schema einer Methode.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Mac,
}

/// Ausführungskontext: aufgelöste Boxen (inkl. Passwort) für fritzbox-Methoden.
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    pub boxes: BTreeMap<String, BoxProfile>,
    pub secrets_dir: Option<std::path::PathBuf>,
}

impl Ctx {
    /// Baut den Kontext aus der Config (Passwörter via env/keyring/datei).
    pub fn from_config(conf: &ConfigFile, secrets_dir: Option<std::path::PathBuf>) -> Self {
        let mut boxes = BTreeMap::new();
        for (id, b) in &conf.boxes {
            if let Some(pass) = pass::resolve(id, secrets_dir.as_deref()) {
                boxes.insert(
                    id.clone(),
                    BoxProfile {
                        name: id.clone(),
                        base_url: b.base_url.trim_end_matches('/').to_string(),
                        user: b.user.clone(),
                        pass,
                    },
                );
            }
        }
        Ctx { boxes, secrets_dir }
    }

    fn box_by_param(&self, p: &Params) -> anyhow::Result<&BoxProfile> {
        let id = p.get("box");
        self.boxes
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("box '{id}' nicht gefunden oder passwort fehlt"))
    }
}

/// Ein Parameterfeld-Wertesatz einer Op.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Params(BTreeMap<String, String>);

impl Params {
    pub fn get(&self, key: &str) -> &str {
        self.0.get(key).map(String::as_str).unwrap_or("")
    }
    pub fn set(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }
}

/// Ein Kartenelement: Action, Status, beides oder keines.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Widget {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub action: Option<Op>,
    #[serde(default)]
    pub status: Option<Op>,
    #[serde(default)]
    pub trigger: Trigger,
}

/// Eine auszuführende/auswertbare Operation.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Op {
    /// Registry-Schlüssel: "<provider>.<methode>"
    pub kind: String,
    #[serde(default)]
    pub params: Params,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Trigger {
    /// manual | schedule
    #[serde(default)]
    pub kind: String,
    /// Sekunden-Intervall (nur bei kind = schedule)
    #[serde(default)]
    pub interval_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetState {
    pub id: String,
    pub title: String,
    /// Action-Teil: OK (gefeuert) | ERR | IDLE
    pub action_state: String,
    pub action_output: String,
    /// Status-Teil: OK | FAIL | ERR | IDLE
    pub status_state: String,
    pub status_output: String,
    pub has_action: bool,
}

/// Registry: alle Methoden mit ihrem Param-Schema (UI generiert Formulare daraus).
pub fn field_defs() -> BTreeMap<&'static str, Vec<FieldDef>> {
    let mut m = BTreeMap::new();
    let mut def = |kind: &'static str, fields: Vec<FieldDef>| {
        m.insert(kind, fields);
    };
    def(
        "fritzbox.wake",
        vec![
            FieldDef { key: "box", label: "Box-ID", kind: FieldKind::Text, required: true },
            FieldDef { key: "mac", label: "MAC-Adresse", kind: FieldKind::Mac, required: true },
        ],
    );
    def(
        "fritzbox.status",
        vec![
            FieldDef { key: "box", label: "Box-ID", kind: FieldKind::Text, required: true },
            FieldDef { key: "mac", label: "MAC-Adresse", kind: FieldKind::Mac, required: true },
        ],
    );
    def(
        "ssh.run",
        vec![
            FieldDef { key: "host", label: "SSH-Ziel", kind: FieldKind::Text, required: true },
            FieldDef { key: "command", label: "Befehl", kind: FieldKind::Text, required: true },
        ],
    );
    def(
        "ping.check",
        vec![FieldDef { key: "host", label: "Host", kind: FieldKind::Text, required: true }],
    );
    m
}

fn ssh(dest: &str, cmd: &str) -> anyhow::Result<String> {
    let out = Command::new("timeout")
        .args([
            "15",
            "ssh",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "LogLevel=ERROR",
            dest,
            cmd,
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        anyhow::bail!(
            "ssh {dest} rc={}: {}",
            out.status.code().unwrap_or(-1),
            if stderr.is_empty() { &stdout } else { &stderr }
        );
    }
    Ok(stdout)
}

fn ping(host: &str) -> anyhow::Result<String> {
    let out = Command::new("ping").args(["-c", "1", "-W", "2", host]).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .find(|l| l.starts_with("rtt") || l.starts_with("round-trip"))
            .unwrap_or("online")
            .to_string())
    } else {
        anyhow::bail!("keine antwort von {host}")
    }
}

/// Führt eine Op aus (Action-Feuer ODER Status-Check).
pub fn execute(op: &Op, ctx: &Ctx) -> anyhow::Result<String> {
    match op.kind.as_str() {
        "ssh.run" => ssh(op.params.get("host"), op.params.get("command")),
        "ping.check" => ping(op.params.get("host")),
        "fritzbox.wake" => {
            let b = ctx.box_by_param(&op.params)?;
            tr064::wake_on_lan(b, op.params.get("mac"))?;
            Ok(format!("wake an {} geschickt", op.params.get("mac")))
        }
        "fritzbox.status" => {
            let b = ctx.box_by_param(&op.params)?;
            let s = tr064::host_status(b, op.params.get("mac"))?;
            Ok(if s.active {
                format!("läuft ({})", s.ip.as_deref().unwrap_or("IP unbekannt"))
            } else {
                "aus".into()
            })
        }
        other => anyhow::bail!("unbekannte methode: {other}"),
    }
}

/// Status einer Op als Kartenzustand.
fn eval(op: &Op, ctx: &Ctx) -> (String, String) {
    match execute(op, ctx) {
        Ok(out) => ("OK".into(), out.chars().take(300).collect()),
        Err(e) => ("ERR".into(), e.to_string()),
    }
}

/// States aller Widgets (Status-Teil live, Action-Teil zuletzt gefeuert).
pub fn widget_states(widgets: &[Widget], ctx: &Ctx) -> Vec<WidgetState> {
    widgets
        .iter()
        .map(|w| {
            let (status_state, status_output) = match &w.status {
                Some(op) => eval(op, ctx),
                None => ("IDLE".into(), String::new()),
            };
            WidgetState {
                id: w.id.clone(),
                title: if w.title.is_empty() {
                    w.id.clone()
                } else {
                    w.title.clone()
                },
                action_state: "IDLE".into(),
                action_output: String::new(),
                status_state,
                status_output,
                has_action: w.action.is_some(),
            }
        })
        .collect()
}

/// Action-Teil eines Widgets feuern.
pub fn fire(w: &Widget, ctx: &Ctx) -> anyhow::Result<String> {
    let Some(op) = &w.action else {
        anyhow::bail!("widget '{}' hat keine action", w.id);
    };
    execute(op, ctx)
}
