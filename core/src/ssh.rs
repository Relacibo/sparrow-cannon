//! SSH-Transport-Dispatch.
//!
//! Desktop: System-ssh (ssh_config-Aliase, Agent, ProxyJump — alles gratis).
//! Android (Phase 3): russh mit keyref aus der Connection.
//! Widgets/Actions/Connections-Test rufen nur `exec()` auf.

use std::process::Command;

/// Führt ein Kommando auf einem SSH-Ziel aus (15s Gesamt, 5s Connect).
pub fn exec(dest: &str, cmd: &str) -> anyhow::Result<String> {
    #[cfg(target_os = "android")]
    {
        let _ = (dest, cmd);
        anyhow::bail!("ssh-transport auf android folgt (russh + in-app-key, phase 3)")
    }
    #[cfg(not(target_os = "android"))]
    {
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
}
