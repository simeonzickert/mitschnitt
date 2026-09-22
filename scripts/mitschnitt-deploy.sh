#!/usr/bin/env bash
#
# Baut Mitschnitt, signiert es RICHTIG und installiert es nach ~/Applications.
#
# Warum dieses Skript existiert (gemessen 31.08.2026):
# `tauri build --debug` signiert nur ad-hoc. Das Ergebnis meldet
#
#     Identifier=desktop-<hash>      Info.plist=not bound      Signature=adhoc
#
# macOS haengt erteilte Berechtigungen an die Signatur. Passt der Identifier
# nicht zur Bundle-Kennung und ist die Info.plist nicht gebunden, laeuft jede
# Berechtigungs-Anfrage ins Leere: der Knopf in den Einstellungen tut sichtbar
# NICHTS, ohne Fehlermeldung. Genau das ist dem Prinzipal passiert, und es haette
# ihn den ganzen Test gekostet.
#
# Ohne dieses Skript kehrt der Fehler bei JEDEM Neubau zurueck.
#
# Gebraucht wird das eigene Developer-ID-Zertifikat im Schluesselbund. Ohne das
# bricht das Skript ab, statt eine kaputt signierte App zu installieren.
#
# ZWEI WEGE (Entscheid des Prinzipals 04.09.2026):
#
#   ohne Schalter   Debug-Bau. Der Alltag. Bleibt die Voreinstellung.
#   --release       Release-Bau. Fuer die Weitergabe an einen fremden Menschen.
#
# Der Unterschied ist nicht Geschmack, er ist gemessen. Ein Debug-Bundle trug
# 522 Quellkarten mit 46 MB TypeScript-Quelltext, den Benutzernamen des
# Prinzipals 5419-mal im Binary und wog 518 MB. Nichts davon gehoert in fremde
# Haende. Der Release-Bau laesst alles drei weg.
#
# BEIDE Wege laufen durch dieselbe Nachsignierung und dieselben zwei Waechter
# (Signatur nicht ad-hoc, Bundle stammt aus diesem Lauf). Der Release-Zweig
# darf daran NICHT vorbeilaufen -- beide Waechter haben real schon angeschlagen.

set -euo pipefail

BUILD_PROFILE="debug"
for arg in "$@"; do
  case "$arg" in
    --release) BUILD_PROFILE="release" ;;
    --debug)   BUILD_PROFILE="debug" ;;
    *) echo "FEHLER: unbekannter Schalter '$arg' (erlaubt: --release, --debug)" >&2; exit 1 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$REPO_ROOT/apps/desktop"
BUNDLE_SRC="$DESKTOP_DIR/src-tauri/target/$BUILD_PROFILE/bundle/macos/Mitschnitt.app"
INSTALL_DIR="$HOME/Applications"
INSTALL_TARGET="$INSTALL_DIR/Mitschnitt.app"
ENTITLEMENTS="$DESKTOP_DIR/src-tauri/Entitlements.local.plist"
BUNDLE_ID="media.zickert.mitschnitt"
# Die Signatur-Identitaet steht absichtlich NICHT im Repo: sie traegt den
# Klarnamen des Zertifikatsinhabers. Ohne die Variable bricht der Lauf ab,
# statt mit einem erfundenen Namen weiterzulaufen, der nie im Schluesselbund
# liegen kann.
if [ -z "${MITSCHNITT_SIGN_IDENTITY:-}" ]; then
  echo "FEHLER: MITSCHNITT_SIGN_IDENTITY ist nicht gesetzt." >&2
  echo "        Erwartet wird die volle Zeile der eigenen Developer-ID," >&2
  echo "        so wie 'security find-identity -v -p codesigning' sie zeigt." >&2
  exit 2
fi
SIGN_ID="$MITSCHNITT_SIGN_IDENTITY"

log() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

log "Bauart: $BUILD_PROFILE"
if [ "$BUILD_PROFILE" = "debug" ]; then
  echo "Debug-Bau: enthaelt Quelltext, Quellkarten und den Benutzernamen."
  echo "Fuer die Weitergabe an andere Menschen: $0 --release"
else
  echo "Release-Bau: ohne Quelltext, ohne Quellkarten. Signiert, NICHT notarisiert."
fi

# Betreiber, 11.09.2026: "wir brauchen bitte mal eine versionsnummer und diese geht
# stringend hoch". Drei Baeume hintereinander hiessen 0.1.0 -- damit war am
# Bildschirm nicht zu erkennen, welcher Stand gerade laeuft. Jeder Lauf hebt
# deshalb die letzte Stelle, in beiden Dateien, die die Version tragen. Die
# Zahl steigt monoton und wird mitcommittet; ein Bau ohne Versionssprung waere
# genau das Problem zurueck.
log "Versionsnummer heben"
VERSION_PKG="$DESKTOP_DIR/package.json"
VERSION_CARGO="$DESKTOP_DIR/src-tauri/Cargo.toml"
ALT="$(sed -n 's/^version = "\(.*\)"/\1/p' "$VERSION_CARGO" | head -1)"
case "$ALT" in
  0.*.*) ;;
  *) echo "FEHLER: unerwartete Version '$ALT' in $VERSION_CARGO" >&2; exit 1 ;;
esac
MAJOR="${ALT%%.*}"; REST="${ALT#*.}"; MINOR="${REST%%.*}"; PATCH="${REST#*.}"
case "$PATCH" in
  ''|*[!0-9]*) echo "FEHLER: Patch-Stelle '$PATCH' ist keine Zahl" >&2; exit 1 ;;
esac
NEU="$MAJOR.$MINOR.$((PATCH + 1))"
/usr/bin/sed -i '' "s/^version = \"$ALT\"/version = \"$NEU\"/" "$VERSION_CARGO"
/usr/bin/sed -i '' "s/\"version\": \"$ALT\"/\"version\": \"$NEU\"/" "$VERSION_PKG"
# Gegenprobe: der Wert steht wirklich in beiden Dateien. Ein sed, das nichts
# getroffen hat, meldet keinen Fehler -- genau so entstuende wieder eine
# stehende Nummer.
for f in "$VERSION_CARGO" "$VERSION_PKG"; do
  grep -q "$NEU" "$f" || { echo "FEHLER: Version $NEU steht nicht in $f" >&2; exit 1; }
done
echo "$ALT -> $NEU"

log "Voraussetzungen pruefen"
if ! security find-identity -v -p codesigning | grep -qF "$SIGN_ID"; then
  echo "FEHLER: Signatur-Identitaet nicht im Schluesselbund: $SIGN_ID" >&2
  echo "Ohne sie wuerde die App ad-hoc signiert und die Berechtigungen brechen." >&2
  exit 1
fi
[ -f "$ENTITLEMENTS" ] || { echo "FEHLER: $ENTITLEMENTS fehlt" >&2; exit 1; }
plutil -lint "$ENTITLEMENTS" >/dev/null || { echo "FEHLER: Entitlements sind kein gueltiges XML" >&2; exit 1; }

# Abhaengigkeits-Gate VOR dem Beenden der laufenden App: ein roter Scan darf
# dem Betreiber keine laufende Aufnahme kosten. Ratchet gegen scripts/audit-baseline.txt,
# Details in scripts/audit-gate.sh (12.09.2026).
log "Abhaengigkeits-Gate (pnpm audit --prod + cargo audit)"
"$REPO_ROOT/scripts/audit-gate.sh"

log "Laufende Instanz beenden"
pkill -f "Applications/Mitschnitt.app" 2>/dev/null || true
# Der Vite-Dev-Server haelt sonst Port 1422 und der naechste Start scheitert.
lsof -ti :1422 2>/dev/null | xargs -r kill -9 2>/dev/null || true
sleep 2

log "Spiegel-Befehl bauen und ins Bundle legen"
# Der Knopf "CLI installieren" in den Einstellungen sucht genau diese Datei.
# Fehlt sie, meldet die App dauerhaft ResourceMissing -- der Knopf ist dann eine
# Attrappe. Deshalb wird das Sidecar hier mitgebaut und NICHT nur nebenher.
CLI_TRIPLE="$(uname -m)-apple-darwin"
[ "$(uname -m)" = "arm64" ] && CLI_TRIPLE="aarch64-apple-darwin"
CLI_RESOURCE="$DESKTOP_DIR/src-tauri/resources/cli/mitschnitt-cli-$CLI_TRIPLE"
cd "$REPO_ROOT"
cargo build --release -p mitschnitt-cli
install -m 0755 "$REPO_ROOT/target/release/mitschnitt" "$CLI_RESOURCE"
[ -x "$CLI_RESOURCE" ] || { echo "FEHLER: Sidecar fehlt unter $CLI_RESOURCE" >&2; exit 1; }
"$CLI_RESOURCE" --version >/dev/null || { echo "FEHLER: gebautes Sidecar laeuft nicht." >&2; exit 1; }

log "Bauen"
cd "$DESKTOP_DIR"
# `--bundles app` erzeugt das .app. Der Lauf endet mit einem Fehler, weil tauri
# danach ein Updater-Archiv signieren will und kein privater Schluessel gesetzt
# ist -- der Updater ist in diesem Fork bewusst AUS (tauri.conf: active=false).
# Das .app ist zu dem Zeitpunkt fertig, deshalb wird der Ausgang toleriert und
# stattdessen das Ergebnis geprueft.
# Marker UNMITTELBAR vor dem Bau. Die frühere Pruefung "juenger als 10
# Minuten" ist keine: ein Bundle aus einem anderen Lauf, der vor neun Minuten
# lief, kommt damit durch, und ein langsamer Bau faellt faelschlich raus. Der
# Marker vergleicht mit DIESEM Lauf statt mit der Uhr.
MARKER=$(mktemp -t mitschnitt-bau)
trap 'rm -f "$MARKER"' EXIT
set +e
if [ "$BUILD_PROFILE" = "release" ]; then
  pnpm exec tauri build --bundles app
else
  pnpm exec tauri build --debug --bundles app
fi
BAU_STATUS=$?
set -e
[ -d "$BUNDLE_SRC" ] || {
  echo "FEHLER: kein Bundle unter $BUNDLE_SRC (tauri-Status $BAU_STATUS)" >&2
  exit 1
}

# Der Bundle-Ordner muss NEUER sein als der Marker, sonst stammt er nicht aus
# diesem Lauf. `-newer` vergleicht die Aenderungszeit; tauri schreibt das .app
# beim Bauen neu.
if [ ! "$BUNDLE_SRC" -nt "$MARKER" ]; then
  echo "FEHLER: das Bundle ist aelter als der Beginn dieses Laufs -- der Bau" >&2
  echo "        hat es nicht erneuert (tauri-Status $BAU_STATUS)." >&2
  exit 1
fi
if [ "$BAU_STATUS" -ne 0 ]; then
  # Toleriert, aber nur mit gemessenem Grund: `tauri build` endet hier mit
  # einem Fehler, weil es danach ein Updater-Archiv signieren will und kein
  # privater Schluessel gesetzt ist -- der Updater ist in diesem Fork bewusst
  # AUS (tauri.conf: active=false). Das .app ist zu dem Zeitpunkt fertig, und
  # der Marker oben beweist, dass es aus DIESEM Lauf stammt. Ohne diesen
  # Beweis waere das Tolerieren eine offene Tuer.
  echo "Hinweis: tauri endete mit Status $BAU_STATUS (erwartet: Updater-Signatur"
  echo "         ohne Schluessel). Das Bundle stammt nachweislich aus diesem Lauf."
fi

log "Installieren nach $INSTALL_TARGET"
mkdir -p "$INSTALL_DIR"
rm -rf "$INSTALL_TARGET"
cp -R "$BUNDLE_SRC" "$INSTALL_TARGET"

log "Signieren"
# --deep ist hier noetig: das Bundle traegt unsignierte Fremd-Bibliotheken
# (whisper, mlx). Kein --options runtime, weil Hardened Runtime genau die
# beanstanden wuerde. Das gilt fuer BEIDE Wege: auch der Release-Bau ergibt ein
# signiertes, aber NICHT notarisiertes Bundle. Wer es weitergibt, muss damit
# rechnen, dass Gatekeeper beim Empfaenger nachfragt. Hardened Runtime plus
# Notarisierung waere der naechste Schritt und ist hier bewusst nicht mit
# erledigt -- er braucht seinen eigenen gemessenen Lauf.
# codesign holt fuer eine Developer-ID-Signatur einen Zeitstempel von Apple;
# ist der Server einen Moment nicht erreichbar, bricht es mit "A timestamp was
# expected but was not found" ab und laesst das Bundle AD-HOC signiert zurueck
# (22.09.2026 so passiert, der zweite Anlauf eine Minute spaeter lief durch).
# Deshalb bis zu drei Versuche mit Pause; erst dann ist es ein echter Fehler.
for versuch in 1 2 3; do
  if codesign --force --deep \
    --sign "$SIGN_ID" \
    --identifier "$BUNDLE_ID" \
    --entitlements "$ENTITLEMENTS" \
    "$INSTALL_TARGET"; then
    break
  fi
  if [ "$versuch" -eq 3 ]; then
    echo "FEHLER: Signieren dreimal gescheitert (Zeitstempel-Server?)." >&2
    exit 1
  fi
  echo "Signieren gescheitert (Versuch $versuch), neuer Anlauf in 10 s ..." >&2
  sleep 10
done
xattr -dr com.apple.quarantine "$INSTALL_TARGET" 2>/dev/null || true

log "Signatur pruefen"
# Das ist der eigentliche Zweck des Skripts -- hier faellt auf, wenn wieder
# ad-hoc signiert wurde.
info="$(codesign -dv "$INSTALL_TARGET" 2>&1)"
echo "$info" | grep -E "Identifier=|TeamIdentifier=|Info.plist"
echo "$info" | grep -q "Identifier=$BUNDLE_ID" \
  || { echo "FEHLER: falscher Identifier -- Berechtigungen wuerden brechen." >&2; exit 1; }
echo "$info" | grep -q "Info.plist entries=" \
  || { echo "FEHLER: Info.plist nicht gebunden -- Berechtigungen wuerden brechen." >&2; exit 1; }
codesign --verify --deep "$INSTALL_TARGET" \
  || { echo "FEHLER: Signatur ungueltig." >&2; exit 1; }

# LGPL-Waechter: libmp3lame (LAME) muss als eigene, austauschbare Datei im
# Bundle liegen und darf nicht wieder im Hauptprogramm stecken. Ein
# Paketupdate wuerde das ohne eine einzige Fehlermeldung zurueckdrehen.
# Begruendung: ATTRIBUTIONS.md, Abschnitt 8.
log "LGPL-Waechter (libmp3lame)"
"$REPO_ROOT/scripts/lgpl-gate.sh" "$INSTALL_TARGET"

log "Fertig"
echo "Installiert: $INSTALL_TARGET"
echo "Starten mit:  open \"$INSTALL_TARGET\""
