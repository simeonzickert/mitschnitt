#!/usr/bin/env bash
# Mutationsprobe fuer die Transkript-Suche (ZICK-282).
#
# Jeder Mutant dreht GENAU EINE Verhaltenszeile um und muss mindestens einen
# Test toeten. Ueberlebt ein Mutant, prueft der zugehoerige Test nicht die
# Sache, die er zu pruefen behauptet.
#
# Aufruf:  bash scripts/mutanten-suche-transkript.sh
set -uo pipefail

REPO="${REPO:-$(cd "$(dirname "$0")/.." && pwd)}"
DESKTOP="${REPO}/apps/desktop"
INDEX="${DESKTOP}/src/session/components/note-input/index.tsx"
BAR="${DESKTOP}/src/session/components/note-input/search/bar.tsx"
SPRUNG="${DESKTOP}/src/session/components/note-input/transcript/renderer/scroll-to-word.ts"

SICHERUNG="$(mktemp -d)"
cp "${INDEX}" "${SICHERUNG}/index.tsx"
cp "${BAR}" "${SICHERUNG}/bar.tsx"
cp "${SPRUNG}" "${SICHERUNG}/scroll-to-word.ts"

VORHER_INDEX="$(shasum -a 256 "${INDEX}" | cut -d' ' -f1)"
VORHER_BAR="$(shasum -a 256 "${BAR}" | cut -d' ' -f1)"
VORHER_SPRUNG="$(shasum -a 256 "${SPRUNG}" | cut -d' ' -f1)"

wiederherstellen() {
  cp "${SICHERUNG}/index.tsx" "${INDEX}"
  cp "${SICHERUNG}/bar.tsx" "${BAR}"
  cp "${SICHERUNG}/scroll-to-word.ts" "${SPRUNG}"
}
# Laeuft auch bei Abbruch, nicht nur im Erfolgsfall.
trap wiederherstellen EXIT INT TERM

TESTS="src/session/components/note-input/index.test.tsx src/session/components/note-input/search/bar.test.tsx src/session/components/note-input/transcript/renderer/scroll-to-word.test.ts"

ueberlebende=0

probe() {
  local name="$1"
  local datei="$2"
  local alt="$3"
  local neu="$4"

  wiederherstellen

  if ! grep -qF -- "${alt}" "${datei}"; then
    echo "!! ${name}: Ankertext nicht gefunden -- Mutant wurde NICHT gesetzt"
    ueberlebende=$((ueberlebende + 1))
    return
  fi

  python3 - "${datei}" "${alt}" "${neu}" <<'PY'
import sys
pfad, alt, neu = sys.argv[1], sys.argv[2], sys.argv[3]
with open(pfad, encoding="utf-8") as f:
    inhalt = f.read()
if inhalt.count(alt) != 1:
    sys.exit("Ankertext ist nicht eindeutig: %d Treffer" % inhalt.count(alt))
with open(pfad, "w", encoding="utf-8") as f:
    f.write(inhalt.replace(alt, neu))
PY
  if [ $? -ne 0 ]; then
    echo "!! ${name}: Mutant konnte nicht gesetzt werden"
    ueberlebende=$((ueberlebende + 1))
    return
  fi

  # Kein `| tail` -- das wuerde den Exit-Code verschlucken.
  local ausgabe
  ausgabe="$(env -C "${DESKTOP}" pnpm vitest run ${TESTS} 2>&1)"
  local code=$?

  if [ ${code} -eq 0 ]; then
    echo "UEBERLEBT  ${name}  <-- der Test prueft die Sache nicht"
    ueberlebende=$((ueberlebende + 1))
  else
    echo "tot        ${name}"
  fi
}

probe "M1 Suchleiste im Transkript wieder gesperrt" \
  "${INDEX}" \
  "const canSearchTab = isEditableTab || isTranscriptTab;" \
  "const canSearchTab = isEditableTab;"

probe "M2 Suchleiste ueberall erlaubt (auch Anhaenge)" \
  "${INDEX}" \
  "const canSearchTab = isEditableTab || isTranscriptTab;" \
  "const canSearchTab = true;"

probe "M3 Sichtbarkeit der Leiste ignoriert" \
  "${INDEX}" \
  "{showSearchBar && canSearchTab && (" \
  "{canSearchTab && ("

probe "M4 Ersetzen auch ohne Editor durchgereicht" \
  "${INDEX}" \
  "allowReplace={isEditableTab}" \
  "allowReplace={true}"

probe "M5 Ersetzen-Schalter immer gerendert" \
  "${BAR}" \
  "          {allowReplace && (" \
  "          {true && ("

probe "M6 Ersetzen-Zeile immer gerendert" \
  "${BAR}" \
  "{allowReplace && showReplace && (" \
  "{showReplace && ("

probe "M7 Sichtbarkeit erst NACH dem Segment-Scroll gemessen" \
  "${SPRUNG}" \
  "    const bereitsSichtbar = isWordCurrentlyVisible(container, wordId);" \
  "    let bereitsSichtbar = false;"

probe "M8 Waechter aus: justiert auch bei sichtbarem Treffer nach" \
  "${SPRUNG}" \
  "    if (bereitsSichtbar) return;" \
  "    if (false) return;"

probe "M9 nur EIN Frame warten statt bis zum Rendern" \
  "${SPRUNG}" \
  "    if (verbleibend > 0) {
      requestFrame(versuch);
    }" \
  "    if (false) {
      requestFrame(versuch);
    }"

probe "M10 Versuchslimit aufgehoben" \
  "${SPRUNG}" \
  "    verbleibend -= 1;" \
  "    verbleibend -= 0;"

probe "M11 Sichtbarkeitsgrenze am unteren Rand verschoben" \
  "${SPRUNG}" \
  "    wordRect.top >= containerRect.top && wordRect.bottom <= containerRect.bottom" \
  "    wordRect.top >= containerRect.top"

wiederherstellen

NACHHER_INDEX="$(shasum -a 256 "${INDEX}" | cut -d' ' -f1)"
NACHHER_BAR="$(shasum -a 256 "${BAR}" | cut -d' ' -f1)"
NACHHER_SPRUNG="$(shasum -a 256 "${SPRUNG}" | cut -d' ' -f1)"

echo
echo "Pruefsumme index.tsx  vorher ${VORHER_INDEX}"
echo "Pruefsumme index.tsx  nachher ${NACHHER_INDEX}"
echo "Pruefsumme bar.tsx    vorher ${VORHER_BAR}"
echo "Pruefsumme bar.tsx    nachher ${NACHHER_BAR}"
echo "Pruefsumme scroll-to-word.ts vorher ${VORHER_SPRUNG}"
echo "Pruefsumme scroll-to-word.ts nachher ${NACHHER_SPRUNG}"

if [ "${VORHER_INDEX}" != "${NACHHER_INDEX}" ] ||
   [ "${VORHER_BAR}" != "${NACHHER_BAR}" ] ||
   [ "${VORHER_SPRUNG}" != "${NACHHER_SPRUNG}" ]; then
  echo "FEHLER: Arbeitskopie ist nicht unversehrt zurueck."
  exit 2
fi

if [ ${ueberlebende} -ne 0 ]; then
  echo "FEHLER: ${ueberlebende} Mutant(en) ueberlebt."
  exit 1
fi

echo "Alle Mutanten tot, Dateien unversehrt."
