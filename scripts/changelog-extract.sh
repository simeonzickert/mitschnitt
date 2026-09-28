#!/usr/bin/env bash
#
# Liest den Abschnitt einer Version aus CHANGELOG.md aus. Eigenstaendig
# aufrufbar (fuer Proben/Tests) und von scripts/mitschnitt-release.sh genutzt,
# damit die GitHub-Release-Notiz und latest.json nie mehr auf den Platzhalter
# "Mitschnitt X.Y.Z" zurueckfallen -- genau das ist 0.1.7 und 0.1.8 passiert,
# weil es fuer beide Versionen keinen Changelog-Eintrag gab (Entscheid
# 28.09.2026: "die releasenotes musst du viel besser pflegen!").
#
# Erwartetes Format in CHANGELOG.md, ein Abschnitt pro Version:
#
#   ## 0.1.8 (2026-09-28)
#
#   ### Note
#   <optionaler Hinweis, z.B. eine Handlungsanweisung wie "einmal von Hand
#   installieren">
#
#   ### What's new
#   - Stichpunkt
#   - Stichpunkt
#
# "### Note" ist optional. "### What's new" ist PFLICHT und darf nicht leer
# sein -- fehlt der Versions-Abschnitt komplett oder ist "What's new" leer,
# brechen ALLE drei Felder unten mit Exit 1 ab (auch "note"), damit ein
# fehlender Changelog-Eintrag nicht erst unbemerkt beim Feld "note" durchrutscht.
#
# Die Versions-Erkennung ist eine exakte Zeichenkettenprobe auf das, was
# zwischen "## " und " (" steht -- keine Regex uebers ganze Changelog, damit
# ein Punkt in der Versionsnummer nicht als Regex-Sonderzeichen durchschlaegt.
#
# Aufruf:
#   scripts/changelog-extract.sh <changelog-datei> <version> <feld>
#
#   <feld>  what-new     -- Markdown-Koerper von "### What's new", fuer die
#                            GitHub-Release-Notiz ({{CHANGELOG}})
#           note         -- Markdown-Koerper von "### Note", leer (Exit 0,
#                            keine Ausgabe) wenn der Abschnitt keinen hat
#                            ({{NOTE}}, wird dann aus der Vorlage entfernt)
#           notes-plain  -- "What's new" als Klartext ohne Markdown-Links und
#                            ohne Fett-/Kursiv-Sternchen, fuer latest.json
#                            "notes" (der Tauri-Updater-Dialog rendert kein
#                            Markdown)
#
# Beispiel-Proben:
#   scripts/changelog-extract.sh CHANGELOG.md 0.1.8 what-new
#   scripts/changelog-extract.sh CHANGELOG.md 0.1.8 note
#   scripts/changelog-extract.sh CHANGELOG.md 0.1.8 notes-plain
#   scripts/changelog-extract.sh CHANGELOG.md 0.1.99 what-new   # muss Exit 1 sein

set -euo pipefail

if [ $# -ne 3 ]; then
  echo "FEHLER: Aufruf ist scripts/changelog-extract.sh <changelog-datei> <version> <feld>" >&2
  echo "        Feld ist eines von: what-new, note, notes-plain" >&2
  exit 2
fi
DATEI="$1"
VERSION="$2"
FELD="$3"

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "FEHLER: Version '$VERSION' sieht nicht wie X.Y.Z aus." >&2
  exit 2
fi
case "$FELD" in
  what-new | note | notes-plain) ;;
  *)
    echo "FEHLER: unbekanntes Feld '$FELD' (erlaubt: what-new, note, notes-plain)" >&2
    exit 2
    ;;
esac
[ -f "$DATEI" ] || { echo "FEHLER: $DATEI existiert nicht." >&2; exit 2; }

# Fuehrende und folgende Leerzeilen kappen (klassische sed-Idiome), damit
# ein Abschnitt mit Leerzeile direkt nach der Ueberschrift nicht mit einer
# leeren Zeile beginnt und die Leerzeilenpruefung unten trotzdem greift.
trim_leerzeilen() {
  sed -e '/./,$!d' | sed -e :a -e '/^\n*$/{$d;N;};/\n$/ba'
}

# Schritt 1: den vollen Koerper des Versions-Abschnitts isolieren (alle
# Zeilen zwischen der Ueberschrift "## <version> (" und der naechsten
# "## "-Ueberschrift oder Dateiende). Exit 3 aus awk, wenn die Version nicht
# gefunden wurde -- eigener Code, damit der Aufrufer unten gezielt darauf
# reagieren kann statt jeden awk-Fehler gleich zu behandeln.
if ! ABSCHNITT="$(awk -v ver="$VERSION" '
    BEGIN { aktiv = 0; gefunden = 0; fertig = 0 }
    /^## / {
      zeile = $0
      sub(/^## /, "", zeile)
      p = index(zeile, " (")
      v = (p > 0) ? substr(zeile, 1, p - 1) : zeile
      if (aktiv) { aktiv = 0; fertig = 1 }
      else if (!fertig && v == ver) { aktiv = 1; gefunden = 1 }
      next
    }
    aktiv { print }
    END { exit (gefunden ? 0 : 3) }
  ' "$DATEI")"; then
  echo "FEHLER: CHANGELOG.md hat keinen Abschnitt fuer $VERSION (erwartet eine Zeile '## $VERSION (JJJJ-MM-TT)')." >&2
  exit 1
fi

# Schritt 2: innerhalb des isolierten Abschnitts eine "### <ueberschrift>"-
# Unterueberschrift herausziehen (alle Zeilen bis zur naechsten "### "-Zeile
# oder Abschnittsende). Fehlt sie, ist die Ausgabe einfach leer -- das ist
# fuer "note" gueltig (optional) und wird fuer "what-new" unten geprueft.
subabschnitt_holen() {
  local ueberschrift="$1"
  awk -v hdr="$ueberschrift" '
    BEGIN { aktiv = 0 }
    /^### / {
      zeile = $0
      sub(/^### /, "", zeile)
      aktiv = (zeile == hdr)
      next
    }
    aktiv { print }
  '
}

WHAT_NEW="$(printf '%s\n' "$ABSCHNITT" | subabschnitt_holen "What's new" | trim_leerzeilen)"
if [ -z "$WHAT_NEW" ]; then
  echo "FEHLER: CHANGELOG.md-Abschnitt fuer $VERSION hat kein (oder ein leeres) \"### What's new\"." >&2
  exit 1
fi

case "$FELD" in
  what-new)
    printf '%s\n' "$WHAT_NEW"
    ;;
  note)
    # "if" statt "[ -n ... ] && printf": bei leerem NOTE (0.1.7 hat keinen
    # "### Note") ist "[ -n \"\" ]" falsch, und als letzter Befehl des
    # Skripts wuerde dessen Exit-Code sonst der Exit-Code des ganzen Skripts
    # -- der leere Fall soll aber Exit 0 sein, kein Fehler (gemessen: ohne
    # "if" bricht das Skript hier mit Exit 1 ab, obwohl "kein Note" gueltig
    # ist).
    NOTE="$(printf '%s\n' "$ABSCHNITT" | subabschnitt_holen "Note" | trim_leerzeilen)"
    if [ -n "$NOTE" ]; then
      printf '%s\n' "$NOTE"
    fi
    ;;
  notes-plain)
    # Links [Text](URL) -> Text, dann **fett**/__fett__ und danach das
    # verbleibende einzelne *kursiv*/_kursiv_ entfernen. Kein Rueckverweis
    # (\1) im SUCHmuster: BSD/macOS-sed unterstuetzt das im Gegensatz zu GNU
    # sed nicht -- ein Muster wie 's/(\*\*|__)(...)\1/.../ ' liefert auf
    # macOS lautlos keinen Treffer (gemessen). Deshalb pro Markup-Form eine
    # eigene, feste Ersetzung statt einer gemeinsamen mit Rueckverweis.
    printf '%s\n' "$WHAT_NEW" \
      | sed -E 's/\[([^]]+)\]\([^)]+\)/\1/g' \
      | sed -E 's/\*\*([^*]+)\*\*/\1/g' \
      | sed -E 's/__([^_]+)__/\1/g' \
      | sed -E 's/\*([^*]+)\*/\1/g' \
      | trim_leerzeilen
    ;;
esac
