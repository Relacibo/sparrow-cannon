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
}

/// Passwort-Auflösung: CANNON_PASS → secret-tool (keyring) → Prompt.
fn resolve_pass() -> anyhow::Result<String> {
    sparrow_cannon_core::pass::resolve_from_env_or_keyring().map_or_else(
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
    let pass = resolve_pass()?;
    let profiles = conf.build_with_pass(&pass);

    match cli.cmd {
        Cmd::Wake { host } => {
            let box_ = select_box(&profiles, &cli.r#box)?;
            let hosts = conf.hosts();
            let h: &Host = hosts
                .get(&host)
                .with_context(|| format!("host '{host}' fehlt in der config"))?;
            sparrow_cannon_core::wake(&box_, h)?;
            println!("wake an {host} ({}) geschickt.", h.mac);
        }
        Cmd::Status { host } => {
            let box_ = select_box(&profiles, &cli.r#box)?;
            let mut hosts = conf.hosts();
            if let Some(h) = &host {
                let one = hosts.remove(h).context(format!("host '{h}' fehlt"))?;
                hosts.insert(h.clone(), one);
                hosts.retain(|id, _| id == h);
            }
            println!("{:<10} {:<5} {:<16} {}", "host", "state", "ip", "hostname");
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
        Cmd::Doctor => {
            let box_ = select_box(&profiles, &cli.r#box)?;
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
