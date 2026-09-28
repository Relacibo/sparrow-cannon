# sparrow-cannon — Release-Helfer
#
# Qualitäts-Gate und Releases lokal. Der eigentliche Release (APK-Build +
# Signierung + GitHub Release) läuft als GitHub-Workflow bei jedem Push
# eines v*-Tags: .github/workflows/release.yml

default:
    @just --list

# ---------- Qualitäts-Gate (AGENTS.md: alle vier vor jedem Push) ----------

# Formatierung anwenden
fmt:
    cargo fmt --all

# Formatierung nur prüfen
fmt-check:
    cargo fmt --all -- --check

# Lint, null Warnungen erlaubt
clippy:
    cargo clippy --workspace -- -D warnings

# Tests
test:
    cargo test --workspace

# Schneller Typcheck
check:
    cargo check --workspace

# Komplettes Gate: fmt → fmt-check → clippy → test
gate: fmt fmt-check clippy test

# ---------- Frontend & Builds ----------

# Frontend bauen + lib.rs touchen (stale-dist-Falle)
frontend:
    # cargo bettet sonst das alte dist ein, weil es Änderungen in
    # app/dist nicht bemerkt — deshalb der touch (AGENTS.md)
    npm run build
    touch app/src-tauri/src/lib.rs

# Desktop-Release-Build (Linux)
desktop: frontend
    cargo build --release

# Android-APK bauen (aarch64, unsigniert)
apk: frontend
    # Signieren wie im Release-Workflow: zipalign + apksigner
    cargo tauri android build --apk --target aarch64

# ---------- Release ----------

# Release vorbereiten: V z.B. v0.1.5 — prüft cleanen Tree und freien Tag,
# hebt die Version (tauri.conf.json, Cargo.toml, Cargo.lock), läuft das
# Gate, committet "chore: version X.Y.Z" und setzt den Tag. Push manuell.
release V:
    #!/usr/bin/env bash
    set -euo pipefail
    v="{{V}}"
    num="${v#v}"
    [[ "$v" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Tag muss vMAJOR.MINOR.PATCH sein, z.B. v0.1.5" >&2; exit 1; }
    [ -z "$(git status --porcelain)" ] || { echo "Working tree dirty — erst committen/aufräumen." >&2; exit 1; }
    git rev-parse -q --verify "refs/tags/$v" >/dev/null && { echo "Tag $v existiert bereits." >&2; exit 1; }
    revert() { git checkout -- Cargo.lock app/src-tauri/Cargo.toml app/src-tauri/tauri.conf.json; }
    trap revert ERR
    sed -i "s/^version = \".*\"/version = \"$num\"/" app/src-tauri/Cargo.toml
    sed -i "s/\"version\": \".*\"/\"version\": \"$num\"/" app/src-tauri/tauri.conf.json
    cargo check -q
    grep -qx "version = \"$num\"" app/src-tauri/Cargo.toml || { echo "Bump fehlgeschlagen: app/src-tauri/Cargo.toml != $num" >&2; exit 1; }
    grep -q "\"version\": \"$num\"" app/src-tauri/tauri.conf.json || { echo "Bump fehlgeschlagen: tauri.conf.json != $num" >&2; exit 1; }
    echo "Gate läuft (fmt, fmt-check, clippy, test) …"
    just gate
    git add Cargo.lock app/src-tauri/Cargo.toml app/src-tauri/tauri.conf.json
    git commit -m "chore: version $num"
    git tag -a "$v" -m "cannon $v"
    echo "Fertig: $num als $v committet + getaggt."
    echo "Release starten: git push && git push origin $v"

# Letzte GitHub-Action-Runs (CI/Release-Status)
runs:
    gh run list --limit 5
