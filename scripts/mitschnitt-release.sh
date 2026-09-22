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
# WARUM DIE REIHENFOLGE SO IST (gemessen 22.09.2026, erster Lauf dieses Wegs):
#
#   1. bauen (unsigniert/ad-hoc)      -- tauri build erzeugt dabei bereits ein
#      Updater-Archiv samt Signatur, WEIL bundle.createUpdaterArtifacts=true
#      und die Schluessel gesetzt sind. Dieses Archiv wird VERWORFEN: es
#      stammt aus dem noch nicht Developer-ID-signierten, nicht notarisierten
#      Bundle. Wer es doch ausliefert, verschickt Gatekeeper-Warnungen.
#   2. mit der eigenen Developer-ID signieren (wie mitschnitt-deploy.sh)
#   3. notarisieren + Ticket anheften (mitschnitt-notarisieren.sh, unveraendert
#      wiederverwendet -- das GESTAPELTE Bundle ist das, was am Ende beim
#      Menschen ankommt)
#   4. ERST JETZT das Updater-Archiv von Hand aus dem gestapelten Bundle bauen
#      und mit dem Minisign-Schluessel signieren. Nur diese Fassung ist es wert,
#      ausgeliefert zu werden.
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
#   scripts/mitschnitt-release.sh <version> [--hochladen]
#
#   <version>      z.B. 0.2.0 -- wird in Cargo.toml UND package.json gesetzt.
#   --hochladen    zusaetzlich als GitHub-Release veroeffentlichen (gh release
#                  create, Repo simeonzickert/mitschnitt). Standard: AUS. Ohne
#                  den Schalter bleibt alles lokal unter dem Ausgabeordner --
#                  kein Push, kein Release, nichts Oeffentliches.

set -euo pipefail

if [ $# -lt 1 ]; then
  echo "FEHLER: Aufruf ist scripts/mitschnitt-release.sh <version> [--hochladen]" >&2
  exit 2
fi
VERSION="$1"
shift
HOCHLADEN=0
for arg in "$@"; do
  case "$arg" in
    --hochladen) HOCHLADEN=1 ;;
    *) echo "FEHLER: unbekannter Schalter '$arg' (erlaubt: --hochladen)" >&2; exit 2 ;;
  esac
done

# Echter Semver-Regex, nicht ein Glob: `[0-9]*.[0-9]*.[0-9]*` liess wegen des
# `*` hinter jeder Ziffernklasse auch "1.2.3-beta" oder "1.a.3" durch, solange
# nur die erste Stelle je Segment eine Ziffer war.
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "FEHLER: Version '$VERSION' sieht nicht wie X.Y.Z aus." >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$REPO_ROOT/apps/desktop"
BUNDLE_SRC="$DESKTOP_DIR/src-tauri/target/release/bundle/macos/Mitschnitt.app"
BUNDLE_ID="media.zickert.mitschnitt"
RELEASE_REPO="simeonzickert/mitschnitt"

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
NOTAR_WORK="$OUT_DIR/_notar-work"

log() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

log "Voraussetzungen pruefen"
# latest.json traegt weiter unten eine feste Plattform-Kennung. Auf einem
# Intel-Mac gebaut waere das ein x86_64-Bundle unter der arm64-Kennung --
# jeder arm64-Rechner wuerde es als passendes Update herunterladen und
# scheitern. Erzwungen hier, VOR dem Bau, damit kein Aufwand in ein Ergebnis
# faellt, das ohnehin verworfen werden muesste.
if [ "$(uname -m)" != "arm64" ]; then
  echo "FEHLER: dieses Skript baut nur auf Apple Silicon (uname -m = arm64)." >&2
  echo "        Auf x86_64 wuerde latest.json ein x86_64-Bundle trotzdem als" >&2
  echo "        darwin-aarch64 veroeffentlichen." >&2
  exit 1
fi
if ! security find-identity -v -p codesigning | grep -qF "$SIGN_ID"; then
  echo "FEHLER: Signatur-Identitaet nicht im Schluesselbund: $SIGN_ID" >&2
  exit 1
fi
if ! xcrun notarytool history --keychain-profile mitschnitt-notar >/dev/null 2>&1; then
  echo "FEHLER: Notarisierungs-Profil 'mitschnitt-notar' fehlt oder ist ungueltig." >&2
  exit 1
fi
if [ "$HOCHLADEN" = "1" ] && ! command -v gh >/dev/null 2>&1; then
  echo "FEHLER: --hochladen verlangt die gh-CLI, die ist nicht installiert." >&2
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
CLI_TRIPLE="$(uname -m)-apple-darwin"
[ "$(uname -m)" = "arm64" ] && CLI_TRIPLE="aarch64-apple-darwin"
CLI_RESOURCE="$DESKTOP_DIR/src-tauri/resources/cli/mitschnitt-cli-$CLI_TRIPLE"
cargo build --release -p mitschnitt-cli --manifest-path "$REPO_ROOT/Cargo.toml"
install -m 0755 "$REPO_ROOT/target/release/mitschnitt" "$CLI_RESOURCE"
[ -x "$CLI_RESOURCE" ] || { echo "FEHLER: Sidecar fehlt unter $CLI_RESOURCE" >&2; exit 1; }
"$CLI_RESOURCE" --version >/dev/null || { echo "FEHLER: gebautes Sidecar laeuft nicht." >&2; exit 1; }

log "Bauen (Release, mit Updater-Artefakten)"
# Der Bau erzeugt hier bereits ein Mitschnitt.app.tar.gz + .sig, weil
# bundle.createUpdaterArtifacts=true ist und die Schluessel oben gesetzt sind.
# Das ist ABSICHTLICH und dient nur als frueher Beweis, dass Schluessel und
# Endpunkt technisch zusammenpassen -- das fuer Menschen bestimmte Archiv
# entsteht weiter unten aus dem NOTARISIERTEN Bundle, nicht aus diesem.
MARKER=$(mktemp -t mitschnitt-release-bau)
trap 'rm -f "$MARKER"' EXIT
cd "$DESKTOP_DIR"
TAURI_SIGNING_PRIVATE_KEY="$TAURI_SIGNING_PRIVATE_KEY" \
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" \
  pnpm exec tauri build --bundles app
[ -d "$BUNDLE_SRC" ] || { echo "FEHLER: kein Bundle unter $BUNDLE_SRC" >&2; exit 1; }
[ "$BUNDLE_SRC" -nt "$MARKER" ] || {
  echo "FEHLER: das Bundle ist aelter als der Beginn dieses Laufs." >&2
  exit 1
}

mkdir -p "$OUT_DIR"

log "Mit der eigenen Developer-ID signieren"
# Gleicher Schritt wie in mitschnitt-deploy.sh, aber auf einer Arbeitskopie im
# Ausgabeordner statt in ~/Applications -- dieses Skript fasst die laufende
# Installation des Betreibers nicht an.
WORK_APP="$OUT_DIR/_signiert/Mitschnitt.app"
rm -rf "$OUT_DIR/_signiert"
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
MITSCHNITT_NOTAR_WORK="$NOTAR_WORK" \
  MITSCHNITT_SIGN_IDENTITY="$SIGN_ID" \
  MITSCHNITT_RELEASE_DIR="$OUT_DIR" \
  "$REPO_ROOT/scripts/mitschnitt-notarisieren.sh" "$WORK_APP" "$OUT_DIR"
NOTARIZED_APP="$NOTAR_WORK/Mitschnitt.app"
[ -d "$NOTARIZED_APP" ] || { echo "FEHLER: notarisiertes Bundle fehlt unter $NOTARIZED_APP" >&2; exit 1; }
spctl -a -vv -t exec "$NOTARIZED_APP" 2>&1 | grep -q "Notarized Developer ID" \
  || { echo "FEHLER: spctl meldet kein 'Notarized Developer ID' fuer $NOTARIZED_APP." >&2; exit 1; }

log "Architektur des notarisierten Bundles feststellen"
# latest.json muss die ECHTE Architektur des gebauten Binaries tragen, nicht
# eine hart geschriebene. Der arm64-Zwang oben verhindert das Naheliegendste
# (auf x86_64 gebaut), nicht aber z.B. ein per --target quer gebautes
# Binary -- deshalb wird hier zusaetzlich am fertigen Bundle gemessen.
NOTARIZED_EXE_NAME="$(defaults read "$NOTARIZED_APP/Contents/Info.plist" CFBundleExecutable)"
BUNDLE_ARCH="$(lipo -archs "$NOTARIZED_APP/Contents/MacOS/$NOTARIZED_EXE_NAME")"
case "$BUNDLE_ARCH" in
  arm64) PLATFORM_KEY="darwin-aarch64" ;;
  x86_64) PLATFORM_KEY="darwin-x86_64" ;;
  *)
    echo "FEHLER: unerwartete/gemischte Architektur im gebauten Binary: '$BUNDLE_ARCH'" >&2
    exit 1
    ;;
esac
echo "Architektur: $BUNDLE_ARCH -> $PLATFORM_KEY"

log "Updater-Archiv aus dem notarisierten Bundle bauen"
UPDATE_ARCHIVE="$OUT_DIR/Mitschnitt.app.tar.gz"
rm -f "$UPDATE_ARCHIVE"
tar -czf "$UPDATE_ARCHIVE" -C "$NOTAR_WORK" "Mitschnitt.app"
[ -s "$UPDATE_ARCHIVE" ] || { echo "FEHLER: $UPDATE_ARCHIVE ist leer oder fehlt." >&2; exit 1; }

log "Updater-Archiv signieren (Minisign, derselbe Schluessel wie das eingebettete pubkey)"
TAURI_SIGNING_PRIVATE_KEY="$TAURI_SIGNING_PRIVATE_KEY" \
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" \
  pnpm --dir "$DESKTOP_DIR" exec tauri signer sign "$UPDATE_ARCHIVE"
SIGNATURE_FILE="$UPDATE_ARCHIVE.sig"
[ -s "$SIGNATURE_FILE" ] || { echo "FEHLER: $SIGNATURE_FILE ist leer oder fehlt." >&2; exit 1; }

log "latest.json schreiben"
DOWNLOAD_URL="https://github.com/$RELEASE_REPO/releases/download/v$VERSION/Mitschnitt.app.tar.gz"
PUB_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
CHANGELOG_FILE="$REPO_ROOT/packages/changelog/content/$VERSION.md"
if [ -f "$CHANGELOG_FILE" ]; then
  NOTES="$(cat "$CHANGELOG_FILE")"
else
  NOTES="Mitschnitt $VERSION"
fi
LATEST_JSON="$OUT_DIR/latest.json"
python3 - "$LATEST_JSON" "$VERSION" "$NOTES" "$PUB_DATE" "$SIGNATURE_FILE" "$DOWNLOAD_URL" "$PLATFORM_KEY" <<'PY'
import json
import sys

out_path, version, notes, pub_date, sig_path, url, platform_key = sys.argv[1:8]
with open(sig_path) as f:
    signature = f.read().strip()

data = {
    "version": version,
    "notes": notes,
    "pub_date": pub_date,
    "platforms": {
        platform_key: {
            "signature": signature,
            "url": url,
        }
    },
}

with open(out_path, "w") as f:
    json.dump(data, f, indent=2, ensure_ascii=False)
    f.write("\n")
PY
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$LATEST_JSON" \
  || { echo "FEHLER: $LATEST_JSON ist kein gueltiges JSON." >&2; exit 1; }

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
echo "Update-Feed:     $LATEST_JSON"
echo "Notarisiertes Zip (Weitergabe von Hand): $OUT_DIR/Mitschnitt-$VERSION-notarisiert.zip"

if [ "$HOCHLADEN" = "1" ]; then
  log "Als GitHub-Release veroeffentlichen ($RELEASE_REPO, v$VERSION)"
  gh release create "v$VERSION" \
    "$UPDATE_ARCHIVE" \
    "$SIGNATURE_FILE" \
    "$LATEST_JSON" \
    "$OUT_DIR/Mitschnitt-$VERSION-notarisiert.zip" \
    --repo "$RELEASE_REPO" \
    --title "v$VERSION" \
    --notes "$NOTES" \
    --latest
else
  echo
  echo "Nicht hochgeladen (--hochladen fehlt). Alles bleibt lokal unter $OUT_DIR."
fi
