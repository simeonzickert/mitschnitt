#!/usr/bin/env bash
#
# Mitschnitt: aus einem fertigen App-Bundle eine DMG zum Herunterladen bauen
# (App links, Programme-Ordner rechts, eigener Hintergrund). ZICK-329.
#
# Dieses Skript PACKT nur. Es signiert nichts, notarisiert nichts und fasst das
# Quell-Bundle nicht an. Signieren, Notarisieren und Stapeln der DMG macht
# mitschnitt-release.sh danach -- so laesst sich der Packschritt allein und
# ohne Apple-Einreichung probieren:
#
#   scripts/mitschnitt-dmg.sh <Pfad/zu/Mitschnitt.app> <Ziel.dmg> [Volume-Name]
#
# WARUM dmgbuild und nicht Tauris eigener DMG-Bundler (Entscheid 27.09.2026):
#   - `tauri build --bundles dmg` packt immer das Bundle unter
#     target/release/bundle/macos/ -- also das UNSIGNIERTE aus dem Bau, nicht
#     das Developer-ID-signierte und notarisierte. Ein eigener Pfad laesst sich
#     nicht angeben, und `tauri bundle` baut das .app dabei neu.
#   - Tauris bundle_dmg.sh (ein create-dmg-Fork) steckt nur im CLI-Binary und
#     liegt nicht als Datei im Repo. Es ordnet das Fenster per AppleScript im
#     Finder an; das braucht eine grafische Sitzung und die TCC-Freigabe
#     "Terminal darf Finder steuern" und ist die bekannte Wackelstelle dieses
#     Wegs (Finder-Timing, -1728-Fehler).
#   - dmgbuild (MIT, Python) schreibt die .DS_Store mit Fensterlage,
#     Symbolpositionen und Hintergrund direkt. Kein Finder, kein AppleScript,
#     deterministisch, laeuft auch ohne Bildschirm.
#
# dmgbuild wird NICHT global installiert: beim ersten Lauf entsteht eine
# eigene venv unter $MITSCHNITT_DMG_VENV (Standard: ~/Library/Caches/
# mitschnitt-release/dmgbuild-<version>), installiert mit `pip install
# --require-hashes` gegen scripts/dmgbuild-requirements.txt (Forge-Zweitblick
# 27.09.2026: eine Versionsnummer allein ist kein Supply-Chain-Schutz, sie
# pinnt weder die Datei noch die beiden echten Abhaengigkeiten ds_store/
# mac_alias). Neu erzeugen bei einer Aenderung von DMGBUILD_VERSION, in einer
# Wegwerf-venv AUSSERHALB des Repos (z.B. auf einer externen Platte):
#
#   python3 -m venv /pfad/wegwerf-venv
#   /pfad/wegwerf-venv/bin/pip download --dest /pfad/wegwerf-venv/pakete \
#     "dmgbuild==<neue-version>"
#   for f in /pfad/wegwerf-venv/pakete/*.whl; do
#     /pfad/wegwerf-venv/bin/pip hash --algorithm sha256 "$f"
#   done
#
# Ergebnis (Name==Version + --hash=sha256:... je Paket) von Hand in
# scripts/dmgbuild-requirements.txt eintragen. Alle drei Pakete sind reine
# Python-Wheels (py3-none-any), es braucht also nur einen Eintrag je Paket.
#
# Nach dem Packen prueft dieses Skript zusaetzlich das Bundle IN der DMG
# (codesign, und -- ausser bei $MITSCHNITT_DMG_OHNE_NOTAR_PRUEFUNG=1 -- auch
# Notarisierungs-Ticket + spctl). Der Schalter ist nur fuer Trockenlaeufe mit
# einer unsignierten/nicht notarisierten Testkopie gedacht, NIE fuer eine
# echte Release-DMG.
#
# Fensterlage und Positionen kommen aus tauri.conf.json (bundle.macOS.dmg),
# damit es genau EINE Quelle dafuer gibt.

set -euo pipefail

if [ $# -lt 2 ]; then
  echo "FEHLER: Aufruf ist scripts/mitschnitt-dmg.sh <App-Bundle> <Ziel.dmg> [Volume-Name]" >&2
  exit 2
fi
APP_SRC="$1"
DMG_OUT="$2"
VOLNAME="${3:-Mitschnitt}"

DMGBUILD_VERSION="1.6.7"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$REPO_ROOT/apps/desktop/src-tauri"
TAURI_CONF="$TAURI_DIR/tauri.conf.json"
VENV="${MITSCHNITT_DMG_VENV:-$HOME/Library/Caches/mitschnitt-release/dmgbuild-$DMGBUILD_VERSION}"
DMGBUILD_REQUIREMENTS="$REPO_ROOT/scripts/dmgbuild-requirements.txt"
# Nur fuer Trockenlaeufe mit einer nicht notarisierten Testkopie -- siehe
# Pruefung des Bundles IN der DMG weiter unten.
OHNE_NOTAR_PRUEFUNG="${MITSCHNITT_DMG_OHNE_NOTAR_PRUEFUNG:-0}"

log() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

[ -d "$APP_SRC" ] || { echo "FEHLER: Bundle nicht gefunden: $APP_SRC" >&2; exit 1; }
[ "$(basename "$APP_SRC")" = "Mitschnitt.app" ] \
  || { echo "FEHLER: erwartet wird ein Bundle namens Mitschnitt.app, nicht $(basename "$APP_SRC")" >&2; exit 1; }
case "$DMG_OUT" in
  *.dmg) ;;
  *) echo "FEHLER: Ziel muss auf .dmg enden: $DMG_OUT" >&2; exit 1 ;;
esac
[ -f "$DMGBUILD_REQUIREMENTS" ] \
  || { echo "FEHLER: $DMGBUILD_REQUIREMENTS fehlt." >&2; exit 1; }

# Eine vorhandene venv wurde bisher nur ueber "dmgbuild --help" plus
# "pip show dmgbuild" vertraut -- das prueft weder die beiden echten
# Abhaengigkeiten (ds_store, mac_alias) noch, ob die venv ueberhaupt zur
# Requirements-Datei passt. Jetzt wird JEDES darin gepinnte Paket gegen seine
# tatsaechlich installierte Version geprueft (Forge-Zweitblick 27.09.2026).
venv_entspricht_requirements() {
  [ -x "$VENV/bin/dmgbuild" ] || return 1
  local zeile paket rest version installiert
  while IFS= read -r zeile; do
    paket="${zeile%%==*}"
    rest="${zeile#*==}"
    version="${rest%% *}"
    installiert="$("$VENV/bin/pip" show "$paket" 2>/dev/null | sed -n 's/^Version: //p')"
    [ -n "$installiert" ] && [ "$installiert" = "$version" ] || return 1
  done < <(grep -E '^[A-Za-z0-9_.-]+==' "$DMGBUILD_REQUIREMENTS")
}

log "dmgbuild $DMGBUILD_VERSION bereitstellen ($VENV)"
if ! venv_entspricht_requirements; then
  rm -rf "$VENV"
  mkdir -p "$(dirname "$VENV")"
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install --quiet --require-hashes -r "$DMGBUILD_REQUIREMENTS"
fi
venv_entspricht_requirements \
  || { echo "FEHLER: dmgbuild-venv entspricht nicht $DMGBUILD_REQUIREMENTS." >&2; exit 1; }

WORK="$(mktemp -d -t mitschnitt-dmg)"
MNT="$WORK/mnt"
# Ein Abbruch (Signal oder Fehler) NACH "hdiutil attach" darf das Volume nicht
# gemountet zuruecklassen -- "rm -rf $WORK" loescht bislang nur den
# Mountpunkt-Ordner, haengt aber nichts aus (Forge-Zweitblick 27.09.2026).
# Deshalb zuerst aushaengen, dann erst den Arbeitsordner loeschen. Fuer
# INT/TERM dieselbe Aufraeumfunktion, danach expliziter exit -- sonst liefe
# das Skript nach dem Signal weiter, weil ein Trap den Default-Abbruch ersetzt.
MOUNT_AKTIV=0
aufraeumen() {
  if [ "$MOUNT_AKTIV" = "1" ]; then
    hdiutil detach "$MNT" >/dev/null 2>&1 || hdiutil detach -force "$MNT" >/dev/null 2>&1 || true
    MOUNT_AKTIV=0
  fi
  rm -rf "$WORK"
}
trap aufraeumen EXIT
trap 'aufraeumen; exit 130' INT
trap 'aufraeumen; exit 143' TERM

log "Hintergrund (1x + 2x) und Fensterlage aus tauri.conf.json"
# tauri.conf.json nennt den Hintergrund relativ zu src-tauri/. Die Datei ist
# 2x (1320x800 fuer 660x400 Punkte); dmgbuild baut aus bg.png + bg@2x.png eine
# Retina-TIFF (tiffutil -cathidpicheck), dafuer braucht es die 1x-Fassung.
read -r BG_REL WIN_W WIN_H APP_X APP_Y APPS_X APPS_Y < <(python3 - "$TAURI_CONF" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))["bundle"]["macOS"]["dmg"]
print(d["background"], d["windowSize"]["width"], d["windowSize"]["height"],
      d["appPosition"]["x"], d["appPosition"]["y"],
      d["applicationFolderPosition"]["x"], d["applicationFolderPosition"]["y"])
PY
)
BG_SRC="$TAURI_DIR/$BG_REL"
[ -f "$BG_SRC" ] || { echo "FEHLER: Hintergrund fehlt: $BG_SRC" >&2; exit 1; }
BG_PX_W="$(sips -g pixelWidth "$BG_SRC" | awk '/pixelWidth/{print $2}')"
BG_PX_H="$(sips -g pixelHeight "$BG_SRC" | awk '/pixelHeight/{print $2}')"
if [ $((BG_PX_W % 2)) -ne 0 ] || [ $((BG_PX_H % 2)) -ne 0 ]; then
  echo "FEHLER: Hintergrund ${BG_PX_W}x${BG_PX_H} ist keine gerade 2x-Fassung." >&2
  exit 1
fi
cp "$BG_SRC" "$WORK/hintergrund@2x.png"
sips -z $((BG_PX_H / 2)) $((BG_PX_W / 2)) "$BG_SRC" --out "$WORK/hintergrund.png" >/dev/null
echo "Hintergrund ${BG_PX_W}x${BG_PX_H} -> $((BG_PX_W / 2))x$((BG_PX_H / 2)) Punkte; Fenster ${WIN_W}x${WIN_H}; App ${APP_X}/${APP_Y}; Programme ${APPS_X}/${APPS_Y}"

cat > "$WORK/einstellungen.py" <<EOF
# erzeugt von scripts/mitschnitt-dmg.sh
import os
application = defines["app"]
appname = os.path.basename(application)
files = [application]
symlinks = {"Applications": "/Applications"}
format = "UDZO"
filesystem = "HFS+"
background = defines["background"]
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
window_rect = ((200, 120), ($WIN_W, $WIN_H))
default_view = "icon-view"
icon_size = 128
text_size = 13
arrange_by = None
icon_locations = {
    appname: ($APP_X, $APP_Y),
    "Applications": ($APPS_X, $APPS_Y),
}
EOF

log "DMG packen: $DMG_OUT"
mkdir -p "$(dirname "$DMG_OUT")"
rm -f "$DMG_OUT"
"$VENV/bin/dmgbuild" \
  -s "$WORK/einstellungen.py" \
  -D app="$APP_SRC" \
  -D background="$WORK/hintergrund.png" \
  "$VOLNAME" "$DMG_OUT"
[ -s "$DMG_OUT" ] || { echo "FEHLER: $DMG_OUT ist leer oder fehlt." >&2; exit 1; }
hdiutil verify "$DMG_OUT" >/dev/null || { echo "FEHLER: hdiutil verify lehnt $DMG_OUT ab." >&2; exit 1; }

log "Inhalt pruefen (einhaengen ohne Finder, dann wieder aushaengen)"
mkdir -p "$MNT"
# MOUNT_AKTIV wird VOR "hdiutil attach" gesetzt, nicht danach: ein Abbruch
# GENAU zwischen "hdiutil attach ist fertig" und der naechsten Zeile faellt
# sonst durch das Netz, weil die Aufraeumfunktion beim Flag noch "0" liest und
# den Detach ueberspringt, waehrend der Kernel das Volume laengst gemountet
# hat (real reproduziert per kill -INT genau in diesem Fenster, 27.09.2026 --
# der anschliessende "rm -rf $WORK" lief dabei gegen ein noch gemountetes,
# read-only Volume und liess sowohl das Volume als auch $WORK zurueck). Ein
# Detach-Versuch auf einen Pfad, an dem (noch) nichts hängt, schlaegt ohnehin
# nur harmlos fehl und wird unten mit "|| true" verschluckt.
MOUNT_AKTIV=1
hdiutil attach -nobrowse -readonly -noautoopen -mountpoint "$MNT" "$DMG_OUT" >/dev/null
pruefen() {
  [ -d "$MNT/Mitschnitt.app" ] || { echo "FEHLER: Mitschnitt.app fehlt in der DMG." >&2; return 1; }
  [ "$(readlink "$MNT/Applications")" = "/Applications" ] \
    || { echo "FEHLER: Programme-Verknuepfung fehlt oder zeigt woanders hin." >&2; return 1; }
  [ -f "$MNT/.DS_Store" ] || { echo "FEHLER: .DS_Store (Fensterlage) fehlt." >&2; return 1; }
  ls "$MNT"/.background.* >/dev/null 2>&1 || { echo "FEHLER: Hintergrundbild fehlt." >&2; return 1; }
  # Das gepackte Bundle muss bitgleich zum Quell-Bundle sein -- eine DMG, die
  # die Signatur oder das angeheftete Ticket verliert, waere wertlos. "diff -rq"
  # deckt Inhalt und Dateinamen ab, nicht Modi/xattrs/Resource-Forks -- deshalb
  # zusaetzlich codesign/stapler/spctl direkt auf dem Bundle IN der DMG
  # (Forge-Zweitblick 27.09.2026: das war bisher unabgesichert).
  diff -rq "$APP_SRC" "$MNT/Mitschnitt.app" >/dev/null \
    || { echo "FEHLER: Bundle in der DMG weicht vom Quell-Bundle ab." >&2; return 1; }
  codesign --verify --deep --strict "$MNT/Mitschnitt.app" \
    || { echo "FEHLER: Signatur des Bundles in der DMG ist ungueltig." >&2; return 1; }
  if [ "$OHNE_NOTAR_PRUEFUNG" = "1" ]; then
    echo "Notarisierungs-Pruefung uebersprungen (MITSCHNITT_DMG_OHNE_NOTAR_PRUEFUNG=1 -- nur fuer Trockenlaeufe mit unsignierter/nicht notarisierter Testkopie)."
  else
    xcrun stapler validate "$MNT/Mitschnitt.app" \
      || { echo "FEHLER: Notarisierungs-Ticket am Bundle in der DMG ist ungueltig." >&2; return 1; }
    spctl -a -t exec -vv "$MNT/Mitschnitt.app" 2>&1 | grep -q "Notarized Developer ID" \
      || { echo "FEHLER: spctl meldet fuer das Bundle in der DMG kein 'Notarized Developer ID'." >&2; return 1; }
  fi
  ls -la "$MNT"
}
ERG=0
pruefen || ERG=1
for versuch in 1 2 3; do
  hdiutil detach "$MNT" >/dev/null 2>&1 && break
  sleep 2
  [ "$versuch" -eq 3 ] && hdiutil detach -force "$MNT" >/dev/null
done
MOUNT_AKTIV=0
[ "$ERG" -eq 0 ] || exit 1

log "Fertig"
ls -la "$DMG_OUT"
