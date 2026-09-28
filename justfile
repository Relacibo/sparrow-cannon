# sparrow-cannon — Release-Helfer
#
# Qualitäts-Gate, Builds und Tag-Automatisierung lokal. Der eigentliche
# Release (APK-Build + Signierung + GitHub Release) läuft als GitHub-Workflow
# bei jedem Push eines v*-Tags: .github/workflows/release.yml

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

# Version heben: NUM z.B. 0.1.5 (conf.json, Cargo.toml, Cargo.lock)
bump NUM:
    #!/usr/bin/env bash
    set -euo pipefail
    [[ "{{NUM}}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Version muss MAJOR.MINOR.PATCH sein, z.B. 0.1.5" >&2; exit 1; }
    sed -i 's/^version = ".*"/version = "{{NUM}}"/' app/src-tauri/Cargo.toml
    sed -i 's/"version": ".*"/"version": "{{NUM}}"/' app/src-tauri/tauri.conf.json
    cargo check -q
    echo "Versionen auf {{NUM}} gesetzt — git diff prüfen und committen."

# Release-Tag setzen: V z.B. v0.1.5 (prüft Tree/Versionen/Gate)
tag V:
    #!/usr/bin/env bash
    set -euo pipefail
    v="{{V}}"
    num="${v#v}"
    [[ "$v" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Tag muss vMAJOR.MINOR.PATCH sein, z.B. v0.1.5" >&2; exit 1; }
    [ -z "$(git status --porcelain)" ] || { echo "Working tree dirty — erst committen." >&2; exit 1; }
    git rev-parse -q --verify "refs/tags/$v" >/dev/null && { echo "Tag $v existiert bereits." >&2; exit 1; }
    grep -qx "version = \"$num\"" app/src-tauri/Cargo.toml || { echo "app/src-tauri/Cargo.toml: version != $num — erst 'just bump $num'." >&2; exit 1; }
    grep -q "\"version\": \"$num\"" app/src-tauri/tauri.conf.json || { echo "tauri.conf.json: version != $num — erst 'just bump $num'." >&2; exit 1; }
    echo "Gate läuft (fmt, fmt-check, clippy, test) …"
    just gate
    git tag -a "$v" -m "cannon $v"
    echo "Tag $v gesetzt. Push: git push && git push origin $v"

# Letzte GitHub-Action-Runs (CI/Release-Status)
runs:
    gh run list --limit 5
