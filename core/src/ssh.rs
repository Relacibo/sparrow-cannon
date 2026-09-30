//! SSH-Transport-Dispatch.
//!
//! Desktop: System-ssh (ssh_config-Aliase, Agent, ProxyJump — alles gratis),
//! Multiplexing via ControlMaster/ControlPersist.
//! Android: russh mit In-App-Key (generiert via keys.rs, abgelegt in secrets/),
//! Session-Pool als Multiplexing-Ersatz (keepalive, TTL 10min, Reconnect bei
//! toter Session).

#[cfg(not(target_os = "android"))]
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
mod russh_pool {
    use anyhow::Context;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    /// So lange bleibt eine gepoolte SSH-Session erhalten (≈ ControlPersist).
    const POOL_TTL: Duration = Duration::from_secs(600);

    pub(super) struct Client;
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

    fn runtime() -> &'static tokio::runtime::Runtime {
        static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RT.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"))
    }

    struct PoolEntry {
        handle: Arc<tokio::sync::Mutex<russh::client::Handle<Client>>>,
        inserted: Instant,
    }

    fn pool() -> &'static Mutex<BTreeMap<String, PoolEntry>> {
        static P: OnceLock<Mutex<BTreeMap<String, PoolEntry>>> = OnceLock::new();
        P.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    fn config() -> russh::client::Config {
        russh::client::Config {
            // gepoolt: keine inactivity-timeout, keepalive hält die session offen
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(30)),
            keepalive_max: 4,
            ..Default::default()
        }
    }

    async fn connect(
        host: &str,
        port: u16,
        user: &str,
        key_pair: russh_keys::key::KeyPair,
    ) -> anyhow::Result<russh::client::Handle<Client>> {
        let mut session = russh::client::connect(Arc::new(config()), (host, port), Client).await?;
        let authed = session
            .authenticate_publickey(user.to_string(), Arc::new(key_pair))
            .await
            .context("ssh-auth request")?;
        if !authed {
            anyhow::bail!("ssh-auth fehlgeschlagen ({user}@{host}:{port})");
        }
        Ok(session)
    }

    async fn exec_on(
        handle: &mut russh::client::Handle<Client>,
        cmd: &str,
    ) -> anyhow::Result<String> {
        let mut channel = handle.channel_open_session().await?;
        channel.exec(true, cmd).await?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            match channel.wait().await {
                Some(russh::ChannelMsg::Data { ref data }) => out.extend_from_slice(data),
                Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => {
                    out.extend_from_slice(data)
                }
                Some(russh::ChannelMsg::ExitStatus { .. }) => {}
                None => break,
                _ => {}
            }
        }
        Ok(String::from_utf8_lossy(&out).trim().to_string())
    }

    /// Exec über gepoolte Session (Multiplexing); tote Session → Reconnect.
    pub(super) fn exec_with_key(
        dest: &str,
        user: &str,
        private_key_pem: &str,
        cmd: &str,
    ) -> anyhow::Result<String> {
        let (default_user, host, port) = super::parse_dest(dest, user);
        let key_pair = russh_keys::decode_secret_key(private_key_pem, None)
            .map_err(|e| anyhow::anyhow!("key decode: {e}"))?;
        let pool_key = format!("{default_user}@{host}:{port}");
        runtime().block_on(async {
            // abgelaufene Sessions entsorgen (kosmetisch — Pool ist klein)
            pool()
                .lock()
                .unwrap()
                .retain(|_, e| e.inserted.elapsed() < POOL_TTL);

            // 1. Versuch: bestehende Session
            let pooled = pool()
                .lock()
                .unwrap()
                .get(&pool_key)
                .map(|e| e.handle.clone());
            if let Some(handle) = pooled {
                let mut h = handle.lock().await;
                if !h.is_closed() {
                    match exec_on(&mut h, cmd).await {
                        Ok(out) => return Ok(out),
                        Err(e) => tracing::debug!("mux-session {pool_key} verworfen: {e}"),
                    }
                }
                drop(h);
                pool().lock().unwrap().remove(&pool_key);
            }

            // 2. Versuch: neu verbinden
            let mut session = connect(&host, port, &default_user, key_pair).await?;
            let out = exec_on(&mut session, cmd).await?;
            pool().lock().unwrap().insert(
                pool_key,
                PoolEntry {
                    handle: Arc::new(tokio::sync::Mutex::new(session)),
                    inserted: Instant::now(),
                },
            );
            Ok(out)
        })
    }
}

/// Android: exec mit explizitem User + privatem PKCS8/OpenSSH-Key
/// (Sessions werden gepoolt und wiederverwendet).
#[cfg(target_os = "android")]
pub fn exec_with_key(
    dest: &str,
    user: &str,
    private_key_pem: &str,
    cmd: &str,
) -> anyhow::Result<String> {
    russh_pool::exec_with_key(dest, user, private_key_pem, cmd)
}

#[cfg(target_os = "android")]
fn russh_exec(dest: &str, _cmd: &str) -> anyhow::Result<String> {
    anyhow::bail!("ssh-ziel '{dest}' braucht eine verbindung mit in-app-key (keyref)")
}
