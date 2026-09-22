#!/usr/bin/env bash
#
# LGPL-Waechter fuer libmp3lame.
#
# Warum es ihn gibt (gemessen 22.09.2026): das ausgelieferte Bundle trug 67
# `_lame_`-Symbole im Hauptprogramm. LAME steht unter der LGPL, und deren
# Paragraf 4 erlaubt die Weitergabe eines Combined Work nur, wenn der Empfaenger
# die Bibliothek austauschen kann. Statisch gelinkt kann er das nicht.
#
# Der Ruecksturz waere leise: ein Update von `mp3lame-sys`, ein verlorener
# `[patch.crates-io]`-Eintrag oder ein `cargo update` reichen, und die Symbole
# sind ohne eine einzige Fehlermeldung zurueck im Binary. Deshalb prueft dieser
# Waechter das GEBAUTE Bundle, nicht die Konfiguration.
#
# Aufruf:
#   scripts/lgpl-gate.sh <Pfad zum .app-Bundle>
#   scripts/lgpl-gate.sh                       (sucht den Debug-Bau)
#
# Exit 0 = weitergabefaehig. Exit 1 = NICHT weitergeben.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DYLIB="libmp3lame.0.dylib"

APP="${1:-}"
if [ -z "$APP" ]; then
  # Bewusst als `if`, nicht als `[ ... ] && ... && break`: eine fehlschlagende
  # UND-Kette als letzter Befehl im Schleifenrumpf beendet unter `set -e` das
  # ganze Skript, und zwar schon beim ERSTEN Kandidaten, der nicht existiert.
  for profil in debug release; do
    kandidat="$REPO_ROOT/apps/desktop/src-tauri/target/$profil/bundle/macos/Mitschnitt.app"
    if [ -d "$kandidat" ]; then
      APP="$kandidat"
      break
    fi
  done
fi

if [ -z "$APP" ] || [ ! -d "$APP" ]; then
  echo "FEHLER: kein Bundle gefunden. Aufruf: $0 <Mitschnitt.app>" >&2
  exit 1
fi

BINARY_NAME="$(defaults read "$APP/Contents/Info.plist" CFBundleExecutable)"
BINARY="$APP/Contents/MacOS/$BINARY_NAME"
[ -f "$BINARY" ] || { echo "FEHLER: Hauptprogramm fehlt: $BINARY" >&2; exit 1; }

fehler=0
meldung() { echo "  $1"; }

echo "LGPL-Waechter: $APP"

# 1. Kein LAME-Code IM Hauptprogramm.
#
#    Gemessen werden DEFINIERTE Symbole (`nm -U`), nicht alle (`nm -a`).
#    Der Unterschied ist der ganze Punkt und hat mich am 22.09.2026 fast eine
#    falsche Pruefung gekostet:
#      statisch gelinkt: 67 definiert (T/t/s),  0 undefiniert
#      dynamisch gelinkt: 0 definiert,          9 undefiniert (U, die Importe)
#    `nm -a | grep -c " _lame_"` gibt also auch im GUTEN Fall nicht null aus --
#    eine Pruefung darauf waere dauerhaft rot und damit wertlos. Definiert
#    heisst: der Code liegt im Programm. Genau das darf er nicht.
#
#    `grep -c` liefert bei null Treffern Exit 1 -- deshalb `|| true`, sonst
#    bricht `set -e` genau im guten Fall ab. Das `|| true` darf aber nur den
#    GREP-Teil decken: ein `nm`, das selbst scheitert (falsches Format,
#    falsche Architektur, Datei weg), liefert ebenfalls leere Ausgabe und
#    damit "0 Treffer" -- ohne eigene Pruefung sieht das aus wie ein sauberes
#    Binary. Deshalb wird `nm` HIER separat aufgerufen und sein Status
#    einzeln geprueft, bevor ueberhaupt gezaehlt wird (gemessen 22.09.2026).
set +e
DEFINIERT_ROH="$(nm -U "$BINARY" 2>&1)"
NM_STATUS_DEFINIERT=$?
IMPORTIERT_ROH="$(nm -u "$BINARY" 2>&1)"
NM_STATUS_IMPORTIERT=$?
set -e

if [ "$NM_STATUS_DEFINIERT" -ne 0 ] || [ "$NM_STATUS_IMPORTIERT" -ne 0 ]; then
  meldung "ROT: nm scheiterte am Hauptprogramm (Status $NM_STATUS_DEFINIERT/$NM_STATUS_IMPORTIERT)."
  meldung "     Kein Zaehlen auf dieser Grundlage -- Architektur/Dateiformat pruefen."
  fehler=1
else
  DEFINIERT="$(printf '%s\n' "$DEFINIERT_ROH" | grep -c " _lame_" || true)"
  IMPORTIERT="$(printf '%s\n' "$IMPORTIERT_ROH" | grep -c "_lame_" || true)"
  if [ "$DEFINIERT" -ne 0 ]; then
    meldung "ROT: $DEFINIERT definierte _lame_-Symbole im Hauptprogramm -- LAME ist wieder statisch gelinkt."
    meldung "     Pruefen: [patch.crates-io] in Cargo.toml, vendor/mp3lame-sys/build.rs."
    fehler=1
  elif [ "$IMPORTIERT" -eq 0 ]; then
    meldung "ROT: weder definierte noch importierte _lame_-Symbole -- wird LAME ueberhaupt noch benutzt?"
    meldung "     Ein Waechter, der nach einem Ausbau still gruen bleibt, ist keiner."
    fehler=1
  else
    meldung "gruen: 0 definierte _lame_-Symbole, $IMPORTIERT importierte"
  fi
fi

# 2. Das Hauptprogramm verweist per @rpath auf die Bibliothek.
if otool -L "$BINARY" | grep -q "@rpath/$DYLIB"; then
  meldung "gruen: Hauptprogramm laedt @rpath/$DYLIB"
else
  meldung "ROT: keine Referenz auf @rpath/$DYLIB in otool -L."
  fehler=1
fi

# 3. Die Bibliothek liegt austauschbar im Bundle.
if [ -f "$APP/Contents/Frameworks/$DYLIB" ]; then
  meldung "gruen: $DYLIB liegt in Contents/Frameworks/"
else
  meldung "ROT: $DYLIB fehlt in Contents/Frameworks/ -- die App startet nicht."
  fehler=1
fi

# 4. Ein Suchpfad, unter dem sie auch gefunden wird. Gemessen: Tauri setzt
#    keinen LC_RPATH, der kommt aus .cargo/config.toml.
if otool -l "$BINARY" | grep -q "@executable_path/../Frameworks"; then
  meldung "gruen: LC_RPATH @executable_path/../Frameworks vorhanden"
else
  meldung "ROT: kein LC_RPATH auf ../Frameworks -- die Bibliothek wird nicht gefunden."
  fehler=1
fi

# 5. Lizenztexte und Hinweis liegen bei. Ohne sie ist die Weitergabe
#    unabhaengig vom Linken unzulaessig.
for datei in \
  "licenses/LICENSE-LGPL-2.0.txt" \
  "licenses/LICENSE-LGPL-3.0.txt" \
  "licenses/LICENSE-GPL-3.0.txt" \
  "licenses/LIESMICH-libmp3lame.md" \
  "licenses/ATTRIBUTIONS.md"
do
  if [ -f "$APP/Contents/Resources/$datei" ]; then
    meldung "gruen: $datei"
  else
    meldung "ROT: $datei fehlt im Bundle."
    fehler=1
  fi
done

DYLIB_PATH="$APP/Contents/Frameworks/$DYLIB"
if [ -f "$DYLIB_PATH" ]; then
  # 6. Architektur: Hauptprogramm und Dylib muessen fuer dieselbe (einzige)
  #    Architektur gebaut sein. Eine x86_64-Dylib neben einem arm64-Haupt-
  #    programm (oder umgekehrt) wuerde auf dem Zielrechner beim Laden
  #    scheitern -- fuer den Empfaenger nicht von einer defekten Signatur zu
  #    unterscheiden.
  BINARY_ARCH="$(lipo -archs "$BINARY" 2>/dev/null || true)"
  DYLIB_ARCH="$(lipo -archs "$DYLIB_PATH" 2>/dev/null || true)"
  if [ "$BINARY_ARCH" = "arm64" ] && [ "$DYLIB_ARCH" = "arm64" ]; then
    meldung "gruen: Hauptprogramm und $DYLIB sind beide reines arm64"
  else
    meldung "ROT: Architektur-Mismatch -- Hauptprogramm '$BINARY_ARCH', $DYLIB '$DYLIB_ARCH' (erwartet: beide genau 'arm64')."
    fehler=1
  fi

  # 7. Die Dylib selbst darf nur auf Systempfade und relative (@...) Pfade
  #    verweisen. Ein Homebrew- oder Bauordner-Pfad waere auf dem Rechner des
  #    Empfaengers so gut wie sicher nicht vorhanden -- austauschbar heisst
  #    dann zwar formal LGPL-konform, aber die mitgelieferte Kopie startet
  #    trotzdem nicht.
  DYLIB_FREMDPFAD=0
  while IFS= read -r dep; do
    [ -z "$dep" ] && continue
    case "$dep" in
      /usr/lib/*|/System/Library/*|@*) ;;
      *)
        meldung "ROT: $DYLIB haengt an einem Nicht-Systempfad: $dep"
        DYLIB_FREMDPFAD=1
        ;;
    esac
  done <<EOF
$(otool -L "$DYLIB_PATH" 2>/dev/null | tail -n +2 | awk '{print $1}')
EOF
  if [ "$DYLIB_FREMDPFAD" -eq 0 ]; then
    meldung "gruen: $DYLIB haengt nur an Systempfaden/relativen Pfaden"
  else
    fehler=1
  fi
else
  meldung "ROT: $DYLIB_PATH fehlt -- Architektur/Abhaengigkeiten nicht pruefbar (siehe Fund 3)."
  fehler=1
fi

if [ "$fehler" -ne 0 ]; then
  echo
  echo "LGPL-Waechter ROT -- dieses Bundle darf nicht weitergegeben werden." >&2
  exit 1
fi

echo "LGPL-Waechter gruen."
