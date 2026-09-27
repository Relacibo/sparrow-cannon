//! Handgerollter HTTP-Digest-Client (RFC 2617/7616, qop="auth" und RFC-2069-Fallback).
//! Genau genug für TR-064, mit RFC-Vektortest.

use anyhow::Context;
use md5::{Digest, Md5};
use sha2::Sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algo {
    Md5,
    Sha256,
}

#[derive(Debug)]
pub struct Challenge {
    realm: String,
    nonce: String,
    qop: Option<String>,
    algo: Algo,
    opaque: Option<String>,
}

impl Challenge {
    /// Parst den Wert des `WWW-Authenticate`-Headers.
    pub fn parse(header: &str) -> anyhow::Result<Self> {
        let s = header.trim();
        let s = s.strip_prefix("Digest").unwrap_or(s).trim();
        let attrs = parse_attrs(s);
        let get = |k: &str| {
            attrs
                .iter()
                .find(|(a, _)| a == k)
                .map(|(_, v)| v.clone())
        };

        let realm = get("realm").context("digest-challenge ohne realm")?;
        let nonce = get("nonce").context("digest-challenge ohne nonce")?;

        let algo = match get("algorithm") {
            Some(a) if a.split(',').any(|x| x.trim().eq_ignore_ascii_case("MD5")) => Algo::Md5,
            Some(a) if a.split(',').any(|x| x.trim().eq_ignore_ascii_case("SHA-256")) => {
                Algo::Sha256
            }
            None => Algo::Md5,
            Some(a) => anyhow::bail!("digest: Algorithmus nicht unterstützt: {a}"),
        };

        let qop = get("qop").and_then(|q| {
            q.split(',')
                .map(str::trim)
                .find(|x| x.eq_ignore_ascii_case("auth"))
                .map(str::to_string)
        });

        Ok(Challenge {
            realm,
            nonce,
            qop,
            algo,
            opaque: get("opaque"),
        })
    }

    /// Baut den `Authorization`-Header für die Wiederholung des Requests.
    pub fn authorization(&self, user: &str, pass: &str, method: &str, uri: &str) -> String {
        let cnonce = cnonce();
        let response = response_value(
            self.algo,
            user,
            &self.realm,
            pass,
            method,
            uri,
            &self.nonce,
            "00000001",
            &cnonce,
            self.qop.as_deref(),
        );

        let mut h = format!(
            "Digest username=\"{user}\", realm=\"{}\", nonce=\"{}\", uri=\"{uri}\", response=\"{response}\"",
            self.realm, self.nonce
        );
        if let Some(qop) = &self.qop {
            h.push_str(&format!(", qop={qop}, nc=00000001, cnonce=\"{cnonce}\""));
        }
        if let Some(op) = &self.opaque {
            h.push_str(&format!(", opaque=\"{op}\""));
        }
        let algo_name = match self.algo {
            Algo::Md5 => "MD5",
            Algo::Sha256 => "SHA-256",
        };
        h.push_str(&format!(", algorithm={algo_name}"));
        h
    }
}

fn parse_attrs(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (
                k.trim().to_ascii_lowercase(),
                v.trim().trim_matches('"').to_string(),
            )
        })
        .collect()
}

fn hex_hash(algo: Algo, input: &str) -> String {
    match algo {
        Algo::Md5 => format!("{:x}", Md5::digest(input.as_bytes())),
        Algo::Sha256 => format!("{:x}", Sha256::digest(input.as_bytes())),
    }
}

/// response = H(HA1:nonce:nc:cnonce:qop:HA2) bei qop, sonst RFC-2069 H(HA1:nonce:HA2).
fn response_value(
    algo: Algo,
    user: &str,
    realm: &str,
    pass: &str,
    method: &str,
    uri: &str,
    nonce: &str,
    nc: &str,
    cnonce: &str,
    qop: Option<&str>,
) -> String {
    let ha1 = hex_hash(algo, &format!("{user}:{realm}:{pass}"));
    let ha2 = hex_hash(algo, &format!("{method}:{uri}"));
    match qop {
        Some(qop) => hex_hash(
            algo,
            &format!("{ha1}:{nonce}:{nc}:{cnonce}:{qop}:{ha2}"),
        ),
        None => hex_hash(algo, &format!("{ha1}:{nonce}:{ha2}")),
    }
}

fn cnonce() -> String {
    let mut buf = [0u8; 8];
    getrandom::getrandom(&mut buf).expect("os rng");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 2617 Abschnitt 3.5 — Beispielresponse mit qop=auth.
    #[test]
    fn rfc2617_vector() {
        let resp = response_value(
            Algo::Md5,
            "Mufasa",
            "testrealm@host.com",
            "Circle Of Life",
            "GET",
            "/dir/index.html",
            "dcd98b7102dd2f0e8b11d0f600bfb0c093",
            "00000001",
            "0a4f113b",
            Some("auth"),
        );
        assert_eq!(resp, "6629fae49393a05397450978507c4ef1");
    }

    #[test]
    fn parse_challenge() {
        let c = Challenge::parse(
            r#"Digest realm="fritz.box", nonce="ABC123", qop="auth", algorithm=MD5"#,
        )
        .unwrap();
        assert_eq!(c.realm, "fritz.box");
        assert_eq!(c.nonce, "ABC123");
        assert_eq!(c.qop.as_deref(), Some("auth"));
        assert_eq!(c.algo, Algo::Md5);
    }

    #[test]
    fn rfc2069_without_qop() {
        let c = Challenge::parse(r#"Digest realm="fritz.box", nonce="N1""#).unwrap();
        assert!(c.qop.is_none());
        let h = c.authorization("u", "p", "POST", "/upnp/control/hosts");
        assert!(!h.contains("qop="));
        assert!(h.contains("response=\""));
    }
}
