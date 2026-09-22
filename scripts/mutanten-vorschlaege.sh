#!/usr/bin/env bash
# Chirurgische Mutanten fuer den Vorschlags-Lauf (ZICK-284).
#
# WOZU: Ein gruener Test beweist nichts. Er beweist erst etwas, wenn er stirbt,
# sobald genau die Verhaltenszeile umgedreht wird, die er zu pruefen behauptet.
# In dieser Strecke war FUENFMAL in Folge der teuerste Befund ein Test, den
# eine ANDERE Bedingung gruen hielt als die, um die es ging.
#
# ZWEI FALLEN, an denen die Vorrunde gescheitert ist, und wie sie hier
# geschlossen sind:
#
#  1. Ein Skript, das cargos "error: test failed" fuer einen Baufehler haelt,
#     meldet tote Mutanten als "baut nicht" -- also als Fehlschlag des
#     Werkzeugs statt als Erfolg der Pruefung. Hier entscheidet
#     `bewerte_lauf`: nur "could not compile" und "error[E..." sind
#     Baufehler.
#  2. Ein Wiederherstellen, das nur im Erfolgsfall laeuft, ist keins. Der
#     `trap` unten greift auch bei Ctrl-C und bei jedem Abbruch.
#
# ERREICHBARKEIT: Jeder Mutant hier dreht eine Verhaltenszeile um, die der
# echte Aufrufpfad erreicht. Ein Mutant, der die Erreichbarkeit selbst
# wegmutiert, beweist nichts -- deshalb steht bei jedem, welcher Test ihn
# toeten MUSS.
#
# Aufruf:  bash scripts/mutanten-vorschlaege.sh
# Ausgang: 0 = jeder Mutant ist gestorben. 1 = mindestens einer hat ueberlebt.

set -uo pipefail

WURZEL="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST="$WURZEL/crates/vocabulary/src/proposals.rs"
RUST_TESTS="$WURZEL/crates/vocabulary/src/proposals/tests.rs"
STOPWORDS="$WURZEL/crates/vocabulary/src/stopwords.rs"
TS="$WURZEL/apps/desktop/src/stt/proposals.ts"
SICHERUNG="$(mktemp -d)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cargo-target-284}"

cp "$RUST" "$SICHERUNG/proposals.rs"
cp "$STOPWORDS" "$SICHERUNG/stopwords.rs"
cp "$RUST_TESTS" "$SICHERUNG/tests.rs"
cp "$TS" "$SICHERUNG/proposals.ts"

wiederherstellen() {
  cp "$SICHERUNG/proposals.rs" "$RUST"
  cp "$SICHERUNG/stopwords.rs" "$STOPWORDS"
  cp "$SICHERUNG/tests.rs" "$RUST_TESTS"
  cp "$SICHERUNG/proposals.ts" "$TS"
}
# Auch bei Ctrl-C und bei jedem Abbruch, nicht nur im Erfolgsfall.
trap 'wiederherstellen' EXIT INT TERM

GESTORBEN=0
UEBERLEBT=0
UEBERLEBENDE=()

# Ersetzt genau eine Zeichenfolge in einer Datei. Trifft sie nicht genau
# einmal, ist der Mutant selbst kaputt und das Skript sagt es, statt still
# nichts zu tun.
patche() {
  python3 - "$1" "$2" "$3" <<'PY'
import sys
from pathlib import Path
pfad, alt, neu = sys.argv[1], sys.argv[2], sys.argv[3]
p = Path(pfad)
s = p.read_text()
treffer = s.count(alt)
if treffer != 1:
    print(f"ANKER TRIFFT {treffer}x STATT 1x", file=sys.stderr)
    sys.exit(2)
p.write_text(s.replace(alt, neu))
PY
}

# Unterscheidet einen toten Mutanten von einem Bau-Unfall. Genau hier ist die
# Vorrunde falsch abgebogen.
bewerte_lauf() {
  local ausgabe="$1"
  if grep -q "could not compile" <<<"$ausgabe" || grep -qE "^error\[E" <<<"$ausgabe"; then
    echo "BAUFEHLER"
  elif grep -q "test result: FAILED" <<<"$ausgabe"; then
    echo "TOT"
  else
    echo "LEBT"
  fi
}

pruefe_rust() {
  local name="$1" erwarteter_test="$2"
  local ausgabe urteil
  ausgabe="$(cargo test --manifest-path "$WURZEL/crates/vocabulary/Cargo.toml" \
    -p vocabulary 2>&1)"
  urteil="$(bewerte_lauf "$ausgabe")"
  if [ "$urteil" = "TOT" ]; then
    # NUR im Fehlerblock suchen. `grep` ueber die ganze Ausgabe findet den
    # erwarteten Testnamen auch in seiner BESTANDENEN "... ok"-Zeile -- das
    # Skript meldete dann "TOT, wie erwartet", waehrend ein ganz anderer Test
    # starb. Genau so ist M5 am 04.09.2026 durchgerutscht.
    local fehlerblock
    fehlerblock="$(sed -n '/^failures:/,$p' <<<"$ausgabe")"
    if grep -q "$erwarteter_test" <<<"$fehlerblock"; then
      echo "  TOT -- $erwarteter_test ist rot, wie erwartet"
      GESTORBEN=$((GESTORBEN + 1))
    else
      # Rot am falschen Test heisst: der Mutant wurde gefangen, aber nicht von
      # dem Test, der die Zusage traegt. Das ist ein Befund, kein Erfolg --
      # und deshalb zaehlt er als UEBERLEBT und faerbt den Ausgang rot.
      echo "  TOT, ABER AM FALSCHEN TEST -- erwartet war $erwarteter_test"
      grep -E "^    proposals::" <<<"$fehlerblock" | sed 's/^/      /'
      UEBERLEBT=$((UEBERLEBT + 1))
      UEBERLEBENDE+=("$name (starb am falschen Test)")
    fi
  else
    echo "  $urteil -- ueberlebt oder baut nicht"
    UEBERLEBT=$((UEBERLEBT + 1))
    UEBERLEBENDE+=("$name ($urteil)")
  fi
}

pruefe_ts() {
  local name="$1" erwarteter_test="$2"
  local ausgabe
  ausgabe="$(cd "$WURZEL/apps/desktop" && pnpm vitest run src/stt/proposals.test.ts \
    src/stt/proposals.sql.test.ts 2>&1)"
  if grep -qE "Tests +[0-9]+ failed" <<<"$ausgabe"; then
    # Dieselbe Falle wie auf der Rust-Seite: bestandene Tests stehen mit
    # ihrem Namen in derselben Ausgabe. Nur die Fehlerzeilen zaehlen --
    # vitest markiert sie mit "×"/"✕" bzw. "FAIL", bestandene mit "✓".
    local fehlerzeilen
    fehlerzeilen="$(grep -E "(×|✕|FAIL)" <<<"$ausgabe")"
    if grep -q "$erwarteter_test" <<<"$fehlerzeilen"; then
      echo "  TOT -- $erwarteter_test ist rot, wie erwartet"
      GESTORBEN=$((GESTORBEN + 1))
    else
      echo "  TOT, ABER AM FALSCHEN TEST -- erwartet war $erwarteter_test"
      sed 's/^/      /' <<<"$fehlerzeilen"
      UEBERLEBT=$((UEBERLEBT + 1))
      UEBERLEBENDE+=("$name (starb am falschen Test)")
    fi
  else
    echo "  LEBT -- ueberlebt"
    UEBERLEBT=$((UEBERLEBT + 1))
    UEBERLEBENDE+=("$name")
  fi
}

lauf_rust() {
  local name="$1" datei="$2" alt="$3" neu="$4" erwartet="$5"
  echo ""
  echo "[$name]"
  if ! patche "$datei" "$alt" "$neu"; then
    echo "  ANKER VERFEHLT -- Mutant nicht gesetzt"
    UEBERLEBT=$((UEBERLEBT + 1))
    UEBERLEBENDE+=("$name (Anker verfehlt)")
    wiederherstellen
    return
  fi
  pruefe_rust "$name" "$erwartet"
  wiederherstellen
}

lauf_ts() {
  local name="$1" alt="$2" neu="$3" erwartet="$4"
  echo ""
  echo "[$name]"
  if ! patche "$TS" "$alt" "$neu"; then
    echo "  ANKER VERFEHLT -- Mutant nicht gesetzt"
    UEBERLEBT=$((UEBERLEBT + 1))
    UEBERLEBENDE+=("$name (Anker verfehlt)")
    wiederherstellen
    return
  fi
  pruefe_ts "$name" "$erwartet"
  wiederherstellen
}

echo "=== Mutanten fuer den Vorschlags-Lauf ==="

# --- Punkt 3: eine Adresse im freien Kontext ankert nicht -----------------

lauf_rust "M1 die Adresse ankert wieder (der Fehler vom 04.09.)" "$RUST" \
  '                if wort.chars().count() >= 3 {
                    nur_immun.push(wort.to_string());
                }' \
  '                if wort.chars().count() >= 3 {
                    anker.push(wort.to_string());
                }' \
  "eine_adresse_im_freien_text_begruendet_kein_ziel"

lauf_rust "M2 Adresse wird gar nicht mehr erkannt" "$RUST" \
  "        if stueck.contains('@') {" \
  "        if stueck.contains('\\u{0}') {" \
  "eine_adresse_im_freien_text_begruendet_kein_ziel"

lauf_rust "M3 Adresse stiftet keine Immunitaet mehr" "$RUST" \
  '        for term in woerter.nur_immun {
            immun.insert(key(&term));
        }' \
  '        for term in woerter.nur_immun {
            let _ = term;
        }' \
  "eine_adresse_im_freien_text_begruendet_kein_ziel"

lauf_rust "M4 Untergrenze der Immunitaet unerreichbar hoch" "$RUST" \
  '                if wort.chars().count() >= 3 {
                    nur_immun.push(wort.to_string());' \
  '                if wort.chars().count() >= 99 {
                    nur_immun.push(wort.to_string());' \
  "eine_adresse_im_freien_text_begruendet_kein_ziel"

# M5 war bis zum 04.09.2026 ein Tausch von Lokalteil und Domain -- und damit
# ein No-op: BEIDE Haelften gingen ohnehin nach `nur_immun`, der Mutant machte
# die Domain immun statt zum Anker und pruefte gar nicht, was er pruefen
# sollte. Er starb an einem ANDEREN Test, und das Skript merkte es nicht.
# Der gueltige Mutant schiebt die Domain wirklich in die Anker-Menge.
lauf_rust "M5 Domainteil ankert mit" "$RUST" \
  '        if stueck.contains('"'"'@'"'"') {
            for wort in words_with_halves(stueck) {
                if wort.chars().count() >= 3 {
                    nur_immun.push(wort.to_string());
                }
            }
        } else {' \
  '        if stueck.contains('"'"'@'"'"') {
            for wort in words_with_halves(stueck) {
                if wort.chars().count() >= 3 {
                    nur_immun.push(wort.to_string());
                }
            }
            if let Some((_lokal, domain)) = stueck.split_once('"'"'@'"'"') {
                for wort in words_with_halves(domain) {
                    anker.push(wort.to_string());
                }
            }
        } else {' \
  "der_domainteil_einer_adresse_im_freien_text_ankert_nicht"

# M13: der Rueckfall auf den Lokalteil -- genau der Befund vom 04.09.2026.
# Was ohne Leerzeichen hinter der Adresse klebt (minifiziertes JSON), verliert
# damit Anker UND Immunitaet.
lauf_rust "M13 nur der Lokalteil stiftet Immunitaet" "$RUST" \
  '        if stueck.contains('"'"'@'"'"') {
            for wort in words_with_halves(stueck) {' \
  '        if stueck.contains('"'"'@'"'"') {
            for wort in words_with_halves(stueck.split_once('"'"'@'"'"').map_or("", |(lokal, _)| lokal)) {' \
  "ein_stueck_ohne_leerzeichen_verliert_seine_nachbarn_nicht"

# M6 traf bis zum 04.09.2026 den falschen Test: `context_text_terms` speist
# NETZ 2 mit Zielen, waehrend die Anker-Rolle ueber `context_text_fields`
# direkt laeuft. Der Mutant starb deshalb an einem Zerleger-Test statt an
# einer Verhaltenszusage -- gefunden, als das Skript endlich im Fehlerblock
# statt in der ganzen Ausgabe suchte.
lauf_rust "M6 Kalendertext liefert Netz 2 kein Ziel mehr" "$RUST" \
  "fn context_text_terms(text: &str) -> Vec<String> {
    context_text_fields(text).anker
}" \
  "fn context_text_terms(text: &str) -> Vec<String> {
    context_text_fields(text).nur_immun
}" \
  "der_kalendertext_liefert_netz_zwei_sein_ziel"

# --- Punkt 4: ohne Beleg kein Vorschlag -----------------------------------

lauf_rust "M7 Netz 1 faellt ohne Anker auf die erste Schreibweise zurueck" "$RUST" \
  '            let Some(canonical_index) = members
                .iter()
                .copied()
                .find(|&index| known.anker.contains(&key(names[index])))
            else {
                continue;
            };' \
  '            let Some(canonical_index) = members
                .iter()
                .copied()
                .find(|&index| known.anker.contains(&key(names[index])))
                .or_else(|| members.first().copied())
            else {
                continue;
            };' \
  "ohne_jeden_beleg_entsteht_kein_vorschlag"

lauf_rust "M8 Netz 1 ankert wieder aus der weiten Menge" "$RUST" \
  ".find(|&index| known.anker.contains(&key(names[index])))" \
  ".find(|&index| known.immun.contains(&key(names[index])))" \
  "eine_mailadresse_begruendet_auch_ueber_zwei_gespraeche_kein_ziel"

# M9 war zuerst ein anderer: "if targets.is_empty() { continue; }" auf
# "if false" umgestellt. Der Mutant hat UEBERLEBT, und das war richtig so --
# die Zeile ist eine Abkuerzung, keine Verhaltenszeile. Direkt darunter
# iteriert "for (target, source) in &targets" ueber eine leere Liste ohnehin
# nicht, das Ergebnis ist mit und ohne Abkuerzung dasselbe. Ein Mutant, der
# eine Optimierung umdreht, beweist nichts; er gehoert ersetzt und nicht mit
# einem neuen Test "gefangen".
#
# Der gueltige Mutant fuer dieselbe Zusage fuegt Netz 2 eine VIERTE Zielquelle
# hinzu -- und zwar genau die verbotene. Das ist die realistische Regression:
# der Serredi-Fehler auf der Rust-Seite.
lauf_rust "M9 Netz 2 nimmt den erzeugten Sitzungstitel als Ziel" "$RUST" \
  '        for term in context_text_terms(&session.context_text) {
            merke_ziel(term, "Titel oder Notiz");
        }' \
  '        for term in context_text_terms(&session.context_text) {
            merke_ziel(term, "Titel oder Notiz");
        }
        for term in context_text_terms(&session.generated_title) {
            merke_ziel(term, "Titel oder Notiz");
        }' \
  "ein_erzeugter_titel_ist_kein_beleg"

# --- Der Ausbau: der Sitzungstitel bleibt aus dem Kontext -----------------

lauf_ts "M10 der erzeugte Titel kommt in den Kontext zurueck" \
  'contextText: [row.context_text ?? "", row.note_body ?? ""]' \
  'contextText: [row.context_text ?? "", row.note_body ?? "", row.generated_title ?? ""]' \
  "nimmt einen erfundenen Titel nicht in den Kontext"

lauf_ts "M11 die Notiz erreicht den Kontext gar nicht mehr" \
  'contextText: [row.context_text ?? "", row.note_body ?? ""]' \
  'contextText: [row.context_text ?? "", ""]' \
  "laesst den erfundenen Titel in der Notiz stehen"

lauf_ts "M12 der Termin wird nicht mehr ueber die Sitzungskennung aufgeloest" \
  "            event.id = session.event_id" \
  "            event.id = '' AND session.event_id = session.event_id" \
  "findet den Kalendertermin auch, wenn die Sitzung ihn direkt nennt"

# --- Die grosse Wortliste (04.09.2026) --------------------------------------
#
# Drei Stufen halten den Vorschlags-Lauf von zwei gegeneinander getauschten
# Alltagswoertern ab, und jede allein wuerde reichen. Ein Ende-zu-Ende-Test
# kann deshalb keine einzelne davon beweisen -- er bleibt gruen, egal welche
# man entfernt. Jeder Mutant hier zeigt auf den Test, der SEINE Stufe anfasst.

lauf_rust "M14 die grosse Wortliste kommt leer an" "$STOPWORDS" \
  '            .filter(|zeile| !zeile.is_empty())' \
  '            .filter(|zeile| zeile.is_empty())' \
  "die_grosse_wortliste_traegt_die_inhaltswoerter_der_handliste_nach"

lauf_rust "M15 die weite Pruefung faellt auf die Handliste zurueck" "$STOPWORDS" \
  '    wide_lookup().contains(normalized.as_str())' \
  '    false' \
  "die_grosse_wortliste_traegt_die_inhaltswoerter_der_handliste_nach"

lauf_rust "M16 die Kandidatenstufe prueft wieder nur die Handliste" "$RUST" \
  '            || is_ordinary_german_word(core)' \
  '            || is_common_german_word(core)' \
  "ein_alltagswort_ist_kein_kandidat"

lauf_rust "M17 der Kalendertext ankert wieder jedes Alltagswort" "$RUST" \
  '                    && !is_ordinary_german_word(wort)' \
  '                    && !is_common_german_word(wort)' \
  "ein_alltagswort_im_kalendertext_wird_kein_anker"

lauf_rust "M18 Netz 2 nimmt ein Alltagswort aus dem Woerterbuch wieder als Ziel" "$RUST" \
  '            if !is_ordinary_german_word(&wort) {' \
  '            if !is_common_german_word(&wort) {' \
  "ein_alltagswort_aus_dem_woerterbuch_wird_kein_ziel"

lauf_rust "M19 die Beugungsgrenze steht wieder bei sechs" "$RUST" \
  '    const MIN_LEN: usize = 5;' \
  '    const MIN_LEN: usize = 6;' \
  "die_laengengrenze_der_beugungsregel_wirkt_wirklich"

lauf_rust "M20 die Beugungsgrenze rutscht auf vier" "$RUST" \
  '    const MIN_LEN: usize = 5;' \
  '    const MIN_LEN: usize = 4;' \
  "die_laengengrenze_der_beugungsregel_wirkt_wirklich"

lauf_rust "M21 die Kompositum-Regel ist unerreichbar" "$RUST" \
  '        && rest.chars().count() >= 3' \
  '        && rest.chars().count() >= 999' \
  "ein_kompositum_ist_keine_verhoerung_seines_ersten_teils"

lauf_rust "M22 die Kompositum-Regel verlangt kein Wort mehr als Rest" "$RUST" \
  '        && is_ordinary_german_word(rest)' \
  '        && !rest.is_empty()' \
  "ein_kompositum_ist_keine_verhoerung_seines_ersten_teils"

lauf_rust "M23 die Wortfolgen-Pruefung bekommt die grosse Liste" "$STOPWORDS" \
  '        if !is_common_german_word(word) {' \
  '        if !is_ordinary_german_word(word) {' \
  "die_wortfolgen_pruefung_bleibt_bei_der_handliste"

echo ""
echo "=== Ergebnis ==="
echo "gestorben: $GESTORBEN   ueberlebt: $UEBERLEBT"
if [ "$UEBERLEBT" -gt 0 ]; then
  printf 'UEBERLEBT: %s\n' "${UEBERLEBENDE[@]}"
fi

# Der Beweis, dass der Baum unversehrt zurueck ist -- eine Behauptung reicht
# nicht.
wiederherstellen
echo ""
echo "Pruefsummen nach dem Lauf:"
shasum -a 256 "$RUST" "$RUST_TESTS" "$STOPWORDS" "$TS"

[ "$UEBERLEBT" -eq 0 ]
