use anyhow::Context;
use clap::{Parser, Subcommand};
use sparrow_cannon_core::config::ConfigFile;
use sparrow_cannon_core::{BoxProfile, Host};
use std::io::Write;

#[derive(Parser)]
#[command(
    name = "cannon",
    about = "FRITZ!Box-Kanone: WoL & mehr über TR-064 (funktiert auch über WireGuard)",
    version
)]
struct Cli {
    /// Box aus der Config (Default: die erste).
    #[arg(long, global = true)]
    r#box: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Weckt einen Host über die FRITZ!Box.
    Wake { host: String },
    /// Zeigt den Live-Zustand eines Hosts (oder aller).
    Status { host: Option<String> },
    /// Listet die TR-064-Services der Box (Diagnose + später generischer Runner).
    Doctor,
    /// Zeigt alle Actions mit Live-Status.
    Actions,
    /// Führt eine Action aus.
    Run { action: String },
    /// Zeigt alle Widgets mit Live-Status.
    Widgets,
    /// Feuert die Action eines Widgets.
    Fire { widget: String },
}

/// Passwort-Auflösung für die gewählte Box: CANNON_PASS → keyring(box-id) → Prompt.
fn resolve_pass(box_id: &str) -> anyhow::Result<String> {
    sparrow_cannon_core::pass::resolve(box_id, None).map_or_else(
        || rpassword::prompt_password("Fritzbox-Passwort: ").context("passwort-eingabe"),
        Ok,
    )
}

fn select_box(
    profiles: &std::collections::BTreeMap<String, BoxProfile>,
    wanted: &Option<String>,
) -> anyhow::Result<BoxProfile> {
    match wanted {
        Some(name) => profiles
            .get(name)
            .cloned()
            .with_context(|| format!("box '{name}' fehlt in der config")),
        None => profiles
            .values()
            .next()
            .cloned()
            .context("keine [boxes.*] in der config"),
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let conf = ConfigFile::load_default().context("config-problem")?;
    let box_name = cli
        .r#box
        .clone()
        .or_else(|| conf.boxes.keys().next().cloned())
        .context("keine [boxes.*] in der config")?;
    let pass = resolve_pass(&box_name)?;
    let profiles = conf.build_with_pass(&pass);
    let box_ = select_box(&profiles, &cli.r#box)?;

    match cli.cmd {
        Cmd::Wake { host } => {
            let hosts = conf.hosts();
            let h: &Host = hosts
                .get(&host)
                .with_context(|| format!("host '{host}' fehlt in der config"))?;
            sparrow_cannon_core::wake(&box_, h)?;
            println!("wake an {host} ({}) geschickt.", h.mac);
        }
        Cmd::Status { host } => {
            let mut hosts = conf.hosts();
            if let Some(h) = &host {
                let one = hosts.remove(h).context(format!("host '{h}' fehlt"))?;
                hosts.insert(h.clone(), one);
                hosts.retain(|id, _| id == h);
            }
            println!("{:<10} {:<5} {:<16} hostname", "host", "state", "ip");
            for (id, h) in &hosts {
                match sparrow_cannon_core::status(&box_, h) {
                    Ok(s) => println!(
                        "{:<10} {:<5} {:<16} {}",
                        id,
                        if s.active { "UP" } else { "DOWN" },
                        s.ip.as_deref().unwrap_or("-"),
                        s.hostname.as_deref().unwrap_or("-"),
                    ),
                    Err(e) => println!("{:<10} ERR   {:<16} {}", id, "-", e.root_cause()),
                }
            }
        }
        Cmd::Actions => {
            for (id, a) in &conf.actions {
                let dest = conf.ssh_dest(&a.host).unwrap_or_else(|e| e.to_string());
                let r = sparrow_cannon_core::actions::check(&dest, id, a);
                println!(
                    "{:<20} {:<5} {}",
                    id,
                    r.state,
                    r.output.lines().next().unwrap_or("")
                );
            }
            if conf.actions.is_empty() {
                println!("keine [actions.*] in der config");
            }
        }
        Cmd::Run { action } => {
            let a = conf
                .actions
                .get(&action)
                .with_context(|| format!("action '{action}' fehlt in der config"))?;
            let dest = conf.ssh_dest(&a.host)?;
            let out = sparrow_cannon_core::actions::run(&dest, a)?;
            if !out.is_empty() {
                println!("{out}");
            }
        }
        Cmd::Widgets => {
            let ctx = sparrow_cannon_core::widgets::Ctx::from_config(&conf, None);
            for w in &conf.widgets {
                let st = sparrow_cannon_core::widgets::widget_states(std::slice::from_ref(w), &ctx)
                    .into_iter()
                    .next()
                    .unwrap();
                println!(
                    "{:<20} status={:<5} {}",
                    st.id,
                    st.status_state,
                    st.status_output.lines().next().unwrap_or("")
                );
            }
            if conf.widgets.is_empty() {
                println!("keine [[widgets]] in der config");
            }
        }
        Cmd::Fire { widget } => {
            let ctx = sparrow_cannon_core::widgets::Ctx::from_config(&conf, None);
            let w = conf
                .widgets
                .iter()
                .find(|w| w.id == widget)
                .with_context(|| format!("widget '{widget}' fehlt in der config"))?;
            let out = sparrow_cannon_core::widgets::fire(w, 0, &ctx)?;
            if !out.is_empty() {
                println!("{out}");
            }
        }
        Cmd::Doctor => {
            println!(
                "box '{}' → {} (user {})",
                box_.name, box_.base_url, box_.user
            );
            let svcs = sparrow_cannon_core::tr064::services(&box_)?;
            println!("{} services:", svcs.len());
            for (stype, url) in svcs {
                println!("  {stype:<60} {url}");
            }
        }
    }
    let _ = std::io::stdout().flush();
    Ok(())
}
