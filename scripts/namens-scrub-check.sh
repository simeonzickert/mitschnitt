#!/usr/bin/env bash
# Waechter vor dem oeffentlichen Gang.
#
# Prueft den gesamten getrackten Arbeitsbaum auf zwei Dinge:
#
#   1. Klarnamen aus einer Namensliste, die BEWUSST ausserhalb dieses Repos
#      liegt. Waere die Liste hier eingecheckt, waere der Waechter selbst die
#      Undichtigkeit, die er verhindern soll.
#   2. Echte E-Mail-Adressen. Dieses Muster ist eingebaut und namensfrei:
#      es meldet jede Adresse, die nicht in einer eng umrissenen Liste
#      reservierter oder Upstream-Domains liegt.
#
# Aufruf:
#   MITSCHNITT_NAMENSLISTE=<pfad> scripts/namens-scrub-check.sh [--liste]
#
# GESUCHT WIRD OHNE RUECKSICHT AUF GROSS-/KLEINSCHREIBUNG. Das ist die teure
# Seite und deshalb die Voreinstellung: ein uebersehener Name im
# veroeffentlichten Verlauf ist endgueltig, ein Fehlalarm kostet eine
# Wortgrenze in der externen Liste. Kurze Eintraege gehoeren deshalb mit \b
# geschrieben, sonst treffen sie Teilzeichenketten gewoehnlicher Bezeichner.
#
# Erwartet wird eine Textdatei mit einer PCRE-Alternative je Zeile; Leerzeilen
# und Zeilen, die mit # beginnen, werden uebersprungen. Die Datei gehoert an
# einen Ort ausserhalb dieses Repos. Die Umgebungsvariable ist PFLICHT und hat
# absichtlich keine Voreinstellung im Skript -- der Pfad traegt selbst einen
# Namen.
#
# Exit 0 = sauber. Exit 1 = Treffer, also nicht oeffentlich schalten.
# Exit 2 = es wurde NICHTS geprueft (falscher Ort, fehlende, leere oder
#          fehlerhafte Liste, fehlgeschlagene Suche).

set -o pipefail
export LC_ALL=C

LISTE=0
for arg in "$@"; do
  case "$arg" in
    --liste) LISTE=1 ;;
    *) echo "FEHLER: unbekannter Schalter '$arg' (erlaubt: --liste)" >&2; exit 2 ;;
  esac
done

# ---------------------------------------------------------------- Ort pruefen
# Ein Waechter, der ausserhalb seines Repos "sauber" meldet, ist schlimmer als
# keiner. Genau das ist einmal passiert: eine Kopie des Skripts lag in einem
# fremden Ordner, die Suche lief ins Leere, die Ausgabe sagte "sauber".
cd "$(dirname "$0")/.." || exit 2

if ! git rev-parse --show-toplevel >/dev/null 2>&1; then
  echo "🔴 Kein git-Repo unter $(pwd). Es wurde NICHTS geprueft."
  exit 2
fi
if [ ! -d crates ] || [ ! -d apps ]; then
  echo "🔴 $(pwd) sieht nicht nach dem Projekt-Repo aus (crates/ und apps/"
  echo "   fehlen). Es wurde NICHTS geprueft."
  exit 2
fi
WURZEL=$(git rev-parse --show-toplevel)

# ------------------------------------------------------------- Liste pruefen
if [ -z "${MITSCHNITT_NAMENSLISTE:-}" ]; then
  echo "🔴 MITSCHNITT_NAMENSLISTE ist nicht gesetzt. Es wurde NICHTS geprueft."
  echo
  echo "   Erwartet wird der Pfad zu einer Textdatei ausserhalb dieses Repos,"
  echo "   die je Zeile eine PCRE-Alternative mit einem zu suchenden Klarnamen"
  echo "   enthaelt (# und Leerzeilen werden uebersprungen)."
  echo
  echo "   Gesucht wird OHNE Ruecksicht auf Gross-/Kleinschreibung. Kurze"
  echo "   Eintraege brauchen deshalb Wortgrenzen (\\bName\\b), sonst treffen"
  echo "   sie Teile gewoehnlicher Bezeichner."
  echo
  echo "   Aufruf:  MITSCHNITT_NAMENSLISTE=<pfad> $0 [--liste]"
  exit 2
fi
if [ ! -f "$MITSCHNITT_NAMENSLISTE" ]; then
  echo "🔴 Namensliste nicht gefunden: $MITSCHNITT_NAMENSLISTE"
  echo "   Es wurde NICHTS geprueft."
  exit 2
fi
case "$(cd "$(dirname "$MITSCHNITT_NAMENSLISTE")" && pwd -P)/" in
  "$WURZEL"/*)
    echo "🔴 Die Namensliste liegt INNERHALB des Repos:"
    echo "   $MITSCHNITT_NAMENSLISTE"
    echo "   Damit waere sie selbst die Undichtigkeit. Es wurde NICHTS geprueft."
    exit 2
    ;;
esac

# Jede Zeile EINZELN uebersetzen, bevor irgendetwas gesucht wird. Eine kaputte
# Zeile bringt sonst die ganze Suche zum Absturz, und ein Absturz sieht in der
# Ausgabe genauso aus wie ein sauberer Baum. Gemessen am 21.09.2026:
# `git grep -P '(unbalanced'` endet mit Status 128 und gibt NICHTS aus.
ZEILENPRUEFUNG=$(
  perl -e '
    my ($pfad) = @ARGV;
    open(my $fh, "<", $pfad) or do { print "OEFFNEN\n"; exit 0 };
    my $nr = 0; my $wirksam = 0;
    while (my $zeile = <$fh>) {
      $nr++;
      chomp $zeile;
      next if $zeile =~ /^\s*(#|$)/;
      $wirksam++;
      my $ok = eval { qr/$zeile/i; 1 };
      unless ($ok) {
        my $grund = $@; $grund =~ s/\s+at .*//s; $grund =~ s/\n.*//s;
        print "KAPUTT\t$nr\t$zeile\t$grund\n";
      }
    }
    print "WIRKSAM\t$wirksam\n";
  ' "$MITSCHNITT_NAMENSLISTE"
)
if printf '%s' "$ZEILENPRUEFUNG" | grep -q '^OEFFNEN'; then
  echo "🔴 Die Namensliste laesst sich nicht lesen. Es wurde NICHTS geprueft."
  exit 2
fi
if printf '%s' "$ZEILENPRUEFUNG" | grep -q '^KAPUTT'; then
  echo "🔴 Die Namensliste enthaelt fehlerhafte Zeilen. Es wurde NICHTS geprueft."
  echo
  printf '%s\n' "$ZEILENPRUEFUNG" | awk -F'\t' '$1=="KAPUTT" {printf "   Zeile %s: %s\n     %s\n", $2, $3, $4}'
  exit 2
fi
ZEILEN=$(printf '%s' "$ZEILENPRUEFUNG" | awk -F'\t' '$1=="WIRKSAM" {print $2}')
if [ -z "$ZEILEN" ] || [ "$ZEILEN" -eq 0 ]; then
  echo "🔴 Die Namensliste enthaelt keine wirksame Zeile."
  echo "   Es wurde NICHTS geprueft."
  exit 2
fi

MUSTER=$(grep -vE '^[[:space:]]*(#|$)' "$MITSCHNITT_NAMENSLISTE" | paste -sd'|' -)

# EINE Suchmaschine, nicht zwei. Gesucht wird mit `git grep -P` (PCRE),
# nachgefiltert wird mit perl -- derselbe Dialekt. Vorher lief die
# Nachfilterung ueber das `grep` aus dem PATH mit -E; welches Programm das ist
# und welchen Dialekt es versteht, entscheidet die Umgebung. Ein Muster, das
# die eine Maschine findet und die andere nicht, faellt dabei still unter den
# Tisch.
#
# Wortgrenzen (\b) kann `git grep -E` nicht; es liefert dafuer stumm null
# Treffer. Faellt PCRE aus, bricht der Lauf ab, statt still zurueckzufallen.
# Selbstprobe: das Wort PCRE steht im Kommentar darueber.
if ! git grep -q -P '\bPCRE\b' -- scripts/namens-scrub-check.sh 2>/dev/null; then
  echo "🔴 Dieses git findet Wortgrenzen (\\b) nicht -- es kann nicht mit -P"
  echo "   (PCRE) suchen. Listeneintraege mit Wortgrenzen waeren blind."
  echo "   Es wurde NICHTS geprueft."
  exit 2
fi

# ------------------------------------------------------------------- Bereich
# Lockdateien werden MITDURCHSUCHT: sie sind zwar maschinengeschrieben, tragen
# aber Paket- und Quellangaben, und ein Treffer dort ist eine Entscheidung,
# keine Selbstverstaendlichkeit.
#
# Ausgenommen sind Modellgewichte (binaer, git haelt einzelne davon
# faelschlich fuer Text, ihr Zufallsrauschen erzeugt Zeichenfolgen, die wie
# eine Mailadresse aussehen, gemessen 21.09.2026 in einer .onnx) UND der
# eingebettete Upstream-Quelltext von LAME (vendor/mp3lame-sys/lame-3.100/).
#
# Der zweite Ausschluss ist bewusst eng auf den UNVERAENDERTEN Upstream-Baum
# geschnitten, nicht auf ganz vendor/mp3lame-sys/ -- unsere eigene Rust-
# Anbindung (build.rs, src/, Cargo.toml) bleibt geprueft. In lame-3.100/
# stehen ChangeLog und AUTHORS mit ~38 Namens- und ~250 Mailstellen echter
# LAME-Entwickler (gemessen 22.09.2026, u.a. der urspruengliche mpglib-Autor).
# Das sind Urheberangaben eines Fremdprojekts, keine Leaks aus diesem Fork --
# sie stehen so seit Jahrzehnten oeffentlich im LAME-Repository, und sie zu
# schwaerzen waere selbst eine Verfaelschung der Lizenz-/Autorenlage. Zaehlt
# deshalb nicht als "geprueft", sondern separat in der Ungeprueft-Zeile.
AUSSCHLUSS=(
  ':(exclude)*.onnx'
  ':(exclude)*.safetensors'
  ':(exclude)*.gguf'
  ':(exclude)*.bin'
  ':(exclude)vendor/mp3lame-sys/lame-3.100/**'
)

# ------------------------------------------------------- erlaubte Klarnamen
#
# ZWEI ARTEN VON AUSNAHME, beide bewusst eng:
#
# 1. FORM, nicht Name: die Bundle-Kennung kommt in ueber hundert Zeilen vor.
#    Sie wird als Muster beschrieben, das den Firmennamen nicht ausschreibt --
#    sonst stuende er hier im Repo und der Waechter waere sein eigener Fund.
ERLAUBT_FORM='media\.[a-z]+\.mitschnitt(\.(stable|staging|desktop|flatpak))*|media/[a-z]+/mitschnitt|id="media\.[a-z]+"'
#
# 2. GENAUE ZEILE: alles andere, was absichtlich einen Klarnamen traegt, wird
#    als Paar aus Pfad und SHA-256 der getrimmten Zeile geführt. Damit steht
#    der Name nicht im Skript, und eine geaenderte Zeile faellt sofort wieder
#    auf -- eine Musterausnahme wie "beliebige zweigliedrige Person" waere
#    dagegen eine offene Tuer.
#
#    Die Urheberzeilen fremder Autoren stehen ebenfalls hier: eine
#    Autorenangabe zu faelschen ist keine Anonymisierung.
#
#    AUCH DER PFAD ist eine Pruefsumme: eine der erlaubten Dateien traegt den
#    Firmennamen im Dateinamen, und ausgeschrieben stuende er damit hier.
#
#    Neu aufnehmen -- beide Haelften aus dem Repo-Wurzelverzeichnis:
#      printf '%s' "<pfad>"            | shasum -a 256
#      printf '%s' "<getrimmte zeile>" | shasum -a 256
ERLAUBTE_ZEILEN=$(cat <<'EOF'
f6ed156e4bf5c791680662464b94ea5d753f219ee816b385f67870e2c0d7d4c7	2385a0552309717949bc9d1860c89e18b9cf895841fb66b0ee7f30b4212fefcf
f6ed156e4bf5c791680662464b94ea5d753f219ee816b385f67870e2c0d7d4c7	c26cafbbf3ffa467553fdaf3f879ecbf2b3a12ec31681ec525131b5aa98d0ab9
25f93a9e45647c833a5769e5c21fb1735e0ffd08f536dad2337de462ed7d14dd	8d688a92ed8ce493ec82468be34e113d2197f10408f0ad8a4eaa7215c9105a83
25f93a9e45647c833a5769e5c21fb1735e0ffd08f536dad2337de462ed7d14dd	63f7ad33114a0df88cffe5e92c68e4e027055ac99c422d7715620e44b613fc83
a070aae56eee2f46181846e951d93162957c2054ade297376c34d1ba46443cd1	7ddc8738859a02c2f4bde09879fa0dfb6c8ef6de76dc24f372b2ffa091c2c9b5
83749278577f0efd639c952231ba282d0baddadcff7eda799a0a05adda80990e	4fe7edc0f0558e68be6e1109a1ae0a6c4ec1febe996d35d6e8f9045044b9a15a
dcbe97359bb32312a4a669130bd4a377c9399bb3b64fd15dd22eff87c5a30cce	7d780fa09378e35d7bf60ada2e5cc3ccea3ebe1aa9acd3efaf4e9cbace81d8cc
dcbe97359bb32312a4a669130bd4a377c9399bb3b64fd15dd22eff87c5a30cce	da1df899ef292e581211ac39f23e468963e2011cf5058d8dbc28d760dd1f1b50
25f93a9e45647c833a5769e5c21fb1735e0ffd08f536dad2337de462ed7d14dd	06dc9a48161f1f3c461c5bf9cdc089d6671fd8b8a7bcf6a1c5b99da30c7882f9
643b8ef4f60f2d4c47f1bbb7988d8dab5cf1d91ca84e21de9c3bfc5c5b2f35bd	e8891a51d378e2a0897173633f39c8faab2339479ddf3b1589d96f19a7de7611
e7eb5ff74d543f13d6406efee81194ac158fd181a5573cb529a81271247daf63	4fb5646f9cdf8f04a310663c9a2aa99781b17963fbd58ca9854a3dd3a2eb756f
e7eb5ff74d543f13d6406efee81194ac158fd181a5573cb529a81271247daf63	698f220bbdf8666371955fc8eef0b98fa04f5ac2effb431634fad87c838c23e6
EOF
)

# ------------------------------------------------------ erlaubte Mailadressen
# Eng: reservierte Domains nach RFC 2606/6761 und die drei Upstream-Projekte,
# aus denen dieser Fork stammt. Eine Nicht-Antwort-Adresse ist nur zusammen
# mit einer dieser Domains erlaubt -- "noreply@" allein sagt nichts darueber,
# wem die Domain gehoert.
MAIL_ERLAUBT_DOMAIN='@([a-z0-9-]+\.)*example\.(com|net|org)$|@([a-z0-9-]+\.)*example$|\.(test|invalid|localhost)$|@users\.noreply\.github\.com$|@([a-z0-9-]+\.)*(anarlog\.so|hyprnote\.com|fastrepl\.com)$'
# Kein Mail, sondern ein Dateiname wie 128x128@2x.png.
MAIL_KEINE='\.(png|jpe?g|gif|svg|webp|ico|bmp)$'
MAIL_MUSTER='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'

# ------------------------------------------------------------------- Suchen
# `git grep` gibt 0 bei Treffern, 1 bei keinem Treffer und >=2 bei einem
# Fehler. Nur der Fehlerfall ist gefaehrlich: er sieht ohne diese Pruefung wie
# ein sauberer Baum aus.
suche() {
  local muster="$1"; shift
  local ausgabe status
  ausgabe=$(git grep -n -I -i -P -e "$muster" -- "$@" 2>&1)
  status=$?
  if [ "$status" -ge 2 ]; then
    echo "🔴 Die Suche ist fehlgeschlagen (git grep, Status $status)." >&2
    echo "   $ausgabe" >&2
    echo "   Es wurde NICHTS geprueft." >&2
    return 9
  fi
  [ "$status" -eq 1 ] && return 0
  printf '%s\n' "$ausgabe"
}

# Die NAMENS-Suche laeuft auch ueber Lockdateien: dort stehen Paket- und
# Quellangaben, und ein Klarname darin waere ein Fund. Die MAIL-Suche nimmt sie
# aus -- sie tragen ausschliesslich die Adressen fremder Paketautoren, und die
# stehen dort so wenig zur Disposition wie in einer Lizenzdatei. Ihre Zeilen
# aendern sich bei jedem Abhaengigkeits-Update, eine Pruefsumme je Zeile waere
# dort nach dem naechsten Update wertlos.
MAIL_AUSSCHLUSS=(
  "${AUSSCHLUSS[@]}"
  ':(exclude)*.lock'
  ':(exclude)*-lock.json'
  ':(exclude)*.lockb'
  ':(exclude)pnpm-lock.yaml'
  ':(exclude)**/cargo-sources.json'
  ':(exclude)**/pnpm-sources.json'
)

N_ROH=$(suche "$MUSTER" "${AUSSCHLUSS[@]}") || exit 2
M_ROH=$(suche "$MAIL_MUSTER" "${MAIL_AUSSCHLUSS[@]}") || exit 2

# Nachfilterung in EINEM perl-Lauf: derselbe Dialekt wie die Suche, und
# nebenbei ein Prozess statt einer Schleife mit drei Kindprozessen je Zeile.
#
# `set -e` ist fuer diesen ganzen Waechter bewusst AUS (Selbstprobe oben,
# git-grep-Exit-1-bei-keinem-Treffer waere sonst schon frueher toedlich).
# Ohne eigene Absicherung wuerde ein scheiterndes perl (kaputtes Encoding,
# ein `die`, ein Interpreter-Absturz) hier also NICHT auffallen -- die
# Zuweisung faengt einfach nur eine leere Ausgabe ein, und leer sieht exakt
# wie "keine Treffer" aus. Jede der drei Zuweisungen wird deshalb einzeln
# gegen ihren eigenen Exit-Status (via `set -o pipefail` oben) abgesichert.
N_TREFFER=$(
  printf '%s' "$N_ROH" | perl -MDigest::SHA=sha256_hex -e '
    my ($muster, $form, $erlaubte) = @ARGV;
    my %erlaubt;
    for my $zeile (split /\n/, $erlaubte) {
      next unless $zeile =~ /^(\S+)\t([0-9a-f]+)$/;
      $erlaubt{"$1\t$2"} = 1;
    }
    while (my $zeile = <STDIN>) {
      chomp $zeile;
      next unless $zeile =~ /^([^:]*):(\d+):(.*)$/s;
      my ($ort, $nr, $text) = ($1, $2, $3);
      my $getrimmt = $text; $getrimmt =~ s/^\s+|\s+$//g;
      next if $erlaubt{sha256_hex($ort) . "\t" . sha256_hex($getrimmt)};
      my $rein = $text;
      $rein =~ s/$form//gi;
      next unless $rein =~ /$muster/i;
      print "$ort:$nr:$text\n";
    }
  ' "$MUSTER" "$ERLAUBT_FORM" "$ERLAUBTE_ZEILEN"
) || { echo "🔴 Die Namens-Nachfilterung (perl) ist fehlgeschlagen. Es wurde NICHTS geprueft." >&2; exit 2; }

M_TREFFER=$(
  printf '%s' "$M_ROH" | perl -MDigest::SHA=sha256_hex -e '
    my ($mail, $domain, $keine, $erlaubte) = @ARGV;
    my %erlaubt;
    for my $zeile (split /\n/, $erlaubte) {
      next unless $zeile =~ /^(\S+)\t([0-9a-f]+)$/;
      $erlaubt{"$1\t$2"} = 1;
    }
    my %gesehen;
    while (my $zeile = <STDIN>) {
      chomp $zeile;
      next unless $zeile =~ /^([^:]*):(\d+):(.*)$/s;
      my ($ort, $nr, $text) = ($1, $2, $3);
      my $getrimmt = $text; $getrimmt =~ s/^\s+|\s+$//g;
      next if $erlaubt{sha256_hex($ort) . "\t" . sha256_hex($getrimmt)};
      while ($text =~ /($mail)/g) {
        my $adr = $1;
        my $klein = lc $adr;
        next if $klein =~ /$keine/;
        next if $klein =~ /$domain/;
        my $schluessel = "$ort:$nr: $adr";
        next if $gesehen{$schluessel}++;
        print "$schluessel\n";
      }
    }
  ' "$MAIL_MUSTER" "$MAIL_ERLAUBT_DOMAIN" "$MAIL_KEINE" "$ERLAUBTE_ZEILEN" | sort -u
) || { echo "🔴 Die Mail-Nachfilterung (perl) ist fehlgeschlagen. Es wurde NICHTS geprueft." >&2; exit 2; }

# Auch ein DATEINAME kann einen Klarnamen tragen.
P_TREFFER=$(
  git ls-files | perl -e '
    my ($muster, $form) = @ARGV;
    while (my $pfad = <STDIN>) {
      chomp $pfad;
      my $rein = $pfad;
      $rein =~ s/$form//gi;
      print "$pfad\n" if $rein =~ /$muster/i;
    }
  ' "$MUSTER" "$ERLAUBT_FORM"
) || { echo "🔴 Die Dateinamens-Pruefung (perl) ist fehlgeschlagen. Es wurde NICHTS geprueft." >&2; exit 2; }

# ------------------------------------------------------------------- Ausgabe
N_DATEIEN=$(printf '%s' "$N_TREFFER" | sed -E 's#:[0-9]+:.*##' | grep . | sort -u)
M_DATEIEN=$(printf '%s' "$M_TREFFER" | sed -E 's#:[0-9]+: .*##' | grep . | sort -u)
ALLE_DATEIEN=$(printf '%s\n%s\n%s\n' "$N_DATEIEN" "$M_DATEIEN" "$P_TREFFER" | grep . | sort -u)
ANZAHL_DATEIEN=$(printf '%s' "$ALLE_DATEIEN" | grep -c . )
ANZAHL_NAMEN=$(printf '%s' "$N_TREFFER" | grep -c . )
ANZAHL_MAILS=$(printf '%s' "$M_TREFFER" | grep -c . )
ANZAHL_PFADE=$(printf '%s' "$P_TREFFER" | grep -c . )

# Ein Waechter sagt, was er NICHT geprueft hat.
UNGEPRUEFT=$(git ls-files -- '*.onnx' '*.safetensors' '*.gguf' '*.bin' | wc -l | tr -d ' ')
UNGEPRUEFT_LAME=$(git ls-files -- 'vendor/mp3lame-sys/lame-3.100/**' | wc -l | tr -d ' ')
NICHT_GEPRUEFT="   Ungeprueft: $UNGEPRUEFT Modellgewichte, alles, was git als binaer einstuft (-I),
   und $UNGEPRUEFT_LAME Dateien unter vendor/mp3lame-sys/lame-3.100/ (Upstream-LAME-Quelltext,
   Begruendung im Skriptkopf beim Ausschluss)."

if [ "$ANZAHL_NAMEN" -eq 0 ] && [ "$ANZAHL_MAILS" -eq 0 ] && [ "$ANZAHL_PFADE" -eq 0 ]; then
  echo "sauber: keine Klarnamen ($ZEILEN Listeneintraege, ohne Ruecksicht auf"
  echo "        Gross-/Kleinschreibung) und keine echten Mailadressen."
  echo "$NICHT_GEPRUEFT"
  exit 0
fi

echo "🔴 $ANZAHL_DATEIEN Dateien tragen Klarnamen oder echte Mailadressen"
echo "   -- NICHT oeffentlich schalten."
echo "   Namensstellen: $ANZAHL_NAMEN   Mailstellen: $ANZAHL_MAILS   Pfade: $ANZAHL_PFADE"
echo "$NICHT_GEPRUEFT"

if [ "$LISTE" -eq 1 ]; then
  if [ "$ANZAHL_PFADE" -gt 0 ]; then
    echo
    echo "--- Dateinamen ---"
    printf '%s\n' "$P_TREFFER"
  fi
  if [ "$ANZAHL_NAMEN" -gt 0 ]; then
    echo
    echo "--- Klarnamen ---"
    printf '%s\n' "$N_TREFFER"
  fi
  if [ "$ANZAHL_MAILS" -gt 0 ]; then
    echo
    echo "--- Mailadressen ---"
    printf '%s\n' "$M_TREFFER"
  fi
else
  echo
  printf '%s\n' "$ALLE_DATEIEN"
  echo
  echo "   Fundstellen einzeln: $0 --liste"
fi

exit 1
