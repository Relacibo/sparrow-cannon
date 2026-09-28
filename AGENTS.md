# Agent-Regeln

## Qualität vor Push (zwingend)

Vor jedem `git push` müssen alle vier Schritte fehlerfrei durchlaufen:

1. `cargo fmt --all` — Formatierung anwenden
2. `cargo fmt --all -- --check` — muss clean sein
3. `cargo clippy --workspace -- -D warnings` — null Warnungen
4. `cargo test --workspace` — alle Tests grün

Regeln:

- Schlägt ein Schritt fehl: Ursache fixen und alle vier Schritte erneut ausführen.
  Kein Umgehen mit `#[allow(...)]`, `#[expect(...)]` oder clippy-Flags — Ausnahmen
  nur mit Begründung direkt am Code als Kommentar.
- Nach `cargo fmt` geänderte Dateien in den Commit aufnehmen.
- Commits und Pushes erst nach vollständig grünem Durchlauf.
- Nicht mit einem still fehlgeschlagenen Build-Kommando weiterarbeiten:
  Build-/Check-Output immer prüfen (Exit-Code oder Output-Match), bevor der
  nächste Schritt behauptet wird.

## Build-Falle: dist einbetten

- Nach jedem Frontend-Build (`npm run build` in `app/`) muss `touch app/src-tauri/src/lib.rs`
  laufen, bevor `cargo build`/`cargo tauri android build` — cargo erkennt dist-Änderungen
  sonst NICHT und bettet das alte Frontend ein (stale-dist).
- Build-Output immer verifizieren (Exit-Code/output match) — npm/cargo-Ketten aus
  falschem Arbeitsverzeichnis scheitern still.

## Release (justfile)

- Der gesamte Release-Prozess läuft über **ein** Rezept: `just release v0.1.5`.
  Es prüft der Reihe nach: Version-Format → cleanen Tree → Tag noch frei →
  hebt die Version in `tauri.conf.json`, `app/src-tauri/Cargo.toml`, `Cargo.lock` →
  Gate (alle vier Schritte oben) → Commit `chore: version X.Y.Z` → annotierter Tag.
  Bei Fehler in der Mitte revertet es die Bump-Änderungen selbst (trap ERR).
- **Versionen nie von Hand bumpen** — das Rezept ist die einzige Bump-Stelle,
  damit alle drei Dateien synchron bleiben.
- **Push bleibt manuell**: `git push && git push origin vX.Y.Z`. Der Tag-Push
  startet den Release-Workflow (`.github/workflows/release.yml`: APK aarch64,
  Signierung, GitHub Release). Vor dem Tag-Push das Gate grün haben — das
  Rezept erledigt das, aber kein Push über einen roten Stand hinweg.
- CI-/Release-Status: `just runs` (`gh run list`); einzelner Run: `gh run view <id>`.
  Ergebnis eines Releases immer verifizieren: `gh release view vX.Y.Z` zeigt
  published-Status + APK-Asset.
