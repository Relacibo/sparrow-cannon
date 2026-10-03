# App-Updates & Verteilung (Android + Desktop)

> Wie kommt die App auf die Geräte und wie updatet sie sich?
> Stand: v0.1.11 — Release-Workflow baut APK (aarch64, signiert) → GitHub Release.
> Desktop wird bisher nur lokal gebaut (`just desktop`), kein Icon im Menü.

## Rahmen

- Nur ich benutze die App → Infrastruktur so klein wie möglich, keine
  Kanonen auf Spatzen.
- Wunsch: Desktop-Icon im App-Menü (Fedora + CachyOS).
- Android und Linux brauchen getrennte Antworten — ein cross-platform
  Updater-Tool existiert nicht.

## Android — Vergleich

| Weg | Aufwand | Urteil |
|---|---|---|
| **Obtainium** | ~5 min (einmal aufs Repo zeigen) | **Empfehlung.** Checkt GitHub-Releases automatisch, lädt + installiert APK. Nutzt exakt den existierenden Workflow, null Infrastruktur |
| F-Droid offiziell | hoch | Nein. Review-Prozess, F-Droid baut selbst und signiert mit **eigenem Key** → kollidiert mit dem GitHub-signierten APK |
| Eigenes F-Droid-Repo | mittel (Hosting + Repo-Signing) | Geht nur mit konsistentem Keystore. Overhead für eine App — erst bei mehreren Apps interessant |
| In-App-Autoupdater | hoch | Tauri-Updater unterstützt **Android nicht**. Self-built (Release-API → APK → Install-Intent) = eigener Code + `REQUEST_INSTALL_PACKAGES`-Permission |
| adb install | — | Nur Entwicklung, kein Dauerzustand |

## Desktop — Vergleich

| Weg | Aufwand | Update | Icon | Urteil |
|---|---|---|---|---|
| Flatpak | hoch (Manifest, Runtime, Repo/Flathub, Sandbox-Permissions für SSH-Keys/Secret-Service) | `flatpak update` | ✓ | Overhead für single-user |
| AUR (CachyOS) + COPR (Fedora) | hoch × 2 (zwei Paketformate pflegen) | mit Systemupdate | ✓ | Infrastruktur für Apps, die andere benutzen |
| `cargo install` | klein | `cargo install --path .` | ✗ | Falsch allein: braucht Node/Rust-Toolchain lokal, `npm run build` vorher, kein Icon |
| **CI-AppImage + `just install`/`just update`** | einmalig klein (3 Zeilen Workflow + 2 Rezepte) | `just update` (1 Befehl) | ✓ (via `just install`) | **Empfehlung.** AppImage bundle't webkit2gtk → kein Toolchain-Bedarf auf den Desktops |
| CI-AppImage + tauri-plugin-updater | mittel (~1h: Keypair, Secrets, Plugin Rust+JS, `latest.json`, Update-UI) | automatisch in-App | ✓ | Die "richtige" Lösung, aber ~70% des Setups für den Komfort "Dialog statt Befehl". Später ergänzbar, CI-Artefakt identisch |

## Technische Randnotizen

- Updater (falls später): Endpoint wäre
  `https://github.com/Relacibo/sparrow-cannon/releases/latest/download/latest.json`
  — kein Server nötig. Tauscht die AppImage-Datei **in-place** →
  `.desktop` + Icon bleiben gültig.
- AppImage ohne Plugin kann der Updater-Pfad auch ohne Signatur sein:
  `just update` = `gh release download` + Datei ersetzen.
- Desktop-Binary heißt `sparrow-cannon`, Icon liegt in
  `app/src-tauri/icons/icon.png`.

## Offene Entscheidung

- [ ] Android: Obtainium einrichten (macht der Mensch, 5 min)
- [ ] Desktop: CI-AppImage + `just install`/`just update` umsetzen?
- [ ] Evtl. später: tauri-plugin-updater ergänzen, falls `just update` nervt

---

# Config-Sync: Architektur

> Stand: SSH-Pull (joplin-artig, Desktop gewinnt, Konflikt-Stash). Funktioniert,
> ist aber „suboptimal": Key-Dance pro Gerät, manueller Sync-Button, Pull-only.

## Ist-Zustand / Einschränkungen

- Pull-only: Desktop ist faktisch die Source of Truth, Handy-Edits bleiben
  lokal (Merge mit Desktop-Wins) und wandern nie zurück.
- Für den Pull braucht das Handy eine SSH-Verbindung + autorisierten
  Device-Key auf jedem Desktop → Setup-Friktion ist der eigentliche Schmerz.
- Passwörter wandern bewusst nicht mit (nicht in der Config) → einmalig
  manuell pro Gerät, bleibt bei allen Optionen so.

## Optionen

| Option | Transport | App-Aufwand | Friktion | Urteil |
|---|---|---|---|---|
| A — SSH-Pull (Status quo) | SSH | 0 | Key-Dance je Desktop + Button-Tap | Funktioniert heute; als Fallback behalten |
| B — Syncthing | P2P-Daemon | klein (Import-Button) | Neue App/Daemon auf allen Geräten | Verworfen — Desktops laufen eh über git/dotfiles; Transporte existieren schon |
| C — GitHub-Raw-Pull beim App-Start | öffentl. dotfiles-Repo (raw.githubusercontent.com) | klein–mittel (ureq GET, kein Token) | Kein Token, funktioniert überall; Overwrite statt Merge | Möglich — aber: Config (MACs, Hostnames, User, Pfade, Befehle) wäre **öffentlich** |
| D′ — sftpgo + FolderSync + Import | SFTP (neuer Service) | klein | — | Überholt durch E (ovilava existiert schon) |
| **E — Dotfiles + ovilava + FolderSync + Import** | Desktops: config als Dotfile (git). Push: rclone copy → WebDAV (nsync-Pattern, Timer). Handy: FolderSync (läuft schon) → SAF-Import | klein (1 Import-Button, Merge bleibt) | Keine Keys für Sync, keine Tokens, kein neuer Service — nur existierende Bausteine | **Empfehlung** |

## E — Architektur (Stand 03.10.)

1. `config.toml` als Dotfile — **ins öffentliche Repo** (credential-frei
   dank `${secret:key}`, siehe Secrets-Strategie), stowen unter
   `~/.dotfiles/hyprland/.config/sparrow-cannon/` (nach Restructure).
   `hyprland-private` nur noch für ssh config o. ä.
   App schreibt durch den Stow-Symlink → dirty tree → Auto-Commit-Timer
   (nsync-Pattern) oder manuell.
2. Push-Job (nsync-Pattern): `rclone copy ~/.config/sparrow-cannon/config.toml
   ovilava-notes:sparrow-cannon/` per systemd-Timer (~15min), push-only —
   Desktop ist Source of Truth, keine Conflict-Logik nötig.
3. Handy: FolderSync-Paar `cloud.rcbnet.work/sparrow-cannon/config.toml` →
   `Documents/sparrow-cannon/` (App läuft schon wegen Notes-Flow).
4. App: Import-Button (SAF-Picker → bestehende Merge-Logik → save). Optional:
   persistierte SAF-URI + Auto-Import-Beim-Start-Angebot.
5. SSH-Sync-Button bleibt als Fallback. Device-Key bleibt nötig — für
   **Widget-Actions** (ssh.run auf pc/cachyos), nicht mehr für den Sync.

## Secrets-Strategie (final, 03.10.)

- **Storage: EIN File `secrets.toml`** (0600, maschinenlokal, nie im Repo)
  statt Unterordner mit per-ID-Files — Gründe: secret-tool kann nicht
  listen (Enumeration nur via File), Sync = ein `cat`, keine Orphans,
  atomic write (tmp+rename), uniform auf Desktop+Android.
  **Hard cut (kein Keyring-/Legacy-Fallback)**: Keyring + secrets/<id>.txt
  fliegen raus — Box-Passwort einmalig pro Desktop neu eintragen.
  Private Keys (PEM) bleiben eigene Dateien (secrets/device.key), nie im
  TOML. `pass::resolve`: env CANNON_PASS → secrets.toml.
- **Feature `${secret:key}`**: Op-Params mit `${secret:id}`-Platzhaltern
  werden in `execute`/`eval` via pass::resolve expandiert → Config wird
  strukturell credential-frei → **darf ins öffentliche Repo**.
- **Transport-Matrix (final):**
  - Config → **öffentliches dotfiles-Repo**: Desktops via git, Handy via
    `raw.githubusercontent.com`-Fetch (kein Token, kein SSH für Config!)
  - Secrets → **SSH gerät-zu-Gerät** (wie bisher die Config): `cat
    secrets.toml`, Union-Merge per ID (remote gewinnt, lokale bleiben);
    funktioniert remote via WireGuard; Sync-Button degeneriert zum
    Secrets-Pull
  - Device-Key → gar nicht (per-Device, revokierbar, für Widget-Actions)
- **Secret-Verwaltung**: CLI (`cannon secret set <id>` / `secrets`) +
  minimale UI-Liste im Verbindungen-Tab.
- Restrisiko öffentliche Config bewusst akzeptiert: MACs, Hostnames,
  Topologie, Befehle — nur keine Credentials.
- Umsetzung nach Dotfiles-Restructure (core + CLI + UI + Sync, ~1 Tag).

## Randnotizen

- Backend liest die Config eh bei jedem Tick/Command neu → eine extern
  aktualisierte Datei ist ohne App-Neustart sichtbar; nur der Import-Pfad
  (app_data_dir vs. öffentlicher Speicher) ist auf Android die Hürde →
  deshalb SAF-Import statt direktem Lesen (Scoped Storage).
- Ein synchronisiertes Verzeichnis wie `~/Public/sparrow-cannon/` (bzw.
  Android-Syncthing-Ordner) würde auch den Device-Key nicht anfassen —
  der bleibt gerätespezifisch, korrekt so.

## Offen

- [ ] ① Handy-Setup fertig machen (Pubkey → authorized_keys → Sync-Tap) — A funktioniert heute, Device-Key wird eh für Widgets gebraucht
- [ ] ② Dotfiles-Restructure: public-Root → `~/.dotfiles/hyprland` (git mv + push), optional `hyprland-private` (nur ssh config), restow auf beiden Dists (cachyos offline → beim nächsten Mal)
- [ ] ③ Secrets-Feature: `secrets.toml`-Store (Keyring-Legacy-Migration) + `${secret:key}`-Expansion + raw-URL-Config-Pull (Handy) + Secrets-Pull via SSH + CLI/UI-Verwaltung
- [ ] E-2: rclone-Push-Timer (nsync-Pattern) + Sync-Konf in dotfiles
- [ ] E-3: FolderSync-Paar auf dem Handy
- [ ] E-4: Import-Button in der App (SAF-Picker → merge → save; optional persistierte URI + Auto-Import beim Start)
- [ ] SSH-Sync-Button als Fallback dokumentieren/behalten

