//! In-App-SSH-Keys (Android): ed25519-Keypair generieren, OpenSSH-Format.
//! Desktop nutzt System-Keys (ssh-keygen) — hier nicht nötig.

/// Erzeugt ein ed25519-Keypair. Rückgabe: (privat, OpenSSH-Format, pub, OpenSSH-Zeile).
#[cfg(target_os = "android")]
pub fn generate_ed25519() -> anyhow::Result<(String, String)> {
    use russh_keys::key::KeyPair;
    let kp = KeyPair::generate_ed25519()?;
    let priv_pem = kp
        .encode_openssh(None)
        .map_err(|e| anyhow::anyhow!("priv-key encode: {e}"))?
        .to_string();
    let pub_line = kp
        .public_key()
        .public_key_base64()
        .map_err(|e| anyhow::anyhow!("pub-key encode: {e}"))?;
    Ok((priv_pem, pub_line))
}

#[cfg(not(target_os = "android"))]
pub fn generate_ed25519() -> anyhow::Result<(String, String)> {
    anyhow::bail!("key-generation nur auf android (desktop: ssh-keygen)")
}
