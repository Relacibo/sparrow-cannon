//! SSH-Transport-Dispatch.
//!
//! Desktop: System-ssh (ssh_config-Aliase, Agent, ProxyJump — alles gratis).
//! Android: russh mit In-App-Key (generiert via keys.rs, abgelegt in secrets/).

use std::process::Command;

/// Führt ein Kommando auf einem SSH-Ziel aus (15s Gesamt, 5s Connect).
pub fn exec(dest: &str, cmd: &str) -> anyhow::Result<String> {
    #[cfg(target_os = "android")]
    {
        russh_exec(dest, cmd)
    }
    #[cfg(not(target_os = "android"))]
    {
        system_ssh(dest, cmd)
    }
}

/// SSH-Zielparser: "user@host:port" | "host:port" | "host".
pub fn parse_dest(dest: &str, default_user: &str) -> (String, String, u16) {
    let (user_part, host_port) = match dest.split_once('@') {
        Some((u, rest)) => (u.to_string(), rest.to_string()),
        None => (default_user.to_string(), dest.to_string()),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
            (h.to_string(), p.parse().unwrap_or(22u16))
        }
        _ => (host_port.clone(), 22u16),
    };
    (user_part, host, port)
}

#[cfg(not(target_os = "android"))]
fn system_ssh(dest: &str, cmd: &str) -> anyhow::Result<String> {
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
            "-o",
            "ControlMaster=auto",
            "-o",
            "ControlPath=%d/.ssh/cannon-mux-%r@%h:%p",
            "-o",
            "ControlPersist=10m",
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

#[cfg(target_os = "android")]
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"))
}

#[cfg(target_os = "android")]
async fn russh_connect_exec(
    host: &str,
    port: u16,
    user: &str,
    key_pair: russh::keys::key::KeyPair,
    cmd: &str,
) -> anyhow::Result<String> {
    use std::sync::Arc;

    struct Client;
    #[async_trait::async_trait]
    impl russh::client::Handler for Client {
        type Error = anyhow::Error;
        async fn check_server_key(
            &mut self,
            _key: &russh::keys::key::PublicKey,
        ) -> Result<bool, Self::Error> {
            // Private-Use: LAN/WireGuard only. Hostkey-Pinning später.
            Ok(true)
        }
    }

    let config = russh::client::Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(15)),
        ..Default::default()
    };
    let mut session = russh::client::connect(Arc::new(config), (host, port), Client).await?;
    let authed = session
        .authenticate_publickey(user.to_string(), Arc::new(key_pair))
        .await?;
    if !authed {
        anyhow::bail!("ssh-auth fehlgeschlagen ({user}@{host}:{port})");
    }
    let mut channel = session.channel_open_session().await?;
    channel.exec(true, cmd).await?;
    let mut out: Vec<u8> = Vec::new();
    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Data { ref data }) => out.extend_from_slice(data),
            Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => out.extend_from_slice(data),
            Some(russh::ChannelMsg::ExitStatus { .. }) => {}
            None => break,
            _ => {}
        }
    }
    let _ = session
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;
    Ok(String::from_utf8_lossy(&out).trim().to_string())
}

/// Android: exec mit explizitem User + privatem PKCS8/OpenSSH-Key.
#[cfg(target_os = "android")]
pub fn exec_with_key(
    dest: &str,
    user: &str,
    private_key_pem: &str,
    cmd: &str,
) -> anyhow::Result<String> {
    let (default_user, host, port) = parse_dest(dest, user);
    let key_pair = russh_keys::decode_secret_key(private_key_pem, None)
        .map_err(|e| anyhow::anyhow!("key decode: {e}"))?;
    let rt = runtime();
    rt.block_on(russh_connect_exec(
        &host,
        port,
        &default_user,
        key_pair,
        cmd,
    ))
}

#[cfg(target_os = "android")]
fn russh_exec(dest: &str, _cmd: &str) -> anyhow::Result<String> {
    anyhow::bail!("ssh-ziel '{dest}' braucht eine verbindung mit in-app-key (keyref)")
}
