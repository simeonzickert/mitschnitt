#!/bin/bash
# Erzeugt die Vorschaubilder fuer die App-Icon-Auswahl (Einstellungen ->
# Erscheinungsbild) aus dem freigegebenen Wellen-Icon. Nur auf macOS -- die
# Auswahl gibt es nur dort, und sips ist ein macOS-Werkzeug.
#
# Die Dock-/Finder-Icons selbst kommen NICHT von hier: sie liegen als
# icons/<kanal>/icon.icns (stable, dev, staging) im Baum, erzeugt mit
#   pnpm exec tauri icon src-tauri/icons/src/mitschnitt-welle.png -o src-tauri/icons/<kanal>
# und in tauri.conf.json unter bundle.icon, bundle.resources (icons/<kanal>.icns)
# und macOS.files (Resources/AppIcon.icns) verdrahtet.
#
# Bis zum 02.09.2026 backte dieses Skript zusaetzlich zehn geerbte Icons
# (actool aus stable.icon = das "a."-Logo des Originals, sips aus
# anarlog-<variante>.png). Alle weg (ZICK-278); es bleibt die Welle, und die
# ist in beiden Erscheinungsbildern dieselbe Datei.
#
# Vorhandene Ausgaben werden uebersprungen; `--force` erzeugt sie neu (nach
# einer Aenderung am Quellbild). Das Ergebnis liegt im Baum und wird
# eingecheckt -- sips ist nicht byte-deterministisch, deshalb kein
# Neu-Erzeugen bei jedem Bau.

set -euo pipefail

if [[ "$(uname)" != "Darwin" ]]; then
  echo "Skipping icon previews (not macOS)"
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC_TAURI="$(cd "$SCRIPT_DIR/.." && pwd)"
SOURCE_IMAGE="$SRC_TAURI/icons/src/mitschnitt-welle.png"
PREVIEW_DIR="$SRC_TAURI/../public/assets/app-icons"
FORCE="${1:-}"

# Muss mit AppIconName in apps/desktop/src/shared/theme/icon.ts uebereinstimmen.
VARIANTS=("stable" "dev" "staging")

if [[ ! -f "$SOURCE_IMAGE" ]]; then
  echo "Error: $SOURCE_IMAGE not found" >&2
  exit 1
fi

mkdir -p "$PREVIEW_DIR"

for variant in "${VARIANTS[@]}"; do
  preview_image="$PREVIEW_DIR/${variant}.png"

  if [[ -f "$preview_image" && "$FORCE" != "--force" ]]; then
    echo "Skipping $variant preview (already exists)"
    continue
  fi

  echo "Generating $variant preview..."
  sips -z 128 128 "$SOURCE_IMAGE" --out "$preview_image" >/dev/null
done

echo "Icon previews complete"
