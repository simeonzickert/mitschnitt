#!/usr/bin/env bash
#
# Mitschnitt: eine oeffentliche Release-Version bauen, signieren, notarisieren
# und die Update-Feed-Dateien fuer den Tauri-Updater erzeugen.
#
# Anders als mitschnitt-deploy.sh (Alltag, hebt den Patch automatisch, installiert
# nach ~/Applications) ist das hier der bewusste Schnitt: die Versionsnummer wird
# EXPLIZIT angegeben, das Ergebnis landet in einem eigenen Ausgabeordner, und am
# Ende steht ein fertiges Update-Paket -- nicht eine lokale Installation.
#
# MEHRERE PLATTFORMEN (28.09.2026):
#   Der Release-Prozess durchlaeuft drei unabhaengige Laeufe:
#   1. aarch64-Lauf (Apple Silicon, --ziel aarch64, Default)
#   2. x86_64-Lauf (Intel, --ziel x86_64, auf Apple-Silicon-Host quer gebaut)
#   3. Windows (scripts/mitschnitt-windows-signieren.sh, eigenstaendig)
#   Danach fuehrt scripts/mitschnitt-veroeffentlichen.sh die Bausteine aus
#   allen Laeufen zu einer gemeinsamen latest.json zusammen und erstellt das
#   GitHub-Release.
#
# WARUM DIE REIHENFOLGE SO IST (gemessen 22.09.2026, erster Lauf dieses Wegs):
#
#   1. bauen (unsigniert/ad-hoc)      -- tauri build erzeugt dabei KEINE
#      Updater-Artefakte (bundle.createUpdaterArtifacts=false). Das Archiv
#      aus diesem Bau wird verworfen; das echte Updater-Archiv entsteht
#      weiter unten aus dem notarisierten Bundle.
#   2. mit der eigenen Developer-ID signieren (wie mitschnitt-deploy.sh)
#   3. notarisieren + Ticket anheften (mitschnitt-notarisieren.sh, unveraendert
#      wiederverwendet -- das GESTAPELTE Bundle ist das, was am Ende beim
#      Menschen ankommt)
#   4. ERST JETZT das Updater-Archiv von Hand aus dem gestapelten Bundle bauen
#      und mit dem Minisign-Schluessel signieren. Nur diese Fassung ist es wert,
#      ausgeliefert zu werden.
#   5. Seit 0.1.7 (ZICK-329): aus DEMSELBEN gestapelten Bundle die DMG fuer
#      Menschen packen (mitschnitt-dmg.sh), die DMG selbst mit der Developer-ID
#      signieren, notarisieren und das Ticket an die DMG heften. Das Bundle
#      darin traegt sein eigenes Ticket schon aus Schritt 3; die DMG braucht
#      ein zweites, sonst prueft Gatekeeper beim Oeffnen der DMG online nach
#      und meldet sich ohne Netz. Das fruehere "...-notarisiert.zip" entfaellt.
#      Die Updater-Dateien (latest.json, Mitschnitt.app.tar.gz, .sig) bleiben
#      unveraendert: installierte Versionen fragen
#      releases/latest/download/latest.json ab.
#
# Format des Updater-Archivs (gemessen im Quelltext von tauri-plugin-updater
# 2.10.1, updater.rs "impl Update for macos"): ein gzip-komprimiertes tar mit
# GENAU EINEM Wurzel-Verzeichnis "Mitschnitt.app" darin -- der Entpacker
# ueberspringt beim Auspacken die erste Pfadkomponente jedes Eintrags. Eine ZIP-
# Datei (wie ditto sie fuer die Notarisierung baut) waere hier falsch.
#
# Voraussetzungen:
#   - Entwickler-ID im Schluesselbund (security find-identity -v -p codesigning)
#   - Notarisierungs-Profil "mitschnitt-notar" (xcrun notarytool store-credentials)
#   - Zwei generische Schluesselbund-Eintraege mit dem Updater-Minisign-Schluessel:
#     "mitschnitt-updater-private-key" und "mitschnitt-updater-key-password".
#     Diese Werte verlassen den Schluesselbund NIE in eine Datei, nur in
#     Umgebungsvariablen fuer die Dauer dieses Laufs.
#
# Aufruf:
#   scripts/mitschnitt-release.sh <version> [--ziel aarch64|x86_64]
#
#   <version>      z.B. 0.2.0 -- wird in Cargo.toml UND package.json gesetzt.
#   --ziel <wert>  Ziel-Architektur: aarch64 (Default, Apple Silicon) oder
#                  x86_64 (Intel, quer gebaut auf Apple-Silicon-Host).
#                  Auch --ziel=<wert> erlaubt.
#
#   Veroeffentlichen ist ein eigener Schritt nach allen Plattform-Laeufen:
#   scripts/mitschnitt-veroeffentlichen.sh <version>

set -euo pipefail

if [ $# -lt 1 ]; then
  echo "FEHLER: Aufruf ist scripts/mitschnitt-release.sh <version> [--ziel aarch64|x86_64]" >&2
  exit 2
fi
VERSION="$1"
shift
HOCHLADEN=0
ZIEL="aarch64"
while [ $# -gt 0 ]; do
  case "$1" in
    --hochladen) HOCHLADEN=1; shift ;;
    --ziel=*) ZIEL="${1#--ziel=}"; shift ;;
    --ziel) 
      if [ $# -lt 2 ]; then
        echo "FEHLER: --ziel braucht einen Wert (aarch64 oder x86_64)." >&2
        exit 2
      fi
      ZIEL="$2"; shift 2 ;;
    *) echo "FEHLER: unbekannter Schalter '$1' (erlaubt: --ziel aarch64|x86_64)" >&2; exit 2 ;;
  esac
done

# --hochladen ist hier nicht mehr erlaubt: Veroeffentlichen ist ein eigener
# Schritt nach allen Plattform-Laeufen.
if [ "$HOCHLADEN" = "1" ]; then
  echo "FEHLER: Veroeffentlichen ist ein eigener Schritt nach allen Plattform-Laeufen: scripts/mitschnitt-veroeffentlichen.sh <version>" >&2
  exit 2
fi

# Ziel-Architektur validieren
case "$ZIEL" in
  aarch64|x86_64) ;;
  *) echo "FEHLER: unbekannte Ziel-Architektur '$ZIEL' (erlaubt: aarch64, x86_64)" >&2; exit 2 ;;
esac

# Echter Semver-Regex, nicht ein Glob: `[0-9]*.[0-9]*.[0-9]*` liess wegen des
# `*` hinter jeder Ziffernklasse auch "1.2.3-beta" oder "1.a.3" durch, solange
# nur die erste Stelle je Segment eine Ziffer war.
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "FEHLER: Version '$VERSION' sieht nicht wie X.Y.Z aus." >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$REPO_ROOT/apps/desktop"
BUNDLE_ID="media.zickert.mitschnitt"
RELEASE_REPO="simeonzickert/mitschnitt"

# RUST_TRIPLE und Ziel-spezifische Pfade
if [ "$ZIEL" = "aarch64" ]; then
  RUST_TRIPLE="aarch64-apple-darwin"
  ARCHIV_NAME="Mitschnitt.app.tar.gz"
  DMG_NAME="Mitschnitt-$VERSION.dmg"
  PLATTFORM_KEY="darwin-aarch64"
  # lipo-Schreibweise der Zielarchitektur fuer den LGPL-Waechter. Der Waechter
  # laeuft als Kindprozess von mitschnitt-notarisieren.sh und erbt die
  # Umgebung; ohne diesen Wert prueft er gegen seinen Default arm64 und macht
  # jedes x86_64-Bundle dauerhaft rot (gemessen 28.09.2026).
  LGPL_ARCH="arm64"
  # Bei aarch64 OHNE --target bauen (wie heute), damit der Pfad bitgleich bleibt
  TAURI_TARGET_ARGS=""
  BUNDLE_SRC="$DESKTOP_DIR/src-tauri/target/release/bundle/macos/Mitschnitt.app"
  # Pruefe, ob CARGO_TARGET_DIR gesetzt ist -- wenn ja, liegt das Bundle dort
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    BUNDLE_SRC="$CARGO_TARGET_DIR/release/bundle/macos/Mitschnitt.app"
  fi
else
  RUST_TRIPLE="x86_64-apple-darwin"
  ARCHIV_NAME="Mitschnitt-x86_64.app.tar.gz"
  DMG_NAME="Mitschnitt-$VERSION-Intel.dmg"
  PLATTFORM_KEY="darwin-x86_64"
  LGPL_ARCH="x86_64"
  TAURI_TARGET_ARGS="--target $RUST_TRIPLE"
  # Bei x86_64 mit --target bauen, Pfad enthaelt dann den Triple
  BUNDLE_SRC="$DESKTOP_DIR/src-tauri/target/$RUST_TRIPLE/release/bundle/macos/Mitschnitt.app"
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    BUNDLE_SRC="$CARGO_TARGET_DIR/$RUST_TRIPLE/release/bundle/macos/Mitschnitt.app"
  fi
fi

# Wie in mitschnitt-deploy.sh und mitschnitt-notarisieren.sh: die Signatur-
# Identitaet traegt den Klarnamen des Zertifikatsinhabers und steht deshalb
# nicht fest im Repo. PFLICHT-Variable, kein Default: ein Default mit dem
# echten Namen war hier bis 22.09.2026 eingetragen UND wurde beim Aufruf von
# mitschnitt-notarisieren.sh weiter unten nie als Umgebungsvariable
# weitergegeben -- dessen eigene Pflicht-Variable blieb leer, und der Lauf
# starb dort nach dem vollen Bau (~20 min) statt sofort hier vorne.
SIGN_ID="${MITSCHNITT_SIGN_IDENTITY:?MITSCHNITT_SIGN_IDENTITY setzen (Developer-ID-Zertifikat, wie security find-identity es zeigt)}"

# Gleicher Name wie in mitschnitt-notarisieren.sh (MITSCHNITT_RELEASE_DIR),
# nicht der fruehere eigene Name MITSCHNITT_RELEASE_OUT_ROOT -- eine Variable,
# eine Bedeutung. Ebenfalls Pflicht statt Default mit echtem Plattenpfad.
OUT_ROOT="${MITSCHNITT_RELEASE_DIR:?MITSCHNITT_RELEASE_DIR setzen (Zielordner fuer die Release-Ausgabe)}"
OUT_DIR="$OUT_ROOT/v$VERSION"
# Arbeitsordner je Ziel trennen, damit zwei Laeufe im selben Versionsordner
# sich nicht ueberschreiben
NOTAR_WORK="$OUT_DIR/_notar-work-$ZIEL"

log() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

log "Voraussetzungen pruefen"
# Nur auf Apple-Silicon-Hosts erlaubt (uname -m = arm64)
if [ "$(uname -m)" != "arm64" ]; then
  echo "FEHLER: dieses Skript baut nur auf Apple Silicon (uname -m = arm64)." >&2
  echo "        Auf x86_64-Hosts wird nicht unterstuetzt." >&2
  exit 1
fi

# Bei x86_64-Ziel pruefen, ob der Rust-Target installiert ist
if [ "$ZIEL" = "x86_64" ]; then
  # Im Repo-Ordner fragen: rust-toolchain.toml pinnt eine eigene Toolchain,
  # und nur deren Ziele zaehlen (gemessen 28.09.2026: Ziel an der Standard-
  # Toolchain installiert, am gepinnten 1.94.0 nicht -> "can't find crate
  # for core").
  if ! (cd "$REPO_ROOT" && rustup target list --installed) | grep -q "^x86_64-apple-darwin$"; then
    echo "FEHLER: Rust-Target 'x86_64-apple-darwin' ist nicht installiert." >&2
    echo "        Bitte nachinstallieren: (cd \"$REPO_ROOT\" && rustup target add x86_64-apple-darwin)" >&2
    exit 1
  fi
fi

if ! security find-identity -v -p codesigning | grep -qF "$SIGN_ID"; then
  # Der Klarname aus dem Zertifikat gehoert nicht in ein Log -- die Nachricht
  # nennt nur die Variable, nicht ihren Wert (Forge-Zweitblick 27.09.2026).
  echo "FEHLER: Signatur-Identitaet nicht im Schluesselbund, siehe MITSCHNITT_SIGN_IDENTITY." >&2
  exit 1
fi
if ! xcrun notarytool history --keychain-profile mitschnitt-notar >/dev/null 2>&1; then
  echo "FEHLER: Notarisierungs-Profil 'mitschnitt-notar' fehlt oder ist ungueltig." >&2
  exit 1
fi

log "Changelog fuer $VERSION pruefen"
# GANZ VORNE, vor jedem Bau-Schritt (~20 min): CHANGELOG.md muss einen
# Abschnitt fuer $VERSION mit einem nicht-leeren "### What's new" tragen,
# sonst tragen die Release-Notiz und latest.json wieder nur den Platzhalter
# "Mitschnitt $VERSION" -- genau das ist 0.1.7 und 0.1.8 passiert (Entscheid
# 28.09.2026: "die releasenotes musst du viel besser pflegen!"). Die Probe
# selbst prueft ausfuehrlich (scripts/changelog-extract.sh), hier nur der
# Abbruch mit Ansage, damit niemand erst nach dem vollen Bau merkt, dass der
# Changelog-Eintrag fehlt.
CHANGELOG_FILE="$REPO_ROOT/CHANGELOG.md"
if ! "$REPO_ROOT/scripts/changelog-extract.sh" "$CHANGELOG_FILE" "$VERSION" what-new >/dev/null; then
  echo "FEHLER: CHANGELOG.md hat keinen (oder einen leeren) Abschnitt fuer $VERSION." >&2
  echo "        Vor jedem Release erst CHANGELOG.md pflegen -- siehe Kopfkommentar" >&2
  echo "        dort und scripts/changelog-extract.sh." >&2
  exit 1
fi

# Die Updater-Schluessel verlassen den Schluesselbund nur in den Prozess-Speicher
# dieses Laufs -- nie in eine Datei, nie in eine Logzeile. Bewusst NICHT
# exportiert: ein `export` gaebe sie an JEDEN Kindprozess dieses Laufs weiter
# (pnpm, Vite/Rolldown fuer den Frontend-Bau, mitschnitt-notarisieren.sh),
# nicht nur an die beiden tauri-Aufrufe, die sie tatsaechlich brauchen
# (Bauen weiter unten, Updater-Archiv signieren). Stattdessen werden sie an
# genau diesen beiden Stellen als Praefix mitgegeben.
TAURI_SIGNING_PRIVATE_KEY="$(security find-generic-password -s mitschnitt-updater-private-key -w)"
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s mitschnitt-updater-key-password -w)"
if [ -z "$TAURI_SIGNING_PRIVATE_KEY" ] || [ -z "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" ]; then
  echo "FEHLER: Updater-Schluessel oder -Passwort sind leer aus dem Schluesselbund gekommen." >&2
  exit 1
fi

log "Versionsnummer setzen auf $VERSION"
VERSION_PKG="$DESKTOP_DIR/package.json"
VERSION_CARGO="$DESKTOP_DIR/src-tauri/Cargo.toml"
CARGO_LOCK="$REPO_ROOT/Cargo.lock"
VERSION_BUMPED=0
ALT="$(sed -n 's/^version = "\(.*\)"/\1/p' "$VERSION_CARGO" | head -1)"
if [ "$ALT" = "$VERSION" ]; then
  echo "Steht schon auf $VERSION, nichts zu setzen."
else
  /usr/bin/sed -i '' "s/^version = \"$ALT\"/version = \"$VERSION\"/" "$VERSION_CARGO"
  /usr/bin/sed -i '' "s/\"version\": \"$ALT\"/\"version\": \"$VERSION\"/" "$VERSION_PKG"
  # Gegenprobe wie in mitschnitt-deploy.sh: ein sed, das nicht getroffen hat,
  # meldet keinen Fehler. Je Datei das eigene Format pruefen (Cargo.toml:
  # `version = "X"` am Zeilenanfang, package.json: `"version": "X"`) --
  # ein blosses `grep -q "$VERSION"` waere auch dann gruen, wenn die Zahl nur
  # zufaellig woanders in der Datei steht, etwa im Aufrufbeispiel im Kopf-
  # kommentar dieses Skripts, das dieselbe Versionsform enthalten kann.
  grep -q "^version = \"$VERSION\"" "$VERSION_CARGO" \
    || { echo "FEHLER: Version $VERSION steht nicht in $VERSION_CARGO" >&2; exit 1; }
  grep -q "\"version\": \"$VERSION\"" "$VERSION_PKG" \
    || { echo "FEHLER: Version $VERSION steht nicht in $VERSION_PKG" >&2; exit 1; }
  echo "$ALT -> $VERSION"
  VERSION_BUMPED=1
fi

log "Abhaengigkeits-Gate (pnpm audit --prod + cargo audit)"
"$REPO_ROOT/scripts/audit-gate.sh"

log "Geteiltes UI-Paket bauen (@anlg/ui/globals.css)"
# In einem frischen Arbeitsbaum/Worktree existiert packages/ui/dist noch nicht --
# turbo baut es nur fuer "tauri:dev" automatisch vor (turbo.json,
# "//#dev:desktop" -> dependsOn "@anlg/ui#build"), ein direkter "tauri build"
# geht daran vorbei. Ohne diesen Schritt bricht der Frontend-Bau mit
# "Rolldown failed to resolve import @anlg/ui/globals.css" ab (gemessen
# 22.09.2026, an einem frischen Worktree ohne vorherigen @anlg/ui-Bau).
pnpm --dir "$REPO_ROOT" --filter @anlg/ui build

log "Spiegel-Befehl (CLI-Sidecar) bauen"
CLI_RESOURCE="$DESKTOP_DIR/src-tauri/resources/cli/mitschnitt-cli-$RUST_TRIPLE"
cargo build --release -p mitschnitt-cli --target "$RUST_TRIPLE" --manifest-path "$REPO_ROOT/Cargo.toml"
install -m 0755 "${CARGO_TARGET_DIR:-$REPO_ROOT/target}/$RUST_TRIPLE/release/mitschnitt" "$CLI_RESOURCE"
[ -x "$CLI_RESOURCE" ] || { echo "FEHLER: Sidecar fehlt unter $CLI_RESOURCE" >&2; exit 1; }

# Probelauf des Sidecars: bei x86_64 ueber Rosetta, wenn verfuegbar.
# IMMER auch die Architektur des Binaries mit lipo pruefen.
if [ "$ZIEL" = "x86_64" ]; then
  # Pruefen, ob Rosetta verfuegbar ist
  if /usr/bin/pgrep oahd >/dev/null 2>&1 || arch -x86_64 /usr/bin/true 2>/dev/null; then
    "$CLI_RESOURCE" --version >/dev/null || { echo "FEHLER: gebautes Sidecar (x86_64) laeuft nicht ueber Rosetta." >&2; exit 1; }
  else
    echo "WARNUNG: Rosetta nicht verfuegbar, Sidecar-Probelauf uebersprungen." >&2
  fi
  # lipo-Prüfung IMMER, unabhaengig von Rosetta-Verfuegbarkeit
  if command -v lipo >/dev/null 2>&1; then
    CLI_ARCH="$(lipo -archs "$CLI_RESOURCE")"
    if [ "$CLI_ARCH" != "x86_64" ]; then
      echo "FEHLER: Sidecar hat Architektur '$CLI_ARCH', erwartet 'x86_64'." >&2
      exit 1
    fi
  fi
else
  "$CLI_RESOURCE" --version >/dev/null || { echo "FEHLER: gebautes Sidecar laeuft nicht." >&2; exit 1; }
  # lipo-Prüfung auch fuer aarch64
  if command -v lipo >/dev/null 2>&1; then
    CLI_ARCH="$(lipo -archs "$CLI_RESOURCE")"
    if [ "$CLI_ARCH" != "arm64" ]; then
      echo "FEHLER: Sidecar hat Architektur '$CLI_ARCH', erwartet 'arm64'." >&2
      exit 1
    fi
  fi
fi

log "Bauen (Release, OHNE Updater-Artefakte)"
# Der Bau erzeugt KEINE Updater-Artefakte (bundle.createUpdaterArtifacts=false).
# Das Archiv aus diesem Bau wird verworfen; das echte Updater-Archiv entsteht
# weiter unten aus dem notarisierten Bundle. Die Schluessel werden NICHT als
# Praefix mitgegeben, damit sie nicht an Kindprozesse (Vite, build.rs fremder
# Crates) vererbt werden.
MARKER=$(mktemp -t mitschnitt-release-bau)
trap 'rm -f "$MARKER"' EXIT
cd "$DESKTOP_DIR"

# Vor dem Bau: vendor/mp3lame-sys/build.rs anfassen, damit cargo das
# Build-Skript fuer DIESES Ziel neu laufen laesst. Grund: build.rs legt
# libmp3lame.0.dylib an einem festen Ort ohne Architektur ab
# (vendor/lame-dylib/) und hat kein rerun-if. Nach einem Intel-Lauf kann
# ein folgender Apple-Silicon-Lauf aus seinem Cache eine x86_64-Bibliothek
# einpacken; die App startet dann auf keinem Apple-Silicon-Mac mehr.
touch "$REPO_ROOT/vendor/mp3lame-sys/build.rs"

pnpm exec tauri build --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}' $TAURI_TARGET_ARGS
[ -d "$BUNDLE_SRC" ] || { echo "FEHLER: kein Bundle unter $BUNDLE_SRC" >&2; exit 1; }
[ "$BUNDLE_SRC" -nt "$MARKER" ] || {
  echo "FEHLER: das Bundle ist aelter als der Beginn dieses Laufs." >&2
  exit 1
}

mkdir -p "$OUT_DIR"

log "Architektur aller Mach-O-Dateien im Bundle pruefen"
# Direkt NACH dem Bau und VOR dem Signieren: jede Mach-O-Datei im Bundle
# auf die korrekte Zielarchitektur pruefen. Ausnahme: Dateien unter
# Contents/Resources/cli/, deren Name auf "-<anderes Triple>" endet
# (das Bundle nimmt den ganzen cli-Ordner mit; die App waehlt zur Laufzeit
# nach Triple). FALSCH_ARCH sammelt alle Abweichungen fuer die Fehlermeldung.
FALSCH_ARCH=""
MACHO_ANZAHL=0
command -v lipo >/dev/null 2>&1 || { echo "FEHLER: lipo fehlt (Xcode-Kommandozeilenwerkzeuge)." >&2; exit 1; }
while IFS= read -r -d '' DATEI; do
  # Pruefen, ob es eine Mach-O-Datei ist
  DATEI_TYP="$(file -b "$DATEI")"
  case "$DATEI_TYP" in
    # file schreibt "Mach-O" mit Bindestrich (gemessen 28.09.2026 an
    # libmp3lame.0.dylib: "Mach-O 64-bit dynamically linked shared library
    # arm64"). Ein Muster mit Leerzeichen traefe nie und machte die ganze
    # Pruefung still wirkungslos.
    *Mach-O*) MACHO_ANZAHL=$((MACHO_ANZAHL + 1)) ;;
    *) continue ;;
  esac
  # Relative Pfadkomponente fuer die Ausnahme-Prüfung
  REL_PFAD="${DATEI#$BUNDLE_SRC/}"
  # Ausnahme: Dateien unter Contents/Resources/cli/, deren Name auf
  # "-<anderes Triple>" endet, werden uebersprungen
  if [[ "$REL_PFAD" == Contents/Resources/cli/* ]]; then
    DATEI_NAME="$(basename "$DATEI")"
    # Prüfen, ob der Name auf "-$RUST_TRIPLE" endet
    if [[ "$DATEI_NAME" != *"-$RUST_TRIPLE" ]]; then
      # Das ist eine CLI-Datei fuer eine andere Architektur -> ueberspringen
      continue
    fi
  fi
  # lipo -archs ausfuehren
  if true; then
    GEFUNDENE_ARCH="$(lipo -archs "$DATEI" 2>/dev/null || true)"
    if [ -z "$GEFUNDENE_ARCH" ]; then
      FALSCH_ARCH="$FALSCH_ARCH  $REL_PFAD: keine Architektur (kein Mach-O?)\n"
    else
      ERWARTET=""
      [ "$ZIEL" = "aarch64" ] && ERWARTET="arm64"
      [ "$ZIEL" = "x86_64" ] && ERWARTET="x86_64"
      if [ "$GEFUNDENE_ARCH" != "$ERWARTET" ]; then
        FALSCH_ARCH="$FALSCH_ARCH  $REL_PFAD: $GEFUNDENE_ARCH (erwartet $ERWARTET)\n"
      fi
    fi
  fi
done < <(find "$BUNDLE_SRC/Contents/MacOS" "$BUNDLE_SRC/Contents/Frameworks" "$BUNDLE_SRC/Contents/Resources" -type f -print0)
# Mindestens Haupt-Binary + libmp3lame muessen gefunden worden sein -- sonst
# hat die Pruefung selbst nichts gesehen und waere kein Beweis.
if [ "$MACHO_ANZAHL" -lt 2 ]; then
  echo "FEHLER: nur $MACHO_ANZAHL Mach-O-Dateien im Bundle gefunden, erwartet mindestens 2 -- Pruefung waere wirkungslos." >&2
  exit 1
fi
echo "Mach-O-Dateien geprueft: $MACHO_ANZAHL"
if [ -n "$FALSCH_ARCH" ]; then
  echo "FEHLER: folgende Mach-O-Dateien im Bundle haben die falsche Architektur:" >&2
  printf "%b" "$FALSCH_ARCH" >&2
  exit 1
fi
# Pruefen, dass mitschnitt-cli-$RUST_TRIPLE vorhanden ist
CLI_IM_BUNDLE="$BUNDLE_SRC/Contents/Resources/cli/mitschnitt-cli-$RUST_TRIPLE"
if [ ! -f "$CLI_IM_BUNDLE" ]; then
  echo "FEHLER: CLI-Sidecar $CLI_IM_BUNDLE fehlt im Bundle." >&2
  exit 1
fi

log "Mit der eigenen Developer-ID signieren"
# Gleicher Schritt wie in mitschnitt-deploy.sh, aber auf einer Arbeitskopie im
# Ausgabeordner statt in ~/Applications -- dieses Skript fasst die laufende
# Installation des Betreibers nicht an.
WORK_APP="$OUT_DIR/_signiert-$ZIEL/Mitschnitt.app"
rm -rf "$OUT_DIR/_signiert-$ZIEL"
mkdir -p "$(dirname "$WORK_APP")"
cp -R "$BUNDLE_SRC" "$WORK_APP"
for versuch in 1 2 3; do
  if codesign --force --deep \
    --sign "$SIGN_ID" \
    --identifier "$BUNDLE_ID" \
    --entitlements "$DESKTOP_DIR/src-tauri/Entitlements.local.plist" \
    "$WORK_APP"; then
    break
  fi
  [ "$versuch" -eq 3 ] && { echo "FEHLER: Signieren dreimal gescheitert (Zeitstempel-Server?)." >&2; exit 1; }
  echo "Signieren gescheitert (Versuch $versuch), neuer Anlauf in 10 s ..." >&2
  sleep 10
done
codesign --verify --deep "$WORK_APP" || { echo "FEHLER: Signatur ungueltig." >&2; exit 1; }

log "Notarisieren + Ticket anheften"
rm -rf "$NOTAR_WORK"
# Beide Pflicht-Variablen von mitschnitt-notarisieren.sh EXPLIZIT mitgeben --
# vorher stand hier nur MITSCHNITT_NOTAR_WORK, und MITSCHNITT_SIGN_IDENTITY
# fehlte komplett. Solange SIGN_ID oben einen Default trug, fiel das nicht
# auf (der Default griff still in beiden Skripten getrennt); seit SIGN_ID
# Pflicht ist, wuerde der Aufruf sonst mit einer leeren Pflicht-Variable im
# Kindprozess sterben, nach dem vollen Bau.
#
# MITSCHNITT_LGPL_ARCH zusaetzlich: mitschnitt-notarisieren.sh ruft
# scripts/lgpl-gate.sh als Kindprozess auf und vererbt seine Umgebung. Ohne
# diesen Wert prueft der Waechter gegen seinen Default arm64 und macht jedes
# x86_64-Bundle dauerhaft rot (gemessen 28.09.2026).
MITSCHNITT_NOTAR_WORK="$NOTAR_WORK" \
  MITSCHNITT_NOTAR_KEIN_ZIP=1 \
  MITSCHNITT_SIGN_IDENTITY="$SIGN_ID" \
  MITSCHNITT_RELEASE_DIR="$OUT_DIR" \
  MITSCHNITT_LGPL_ARCH="$LGPL_ARCH" \
  "$REPO_ROOT/scripts/mitschnitt-notarisieren.sh" "$WORK_APP" "$OUT_DIR"
NOTARIZED_APP="$NOTAR_WORK/Mitschnitt.app"
[ -d "$NOTARIZED_APP" ] || { echo "FEHLER: notarisiertes Bundle fehlt unter $NOTARIZED_APP" >&2; exit 1; }
spctl -a -vv -t exec "$NOTARIZED_APP" 2>&1 | grep -q "Notarized Developer ID" \
  || { echo "FEHLER: spctl meldet kein 'Notarized Developer ID' fuer $NOTARIZED_APP." >&2; exit 1; }

log "DMG fuer Menschen packen, signieren, notarisieren, Ticket anheften"
# Gepackt wird das GESTAPELTE Bundle aus Schritt 3, nie das unsignierte aus
# dem Bau. mitschnitt-dmg.sh prueft nach dem Packen, dass das Bundle in der
# DMG bitgleich zum Quell-Bundle ist (Signatur + Ticket unversehrt).
DMG_PATH="$OUT_DIR/$DMG_NAME"
"$REPO_ROOT/scripts/mitschnitt-dmg.sh" "$NOTARIZED_APP" "$DMG_PATH" "Mitschnitt"
for versuch in 1 2 3; do
  if codesign --force --timestamp --sign "$SIGN_ID" "$DMG_PATH"; then
    break
  fi
  [ "$versuch" -eq 3 ] && { echo "FEHLER: DMG-Signieren dreimal gescheitert (Zeitstempel-Server?)." >&2; exit 1; }
  echo "DMG-Signieren gescheitert (Versuch $versuch), neuer Anlauf in 10 s ..." >&2
  sleep 10
done
codesign --verify --verbose=2 "$DMG_PATH" || { echo "FEHLER: DMG-Signatur ungueltig." >&2; exit 1; }
DMG_SUBMIT_LOG="$OUT_DIR/_dmg-submit-$ZIEL.log"
xcrun notarytool submit "$DMG_PATH" --keychain-profile mitschnitt-notar --wait 2>&1 | tee "$DMG_SUBMIT_LOG"
if ! grep -q "status: Accepted" "$DMG_SUBMIT_LOG"; then
  DMG_SID="$(grep -m1 -E '^\s*id:' "$DMG_SUBMIT_LOG" | awk '{print $2}')"
  echo "FEHLER: Apple hat die DMG nicht angenommen (Protokoll: xcrun notarytool log ${DMG_SID:-<id>} --keychain-profile mitschnitt-notar)." >&2
  exit 1
fi
xcrun stapler staple "$DMG_PATH"
xcrun stapler validate "$DMG_PATH"
spctl -a -t open --context context:primary-signature -vv "$DMG_PATH" 2>&1 | tee "$OUT_DIR/_dmg-spctl-$ZIEL.log"
grep -q "Notarized Developer ID" "$OUT_DIR/_dmg-spctl-$ZIEL.log" \
  || { echo "FEHLER: spctl meldet fuer die DMG kein 'Notarized Developer ID'." >&2; exit 1; }

log "Architektur des notarisierten Bundles feststellen und gegen Ziel pruefen"
# latest.json muss die ECHTE Architektur des gebauten Binaries tragen, nicht
# eine hart geschriebene. Der arm64-Zwang oben verhindert das Naheliegendste
# (auf x86_64 gebaut), nicht aber z.B. ein per --target quer gebautes
# Binary -- deshalb wird hier zusaetzlich am fertigen Bundle gemessen und
# gegen das Ziel geprueft.
NOTARIZED_EXE_NAME="$(defaults read "$NOTARIZED_APP/Contents/Info.plist" CFBundleExecutable)"
BUNDLE_ARCH="$(lipo -archs "$NOTARIZED_APP/Contents/MacOS/$NOTARIZED_EXE_NAME")"
case "$BUNDLE_ARCH" in
  arm64) ;;
  x86_64) ;;
  *)
    echo "FEHLER: unerwartete/gemischte Architektur im gebauten Binary: '$BUNDLE_ARCH'" >&2
    exit 1
    ;;
esac
# Pruefen, ob die tatsaechliche Architektur dem Ziel entspricht. lipo nennt
# Apple Silicon "arm64", Rust/--ziel nennt es "aarch64" -- vor dem Vergleich
# auf eine Schreibweise bringen, sonst bricht jeder Apple-Silicon-Lauf hier ab.
BUNDLE_ZIEL="$BUNDLE_ARCH"
[ "$BUNDLE_ARCH" = "arm64" ] && BUNDLE_ZIEL="aarch64"
if [ "$BUNDLE_ZIEL" != "$ZIEL" ]; then
  echo "FEHLER: Bundle-Architektur '$BUNDLE_ARCH' entspricht nicht dem Ziel '$ZIEL'." >&2
  exit 1
fi
echo "Architektur: $BUNDLE_ARCH -> $PLATTFORM_KEY"

log "Updater-Archiv aus dem notarisierten Bundle bauen"
UPDATE_ARCHIVE="$OUT_DIR/$ARCHIV_NAME"
rm -f "$UPDATE_ARCHIVE"
# Das Archiv im tar nur "Mitschnitt.app" nennen (Updater-Format), die Datei
# selbst aber hat den Ziel-spezifischen Namen
tar -czf "$UPDATE_ARCHIVE" -C "$NOTAR_WORK" "Mitschnitt.app"
[ -s "$UPDATE_ARCHIVE" ] || { echo "FEHLER: $UPDATE_ARCHIVE ist leer oder fehlt." >&2; exit 1; }

log "Updater-Archiv signieren (Minisign, derselbe Schluessel wie das eingebettete pubkey)"
# Die Schluessel gehen NUR an den tauri signer sign-Aufruf, nicht an den Bau.
TAURI_SIGNING_PRIVATE_KEY="$TAURI_SIGNING_PRIVATE_KEY" \
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" \
  pnpm --dir "$DESKTOP_DIR" exec tauri signer sign "$UPDATE_ARCHIVE"
SIGNATURE_FILE="$UPDATE_ARCHIVE.sig"
[ -s "$SIGNATURE_FILE" ] || { echo "FEHLER: $SIGNATURE_FILE ist leer oder fehlt." >&2; exit 1; }

log "Changelog-Text aus CHANGELOG.md auslesen"
# Eine Quelle (CHANGELOG.md), zwei Verwendungen: NOTES_PLAIN geht unten in
# latest.json (der Tauri-Updater-Dialog rendert kein Markdown, deshalb
# Klartext ohne Links/Betonung). Der fruehe Check oben hat schon sichergestellt,
# dass der Abschnitt da ist -- dieser Aufruf kann an sich nicht mehr
# fehlschlagen, ausser die Datei aenderte sich waehrend des ~20-min-Baus unter
# uns weg.
NOTES_PLAIN="$("$REPO_ROOT/scripts/changelog-extract.sh" "$CHANGELOG_FILE" "$VERSION" notes-plain)"

log "Baustein fuer Update-Feed erzeugen (mitschnitt-feed.py baustein)"
# latest.json schreibt das Skript NICHT mehr selbst. Stattdessen wird ein
# Baustein fuer die spaetere Zusammenfuehrung erzeugt.
ARCHIV_URL="https://github.com/$RELEASE_REPO/releases/download/v$VERSION/$ARCHIV_NAME"
PUB_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
python3 "$REPO_ROOT/scripts/mitschnitt-feed.py" baustein \
  --ausgabe "$OUT_DIR" \
  --version "$VERSION" \
  --plattform "$PLATTFORM_KEY" \
  --sig "$SIGNATURE_FILE" \
  --datei "$UPDATE_ARCHIVE" \
  --url "$ARCHIV_URL" \
  --notes "$NOTES_PLAIN" \
  --pub-date "$PUB_DATE"

log "Bausteine zusammenfuehren (Vorschau: mitschnitt-feed.py zusammenfuehren)"
python3 "$REPO_ROOT/scripts/mitschnitt-feed.py" zusammenfuehren \
  --ausgabe "$OUT_DIR"

# Versions-Bump committen -- ERST hier am Ende, nicht gleich nach dem Setzen
# oben: Cargo.lock traegt die Versionszeile des "desktop"-Pakets ebenfalls,
# aber die aktualisiert erst der Bau weiter oben (cargo build / tauri build
# schreiben Cargo.lock automatisch fort). Ohne diesen Commit exportiert ein
# danach gefahrener oeffentlicher Export (mitschnitt-oeffentlich-
# exportieren.sh) weiterhin die ALTE Versionsnummer im Verlauf.
if [ "$VERSION_BUMPED" = "1" ]; then
  log "Versions-Bump committen"
  if [ -n "$(git -C "$REPO_ROOT" status --porcelain -- "$VERSION_CARGO" "$VERSION_PKG" "$CARGO_LOCK")" ]; then
    git -C "$REPO_ROOT" commit -m "release: Version auf $VERSION" \
      -- "$VERSION_CARGO" "$VERSION_PKG" "$CARGO_LOCK"
  else
    echo "Nichts zu committen -- Cargo.toml/package.json/Cargo.lock stehen schon so im Baum."
  fi
fi

log "Fertig -- Ausgabeordner: $OUT_DIR"
ls -la "$OUT_DIR"
echo
echo "Updater-Archiv:  $UPDATE_ARCHIVE"
echo "Signatur:        $SIGNATURE_FILE"
echo "DMG fuer Menschen: $DMG_PATH"
echo
echo "Veroeffentlichen ist ein eigener Schritt nach allen Plattform-Laeufen:"
echo "  scripts/mitschnitt-veroeffentlichen.sh $VERSION"
