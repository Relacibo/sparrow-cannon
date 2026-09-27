//! In-App-SSH-Keys (Android): ed25519-Keypair generieren, PKCS8/OpenSSH-Format.
//! Desktop nutzt System-Keys (ssh-keygen) — hier nicht nötig.

/// Erzeugt ein ed25519-Keypair. Rückgabe: (privat PEM, pub "ssh-ed25519 <b64>").
#[cfg(target_os = "android")]
pub fn generate_ed25519() -> anyhow::Result<(String, String)> {
    use russh_keys::PublicKeyBase64;
    use russh_keys::key::KeyPair;
    let kp = KeyPair::generate_ed25519().ok_or_else(|| anyhow::anyhow!("rng fehlgeschlagen"))?;
    let mut priv_pem = Vec::new();
    russh_keys::encode_pkcs8_pem(&kp, &mut priv_pem)
        .map_err(|e| anyhow::anyhow!("priv-key encode: {e}"))?;
    let priv_s = String::from_utf8(priv_pem)?;
    let pub_line = format!("{} {}", kp.name(), kp.public_key_base64());
    Ok((priv_s, pub_line))
}

/// Pubkey-Zeile aus einem gespeicherten privaten PKCS8-Key ableiten.
#[cfg(target_os = "android")]
pub fn public_line(priv_pem: &str) -> anyhow::Result<String> {
    let kp = russh_keys::decode_secret_key(priv_pem, None)
        .map_err(|e| anyhow::anyhow!("key decode: {e}"))?;
    use russh_keys::PublicKeyBase64;
    Ok(format!("{} {}", kp.name(), kp.public_key_base64()))
}

#[cfg(not(target_os = "android"))]
pub fn public_line(_priv_pem: &str) -> anyhow::Result<String> {
    anyhow::bail!("public_line nur auf android")
}

#[cfg(not(target_os = "android"))]
pub fn generate_ed25519() -> anyhow::Result<(String, String)> {
    anyhow::bail!("key-generation nur auf android (desktop: ssh-keygen)")
}
