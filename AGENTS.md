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
