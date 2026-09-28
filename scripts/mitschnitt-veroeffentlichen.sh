#!/usr/bin/env bash
#
# Mitschnitt: Release veroeffentlichen (Bausteine zusammenfuehren, hochladen).
#
# Dieses Skript uebernimmt den Hochlade-Block aus dem Release-Skript und
# arbeitet mit einer dynamischen Dateiliste statt festen vier Dateien.
#
# Aufruf:
#   scripts/mitschnitt-veroeffentlichen.sh <version> [--erwarte <keys>] [--trocken] [--ohne-apple-silicon] [--erlaube-testschluessel]
#
#   <version>      z.B. 0.2.0
#   --erwarte      Komma-separierte Liste erwarteter Plattform-Keys (z.B.
#                  "darwin-aarch64,windows-x86_64")
#   --trocken      Nur die geplanten Dateien ausgeben, nichts hochladen
#   --ohne-apple-silicon  Kein Abbruch, wenn darwin-aarch64 fehlt
#   --erlaube-testschluessel  Erlaubt Testschluessel im Feed (nur mit --trocken)
#
# Umgebungsvariablen:
#   MITSCHNITT_RELEASE_DIR (Pflicht) - Zielordner fuer die Release-Ausgabe
#   GH_TOKEN oder gh-CLI-Authentifizierung (Pflicht, ausser --trocken)

set -euo pipefail

if [ $# -lt 1 ]; then
  echo "FEHLER: Aufruf ist scripts/mitschnitt-veroeffentlichen.sh <version> [--erwarte <keys>] [--trocken] [--ohne-apple-silicon] [--erlaube-testschluessel]" >&2
  exit 2
fi
VERSION="$1"
shift
ERWARTE=""
TROCKEN=0
OHNE_APPLE_SILICON=0
ERLAUBE_TESTSCHLUESSEL=0
# while/shift statt for: --erwarte nimmt ein zweites Argument, das eine
# for-Schleife sonst als eigenen (unbekannten) Schalter liest.
while [ $# -gt 0 ]; do
  case "$1" in
    --erwarte)
      [ $# -ge 2 ] || { echo "FEHLER: --erwarte braucht eine Liste (z.B. darwin-aarch64,windows-x86_64)" >&2; exit 2; }
      ERWARTE="$2"; shift 2 ;;
    --erwarte=*) ERWARTE="${1#--erwarte=}"; shift ;;
    --trocken) TROCKEN=1; shift ;;
    --ohne-apple-silicon) OHNE_APPLE_SILICON=1; shift ;;
    --erlaube-testschluessel) ERLAUBE_TESTSCHLUESSEL=1; shift ;;
    *) echo "FEHLER: unbekannter Schalter '$1' (erlaubt: --erwarte, --trocken, --ohne-apple-silicon, --erlaube-testschluessel)" >&2; exit 2 ;;
  esac
done

# Echter Semver-Regex wie im Release-Skript
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "FEHLER: Version '$VERSION' sieht nicht wie X.Y.Z aus." >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_ROOT="${MITSCHNITT_RELEASE_DIR:?MITSCHNITT_RELEASE_DIR setzen (Zielordner fuer die Release-Ausgabe)}"
OUT_DIR="$OUT_ROOT/v$VERSION"
# Aus der URL-Form abgeleitet: nur "github.com/<konto>/mitschnitt" steht auf
# der Freigabeliste von scripts/namens-scrub-check.sh.
RELEASE_URL="https://github.com/simeonzickert/mitschnitt"
RELEASE_REPO="${RELEASE_URL#https://github.com/}"

# gh-CLI nur pruefen, wenn nicht --trocken
if [ "$TROCKEN" != "1" ] && ! command -v gh >/dev/null 2>&1; then
  echo "FEHLER: gh-CLI ist nicht installiert (ausser --trocken)." >&2
  exit 1
fi

log() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }

log "Bausteine zusammenfuehren"
# --erwarte nur uebergeben, wenn gesetzt
ERWARTE_ARGS=()
if [ -n "$ERWARTE" ]; then
  ERWARTE_ARGS=(--erwarte "$ERWARTE")
fi
# --erlaube-testschluessel an zusammenfuehren durchreichen
TESTSCHLUESSEL_ARGS=()
if [ "$ERLAUBE_TESTSCHLUESSEL" = "1" ]; then
  TESTSCHLUESSEL_ARGS=(--erlaube-testschluessel)
fi
# Pruefen: --erlaube-testschluessel nur mit --trocken erlaubt
if [ "$ERLAUBE_TESTSCHLUESSEL" = "1" ] && [ "$TROCKEN" != "1" ]; then
  echo "FEHLER: --erlaube-testschluessel ist nur im Trockenlauf (--trocken) erlaubt." >&2
  exit 1
fi

# stdout des Python-Skripts auslesen: pro Zeile "plattform\tasset\tbytes"
# In bash 3.2 keine assoziativen Arrays, deshalb Zeilenweise in einer Liste
# speichern und spaeter durchgehen.
# Wichtig: python3 NUR EINMAL aufrufen, stdout einfangen.
BAUSTEIN_AUSGABE="$(python3 "$REPO_ROOT/scripts/mitschnitt-feed.py" zusammenfuehren \
  --ausgabe "$OUT_DIR" \
  "${ERWARTE_ARGS[@]+"${ERWARTE_ARGS[@]}"}" \
  "${TESTSCHLUESSEL_ARGS[@]+"${TESTSCHLUESSEL_ARGS[@]}"}")"

BAUSTEIN_ZEILEN=()
while IFS= read -r zeile; do
  BAUSTEIN_ZEILEN+=("$zeile")
done <<< "$BAUSTEIN_AUSGABE"

# Plattformen aus den Baustein-Zeilen extrahieren
PLATTFORMEN=()
for zeile in "${BAUSTEIN_ZEILEN[@]+"${BAUSTEIN_ZEILEN[@]}"}"; do
  IFS=$'\t' read -r plattform asset bytes <<< "$zeile"
  PLATTFORMEN+=("$plattform")
done

echo "Enthaltene Plattformen: ${PLATTFORMEN[*]}"

# darwin-aarch64 ist Pflicht, ausser mit --ohne-apple-silicon
HAT_DARWIN_AARCH64=0
for p in "${PLATTFORMEN[@]+"${PLATTFORMEN[@]}"}"; do
  if [ "$p" = "darwin-aarch64" ]; then
    HAT_DARWIN_AARCH64=1
    break
  fi
done
if [ "$HAT_DARWIN_AARCH64" != "1" ]; then
  if [ "$OHNE_APPLE_SILICON" = "1" ]; then
    echo "WARNUNG: darwin-aarch64 fehlt im Release (durch --ohne-apple-silicon erlaubt)." >&2
  else
    echo "FEHLER: darwin-aarch64 fehlt im Release. Bestehende Apple-Silicon-Macs" >&2
    echo "        bekommen dann kein Update ueber den Tauri-Updater." >&2
    echo "        Mit --ohne-apple-silicon kann dieser Fehler uebergangen werden." >&2
    exit 1
  fi
fi

log "Hochzuladende Dateien ermitteln"
# Liste der hochzuladenden Dateien: asset + .sig pro Plattform, latest.json,
# DMGs exakt nach Version benannt
HOCHLADE_DATEIEN=()

# latest.json muss existieren und nicht leer sein
LATEST_JSON="$OUT_DIR/latest.json"
if [ ! -f "$LATEST_JSON" ] || [ ! -s "$LATEST_JSON" ]; then
  echo "FEHLER: $LATEST_JSON existiert nicht oder ist leer." >&2
  exit 1
fi
HOCHLADE_DATEIEN+=("$LATEST_JSON")

# Release-Notiz erstellen (auch im Trockenlauf)
log "Release-Notiz erstellen"
RELEASE_NOTES="$OUT_DIR/release-notes.md"

# Platzhalter-Werte sammeln
# DOWNLOADS: je vorhandener Plattform eine Markdown-Listenzeile
DOWNLOADS=""
HAT_DARWIN_X86_64=0
HAT_WINDOWS=0
for zeile in "${BAUSTEIN_ZEILEN[@]+"${BAUSTEIN_ZEILEN[@]}"}"; do
  IFS=$'\t' read -r plattform asset bytes <<< "$zeile"
  if [ "$plattform" = "darwin-aarch64" ]; then
    DOWNLOADS="${DOWNLOADS}- **macOS, Apple silicon (M1 or newer):** [Mitschnitt-${VERSION}.dmg](https://github.com/${RELEASE_REPO}/releases/download/v${VERSION}/Mitschnitt-${VERSION}.dmg)
"
  elif [ "$plattform" = "darwin-x86_64" ]; then
    HAT_DARWIN_X86_64=1
    DOWNLOADS="${DOWNLOADS}- **macOS, Intel:** [Mitschnitt-${VERSION}-Intel.dmg](https://github.com/${RELEASE_REPO}/releases/download/v${VERSION}/Mitschnitt-${VERSION}-Intel.dmg). Cloud transcription only, local transcription needs Apple silicon.
"
  elif [ "$plattform" = "windows-x86_64" ]; then
    HAT_WINDOWS=1
    DOWNLOADS="${DOWNLOADS}- **Windows 10/11 (64-bit):** [Mitschnitt_${VERSION}_x64-setup.exe](https://github.com/${RELEASE_REPO}/releases/download/v${VERSION}/Mitschnitt_${VERSION}_x64-setup.exe). Cloud transcription only. The installer is not signed by Microsoft, so Windows shows a SmartScreen warning the first time: click *More info*, then *Run anyway*.
"
  fi
done

# UPDATE_FILES: kommagetrennte Liste in Backticks aller Update-Dateien
UPDATE_FILES=""
# latest.json immer dabei
UPDATE_FILES="\`latest.json\`"
for zeile in "${BAUSTEIN_ZEILEN[@]+"${BAUSTEIN_ZEILEN[@]}"}"; do
  IFS=$'\t' read -r plattform asset bytes <<< "$zeile"
  # Der Windows-Installer ist zugleich der Download fuer Menschen (steht oben
  # unter Downloads) -- nur seine .sig gehoert zu den "nicht herunterladen"-Dateien.
  if [ "$plattform" = "windows-x86_64" ]; then
    UPDATE_FILES="${UPDATE_FILES}, \`${asset}.sig\`"
  else
    UPDATE_FILES="${UPDATE_FILES}, \`${asset}\`, \`${asset}.sig\`"
  fi
done

# CHANGELOG und NOTE aus CHANGELOG.md extrahieren
# what-new ist Pflicht (Changelog-Pflicht seit 28.09.2026): ohne ihn truege
# die Release-Seite nur den Platzhalter. note ist optional.
if ! CHANGELOG_TEXT="$("$REPO_ROOT/scripts/changelog-extract.sh" "$REPO_ROOT/CHANGELOG.md" "$VERSION" what-new)"; then
  echo "FEHLER: CHANGELOG.md hat keinen (oder einen leeren) Abschnitt fuer $VERSION." >&2
  exit 1
fi
NOTE_TEXT="$("$REPO_ROOT/scripts/changelog-extract.sh" "$REPO_ROOT/CHANGELOG.md" "$VERSION" note 2>/dev/null || true)"

# Release-Notiz mit python3 erstellen (wie im Release-Skript)
python3 - "$REPO_ROOT/docs/release-notes-template.md" "$RELEASE_NOTES" "$VERSION" "$DOWNLOADS" "$UPDATE_FILES" "$CHANGELOG_TEXT" "$NOTE_TEXT" <<'PY'
import sys
tpl, out, version, downloads, update_files, changelog, note = sys.argv[1:8]
text = open(tpl).read()
# Kommentarblock der Vorlage (zwischen <!-- und -->) nicht mitveroeffentlichen.
while "<!--" in text:
    a = text.index("<!--")
    b = text.index("-->", a) + 3
    text = text[:a] + text[b:]
for key, val in {"{{VERSION}}": version, "{{DOWNLOADS}}": downloads,
                 "{{UPDATE_FILES}}": update_files}.items():
    text = text.replace(key, val)
# {{NOTE}} als Markdown-Blockquote einsetzen (jede Zeile mit "> " davor,
# eine leere Zeile im Hinweis wird zu einer blossen ">" -- gaengige Konvention
# fuer mehrzeilige Blockquotes). Traegt die Version keinen Note-Abschnitt,
# ist note="" und note_block bleibt leer, die Zeile verschwindet einfach.
if note:
    note_block = "\n".join("> " + zeile if zeile else ">" for zeile in note.split("\n"))
else:
    note_block = ""
# Pruefen VOR dem Einsetzen von Changelog/Note: deren Text darf selbst "{{" tragen.
platzhalter_frei = text.replace("{{CHANGELOG}}", "").replace("{{NOTE}}", "")
if "{{" in platzhalter_frei:
    sys.exit("FEHLER: unaufgeloester Platzhalter in der Release-Notiz")
text = text.replace("{{CHANGELOG}}", changelog).replace("{{NOTE}}", note_block)
open(out, "w").write(text.lstrip())
PY

if [ ! -f "$RELEASE_NOTES" ] || [ ! -s "$RELEASE_NOTES" ]; then
  echo "FEHLER: $RELEASE_NOTES konnte nicht erstellt werden." >&2
  exit 1
fi

# Pruefen, ob fuer den Tag schon ein Release ODER Entwurf existiert
log "Pruefen, ob Release v$VERSION bereits existiert"
# Im Trockenlauf nur Hinweis: er laedt nichts hoch und soll auch gegen eine
# schon veroeffentlichte Version probelaufen koennen.
if [ "$TROCKEN" = "1" ]; then
  if command -v gh >/dev/null 2>&1 && gh release view "v$VERSION" --repo "$RELEASE_REPO" >/dev/null 2>&1; then
    echo "HINWEIS (Trockenlauf): Release v$VERSION existiert bereits; ein echter Lauf wuerde hier abbrechen." >&2
  fi
elif gh release view "v$VERSION" --repo "$RELEASE_REPO" >/dev/null 2>&1; then
  echo "FEHLER: Release v$VERSION existiert bereits (als Entwurf oder veroeffentlicht)." >&2
  echo "        Bitte den alten Entwurf von Hand loeschen:" >&2
  echo "        gh release delete \"v$VERSION\" --repo \"$RELEASE_REPO\" --yes" >&2
  exit 1
fi

# Pro Plattform: asset und .sig
for zeile in "${BAUSTEIN_ZEILEN[@]+"${BAUSTEIN_ZEILEN[@]}"}"; do
  IFS=$'\t' read -r plattform asset asset_bytes <<< "$zeile"
  asset_path="$OUT_DIR/$asset"
  sig_path="${asset_path}.sig"

  # Asset muss existieren und nicht leer sein
  if [ ! -f "$asset_path" ] || [ ! -s "$asset_path" ]; then
    echo "FEHLER: Asset '$asset_path' existiert nicht oder ist leer." >&2
    exit 1
  fi

  # Groesse des asset muss zu asset_bytes aus dem Baustein passen
  tatsaechliche_bytes="$(stat -f%z "$asset_path" 2>/dev/null || stat -c%s "$asset_path" 2>/dev/null)"
  if [ "$tatsaechliche_bytes" != "$asset_bytes" ]; then
    echo "FEHLER: Asset '$asset' hat $tatsaechliche_bytes Bytes, Baustein sagt $asset_bytes." >&2
    echo "        Datei wurde nach dem Baustein veraendert." >&2
    exit 1
  fi

  HOCHLADE_DATEIEN+=("$asset_path")

  # .sig muss existieren und nicht leer sein
  if [ ! -f "$sig_path" ] || [ ! -s "$sig_path" ]; then
    echo "FEHLER: Signatur '$sig_path' existiert nicht oder ist leer." >&2
    exit 1
  fi
  HOCHLADE_DATEIEN+=("$sig_path")
done

# DMGs exakt nach Version benannt
# darwin-aarch64: Mitschnitt-$VERSION.dmg (Pflicht, wenn darwin-aarch64 dabei ist)
if [ "$HAT_DARWIN_AARCH64" = "1" ]; then
  DMG_AARCH64="$OUT_DIR/Mitschnitt-$VERSION.dmg"
  if [ ! -f "$DMG_AARCH64" ] || [ ! -s "$DMG_AARCH64" ]; then
    echo "FEHLER: $DMG_AARCH64 existiert nicht oder ist leer (Pflicht fuer darwin-aarch64)." >&2
    exit 1
  fi
  HOCHLADE_DATEIEN+=("$DMG_AARCH64")
fi

# darwin-x86_64: Mitschnitt-$VERSION-Intel.dmg (Pflicht, wenn darwin-x86_64 dabei ist)
if [ "$HAT_DARWIN_X86_64" = "1" ]; then
  DMG_X86_64="$OUT_DIR/Mitschnitt-$VERSION-Intel.dmg"
  if [ ! -f "$DMG_X86_64" ] || [ ! -s "$DMG_X86_64" ]; then
    echo "FEHLER: $DMG_X86_64 existiert nicht oder ist leer (Pflicht fuer darwin-x86_64)." >&2
    exit 1
  fi
  HOCHLADE_DATEIEN+=("$DMG_X86_64")
fi

# --trocken: nur ausgeben, nichts hochladen
if [ "$TROCKEN" = "1" ]; then
  echo
  echo "=== TROCKENLAUF ==="
  echo "Geplante Dateien fuer v$VERSION:"
  for datei in "${HOCHLADE_DATEIEN[@]+"${HOCHLADE_DATEIEN[@]}"}"; do
    groesse="$(stat -f%z "$datei" 2>/dev/null || stat -c%s "$datei" 2>/dev/null)"
    echo "  $datei ($groesse Bytes)"
  done
  echo
  echo "latest.json:"
  cat "$LATEST_JSON"
  echo
  echo "Release-Notiz:"
  cat "$RELEASE_NOTES"
  exit 0
fi

log "Als Entwurf anlegen und alle Dateien hochladen ($RELEASE_REPO, v$VERSION)"
# Erst als Entwurf anlegen, ALLE Dateien hochladen, dann vollstaendig
# verifizieren -- und nur bei Erfolg veroeffentlichen. "gh release create"
# mit direktem "--latest" veroeffentlicht sofort und laedt danach einzeln
# hoch; scheitert z.B. der letzte Upload (die DMG), bliebe ein als "latest"
# markiertes Release mit Updater-Dateien, aber ohne DMG zurueck, und die
# Release-Notiz zeigte auf eine nicht vorhandene Datei (Forge-Zweitblick
# 27.09.2026). Ein Entwurf ist dagegen nicht oeffentlich sichtbar, bis wir
# ihn explizit veroeffentlichen. "gh release view/edit <tag>" findet einen
# Entwurf ueber denselben Tag-Namen zuverlaessig (gh loest einen Tag-Namen
# ueber eine parallele REST- UND GraphQL-Abfrage auf, letztere greift genau
# fuer noch unveroeffentlichte Entwuerfe -- geprueft gegen den gh-Quelltext,
# nicht nur vermutet).
if ! gh release create "v$VERSION" \
    "${HOCHLADE_DATEIEN[@]+"${HOCHLADE_DATEIEN[@]}"}" \
    --repo "$RELEASE_REPO" \
    --title "v$VERSION" \
    --notes-file "$RELEASE_NOTES" \
    --draft; then
  echo "FEHLER: Entwurf v$VERSION liess sich nicht vollstaendig anlegen (Upload gescheitert?)." >&2
  echo "        Ein teilweise hochgeladener Entwurf bleibt dabei auf GitHub stehen, nichts wurde" >&2
  echo "        veroeffentlicht. Pruefen: gh release view \"v$VERSION\" --repo \"$RELEASE_REPO\"" >&2
  exit 1
fi

log "Entwurf pruefen: Anzahl und Groesse jeder Datei"
if ! ASSETS_TSV="$(gh release view "v$VERSION" --repo "$RELEASE_REPO" \
    --json assets --jq '.assets[] | "\(.name)\t\(.size)"')"; then
  echo "FEHLER: Entwurf v$VERSION liess sich nicht auslesen (gh release view)." >&2
  echo "        Der Entwurf bleibt auf GitHub stehen, nichts wurde veroeffentlicht." >&2
  exit 1
fi

ANZAHL_ASSETS="$(printf '%s\n' "$ASSETS_TSV" | grep -c . || true)"
ERWARTETE_ANZAHL="${#HOCHLADE_DATEIEN[@]}"
ASSET_PRUEFUNG_OK=1

if [ "$ANZAHL_ASSETS" != "$ERWARTETE_ANZAHL" ]; then
  echo "FEHLER: Entwurf traegt $ANZAHL_ASSETS Assets, erwartet werden $ERWARTETE_ANZAHL." >&2
  ASSET_PRUEFUNG_OK=0
fi

# In bash 3.2 keine assoziativen Arrays, deshalb mit einer temporaeren Datei
# arbeiten oder durch die Liste iterieren. Hier: eine temporaere Datei fuer
# die gefundenen Assets.
TMP_ASSETS="$(mktemp -t mitschnitt-release-assets-XXXXXX)"
trap 'rm -f "$TMP_ASSETS"' EXIT
printf '%s\n' "$ASSETS_TSV" > "$TMP_ASSETS"

for datei in "${HOCHLADE_DATEIEN[@]+"${HOCHLADE_DATEIEN[@]}"}"; do
  datei_name="$(basename "$datei")"
  erwartete_groesse="$(stat -f%z "$datei" 2>/dev/null || stat -c%s "$datei" 2>/dev/null)"
  # In der TSV-Datei nach dem Dateinamen suchen (exakt, nicht per Teilstring)
  gefundene_zeile="$(awk -F'\t' -v n="$datei_name" '$1==n{print $2}' "$TMP_ASSETS" || true)"
  if [ -z "$gefundene_zeile" ]; then
    echo "FEHLER: Asset '$datei_name' fehlt im Entwurf." >&2
    ASSET_PRUEFUNG_OK=0
  else
    gef_groesse="$gefundene_zeile"
    if [ "$gef_groesse" != "$erwartete_groesse" ]; then
      echo "FEHLER: Asset '$datei_name' hat $gef_groesse Bytes, erwartet $erwartete_groesse." >&2
      ASSET_PRUEFUNG_OK=0
    fi
  fi
done

if [ "$ASSET_PRUEFUNG_OK" != "1" ]; then
  echo "FEHLER: Entwurf v$VERSION bleibt ein Draft auf GitHub -- NICHT veroeffentlicht." >&2
  echo "        Fehlende/falsche Datei nachschieben: gh release upload \"v$VERSION\" <Datei> --repo \"$RELEASE_REPO\" --clobber" >&2
  echo "        Danach diesen Lauf erneut versuchen, oder von Hand veroeffentlichen:" >&2
  echo "        gh release edit \"v$VERSION\" --repo \"$RELEASE_REPO\" --draft=false --latest" >&2
  exit 1
fi

log "Vollstaendig -- veroeffentlichen und als 'latest' markieren"
gh release edit "v$VERSION" --repo "$RELEASE_REPO" --draft=false --latest

echo
echo "Release v$VERSION erfolgreich veroeffentlicht:"
echo "  https://github.com/$RELEASE_REPO/releases/tag/v$VERSION"
