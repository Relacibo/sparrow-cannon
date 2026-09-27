# Dashboard: SSH-Actions + Status-Karten

> Erweiterung von sparrow-cannon: vom WoL-Werkzeug zum personalisierten
> Steuerpult — Fritzbox- **und** SSH-Actions mit Live-Status.

## Vision

Dashboard mit Karten (Config-getrieben, kein Code pro Karte):

```
┌──────────────────┐  ┌──────────────────┐
│ PC anton-bruckner│  │ CachyOS-Streaming│
│ Status: 🟢 ping  │  │ Status: 🟢 läuft  │
│ [Wake]           │  │ [on cachyos] [off]│
└──────────────────┘  └──────────────────┘
```

Jede Karte = **Action + Status**. Status ist nur eine schreibgeschützte
Action mit Rückgabewert.

## Kernmodell

```
Action { target, call }          Status { target, check }
├── Provider fritzbox (TR-064)   ├── ping / TR-064 "active"+IP
│     wake, wlan, gast, reboot   └── ssh: Befehl + Output-Regex/Exit-Code
└── Provider ssh (System-SSH)
      beliebige Befehle, z.B. `gstream on cachyos`
```

- **ssh-Provider shellt das System-SSH** (`ssh <host> <cmd>`) — nutzt
  keys, config und `.fritz.box`-Namen, kein eigenes Key-Handling.
- Karten aus TOML-Config, Hot-Reload. „Dashboard-Creator" = Config
  schreiben (UI-Editor erst viel später, wenn überhaupt).

## Phasen

1. **core+cli**: TR-064-Client, `cannon wake/status` (bereits gescaffolded)
2. **Provider-Abstraktion**: Action/Status-Typen generisch, ssh-Provider
3. **app (Tauri v2 + Solid)**: Karten-Grid, Status-Polling, Buttons

## Entscheidungen / Randbedingungen

- Alles strikt **lokal**: Bind nur LAN/WireGuard, kein Cloud-Gedöns.
- WoL-Status = pingbar (cheap); gstream-Status = ssh-Check (parsen).
- Scope-Kontrolle: Das hier ist Phase 2–3, erst core+cli fertig machen.
- Name/Repo bleibt sparrow-cannon (Dashboard ist ein Feature, kein Fork).
