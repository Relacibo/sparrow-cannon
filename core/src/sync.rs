//! Pull-Sync der Config von einem SSH-Ziel (Desktop = Source of Truth).
//!
//! Joplin-ähnlich, pull-only: pro Item ein `base_hash` = der Hash der
//! Version, die beim letzten Sync vom Remote übernommen wurde (state-Datei,
//! selbst nicht synchronisiert). Drei-Fälle-Logik pro Widget:
//!   - lokal unverändert (hash == base) → Remote-Version anwenden
//!   - lokal UND remote geändert → Konflikt: Remote gewinnt, die lokale
//!     Version wird als deaktivierte `(Konflikt)`-Karte abgelegt
//!   - nur lokal geändert → lokale Version bleibt (base bleibt ungesetzt —
//!     das Item bleibt "ungesynct", ein späterer Remote-Edit wird zum Konflikt)
//!
//! Remote gelöscht + lokal unverändert → löschen; + lokal geändert → Stash.
//! Lokal neu (kein base-Eintrag) → bleibt.
//!
//! Verbindungen (ssh/box): gleiche Logik ohne Stash — Desktop gewinnt bei
//! echten Konflikten (trivial wiederherstellbar), lokal neue bleiben.

use crate::config::{BoxFile, ConfigFile, Connections, SshConn};
use crate::widgets::Widget;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Config-Rohtext von einer URL laden (raw.githubusercontent.com — liefert
/// immer den neuesten Stand des Branches).
pub fn fetch_config(url: &str) -> anyhow::Result<String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if resp.status() != 200 {
        anyhow::bail!("http {}", resp.status());
    }
    resp.into_string()
        .map_err(|e| anyhow::anyhow!("response-body: {e}"))
}

/// Sync-Buchhaltung: item-id → hash der zuletzt übernommenen Remote-Version.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default)]
    pub widgets: BTreeMap<String, String>,
    #[serde(default)]
    pub ssh: BTreeMap<String, String>,
    #[serde(default)]
    pub box_conns: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
    pub conflicts: Vec<String>,
    pub kept: Vec<String>,
}

impl SyncReport {
    pub fn summary(&self) -> String {
        let parts = [
            (!self.added.is_empty()).then(|| format!("+{} neu", self.added.len())),
            (!self.updated.is_empty()).then(|| format!("{} aktualisiert", self.updated.len())),
            (!self.removed.is_empty()).then(|| format!("{} entfernt", self.removed.len())),
            (!self.conflicts.is_empty())
                .then(|| format!("{} konflikt(e) gesichert", self.conflicts.len())),
            (!self.kept.is_empty()).then(|| format!("{} nur lokal", self.kept.len())),
        ];
        let msg = parts.into_iter().flatten().collect::<Vec<_>>().join(", ");
        if msg.is_empty() {
            "nichts geändert".into()
        } else {
            msg
        }
    }
}

#[derive(Debug)]
pub struct SyncOutcome {
    pub conf: ConfigFile,
    pub state: SyncState,
    pub report: SyncReport,
}

fn hash<T: Serialize>(item: &T) -> String {
    let raw = toml::to_string(item).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    format!("{:x}", h.finalize())
}

/// Der Konflikt-Zeitstempel für eindeutige Stash-IDs (Sekunden reichen).
fn conflict_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| format!("k{}", d.as_secs()))
        .unwrap_or_else(|_| format!("k{}", std::process::id()))
}

fn stash_widget(w: &Widget) -> Widget {
    let mut s = w.clone();
    s.id = format!("{}-{}", w.id, conflict_suffix());
    s.title = format!(
        "{} (Konflikt)",
        if w.title.is_empty() { &w.id } else { &w.title }
    );
    s.disabled = true;
    s
}

/// Der eigentliche Merge (pure — I/O macht der Aufrufer).
pub fn merge(local: ConfigFile, remote: ConfigFile, state: &SyncState) -> SyncOutcome {
    let mut report = SyncReport::default();
    let mut new_state = SyncState::default();
    let mut out_widgets: Vec<Widget> = Vec::new();

    // --- Widgets: die drei Fälle + Stash ---
    let local_by_id: BTreeMap<String, Widget> = local
        .widgets
        .iter()
        .map(|w| (w.id.clone(), w.clone()))
        .collect();
    for rw in &remote.widgets {
        let rhash = hash(rw);
        match local_by_id.get(&rw.id) {
            None => {
                out_widgets.push(rw.clone());
                new_state.widgets.insert(rw.id.clone(), rhash);
                report.added.push(rw.id.clone());
            }
            Some(lw) => {
                let lhash = hash(lw);
                let local_changed = state
                    .widgets
                    .get(&rw.id)
                    .map(|b| *b != lhash)
                    .unwrap_or(true);
                let remote_changed = state
                    .widgets
                    .get(&rw.id)
                    .map(|b| *b != rhash)
                    .unwrap_or(true);
                if !local_changed {
                    out_widgets.push(rw.clone());
                    new_state.widgets.insert(rw.id.clone(), rhash.clone());
                    if rhash != lhash {
                        report.updated.push(rw.id.clone());
                    }
                } else if !remote_changed {
                    // nur lokal geändert, remote gleich → lokale Version bleibt,
                    // base bleibt ungesetzt (bleibt "ungesynct")
                    out_widgets.push((*lw).clone());
                    report.kept.push(rw.id.clone());
                } else {
                    // Konflikt: Remote gewinnt, lokale Version wird abgelegt
                    out_widgets.push(stash_widget(lw));
                    out_widgets.push(rw.clone());
                    new_state.widgets.insert(rw.id.clone(), rhash);
                    report.conflicts.push(rw.id.clone());
                }
            }
        }
    }
    for (id, lw) in &local_by_id {
        if remote.widgets.iter().any(|rw| rw.id == *id) {
            continue;
        }
        let lhash = hash(lw);
        match state.widgets.get(id) {
            Some(b) if *b == lhash => {
                // Desktop hat gelöscht, lokal unangetastet → mitlöschen
                report.removed.push(id.clone());
            }
            Some(_) => {
                out_widgets.push(stash_widget(lw));
                report.conflicts.push(id.clone());
            }
            None => {
                // lokal erstellt, nie synchronisiert → bleibt
                out_widgets.push(lw.clone());
                report.kept.push(id.clone());
            }
        }
    }

    // --- SSH-Verbindungen: wie oben, Konflikt → Desktop gewinnt ohne Stash ---
    let mut new_ssh: BTreeMap<String, SshConn> = BTreeMap::new();
    for (id, rc) in &remote.connections.ssh {
        let rhash = hash(rc);
        match local.connections.ssh.get(id) {
            None => {
                new_ssh.insert(id.clone(), rc.clone());
                new_state.ssh.insert(id.clone(), rhash);
                report.added.push(id.clone());
            }
            Some(lc) => {
                let lhash = hash(lc);
                let local_changed = state.ssh.get(id).map(|b| *b != lhash).unwrap_or(true);
                let remote_changed = state.ssh.get(id).map(|b| *b != rhash).unwrap_or(true);
                if !local_changed || !remote_changed {
                    if local_changed && !remote_changed {
                        new_ssh.insert(id.clone(), lc.clone());
                        report.kept.push(id.clone());
                    } else {
                        new_ssh.insert(id.clone(), rc.clone());
                        new_state.ssh.insert(id.clone(), rhash.clone());
                        if rhash != lhash {
                            report.updated.push(id.clone());
                        }
                    }
                } else {
                    new_ssh.insert(id.clone(), rc.clone());
                    new_state.ssh.insert(id.clone(), rhash);
                    report.conflicts.push(id.clone());
                }
            }
        }
    }
    for (id, lc) in &local.connections.ssh {
        if new_ssh.contains_key(id) {
            continue;
        }
        let lhash = hash(lc);
        match state.ssh.get(id) {
            Some(b) if *b == lhash => report.removed.push(id.clone()),
            _ => {
                new_ssh.insert(id.clone(), lc.clone());
                report.kept.push(id.clone());
            }
        }
    }

    // --- Box-Verbindungen: analog zu SSH ---
    let mut new_boxes: BTreeMap<String, BoxFile> = BTreeMap::new();
    for (id, rb) in &remote.boxes {
        let rhash = hash(rb);
        match local.boxes.get(id) {
            None => {
                new_boxes.insert(id.clone(), rb.clone());
                new_state.box_conns.insert(id.clone(), rhash);
                report.added.push(id.clone());
            }
            Some(lb) => {
                let lhash = hash(lb);
                let local_changed = state.box_conns.get(id).map(|b| *b != lhash).unwrap_or(true);
                let remote_changed = state.box_conns.get(id).map(|b| *b != rhash).unwrap_or(true);
                if !local_changed || !remote_changed {
                    if local_changed && !remote_changed {
                        new_boxes.insert(id.clone(), lb.clone());
                        report.kept.push(id.clone());
                    } else {
                        new_boxes.insert(id.clone(), rb.clone());
                        new_state.box_conns.insert(id.clone(), rhash.clone());
                        if rhash != lhash {
                            report.updated.push(id.clone());
                        }
                    }
                } else {
                    new_boxes.insert(id.clone(), rb.clone());
                    new_state.box_conns.insert(id.clone(), rhash);
                    report.conflicts.push(id.clone());
                }
            }
        }
    }
    for (id, lb) in &local.boxes {
        if new_boxes.contains_key(id) {
            continue;
        }
        let lhash = hash(lb);
        match state.box_conns.get(id) {
            Some(b) if *b == lhash => report.removed.push(id.clone()),
            _ => {
                new_boxes.insert(id.clone(), lb.clone());
                report.kept.push(id.clone());
            }
        }
    }

    let conf = ConfigFile {
        boxes: new_boxes,
        hosts: local.hosts,
        connections: Connections { ssh: new_ssh },
        actions: local.actions,
        widgets: out_widgets,
        sync: local.sync,
    };
    SyncOutcome {
        conf,
        state: new_state,
        report,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BoxFile, SshConn};

    fn widget(id: &str, title: &str) -> Widget {
        Widget {
            id: id.into(),
            title: title.into(),
            ..Default::default()
        }
    }

    fn conf(widgets: Vec<Widget>, ssh: BTreeMap<String, SshConn>) -> ConfigFile {
        ConfigFile {
            boxes: BTreeMap::new(),
            hosts: BTreeMap::new(),
            connections: Connections { ssh },
            actions: BTreeMap::new(),
            widgets,
            sync: None,
        }
    }

    fn ssh(id: &str, dest: &str) -> (String, SshConn) {
        (
            id.into(),
            SshConn {
                dest: dest.into(),
                user: String::new(),
                note: String::new(),
            },
        )
    }

    #[test]
    fn neuer_remote_widget_wird_uebernommen() {
        let out = merge(
            conf(vec![], BTreeMap::new()),
            conf(vec![widget("pc", "PC")], BTreeMap::new()),
            &SyncState::default(),
        );
        assert_eq!(out.report.added, vec!["pc"]);
        assert_eq!(out.conf.widgets.len(), 1);
        assert!(out.state.widgets.contains_key("pc"));
    }

    #[test]
    fn lokal_unveraendert_remote_ersetzt() {
        let state = SyncState {
            widgets: BTreeMap::from([("pc".into(), hash(&widget("pc", "alt")))]),
            ..Default::default()
        };
        let out = merge(
            conf(vec![widget("pc", "alt")], BTreeMap::new()),
            conf(vec![widget("pc", "NEU")], BTreeMap::new()),
            &state,
        );
        assert_eq!(out.report.updated, vec!["pc"]);
        assert_eq!(out.conf.widgets[0].title, "NEU");
    }

    #[test]
    fn nur_lokal_geaendert_bleibt() {
        let state = SyncState {
            widgets: BTreeMap::from([("pc".into(), hash(&widget("pc", "alt")))]),
            ..Default::default()
        };
        let out = merge(
            conf(vec![widget("pc", "lokal edit")], BTreeMap::new()),
            conf(vec![widget("pc", "alt")], BTreeMap::new()),
            &state,
        );
        assert!(out.report.kept.contains(&"pc".to_string()));
        assert_eq!(out.conf.widgets[0].title, "lokal edit");
        // base bleibt ungesetzt — der nächste remote-edit wird zum Konflikt
        assert!(!out.state.widgets.contains_key("pc"));
    }

    #[test]
    fn konflikt_stasht_lokale_version() {
        let state = SyncState {
            widgets: BTreeMap::from([("pc".into(), hash(&widget("pc", "alt")))]),
            ..Default::default()
        };
        let out = merge(
            conf(vec![widget("pc", "lokal edit")], BTreeMap::new()),
            conf(vec![widget("pc", "remote edit")], BTreeMap::new()),
            &state,
        );
        assert_eq!(out.report.conflicts, vec!["pc"]);
        assert_eq!(out.conf.widgets.len(), 2);
        let stash = out.conf.widgets.iter().find(|w| w.id != "pc").unwrap();
        assert!(stash.disabled);
        assert!(stash.title.contains("(Konflikt)"));
        assert_eq!(
            out.conf
                .widgets
                .iter()
                .find(|w| w.id == "pc")
                .unwrap()
                .title,
            "remote edit"
        );
    }

    #[test]
    fn remote_loeschung_loescht_unveraendertes() {
        let state = SyncState {
            widgets: BTreeMap::from([("pc".into(), hash(&widget("pc", "alt")))]),
            ..Default::default()
        };
        let out = merge(
            conf(vec![widget("pc", "alt")], BTreeMap::new()),
            conf(vec![], BTreeMap::new()),
            &state,
        );
        assert_eq!(out.report.removed, vec!["pc"]);
        assert!(out.conf.widgets.is_empty());
    }

    #[test]
    fn remote_loeschung_bei_lokaler_aenderung_stasht() {
        let state = SyncState {
            widgets: BTreeMap::from([("pc".into(), hash(&widget("pc", "alt")))]),
            ..Default::default()
        };
        let out = merge(
            conf(vec![widget("pc", "lokal edit")], BTreeMap::new()),
            conf(vec![], BTreeMap::new()),
            &state,
        );
        assert_eq!(out.report.conflicts, vec!["pc"]);
        assert!(out.conf.widgets.iter().any(|w| w.disabled));
    }

    #[test]
    fn lokal_neues_widget_bleibt() {
        let out = merge(
            conf(vec![widget("meins", "Mein Widget")], BTreeMap::new()),
            conf(vec![], BTreeMap::new()),
            &SyncState::default(),
        );
        assert!(out.report.kept.contains(&"meins".to_string()));
        assert_eq!(out.conf.widgets.len(), 1);
    }

    #[test]
    fn ssh_konflikt_desktop_gewinnt_ohne_stash() {
        let state = SyncState {
            ssh: BTreeMap::from([("pc".into(), hash(&ssh("pc", "alt").1))]),
            ..Default::default()
        };
        let local = conf(vec![], BTreeMap::from([ssh("pc", "lokal edit")]));
        let remote = conf(vec![], BTreeMap::from([ssh("pc", "remote edit")]));
        let out = merge(local, remote, &state);
        assert_eq!(out.report.conflicts, vec!["pc"]);
        assert_eq!(out.conf.connections.ssh["pc"].dest, "remote edit");
        assert_eq!(out.conf.connections.ssh.len(), 1);
    }

    #[test]
    fn ssh_lokal_neu_bleibt() {
        let out = merge(
            conf(vec![], BTreeMap::from([ssh("pc", "hier")])),
            conf(vec![], BTreeMap::new()),
            &SyncState::default(),
        );
        assert!(out.report.kept.contains(&"pc".to_string()));
        assert_eq!(out.conf.connections.ssh["pc"].dest, "hier");
    }

    #[test]
    fn box_sync_funktioniert() {
        let bf = BoxFile {
            base_url: "http://x".into(),
            user: "u".into(),
        };
        let state = SyncState {
            box_conns: BTreeMap::from([("daheim".into(), hash(&bf))]),
            ..Default::default()
        };
        let mut local = conf(vec![], BTreeMap::new());
        local.boxes.insert("daheim".into(), bf.clone());
        let out = merge(local, conf(vec![], BTreeMap::new()), &state);
        assert_eq!(out.report.removed, vec!["daheim"]);
        assert!(out.conf.boxes.is_empty());
    }
}
