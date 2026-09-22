#!/usr/bin/env bash
#
# Legt das SpeechCore-xcframework in den globalen SwiftPM-Artefakt-Cache.
#
# WARUM ES DAS GIBT
# -----------------
# crates/transcribe-soniqo/swift-lib haengt ueber soniqo/speech-swift an einem
# binaryTarget, das SwiftPM per URL nachlaedt. Diesen einen Download macht
# SwiftPM nicht mit git und nicht mit curl, sondern mit NSURLSession, also ueber
# den macOS-Systemdienst nsurlsessiond. Wenn der Dienst klemmt, haengt
# `swift package resolve` unbegrenzt an der Zeile
#
#     Downloading binary artifact https://github.com/soniqo/speech-core/...
#
# und zwar ohne Fehler, ohne Zeitgrenze und ohne dass je ein Socket aufgemacht
# wird (nachgemessen am 27.08.2026: 9 Minuten, 0 Bytes, `lsof -i` leer, waehrend
# dieselbe Datei per curl in 4,8 s da war). Weil cargo das OUT_DIR eines
# fehlgeschlagenen Build-Skripts wieder wegraeumt, faengt jeder Neuversuch von
# vorn an -- der Cache hier ist die einzige Schicht, die das ueberlebt.
#
# Weder ein Neustart von nsurlsessiond noch der Umweg ueber
# `xcodebuild -resolvePackageDependencies` haben geholfen; beide gehen durch
# dieselbe NSURLSession. Der Cache-Seed umgeht sie als einziges.
#
# Das Skript aendert nichts am Repo. Es legt nur die Datei ab, die SwiftPM sonst
# selbst geladen haette -- die Pruefsumme unten ist genau die, die
# speech-swift in seiner Package.swift deklariert, und SwiftPM prueft sie beim
# Entpacken noch einmal selbst nach.
#
# BENUTZUNG
#   ./scripts/seed-swiftpm-artifact-cache.sh
#
# Danach laeuft `swift package resolve` in ~40 s durch und meldet
# "Fetched ... from cache".
#
# Noetig auf jeder frischen Maschine und immer dann, wenn macOS
# ~/Library/Caches aufgeraeumt hat. Mehrfaches Ausfuehren schadet nicht.

set -euo pipefail

URL="https://github.com/soniqo/speech-core/releases/download/v0.0.6/SpeechCore.xcframework.zip"
CHECKSUM="aca6733cd04b873e1f7a428993e8d4f23ffceed42f7507cd1196c0b89d34f170"

CACHE_DIR="${HOME}/Library/Caches/org.swift.swiftpm/artifacts"
# SwiftPM benennt die Datei im Cache nach der URL und ersetzt dabei jedes
# Zeichen ausserhalb [A-Za-z0-9] durch einen Unterstrich.
CACHE_NAME="$(printf '%s' "$URL" | sed 's/[^a-zA-Z0-9]/_/g')"
CACHE_PATH="${CACHE_DIR}/${CACHE_NAME}"

verify() {
  [ -f "$1" ] && [ "$(shasum -a 256 "$1" | cut -d' ' -f1)" = "$CHECKSUM" ]
}

if verify "$CACHE_PATH"; then
  echo "Artefakt liegt bereits korrekt im Cache: $CACHE_PATH"
  exit 0
fi

mkdir -p "$CACHE_DIR"

TMP="$(mktemp -t SpeechCore)"
trap 'rm -f "$TMP"' EXIT

echo "Lade $URL"
curl -fsSL -o "$TMP" "$URL"

if ! verify "$TMP"; then
  echo "FEHLER: Pruefsumme passt nicht." >&2
  echo "  erwartet:  $CHECKSUM" >&2
  echo "  bekommen:  $(shasum -a 256 "$TMP" | cut -d' ' -f1)" >&2
  exit 1
fi

mv "$TMP" "$CACHE_PATH"
trap - EXIT

echo "Abgelegt unter: $CACHE_PATH"
echo "Pruefsumme bestaetigt: $CHECKSUM"
