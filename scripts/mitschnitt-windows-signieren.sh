#!/usr/bin/env bash
#
# Mitschnitt: Windows-Installer signieren und in den Release-Ordner ablegen.
#
# Der Windows-Installer wird in GitHub Actions gebaut und von Hand
# heruntergeladen. Dieses Skript laeuft auf dem Mac und:
#   - Prueft Version, Datei-Existenz, .exe-Endung und MZ-Magic-Bytes
#   - Prueft den Changelog-Eintrag fuer die Version
#   - Kopiert den Installer unter festem Namen nach OUT_DIR
#   - Signiert mit dem Updater-Minisign-Schluessel (oder Test-Schluessel)
#   - Erzeugt den Plattform-Baustein fuer den Update-Feed
#
# Aufruf:
#   scripts/mitschnitt-windows-signieren.sh <version> <pfad-zum-setup.exe>
#
# Umgebungsvariablen:
#   MITSCHNITT_RELEASE_DIR (Pflicht) - Zielordner fuer die Release-Ausgabe
#   MITSCHNITT_TEST_SIGNING_KEY_FILE (optional) - Pfad zu Test-Schluesseldatei
#   MITSCHNITT_TEST_SIGNING_KEY_PASSWORD (optional) - Passwort fuer Test-Schluessel

set -euo pipefail

if [ $# -lt 2 ]; then
  echo "FEHLER: Aufruf ist scripts/mitschnitt-windows-signieren.sh <version> <pfad-zum-setup.exe>" >&2
  exit 2
fi
VERSION="$1"
INSTALLER_IN="$2"

# Echter Semver-Regex wie im Release-Skript
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "FEHLER: Version '$VERSION' sieht nicht wie X.Y.Z aus." >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$REPO_ROOT/apps/desktop"
OUT_ROOT="${MITSCHNITT_RELEASE_DIR:?MITSCHNITT_RELEASE_DIR setzen (Zielordner fuer die Release-Ausgabe)}"
OUT_DIR="$OUT_ROOT/v$VERSION"

# Praefix fuer Log-Zeilen, wenn Test-Schluessel verwendet werden
TEST_PREFIX=""

log() {
  local msg="$1"
  if [ -n "$TEST_PREFIX" ]; then
    printf '\n\033[1m==> [TESTSCHLUESSEL] %s\033[0m\n' "$msg"
  else
    printf '\n\033[1m==> %s\033[0m\n' "$msg"
  fi
}

# Pruefe, ob Test-Schluessel verwendet werden sollen
if [ -n "${MITSCHNITT_TEST_SIGNING_KEY_FILE:-}" ] && [ -n "${MITSCHNITT_TEST_SIGNING_KEY_PASSWORD:-}" ]; then
  TEST_PREFIX="TESTSCHLUESSEL"
  log "Test-Schluessel werden verwendet (MITSCHNITT_TEST_SIGNING_KEY_FILE und MITSCHNITT_TEST_SIGNING_KEY_PASSWORD gesetzt)"
fi

log "Voraussetzungen pruefen"

# Installer-Datei muss existieren
if [ ! -f "$INSTALLER_IN" ]; then
  echo "FEHLER: Installer-Datei '$INSTALLER_IN' existiert nicht." >&2
  exit 1
fi

# Installer-Datei darf nicht leer sein
if [ ! -s "$INSTALLER_IN" ]; then
  echo "FEHLER: Installer-Datei '$INSTALLER_IN' ist leer." >&2
  exit 1
fi

# Installer-Datei muss auf .exe enden
if [[ ! "$INSTALLER_IN" =~ \.exe$ ]]; then
  echo "FEHLER: Installer-Datei '$INSTALLER_IN' endet nicht auf .exe." >&2
  exit 1
fi

# Installer-Datei muss mit MZ beginnen (Windows-Executable-Magic-Bytes)
MAGIC_BYTES="$(xxd -l 2 -p "$INSTALLER_IN" 2>/dev/null || od -A n -t x1 -N 2 "$INSTALLER_IN" | tr -d ' ')"
if [ "$MAGIC_BYTES" != "4d5a" ]; then
  echo "FEHLER: Installer-Datei '$INSTALLER_IN' beginnt nicht mit MZ (keine gueltige Windows-EXE)." >&2
  exit 1
fi

# Ausgabeverzeichnis anlegen
mkdir -p "$OUT_DIR"

log "Changelog fuer $VERSION pruefen"
CHANGELOG_FILE="$REPO_ROOT/CHANGELOG.md"
if ! "$REPO_ROOT/scripts/changelog-extract.sh" "$CHANGELOG_FILE" "$VERSION" what-new >/dev/null; then
  echo "FEHLER: CHANGELOG.md hat keinen (oder einen leeren) Abschnitt fuer $VERSION." >&2
  echo "        Vor dem Signieren erst CHANGELOG.md pflegen." >&2
  exit 1
fi

log "Changelog-Text aus CHANGELOG.md auslesen"
NOTES_PLAIN="$("$REPO_ROOT/scripts/changelog-extract.sh" "$CHANGELOG_FILE" "$VERSION" notes-plain)"

log "Installer kopieren nach $OUT_DIR"
# Der Dateiname der Eingabe muss exakt Mitschnitt_${VERSION}_x64-setup.exe sein
# (so benennt Tauri den NSIS-Installer nach der Versionsnummer in der Config).
# Sonst Abbruch: eine EXE aus einem aelteren CI-Lauf wuerde als neue Version
# verpackt, und die Windows-App installierte sich bei jedem Start neu (Schleife).
INSTALLER_NAME="Mitschnitt_${VERSION}_x64-setup.exe"
INSTALLER_BASENAME="$(basename "$INSTALLER_IN")"
if [ "$INSTALLER_BASENAME" != "$INSTALLER_NAME" ]; then
  echo "FEHLER: Installer-Datei '$INSTALLER_BASENAME' entspricht nicht dem erwarteten Namen '$INSTALLER_NAME'." >&2
  echo "        Tauri benennt den NSIS-Installer nach der Versionsnummer in der Config." >&2
  echo "        Eine EXE aus einem aelteren CI-Lauf wuerde als neue Version verpackt," >&2
  echo "        und die Windows-App installierte sich bei jedem Start neu (Schleife)." >&2
  exit 1
fi
INSTALLER_OUT="$OUT_DIR/$INSTALLER_NAME"
cp "$INSTALLER_IN" "$INSTALLER_OUT"
if [ ! -s "$INSTALLER_OUT" ]; then
  echo "FEHLER: Kopie des Installers nach $INSTALLER_OUT ist fehlgeschlagen oder leer." >&2
  exit 1
fi

log "Updater-Schluessel bereitstellen"
if [ -n "$TEST_PREFIX" ]; then
  # Test-Schluessel aus Datei lesen
  if [ ! -f "$MITSCHNITT_TEST_SIGNING_KEY_FILE" ]; then
    echo "FEHLER: Test-Schluesseldatei '$MITSCHNITT_TEST_SIGNING_KEY_FILE' existiert nicht." >&2
    exit 1
  fi
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$MITSCHNITT_TEST_SIGNING_KEY_FILE")"
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$MITSCHNITT_TEST_SIGNING_KEY_PASSWORD"
  if [ -z "$TAURI_SIGNING_PRIVATE_KEY" ] || [ -z "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" ]; then
    echo "FEHLER: Test-Schluessel oder -Passwort sind leer." >&2
    exit 1
  fi
else
  # Echte Schluessel aus dem Schluesselbund holen (wie im Release-Skript)
  TAURI_SIGNING_PRIVATE_KEY="$(security find-generic-password -s mitschnitt-updater-private-key -w)"
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s mitschnitt-updater-key-password -w)"
  if [ -z "$TAURI_SIGNING_PRIVATE_KEY" ] || [ -z "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" ]; then
    echo "FEHLER: Updater-Schluessel oder -Passwort sind leer aus dem Schluesselbund gekommen." >&2
    exit 1
  fi
fi

log "Installer signieren (Minisign)"
TAURI_SIGNING_PRIVATE_KEY="$TAURI_SIGNING_PRIVATE_KEY" \
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" \
  pnpm --dir "$DESKTOP_DIR" exec tauri signer sign "$INSTALLER_OUT"
SIGNATURE_FILE="${INSTALLER_OUT}.sig"
if [ ! -s "$SIGNATURE_FILE" ]; then
  echo "FEHLER: $SIGNATURE_FILE ist leer oder fehlt." >&2
  exit 1
fi

log "Plattform-Baustein fuer den Update-Feed erzeugen"
PUB_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
DOWNLOAD_URL="https://github.com/simeonzickert/mitschnitt/releases/download/v${VERSION}/${INSTALLER_NAME}"
# --testschluessel an baustein uebergeben, wenn Test-Schluessel verwendet werden
BAUSTEIN_ARGS=()
if [ -n "$TEST_PREFIX" ]; then
  BAUSTEIN_ARGS=(--testschluessel)
fi
python3 "$REPO_ROOT/scripts/mitschnitt-feed.py" baustein \
  --ausgabe "$OUT_DIR" \
  --version "$VERSION" \
  --plattform "windows-x86_64" \
  --sig "$SIGNATURE_FILE" \
  --datei "$INSTALLER_OUT" \
  --url "$DOWNLOAD_URL" \
  --notes "$NOTES_PLAIN" \
  --pub-date "$PUB_DATE" \
  "${BAUSTEIN_ARGS[@]+"${BAUSTEIN_ARGS[@]}"}"

log "Fertig"
echo
echo "Installer:  $INSTALLER_OUT"
echo "Signatur:   $SIGNATURE_FILE"
echo "Baustein:   $OUT_DIR/plattform-windows-x86_64.json"
