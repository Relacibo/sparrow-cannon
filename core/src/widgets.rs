//! Widgets: das Herz der Kanone. Ein Widget = optionale Action + optionaler
//! Status + Trigger. Methoden sind Registry-Einträge mit Param-Schema —
//! daraus bauen CLI und UI ihre Formulare, ohne pro Provider UI-Code.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::process::Command;

use crate::config::ConfigFile;
use crate::{BoxProfile, pass, tr064};

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
    /// Dropdown über connections.ssh.<id>
    SshConn,
}

/// Ausführungskontext: aufgelöste Boxen (inkl. Passwort) für fritzbox-Methoden
/// und die SSH-Verbindungen (Desktop: dest-String; Android später: russh+keyref).
/// Aufgelöstes SSH-Ziel einer Verbindung.
#[derive(Debug, Clone)]
pub struct SshTarget {
    pub dest: String,
    pub user: String,
    pub keyfile: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct Ctx {
    pub boxes: BTreeMap<String, BoxProfile>,
    pub ssh: BTreeMap<String, SshTarget>,
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
        let mut ssh_conns = BTreeMap::new();
        // globaler device-key für alle ssh-verbindungen (identität des geräts)
        let device_key = secrets_dir
            .as_deref()
            .map(|d| d.join("secrets").join("device.key"))
            .filter(|p| p.exists());
        for (id, c) in &conf.connections.ssh {
            ssh_conns.insert(
                id.clone(),
                SshTarget {
                    dest: c.dest.clone(),
                    user: c.user.clone(),
                    keyfile: device_key.clone(),
                },
            );
        }
        Ctx {
            boxes,
            ssh: ssh_conns,
            secrets_dir,
        }
    }

    /// SSH-Ziel: connection-ID auflösen, sonst rohen host-Parameter nutzen.
    fn ssh_target(&self, p: &Params) -> anyhow::Result<SshTarget> {
        let raw = p.get("host");
        if !raw.is_empty() {
            return Ok(SshTarget {
                dest: raw.to_string(),
                user: String::new(),
                keyfile: None,
            });
        }
        let conn = p.get("connection");
        self.ssh
            .get(conn)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("ssh-verbindung '{conn}' nicht gefunden"))
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

/// Ein Kartenelement: Status + bedingte Actions + Trigger.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Widget {
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// Deaktivierte Widgets werden nicht mehr geprüft/gefeuert.
    #[serde(default)]
    pub disabled: bool,
    /// Periodische Status-Abfrage pausiert (Button oben rechts auf der Karte).
    #[serde(default)]
    pub status_paused: bool,
    /// Karte bietet den Pause/Play-Schalter an (Option beim Erstellen).
    #[serde(default)]
    pub pausable: bool,
    #[serde(default)]
    pub status: Option<Op>,
    /// Buttons mit Bedingung: when = always | ok | fail | <text>
    /// (<text>: Button sichtbar, wenn der Status-Output ihn enthält)
    #[serde(default)]
    pub actions: Vec<CondAction>,
    /// Legacy (einzelner unbedingter Button) — wird als "always" behandelt.
    #[serde(default)]
    pub action: Option<Op>,
    #[serde(default)]
    pub trigger: Trigger,
}

/// Ein bedingter Button: erscheint, wenn der Status `when` erfüllt.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CondAction {
    pub label: String,
    /// always | ok | fail
    #[serde(default)]
    pub when: String,
    pub op: Op,
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
    /// historisch (manual|schedule) — wird nicht mehr ausgewertet
    #[serde(default)]
    pub kind: String,
    /// Prüfintervall in Sekunden (0 = Standard 10)
    #[serde(default)]
    pub interval_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionBtn {
    pub label: String,
    pub when: String,
    pub index: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetState {
    pub id: String,
    pub title: String,
    pub disabled: bool,
    pub status_paused: bool,
    pub pausable: bool,
    /// Vollständige Definition fürs Bearbeiten.
    pub def: Widget,
    /// Status-Teil: OK | FAIL | ERR | IDLE
    pub status_state: String,
    pub status_output: String,
    /// Buttons, deren Bedingung aktuell erfüllt ist.
    pub buttons: Vec<ActionBtn>,
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
            FieldDef {
                key: "box",
                label: "Box-ID",
                kind: FieldKind::Text,
                required: true,
            },
            FieldDef {
                key: "mac",
                label: "MAC-Adresse",
                kind: FieldKind::Mac,
                required: true,
            },
        ],
    );
    def(
        "fritzbox.status",
        vec![
            FieldDef {
                key: "box",
                label: "Box-ID",
                kind: FieldKind::Text,
                required: true,
            },
            FieldDef {
                key: "mac",
                label: "MAC-Adresse",
                kind: FieldKind::Mac,
                required: true,
            },
        ],
    );
    def(
        "ssh.run",
        vec![
            FieldDef {
                key: "connection",
                label: "SSH-Verbindung",
                kind: FieldKind::SshConn,
                required: true,
            },
            FieldDef {
                key: "command",
                label: "Befehl",
                kind: FieldKind::Text,
                required: true,
            },
            FieldDef {
                key: "ok_contains",
                label: "OK-Muster (enthält, optional)",
                kind: FieldKind::Text,
                required: false,
            },
        ],
    );
    def(
        "ping.check",
        vec![FieldDef {
            key: "host",
            label: "Host",
            kind: FieldKind::Text,
            required: true,
        }],
    );
    m
}

fn ssh(t: &SshTarget, cmd: &str) -> anyhow::Result<String> {
    #[cfg(target_os = "android")]
    {
        match &t.keyfile {
            Some(k) => {
                let key = std::fs::read_to_string(k)
                    .map_err(|e| anyhow::anyhow!("key {}: {e}", k.display()))?;
                return crate::ssh::exec_with_key(&t.dest, &t.user, &key, cmd);
            }
            None => anyhow::bail!("verbindung '{}' hat keinen in-app-key", t.dest),
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        crate::ssh::exec(&t.dest, cmd)
    }
}

fn ping(host: &str) -> anyhow::Result<String> {
    let out = Command::new("ping")
        .args(["-c", "1", "-W", "2", host])
        .output()?;
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
        "ssh.run" => {
            let t = ctx.ssh_target(&op.params)?;
            ssh(&t, op.params.get("command"))
        }
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

/// Public-Wrapper für Scheduler/Threads.
pub fn eval_status(op: &Op, ctx: &Ctx) -> (String, String) {
    eval(op, ctx)
}

/// Status einer Op als Kartenzustand.
/// OK = Zustand erfüllt, FAIL = Zustand nicht erfüllt (z.B. aus),
/// ERR = Check selbst fehlgeschlagen (netz/ssh).
fn eval(op: &Op, ctx: &Ctx) -> (String, String) {
    match op.kind.as_str() {
        "ssh.run" => {
            let t = match ctx.ssh_target(&op.params) {
                Ok(t) => t,
                Err(e) => return ("ERR".into(), e.to_string()),
            };
            match ssh(&t, op.params.get("command")) {
                Err(e) => ("ERR".into(), e.to_string()),
                Ok(out) => {
                    let want = op.params.get("ok_contains");
                    let ok = want.is_empty() || out.contains(want);
                    (
                        if ok { "OK".into() } else { "FAIL".into() },
                        out.chars().take(300).collect(),
                    )
                }
            }
        }
        "ping.check" => match ping(op.params.get("host")) {
            Ok(out) => ("OK".into(), out.chars().take(120).collect()),
            Err(e) => ("FAIL".into(), e.to_string()),
        },
        "fritzbox.status" => {
            let b = match ctx.box_by_param(&op.params) {
                Ok(b) => b,
                Err(e) => return ("ERR".into(), e.to_string()),
            };
            match tr064::host_status(b, op.params.get("mac")) {
                Ok(s) if s.active => (
                    "OK".into(),
                    format!("läuft ({})", s.ip.as_deref().unwrap_or("IP unbekannt")),
                ),
                Ok(_) => ("FAIL".into(), "aus".into()),
                Err(e) => ("ERR".into(), e.to_string()),
            }
        }
        other => ("ERR".into(), format!("unbekannte methode: {other}")),
    }
}

/// States aller Widgets: Status live, Buttons nach Bedingung gefiltert.
pub fn widget_states(widgets: &[Widget], ctx: &Ctx) -> Vec<WidgetState> {
    widgets
        .iter()
        .map(|w| {
            let (status_state, status_output) = match &w.status {
                Some(op) => eval(op, ctx),
                None => ("IDLE".into(), String::new()),
            };
            // Legacy action-Feld als always-Button vorne anstellen
            let mut actions: Vec<CondAction> = Vec::new();
            if let Some(op) = &w.action {
                actions.push(CondAction {
                    label: "Ausführen".into(),
                    when: "always".into(),
                    op: op.clone(),
                });
            }
            actions.extend(w.actions.iter().cloned());

            // pausiert: alle optionen verfügbar (status unbekannt → manuell entscheiden)
            let buttons: Vec<ActionBtn> = actions
                .iter()
                .enumerate()
                .filter(|(_, a)| {
                    let when = a.when.trim().to_lowercase();
                    let visible = match when.as_str() {
                        "always" | "" => true,
                        "ok" => status_state == "OK",
                        "fail" => status_state == "FAIL" || status_state == "ERR",
                        needle => status_output.to_lowercase().contains(needle),
                    };
                    w.status_paused || visible
                })
                .map(|(i, a)| ActionBtn {
                    label: a.label.clone(),
                    when: a.when.clone(),
                    index: i,
                })
                .collect();

            WidgetState {
                id: w.id.clone(),
                disabled: w.disabled,
                status_paused: w.status_paused,
                pausable: w.pausable,
                def: w.clone(),
                title: if w.title.is_empty() {
                    w.id.clone()
                } else {
                    w.title.clone()
                },
                status_state,
                status_output,
                buttons,
            }
        })
        .collect()
}

/// Bedingten Button (Index in der Action-Liste) feuern.
pub fn fire(w: &Widget, index: usize, ctx: &Ctx) -> anyhow::Result<String> {
    let mut actions: Vec<CondAction> = Vec::new();
    if let Some(op) = &w.action {
        actions.push(CondAction {
            label: "Ausführen".into(),
            when: "always".into(),
            op: op.clone(),
        });
    }
    actions.extend(w.actions.iter().cloned());
    let a = actions
        .get(index)
        .ok_or_else(|| anyhow::anyhow!("button {index} existiert nicht"))?;
    execute(&a.op, ctx)
}
