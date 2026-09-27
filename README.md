# fritz-cannon

> 🚧 **WIP** — frühes Experiment, nichts Benutzbares. Struktur + Roadmap siehe unten.

Kanonen auf Spatzen: FRITZ!Box-Steuerung über die TR-064-API (SOAP) —
Wake-on-LAN **über die Box statt per Broadcast**, dadurch funktional über
WireGuard von überall. Multi-Host von Tag 1, gedacht als wachsendes
Steuerpult (WLAN-Toggle, Gast-WLAN, Reconnect, Reboot, …).

## Warum über die Box?

Broadcast-WoL funktioniert nur im Heim-LAN (und nicht durch WireGuard-Tunnel).
Die Box hat die TR-064-Action `X_AVM-DE_WakeOnLANByMACAddress` — ein HTTP-Call
mit Digest-Auth, erreichbar unter derselben URL im LAN wie über den Tunnel.

## Architektur

```
fritz-cannon/
├── core/    # TR-064-Client (Digest+SOAP), Config, Action-Registry
├── cli/     # Binary "cannon"  → Terminal / cron
└── app/     # Tauri v2 + Solid → Linux-GUI + Android-APK (später)
```

Datenmodell:

```rust
struct BoxProfile { name: String, base_url: String, user: String }
struct Host       { id: String, name: String, mac: String, note: String }
```

Die Box ist im LAN und über WireGuard unter `http://192.168.178.1:49000`
erreichbar — kein Profilwechsel nötig.

## Roadmap

- [x] `core`: TR-064-Client — Digest-Auth (RFC 2617, handgerollt + Vektortest), SOAP, Service-Discovery
- [x] `core`: Actions `wake` + `status` — gegen echte FRITZ!OS-8.25-Box getestet
- [x] `cli`: `cannon wake <host>`, `cannon status [host]`, `cannon doctor` (Service-Liste)
- [ ] `app`: Tauri v2 Shell (Linux), Host-Liste mit Live-Status-Badges
- [ ] `app`: Android-Build (NDK), Sideload-APK
- [ ] generischer Action-Runner aus der Box-Service-XML (WLAN, Gast-WLAN, Reconnect, Reboot)

## Research (Stand: Sep 2026)

Feld ist frei: GitHub kennt für „fritzbox wake on lan" nur ein einzelnes
2★-Skript-Repo; TR-064-Client für Android+Desktop als App: nichts gefunden.

### Firmware-Notizen (FRITZ!OS 8.25, Box 7530 AX)

- `X_AVM-DE_GetSpecificHostEntry` existiert auf Hosts:1 **nicht mehr** — für
  MAC-Abfragen die plain `GetSpecificHostEntry`, für IPs
  `X_AVM-DE_GetSpecificHostEntryByIP`. Immer gegen die SCPD prüfen
  (`cannon doctor` → `tr64desc.xml` → SCPD-URL).
- `SOAPACTION`-Header muss in doppelten Anführungszeichen geschickt werden.
- Gefundene Munition für später: `X_AVM-DE_SetAutoWakeOnLANByMACAddress`
  (Auto-WoL), `X_AVM-DE_HostDoUpdate` (Firmware-Update),
  `X_AVM-DE_SetPrioritizationByIP` (QoS), `X_AVM-DE_GetHostListPath`.

## Lizenz

MIT — siehe [LICENSE](LICENSE).

*Hinweis: Dieses Projekt ist nicht mit AVM verbunden. FRITZ!Box ist eine Marke der AVM GmbH.*
