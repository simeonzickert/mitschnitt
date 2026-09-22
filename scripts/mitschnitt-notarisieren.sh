#!/bin/bash
# Mitschnitt: ein installiertes, signiertes Bundle notarisieren und als
# weitergabefaehiges Zip ablegen.
#
# Ablauf (gemessen 22.09.2026, erste Notarisierung dieses Forks):
#   1. Kopie des Bundles ziehen (die laufende App bleibt unangetastet)
#   2. Mach-O-Dateien unter Contents/Resources/ EINZELN signieren
#      -- codesign --deep laeuft nur ueber MacOS/, Frameworks/, PlugIns/;
#      der Spiegel-Befehl unter Resources/cli/ blieb beim ersten Anlauf
#      unsigniert, und Apple lehnte genau deshalb ab ("not signed with a
#      valid Developer ID", "no secure timestamp", "no hardened runtime").
#   3. Bundle mit Hardened Runtime + Zeitstempel signieren
#      (Entitlements.notar.plist: nur Mikrofon + Apple Events; die
#      Fremdbibliotheken whisper/mlx hat Apple nicht beanstandet)
#   4. Zip einreichen, auf das Urteil warten (Apple: hier ~8 Minuten)
#   5. bei "Accepted" Ticket anheften, spctl pruefen, Release-Zip bauen
#      bei "Invalid" Apples Protokoll holen und die beanstandeten Dateien zeigen
#
# Voraussetzung: Zugangsdaten einmalig im Schluesselbund:
#   xcrun notarytool store-credentials mitschnitt-notar \
#     --apple-id <apple-id> --team-id 3A97H6V74M --password <app-passwort>
#
# Aufruf:
#   scripts/mitschnitt-notarisieren.sh [Bundle] [Zielordner fuer das Zip]
#   Standard: ~/Applications/Mitschnitt.app, $MITSCHNITT_RELEASE_DIR (Pflicht-Variable)
#
# Das Skript ersetzt das installierte Bundle NICHT. Wer die notarisierte
# Fassung einspielen will, verschiebt das alte Bundle beiseite (Papierkorb
# oder Sicherung) und kopiert die Kopie aus dem Arbeitsordner hin.
set -euo pipefail

SRC="${1:-$HOME/Applications/Mitschnitt.app}"
OUT_DIR="${2:-${MITSCHNITT_RELEASE_DIR:?MITSCHNITT_RELEASE_DIR setzen (Zielordner fuer Release-Zips)}}"
PROFILE="${MITSCHNITT_NOTAR_PROFILE:-mitschnitt-notar}"
SIGN_ID="${MITSCHNITT_SIGN_IDENTITY:?MITSCHNITT_SIGN_IDENTITY setzen (Developer-ID-Zertifikat, wie security find-identity es zeigt)}"
BUNDLE_ID="media.zickert.mitschnitt"
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ENTITLEMENTS="$REPO_DIR/apps/desktop/src-tauri/Entitlements.notar.plist"
WORK="${MITSCHNITT_NOTAR_WORK:-$OUT_DIR/_notar-$(date +%Y%m%d-%H%M)}"
APP="$WORK/Mitschnitt.app"

log() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

[ -d "$SRC" ] || { echo "FEHLER: Bundle nicht gefunden: $SRC" >&2; exit 1; }
[ -f "$ENTITLEMENTS" ] || { echo "FEHLER: $ENTITLEMENTS fehlt" >&2; exit 1; }
xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1 \
  || { echo "FEHLER: Schluesselbund-Profil '$PROFILE' fehlt oder ist ungueltig (store-credentials, s. Kopf)." >&2; exit 1; }

VERSION="$(defaults read "$SRC/Contents/Info.plist" CFBundleShortVersionString)"
log "Kopie von $SRC (Version $VERSION) nach $WORK"
mkdir -p "$WORK"
cp -R "$SRC" "$APP"

log "Nebenprogramme unter Resources/ einzeln signieren"
find "$APP/Contents/Resources" -type f | while read -r f; do
  if file -b "$f" | grep -q "Mach-O"; then
    codesign --force --options runtime --timestamp --sign "$SIGN_ID" "$f"
    echo "signiert: ${f#$APP/}"
  fi
done

# Apple raet von --deep ab: es signiert von innen nach aussen in EINEM
# Aufruf und kann dabei die Reihenfolge verfehlen, in der ein Bundle
# tatsaechlich geprueft wird. Fuer Contents/Frameworks/ (die Dylibs, allen
# voran libmp3lame) wird deshalb VOR dem Bundle-Signieren derselbe Weg
# gegangen wie fuer Resources/ oben: einzeln, ohne --identifier und ohne
# Entitlements (beides gehoert nur dem ausfuehrbaren Hauptprogramm). Der
# --deep-Aufruf unten bleibt als Netz stehen, falls dennoch etwas fehlt.
if [ -d "$APP/Contents/Frameworks" ]; then
  log "Dylibs unter Frameworks/ einzeln signieren"
  find "$APP/Contents/Frameworks" -type f | while read -r f; do
    if file -b "$f" | grep -q "Mach-O"; then
      codesign --force --options runtime --timestamp --sign "$SIGN_ID" "$f"
      echo "signiert: ${f#$APP/}"
    fi
  done
fi

log "Bundle mit Hardened Runtime signieren"
for versuch in 1 2 3; do
  if codesign --force --deep --options runtime --timestamp \
      --sign "$SIGN_ID" --identifier "$BUNDLE_ID" \
      --entitlements "$ENTITLEMENTS" "$APP"; then
    break
  fi
  [ "$versuch" -eq 3 ] && { echo "FEHLER: Signieren dreimal gescheitert (Zeitstempel-Server?)." >&2; exit 1; }
  echo "Signieren gescheitert (Versuch $versuch), neuer Anlauf in 10 s ..." >&2
  sleep 10
done
codesign --verify --deep --strict "$APP" || { echo "FEHLER: Signatur ungueltig." >&2; exit 1; }
codesign -dv --verbose=2 "$APP" 2>&1 | grep -E "flags=|Timestamp|TeamIdentifier"

# LGPL-Waechter VOR dem Einreichen: eine notarisierte Fassung ist genau die,
# die das Haus verlaesst. Steckt LAME wieder statisch im Hauptprogramm oder
# fehlen die Lizenztexte, waere die Weitergabe unzulaessig -- und der Lauf soll
# hier abbrechen, nicht erst beim Empfaenger auffallen.
# Begruendung: ATTRIBUTIONS.md, Abschnitt 8.
log "LGPL-Waechter (libmp3lame)"
"$REPO_DIR/scripts/lgpl-gate.sh" "$APP"

log "Einreichen"
ZIP="$WORK/Mitschnitt.zip"
ditto -c -k --keepParent "$APP" "$ZIP"
xcrun notarytool submit "$ZIP" --keychain-profile "$PROFILE" --wait 2>&1 | tee "$WORK/submit.log"
SID="$(grep -m1 -E '^\s*id:' "$WORK/submit.log" | awk '{print $2}')"

if ! grep -q "status: Accepted" "$WORK/submit.log"; then
  log "Abgelehnt, Apples Protokoll"
  [ -n "$SID" ] && xcrun notarytool log "$SID" --keychain-profile "$PROFILE" "$WORK/notar-log.json" \
    && python3 -c "import json,sys;d=json.load(open(sys.argv[1]));print(d.get('status'));[print(i.get('severity'),i.get('path'),i.get('message')) for i in d.get('issues') or []]" "$WORK/notar-log.json"
  exit 1
fi

log "Ticket anheften und pruefen"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
spctl -a -vv -t exec "$APP" 2>&1 | grep -q "Notarized Developer ID" \
  || { echo "FEHLER: spctl meldet kein 'Notarized Developer ID'." >&2; exit 1; }

log "Release-Zip"
mkdir -p "$OUT_DIR"
REL="$OUT_DIR/Mitschnitt-$VERSION-notarisiert.zip"
ditto -c -k --keepParent "$APP" "$REL"
ls -la "$REL"

log "Fertig"
echo "Notarisierte Kopie: $APP"
echo "Zum Weitergeben:    $REL"
echo "Einspielen: altes Bundle beiseite, dann  cp -R \"$APP\" \"$SRC\""
