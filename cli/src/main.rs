use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cannon", about = "FRITZ!Box-Kanone: WoL & mehr über TR-064")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Weckt einen Host über die FRITZ!Box (funktioniert auch über WireGuard).
    Wake { host: String },
    /// Zeigt den Live-Zustand eines Hosts (oder aller).
    Status { host: Option<String> },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Wake { host } => {
            let _ = (host, fritz_cannon_core::Config::default());
            anyhow::bail!("WIP: Config-Laden + HTTP-Client fehlen noch")
        }
        Cmd::Status { host } => {
            let _ = (host, fritz_cannon_core::Config::default());
            anyhow::bail!("WIP: Config-Laden + HTTP-Client fehlen noch")
        }
    }
}
