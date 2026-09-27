//! TR-064: SOAP über HTTP mit Digest-Auth gegen die FRITZ!Box.

use anyhow::Context;
use std::time::Duration;

use crate::digest::Challenge;
use crate::{BoxProfile, HostStatus};

pub const SERVICE_HOSTS: &str = "Hosts";
pub const CONTROL_HOSTS: &str = "/upnp/control/hosts";

fn soap_envelope(service: &str, action: &str, args: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action} xmlns:u="urn:dslforum-org:service:{service}:1">{args}</u:{action}>
  </s:Body>
</s:Envelope>"#
    )
}

fn tag_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&close)?;
    Some(xml[start..end].trim().to_string())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
}

/// Aus einem Non-200-Response den SOAP-Faulttext ziehen (sonst: Rohbody).
fn fault_of(action: &str, code: u16, mut resp: ureq::Response) -> anyhow::Error {
    if code == 401 {
        return anyhow::anyhow!(
            "SOAP {action}: HTTP 401 — Passwort falsch oder User unbekannt?"
        );
    }
    let text = resp.into_string().unwrap_or_default();
    if let Some(desc) = tag_value(&text, "errorDescription") {
        let code_upnp = tag_value(&text, "errorCode").unwrap_or_default();
        return anyhow::anyhow!("SOAP {action}: HTTP {code}: UPnP {code_upnp} {desc}");
    }
    let fault = tag_value(&text, "faultstring").unwrap_or(text);
    anyhow::anyhow!("SOAP {action}: HTTP {code}: {fault}")
}

/// POSTet eine SOAP-Action (mit Digest-Auth bei 401) und liefert den Antwort-Body.
fn call(box_: &BoxProfile, service: &str, control: &str, action: &str, args: &str) -> anyhow::Result<String> {
    let url = format!("{}{}", box_.base_url, control);
    let soapaction = format!("urn:dslforum-org:service:{service}:1#{action}");
    let body = soap_envelope(service, action, args);
    let a = agent();

    let first = a
        .post(&url)
        .set("Content-Type", "text/xml; charset=\"utf-8\"")
        .set("SOAPACTION", &format!("\"{soapaction}\""))
        .send_string(&body);

    let resp = match first {
        Ok(r) => r,
        Err(ureq::Error::Status(401, r)) => {
            let header = r
                .header("WWW-Authenticate")
                .context("401 ohne WWW-Authenticate")?;
            let challenge = Challenge::parse(header)?;
            let auth = challenge.authorization(&box_.user, &box_.pass, "POST", control);
            match a.post(&url)
                .set("Content-Type", "text/xml; charset=\"utf-8\"")
                .set("SOAPACTION", &format!("\"{soapaction}\""))
                .set("Authorization", &auth)
                .send_string(&body)
            {
                Ok(r) => r,
                Err(ureq::Error::Status(code, r)) => return Err(fault_of(action, code, r)),
                Err(e) => return Err(e.into()),
            }
        }
        Err(ureq::Error::Status(code, r)) => return Err(fault_of(action, code, r)),
        Err(e) => return Err(e.into()),
    };

    let status = resp.status();
    let text = resp.into_string()?;
    if status == 200 {
        Ok(text)
    } else {
        let fault = tag_value(&text, "faultstring")
            .unwrap_or_else(|| "unbekannter SOAP-Fehler".into());
        anyhow::bail!("SOAP {action}: HTTP {status}: {fault}")
    }
}

/// `X_AVM-DE_WakeOnLANByMACAddress` (Hosts:1) — Magic Packet über die Box.
pub fn wake_on_lan(box_: &BoxProfile, mac: &str) -> anyhow::Result<()> {
    call(
        box_,
        SERVICE_HOSTS,
        CONTROL_HOSTS,
        "X_AVM-DE_WakeOnLANByMACAddress",
        &format!("<NewMACAddress>{mac}</NewMACAddress>"),
    )
    .map(|_| ())
}

/// `GetSpecificHostEntry` (Hosts:1) — active + IP + Hostname.
/// (Das X_AVM-DE-Pendant heißt auf aktuellen Firmware-Ständen …ByIP.)
pub fn host_status(box_: &BoxProfile, mac: &str) -> anyhow::Result<HostStatus> {
    let xml = call(
        box_,
        SERVICE_HOSTS,
        CONTROL_HOSTS,
        "GetSpecificHostEntry",
        &format!("<NewMACAddress>{mac}</NewMACAddress>"),
    )?;
    let active = tag_value(&xml, "NewActive")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    Ok(HostStatus {
        active,
        ip: tag_value(&xml, "NewIPAddress").filter(|s| !s.is_empty()),
        hostname: tag_value(&xml, "NewHostName").filter(|s| !s.is_empty()),
    })
}

/// Liest `tr64desc.xml` und listet (serviceType, controlURL) — für Doctor
/// und später den generischen Action-Runner.
pub fn services(box_: &BoxProfile) -> anyhow::Result<Vec<(String, String)>> {
    let url = format!("{}/tr64desc.xml", box_.base_url);
    let xml = agent()
        .get(&url)
        .call()
        .context("tr64desc.xml nicht abrufbar")?
        .into_string()?;
    Ok(xml
        .split("<service>")
        .filter_map(|chunk| {
            let stype = tag_value(chunk, "serviceType")?;
            let curl = tag_value(chunk, "controlURL")?;
            Some((stype, curl))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tags() {
        let xml = r#"<u:R xmlns:u="urn"><NewActive>1</NewActive><NewIPAddress>192.168.178.21</NewIPAddress></u:R>"#;
        assert_eq!(tag_value(xml, "NewActive").as_deref(), Some("1"));
        assert_eq!(
            tag_value(xml, "NewIPAddress").as_deref(),
            Some("192.168.178.21")
        );
        assert_eq!(tag_value(xml, "Nö"), None);
    }

    #[test]
    fn envelope_contains_action() {
        let e = soap_envelope("Hosts", "Foo", "<X>1</X>");
        assert!(e.contains("<u:Foo xmlns:u=\"urn:dslforum-org:service:Hosts:1\">"));
    }
}
