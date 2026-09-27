//! TR-064-Client (SOAP über HTTP, Digest-Auth) für die FRITZ!Box.
//!
//! Box-Endpunkt: `http://<box>:49000/upnp/control/<service-control-url>`
//! Auth: HTTP Digest mit Box-Benutzer + Passwort.
//!
//! WIP — Implementierung folgt. Signaturen stehen schon, damit cli/app dagegen bauen.

use crate::{BoxProfile, HostStatus};

/// `urn:dslforum-org:service:Hosts:1` — Control-URL laut Gerätebeschreibung.
pub const HOSTS_CONTROL_URL: &str = "/upnp/control/hosts";

fn soap_envelope(service: &str, action: &str, args: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"
            s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action} xmlns:u="urn:dslforum-org:service:{service}:1">{args}</u:{action}>
  </s:Body>
</s:Envelope>"#,
        action = action,
        service = service,
        args = args
    )
}

/// `X_AVM-DE_WakeOnLANByMACAddress` (Hosts:1).
pub fn wake_on_lan(box_: &BoxProfile, mac: &str) -> anyhow::Result<()> {
    let _body = soap_envelope("Hosts", "X_AVM-DE_WakeOnLANByMACAddress", &format!(
        "<NewMACAddress>{mac}</NewMACAddress>"
    ));
    // TODO: POST mit Digest-Auth gegen {base_url}{HOSTS_CONTROL_URL},
    //       SOAPACTION-Header: "urn:dslforum-org:service:Hosts:1#X_AVM-DE_WakeOnLANByMACAddress"
    anyhow::bail!("WIP: HTTP-Client fehlt noch")
}

/// `X_AVM-DE_GetSpecificHostEntry` (Hosts:1) → active + IP.
pub fn host_status(box_: &BoxProfile, mac: &str) -> anyhow::Result<HostStatus> {
    let _body = soap_envelope("Hosts", "X_AVM-DE_GetSpecificHostEntry", &format!(
        "<NewMACAddress>{mac}</NewMACAddress>"
    ));
    // TODO: Antwort-XML parsen (NewActive, NewIPAddress)
    anyhow::bail!("WIP: HTTP-Client fehlt noch")
}
