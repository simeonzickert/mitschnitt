#!/usr/bin/env bash
# mitschnitt-oeffentlich-exportieren.sh — der Baum von HEAD als frisches,
# geschrubbtes Startverzeichnis fuer ein NEUES oeffentliches Repo.
#
# WARUM (22.09.2026): Die heutige git-Historie dieses privaten Forks traegt
# Kunden- und Kollegennamen in Commit-Botschaften und alten Dateiständen. Ein
# oeffentliches Repo bekommt deshalb NIE diese Historie -- es bekommt einen
# einzigen frischen Startcommit aus dem Baum, den dieses Skript hier baut.
# Dieses Skript pusht nichts, legt kein GitHub-Repo an und loescht nichts
# Bestehendes; es erzeugt nur einen neuen Zielordner.
#
# WAS PASSIERT:
#   1. `git archive HEAD` in den Zielordner entpacken (nur der commitete
#      Stand, keine unversionierten oder ignorierten Dateien -- die stecken
#      in einem Archiv von HEAD ohnehin nie drin).
#   2. Bekannte Ausschluesse anwenden (siehe AUSSCHLUSS_PFADE unten) und einen
#      `.github/WORKFLOWS.md` an die Stelle der entfernten Workflows legen.
#   3. Verdaechtige Dateien AUFLISTEN (Testaudio, Datenbanken, Schluessel,
#      Sicherungen) -- nichts davon automatisch entfernen. Diese Entscheidung
#      gehoert einem Menschen, nicht diesem Skript.
#   4. Drei Gates, jedes bricht bei Treffer sofort ab (Exit 1):
#        a) Klarnamen-Waechter (scripts/namens-scrub-check.sh) ueber den
#           Zielordner, der dafuer ein eigenes kleines git-Repo bekommt --
#           der Waechter selbst sucht per `git grep` in SEINEM eigenen Baum
#           und wird deshalb nicht veraendert, sondern nur an einen Ort
#           gestellt, an dem seine eigenen Annahmen (git-Repo, crates/+apps/
#           im Wurzelverzeichnis) erfuellt sind.
#        b) Geheimnis-Scan (in diesem Skript): API-Schluessel-Muster + echte
#           Mailadressen ausserhalb einer engen Freigabeliste + Wertabgleich
#           gegen lokale .env-Dateien (die Claude-Code-Konfiguration, dieses
#           Repo, das urspruengliche Arbeitsverzeichnis).
#        c) LICENSE + README.md (mit Fork-Hinweis) + ATTRIBUTIONS.md muessen
#           im Zielordner liegen.
#   5. Bei drei gruenen Gates: `git init` + EIN Startcommit im Zielordner.
#
# Aufruf:
#   scripts/mitschnitt-oeffentlich-exportieren.sh <zielordner>
#
# Der Zielordner darf nicht existieren oder muss leer sein -- dieses Skript
# raeumt nie einen bestehenden, nicht-leeren Ordner weg (das waere genau das
# Loeschen fremden Inhalts, das hier verboten ist).
#
# Umgebung:
#   MITSCHNITT_NAMENSLISTE   Pfad zur Namensliste fuer Gate (a). PFLICHT, wie
#                            in namens-scrub-check.sh selbst: kein Default in
#                            diesem oeffentlichen Skript, damit kein privater
#                            Pfad im veroeffentlichten Quelltext steht. Wer
#                            dieses Skript aufruft, setzt die Variable vorher.
#
# Exit 0 = Export gebaut, alle drei Gates gruen, Startcommit liegt.
# Exit 1 = ein Gate hat einen echten Treffer gemeldet.
# Exit 2 = der Export konnte gar nicht erst gebaut werden (falscher Aufruf,
#          Zielordner belegt, `git archive` fehlgeschlagen, Werkzeug fehlt).
set -euo pipefail

HIER="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HIER/.." && pwd)"

# ---------------------------------------------------------------- Argumente
if [ "$#" -ne 1 ]; then
  echo "🔴 Aufruf: MITSCHNITT_NAMENSLISTE=<pfad> $0 <zielordner>" >&2
  exit 2
fi
ZIEL="$1"

# Fail fast, bevor irgendetwas gebaut wird: kein Default fuer diese Variable
# in einem Skript, das selbst Teil des oeffentlichen Exports wird -- ein
# privater Pfad gehoert nicht in oeffentlichen Quelltext (namens-scrub-check.sh
# verlangt dieselbe Disziplin von sich selbst, siehe dessen eigener Kopf).
if [ -z "${MITSCHNITT_NAMENSLISTE:-}" ]; then
  echo "🔴 MITSCHNITT_NAMENSLISTE ist nicht gesetzt. Es wurde NICHTS gebaut." >&2
  echo "   Aufruf: MITSCHNITT_NAMENSLISTE=<pfad-ausserhalb-des-repos> $0 <zielordner>" >&2
  exit 2
fi
export MITSCHNITT_NAMENSLISTE

if [ -e "$ZIEL" ]; then
  if [ ! -d "$ZIEL" ]; then
    echo "🔴 $ZIEL existiert und ist kein Ordner. Es wurde NICHTS gebaut." >&2
    exit 2
  fi
  if [ -n "$(ls -A "$ZIEL" 2>/dev/null)" ]; then
    echo "🔴 $ZIEL existiert bereits und ist nicht leer." >&2
    echo "   Dieses Skript raeumt nie einen bestehenden Ordner weg -- das waere" >&2
    echo "   das Loeschen fremden Inhalts. Einen neuen, leeren Zielpfad waehlen." >&2
    exit 2
  fi
else
  mkdir -p "$ZIEL"
fi
ZIEL="$(cd "$ZIEL" && pwd)"

if ! git -C "$REPO" rev-parse --verify HEAD >/dev/null 2>&1; then
  echo "🔴 $REPO hat keinen HEAD-Commit. Es wurde NICHTS gebaut." >&2
  exit 2
fi

log() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

# ------------------------------------------------------------- 1) Archivieren
log "Exportiere HEAD ($(git -C "$REPO" rev-parse --short HEAD)) nach $ZIEL"
git -C "$REPO" archive --format=tar HEAD | tar -x -C "$ZIEL"

DATEIEN_ROH=$(find "$ZIEL" -type f | wc -l | tr -d ' ')
echo "   $DATEIEN_ROH Dateien aus dem HEAD-Archiv entpackt."

# --------------------------------------------------------- 2) Ausschluesse
# Jeder Eintrag hier traegt seinen eigenen Grund. Nichts wird stillschweigend
# ausgelassen -- wer diese Liste liest, weiss, was fehlt und warum.
#
#   .github/workflows/          10 geerbte Upstream-CI-Rezepte mit
#                                Cloud-Zugangsdaten-Erwartungen (Secrets,
#                                Runner, Deploy-Ziele), die fuer dieses
#                                Repo nicht gelten. Ersatz: .github/WORKFLOWS.md
#                                unten.
#   .github/reports/            Interne Berichte des Originalprojekts ueber
#                                DESSEN eigene Funktionen (z.B. der
#                                Legal-Review-Bericht vom April, der Features
#                                bewertet, die es in diesem Fork nicht gibt --
#                                bereits aus dem Repo entfernt, hier nur als
#                                zweite Sicherung fuer den Fall, dass die
#                                Datei je zurueckkommt).
#   (scripts/mitschnitt-notarisieren.sh bleibt drin: seit 22.09. ohne Klarnamen
#                                und Pfade als Vorgabewerte, alles Pflicht-Variablen.)
#   Weitere Upstream-Reste (22.09.2026, oeffentliche Repo-Seite): die
#                                CI-Bausteine hinter den entfernten Workflows
#                                (.github/actions, .github/scripts), die
#                                CODEOWNERS- und AGENTS-Dateien des Originals
#                                unter .github, die Plugin-Marktplaetze von
#                                Fastrepl (.claude-plugin, .cursor-plugin,
#                                .agents/plugins, zeigen auf ein Verzeichnis,
#                                das es hier nicht gibt), die Expo-MCP-
#                                Konfiguration des Mobil-Clients (.mcp.json,
#                                .claude/settings.json), die Codex-Cloud-
#                                Umgebung (.codex) und die Versionsnotiz-
#                                Konfiguration der Upstream-Pipeline
#                                (doxxer.desktop.toml). Nichts davon wird
#                                gebaut oder gelesen, gemessen per git grep.
AUSSCHLUSS_PFADE=(
  ".github/workflows"
  ".github/reports"
  ".github/actions"
  ".github/scripts"
  ".github/CODEOWNERS"
  ".github/AGENTS.md"
  ".claude-plugin"
  ".cursor-plugin"
  ".agents/plugins"
  ".claude/settings.json"
  ".mcp.json"
  ".codex"
  "doxxer.desktop.toml"
)

log "Entferne bekannte Ausschluesse"
for pfad in "${AUSSCHLUSS_PFADE[@]}"; do
  ziel_pfad="$ZIEL/$pfad"
  if [ -e "$ziel_pfad" ]; then
    rm -rf "$ziel_pfad"
    echo "   entfernt: $pfad"
  fi
done

# Defensiv, falls doch mal etwas Ignoriertes im Baum landet (sollte durch
# `git archive HEAD` bereits ausgeschlossen sein -- das Archiv kennt nur
# committete Dateien, .gitignore spielt dafuer keine Rolle mehr. Diese
# Schleife ist ein zweites Netz, kein Ersatz dafuer).
for muster in "node_modules" "target" ".env" ".env.*"; do
  while IFS= read -r -d '' fund; do
    rm -rf "$fund"
    echo "   defensiv entfernt: ${fund#"$ZIEL"/}"
  done < <(find "$ZIEL" -mindepth 1 -iname "$muster" ! -iname ".env.sample" -print0 2>/dev/null)
done

mkdir -p "$ZIEL/.github"
cat > "$ZIEL/.github/WORKFLOWS.md" <<'EOF'
# Why there are no workflows here

Mitschnitt is built, signed and notarized locally with `scripts/mitschnitt-release.sh`.
The CI pipeline of the project it started from expected that project's own cloud
credentials, runners and deploy targets, so it was removed rather than left to fail.
To build from source, see the README.
EOF
echo "   geschrieben: .github/WORKFLOWS.md"

# ----------------------------------------------- 3) Kandidaten nur AUFLISTEN
# Testaudio, Datenbanken, Schluessel, Sicherungen: koennen legitime
# Testfixtures sein (das sind die meisten hier) oder nicht hierher gehoeren.
# Dieses Skript entscheidet das nicht -- es zeigt die Liste.
log "Kandidaten fuer eine Ja/Nein-Entscheidung (nichts davon wurde entfernt)"
KANDIDATEN_MUSTER='\.(db|sqlite3?|wav|mp3|mp4|m4a|mov|key|pem|p12|pfx|bak|orig)$|(^|/)\.env(\..*)?$|id_rsa|id_ed25519|backup'
KANDIDATEN=$(cd "$ZIEL" && find . -type f | sed 's#^\./##' | grep -viE '\.(png|jpe?g|gif|svg|webp|ico|icns|bmp)$' | grep -iE "$KANDIDATEN_MUSTER" | sort || true)
if [ -z "$KANDIDATEN" ]; then
  echo "   keine."
else
  while IFS= read -r pfad; do
    groesse=$(du -h "$ZIEL/$pfad" 2>/dev/null | cut -f1)
    echo "   ? $pfad ($groesse)"
  done <<< "$KANDIDATEN"
fi

# ------------------------------------------------------------------ Gate a
log "Gate a: Klarnamen-Waechter (scripts/namens-scrub-check.sh)"
# Der Waechter sucht per `git grep` im Baum, der SEINEN eigenen Dateipfad
# enthaelt -- er wird hier nicht veraendert, sondern in einen Ordner
# gestellt, der seine eigenen Annahmen erfuellt: ein git-Repo mit crates/
# und apps/ im Wurzelverzeichnis (beide sind Teil des Exports). Tracked
# reicht ihm: `git grep` ohne Revision durchsucht auch nur vorgemerkten,
# noch nicht committeten Inhalt. (MITSCHNITT_NAMENSLISTE ist bereits weiter
# oben geprueft und exportiert.)

if [ ! -d "$ZIEL/.git" ]; then
  git -C "$ZIEL" init -q -b main
fi
git -C "$ZIEL" add -A

if ! bash "$ZIEL/scripts/namens-scrub-check.sh" --liste; then
  echo
  echo "🔴 GATE A ROT -- Export bricht ab, nichts wird committet." >&2
  exit 1
fi

# ------------------------------------------------------------------ Gate b
log "Gate b: Geheimnis-Scan"
# Drei unabhaengige Faelle: (0) Dateiendungen, die typischerweise Geheimnisse
# BINAER tragen (Zertifikate, Datenbanken, Archive) -- `git grep -I` ueber-
# springt binaere Dateien stillschweigend, deshalb ist das ein eigener,
# ausdruecklicher Check, kein Nebeneffekt der Textsuche; (1) Textmuster, die
# wie ein API-Schluessel aussehen, ueberall im Baum; (2) Mailadressen
# ausserhalb einer engen Freigabeliste. Lockdateien/generierte Quellenlisten
# sind aus der MAIL-Suche ausgenommen -- dieselbe Begruendung wie in
# namens-scrub-check.sh: sie tragen Adressen fremder Paketautoren, die so
# wenig zur Disposition stehen wie in einer Lizenzdatei, und aendern sich bei
# jedem Abhaengigkeits-Update. Modellgewichte sind ausgenommen -- dieselbe
# Begruendung wie dort: binaer, und ihr Zufallsrauschen erzeugt zufaellig
# mailfoermige Zeichenfolgen.
SCHLUESSEL_MUSTER='sk-[A-Za-z0-9]{10,}|sk-proj-[A-Za-z0-9_-]{10,}|AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|lin_api_[A-Za-z0-9]{20,}|xox[baprs]-[A-Za-z0-9-]{10,}|AIza[0-9A-Za-z_-]{35}|eyJ[A-Za-z0-9_-]+\.eyJ|Bearer [A-Za-z0-9._-]{20,}|-----BEGIN[ A-Z]*PRIVATE KEY-----'
MAIL_MUSTER='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
# vendor/mp3lame-sys/lame-3.100/ ist derselbe Ausschluss wie in
# namens-scrub-check.sh und aus demselben Grund: unveraenderter LAME-
# Upstream-Quelltext, dessen ChangeLog/AUTHORS/configure ~250 echte
# Mailadressen fremder Entwickler tragen -- Urheberangaben eines
# Fremdprojekts, keine Leaks aus diesem Fork. Gate a hatte diesen Ausschluss
# bereits; Gate b lief bis 22.09.2026 nie bis zum Ende durch (der `set -e`-
# Fehler in git_grep_status weiter oben brach vorher immer ab), deshalb fiel
# die fehlende Symmetrie zwischen beiden Gates erst jetzt auf.
BINAER_AUSSCHLUSS=(':(exclude)*.onnx' ':(exclude)*.safetensors' ':(exclude)*.gguf' ':(exclude)*.bin' ':(exclude)vendor/mp3lame-sys/lame-3.100/**')

# `git grep` gibt 0 bei Treffern, 1 bei keinem Treffer und >=2 bei einem
# echten Fehler (kaputtes PCRE-Muster, ungueltige Flags). Ein blosses
# `git grep ... || true` (wie hier bis 22.09.2026 an drei Stellen) verwischt
# den Unterschied unter diesem Skripts `set -euo pipefail`: Status >=2 landet
# genauso als "leere Ausgabe" wie Status 1 (kein Treffer) -- das Gate meldet
# dann "sauber", OHNE dass ueberhaupt gesucht wurde. `-e` wird deshalb genau
# fuer den einen fehlschlagbaren Aufruf ausgeschaltet, der Status separat
# gelesen (wie `suche()` in namens-scrub-check.sh), und nur >=2 gilt als
# echter Fehler.
git_grep_status() {
  set +e
  GIT_GREP_AUSGABE=$(git -C "$ZIEL" grep "$@" 2>&1)
  GIT_GREP_STATUS=$?
  set -e
  if [ "$GIT_GREP_STATUS" -eq 1 ]; then
    GIT_GREP_AUSGABE=""
  fi
  # Diese Funktion IMMER mit 0 verlassen: der Aufrufer prueft GIT_GREP_STATUS
  # selbst, nicht den Rueckgabewert der Funktion. Ohne dieses `return 0` traegt
  # die Funktion den Exit-Status ihrer letzten Zeile weiter -- bei einem echten
  # Treffer (Status 0) ist das `if` oben falsch, und unter `set -e` wuerde ein
  # bloss "falsches" `if` (ohne eigenes else) den Aufruf abbrechen, und zwar
  # STILL, ohne die roten Meldungen weiter unten je zu erreichen (selbst so
  # erlitten: 22.09.2026, derselbe Fehler, den dieses Skript beim `|| true`
  # gerade beheben sollte, hier neu eingebaut).
  return 0
}

# Erlaubt: generische RFC-2606-Beispieldomains (inkl. .test/.invalid/.localhost
# -- dieselben drei wie in namens-scrub-check.sh, hier zunaechst vergessen und
# im ersten Probelauf an drei echten Testdateien aufgefallen), jede
# noreply@-Adresse, die drei Upstream-Domains, aus denen dieser Fork stammt,
# und die oeffentliche Kontaktadresse dieses Forks -- die wird bewusst NICHT
# hier als Literal eingetragen, sondern zur Laufzeit aus SECURITY.md gelesen
# (eine Quelle der Wahrheit statt einer zweiten Kopie im Quelltext).
MAIL_ERLAUBT_STATISCH='@([a-z0-9-]+\.)*example\.(com|net|org)$|@([a-z0-9-]+\.)*example$|\.(test|invalid|localhost)$|^noreply@|@([a-z0-9-]+\.)*(anarlog\.so|hyprnote\.com|fastrepl\.com)$|@users\.noreply\.github\.com$'
KONTAKT_AUS_SECURITY_MD=""
if [ -f "$ZIEL/SECURITY.md" ]; then
  KONTAKT_AUS_SECURITY_MD=$(grep -oE "$MAIL_MUSTER" "$ZIEL/SECURITY.md" | head -1)
fi
if [ -n "$KONTAKT_AUS_SECURITY_MD" ]; then
  KONTAKT_ESCAPED=$(printf '%s' "$KONTAKT_AUS_SECURITY_MD" | perl -pe 's/([.^$|()\[\]{}*+?\\])/\\$1/g')
  MAIL_ERLAUBT="^${KONTAKT_ESCAPED}\$|${MAIL_ERLAUBT_STATISCH}"
else
  MAIL_ERLAUBT="$MAIL_ERLAUBT_STATISCH"
fi
MAIL_AUSSCHLUSS_PFADE=(
  "${BINAER_AUSSCHLUSS[@]}"
  ':(exclude)*.lock' ':(exclude)*-lock.json' ':(exclude)*.lockb'
  ':(exclude)pnpm-lock.yaml' ':(exclude)**/cargo-sources.json'
  ':(exclude)**/pnpm-sources.json'
)

# Zwei sehr enge, dateigenaue Ausnahmen -- keine Domain-Freigabe, weil das zu
# weit oeffnen wuerde, sondern genau die zwei Dateien, in denen eine fremde
# Autorenadresse zur SACHE gehoert: eine SIL-Font-Lizenz (der Namensnennungs-
# pflicht der Lizenz selbst) und die pyproject.toml des Originalprojekts
# (Autorenangabe eines Upstream-Werkzeugs, kein eigener Code). Gefunden im
# ersten Probelauf, nicht vorher bekannt.
MAIL_AUSNAHME_PFADE_MUSTER='^plugins/windows/swift-lib/src/Resources/OFL\.txt$|^scripts/pyproject\.toml$'

# Genaue Ausnahmen fuer den Schluessel-Musterfund, nach demselben Verfahren
# wie ERLAUBTE_ZEILEN in namens-scrub-check.sh: Pfad-SHA-256 + SHA-256 der
# getrimmten Zeile, NIE ein Muster ueber "sieht nach Testcode aus". Bis
# 22.09.2026 stand hier stattdessen eine Heuristik ("ein BEGIN ohne
# Base64-Rumpf IN DERSELBEN ZEILE ist ein Test-Marker") -- das ist bei einer
# ECHTEN PEM-Datei so gut wie immer wahr, weil der Base64-Rumpf einer echten
# Datei erst in den Zeilen NACH der BEGIN-Zeile steht. Die Heuristik haette
# also gerade den Fall durchgelassen, den sie fangen soll.
#
# Neu aufnehmen -- beide Haelften aus dem Repo-Wurzelverzeichnis:
#   printf '%s' "<pfad>"            | shasum -a 256
#   printf '%s' "<getrimmte zeile>" | shasum -a 256
#
# Sechs Zeilen hier: die PEM-Testzeile oben, plus fuenf Redaction-Testfixtures
# (apps/desktop/src/error-reporting.test.ts, crates/settings-transfer/tests/
# file_probe.rs, plugins/tracing/src/redaction.rs), gefunden beim ersten
# Probelauf NACH der Musterweitung um sk-proj-/Bearer/JWT (22.09.2026) --
# jede davon testet mit einem erfundenen Platzhalter genau die Funktion, die
# ein echtes Geheimnis spaeter redigieren soll.
ERLAUBTE_ZEILEN_SCHLUESSEL=$(cat <<'EOF'
182687ce561eba3fb4bc8007c2d89c86af28d5d05fac0527d2b3f6254d42eb6a	16932de42d9969ed1e3ff181de7ab95faaf55f9431c36b74bb1d75fd5f28e029
0b08d0867166a9899e23b8adc88f57ae5b52927088b23583069a52c4fde5c1f8	ac6ec246966e8e3a747e1fbeb64ae734a9e3958726b19960187bc23e7c794c47
0b08d0867166a9899e23b8adc88f57ae5b52927088b23583069a52c4fde5c1f8	b3fe22309d20be6819f293378f741b20fb4544524d96206ab5f302e513d8e9a6
5d40926e58c0de5fef16028000c083f4a249d4b0392d95c227c4037e5af8286d	e9b8fc2195fb102f933225ef0dd0db222480690a53f7f1e36e6d21510e85f86d
2d917a1f465b8a21191a794c8217f23f1f36fc56765050cbeed583d6603a6959	ffad4a52f3f32ccb53b6b0f6373a711f777fbf98c9f44b023fc4d54623243244
2d917a1f465b8a21191a794c8217f23f1f36fc56765050cbeed583d6603a6959	e95623f0163f70afc2323b1b87a4d5b248a566846db8982295427ebd9a0b5aa5
EOF
)

# Teil 0: Dateiendungen, die Geheimnisse BINAER tragen. `git grep -I`
# uebergeht binaere Dateien lautlos -- ein echtes .p12-Zertifikat oder eine
# .sqlite-Datenbank mit Zugangsdaten waere fuer die Textsuche unsichtbar und
# landete bislang nur als unverbindlicher Kandidat in der Auflistung weiter
# oben. Hier ist es ein hartes Gate: Fund = Abbruch, keine Ja/Nein-Frage.
GEHEIMNIS_ENDUNGEN=(p12 pfx key pem keychain mobileprovision db sqlite sqlite3 zip)
GEHEIMNIS_DATEIEN=()
while IFS= read -r -d '' fund; do
  GEHEIMNIS_DATEIEN+=("${fund#"$ZIEL"/}")
done < <(
  finde_muster=()
  for endung in "${GEHEIMNIS_ENDUNGEN[@]}"; do
    finde_muster+=(-o -iname "*.$endung")
  done
  find "$ZIEL" -type f \( "${finde_muster[@]:1}" \) -print0 2>/dev/null
)

git_grep_status -n -I -i -P -e "$SCHLUESSEL_MUSTER" -- . "${BINAER_AUSSCHLUSS[@]}"
if [ "$GIT_GREP_STATUS" -ge 2 ]; then
  echo "🔴 Gate b: git grep (Schluessel-Muster) ist fehlgeschlagen (Status $GIT_GREP_STATUS)." >&2
  echo "   $GIT_GREP_AUSGABE" >&2
  echo "🔴 GATE B ROT -- Export bricht ab, nichts wird committet." >&2
  exit 1
fi
SCHLUESSEL_ROH="$GIT_GREP_AUSGABE"
SCHLUESSEL_TREFFER=$(
  printf '%s' "$SCHLUESSEL_ROH" | perl -MDigest::SHA=sha256_hex -e '
    my ($erlaubte) = @ARGV;
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
      print "$ort:$nr:$text\n";
    }
  ' "$ERLAUBTE_ZEILEN_SCHLUESSEL"
) || { echo "🔴 Gate b: die Schluessel-Nachfilterung (perl) ist fehlgeschlagen." >&2; exit 1; }

git_grep_status -n -I -P -e "$MAIL_MUSTER" -- . "${MAIL_AUSSCHLUSS_PFADE[@]}"
if [ "$GIT_GREP_STATUS" -ge 2 ]; then
  echo "🔴 Gate b: git grep (Mailadressen) ist fehlgeschlagen (Status $GIT_GREP_STATUS)." >&2
  echo "   $GIT_GREP_AUSGABE" >&2
  echo "🔴 GATE B ROT -- Export bricht ab, nichts wird committet." >&2
  exit 1
fi
MAIL_ROH="$GIT_GREP_AUSGABE"

MAIL_TREFFER=$(
  printf '%s' "$MAIL_ROH" | perl -e '
    my ($muster, $erlaubt, $keinemail, $ausnahmepfad) = @ARGV;
    while (my $zeile = <STDIN>) {
      chomp $zeile;
      next unless $zeile =~ /^([^:]*):(\d+):(.*)$/s;
      my ($ort, $nr, $text) = ($1, $2, $3);
      next if $ort =~ /$ausnahmepfad/;
      while ($text =~ /($muster)/g) {
        my $adr = $1;
        next if $adr =~ /$keinemail/i;
        next if $adr =~ /$erlaubt/i;
        print "$ort:$nr: $adr\n";
      }
    }
  ' "$MAIL_MUSTER" "$MAIL_ERLAUBT" '\.(png|jpe?g|gif|svg|webp|ico|icns|bmp)$' "$MAIL_AUSNAHME_PFADE_MUSTER" | sort -u
) || { echo "🔴 Gate b: die Mail-Nachfilterung (perl) ist fehlgeschlagen." >&2; exit 1; }

# Wertabgleich: liegt einer der KONKRETEN Werte aus lokalen .env-Quellen
# (nie deren Zeilen selbst, nur die extrahierten Werte) im exportierten
# Baum? Diese Dateien werden NUR gelesen, um Werte herauszuziehen -- ihr
# Inhalt landet nie im Bericht. $HOME statt eines festen Nutzerpfads, damit
# hier kein privater Pfad im oeffentlichen Quelltext dieses Skripts steht.
#
# Ein Wert, der selbst wie eine bereits erlaubte Mailadresse aussieht (z.B.
# wenn ein Dienst-Login zufaellig die oeffentliche Kontaktadresse ist), ist
# kein Fund -- SECURITY.md DARF diese Adresse tragen, das ist ihr Zweck.
WERT_TREFFER=""
shopt -s nullglob
ENV_QUELLEN=("$HOME/.claude/.env" "$REPO"/.env "$REPO"/.env.* "$HOME/Code/mitschnitt"/.env "$HOME/Code/mitschnitt"/.env.*)
shopt -u nullglob
for envdatei in "${ENV_QUELLEN[@]}"; do
  [ -f "$envdatei" ] || continue
  while IFS='=' read -r schluessel wert; do
    case "$schluessel" in ''|'#'*) continue ;; esac
    # Beide Anfuehrungszeichen-Arten strippen: ein Wert wie KEY='abc...'
    # wurde bis 22.09.2026 NUR um doppelte Anfuehrungszeichen gekuerzt und
    # damit samt der einfachen Quotes gesucht -- eine Suche, die praktisch
    # nie einen Treffer findet, weil der Wert im exportierten Baum ohne
    # umschliessende Quotes vorkaeme.
    wert="${wert%\"}"; wert="${wert#\"}"
    wert="${wert%\'}"; wert="${wert#\'}"
    [ "${#wert}" -lt 12 ] && continue
    printf '%s' "$wert" | grep -qiE "$MAIL_ERLAUBT" 2>/dev/null && continue
    git_grep_status -F -n -I -- "$wert"
    if [ "$GIT_GREP_STATUS" -ge 2 ]; then
      echo "🔴 Gate b: git grep (.env-Wertabgleich, Schluessel $schluessel aus $(basename "$envdatei")) ist fehlgeschlagen (Status $GIT_GREP_STATUS)." >&2
      echo "   $GIT_GREP_AUSGABE" >&2
      echo "🔴 GATE B ROT -- Export bricht ab, nichts wird committet." >&2
      exit 1
    fi
    treffer="$GIT_GREP_AUSGABE"
    if [ -n "$treffer" ]; then
      WERT_TREFFER="${WERT_TREFFER}$(printf '%s\n' "$treffer" | sed "s/\$/  <- Wert aus $(basename "$envdatei")::$schluessel/")
"
    fi
  done < "$envdatei"
done

# gitleaks als zusaetzlicher Scan, NUR wenn es bereits installiert ist --
# dieses Skript installiert es nie selbst (Werkzeugwahl auf dem eigenen
# Rechner bleibt eine Entscheidung des Betreibers, nicht dieses Skripts).
# Fehlt es, wird das im Bericht gesagt, das Gate laeuft ohne diesen Scan weiter.
GITLEAKS_TREFFER=""
if command -v gitleaks >/dev/null 2>&1; then
  log "Gate b, zusaetzlich: gitleaks (installiert gefunden)"
  GITLEAKS_LOG="$(mktemp -t mitschnitt-export-gitleaks)"
  set +e
  gitleaks detect --no-git --source "$ZIEL" --exit-code 1 >"$GITLEAKS_LOG" 2>&1
  GITLEAKS_STATUS=$?
  set -e
  if [ "$GITLEAKS_STATUS" -eq 1 ]; then
    GITLEAKS_TREFFER="$(cat "$GITLEAKS_LOG")"
  elif [ "$GITLEAKS_STATUS" -ne 0 ]; then
    GITLEAKS_TREFFER="gitleaks ist mit Status $GITLEAKS_STATUS fehlgeschlagen (kein sauberer Lauf):
$(cat "$GITLEAKS_LOG")"
  fi
  rm -f "$GITLEAKS_LOG"
else
  echo "   gitleaks: nicht installiert -- dieser zusaetzliche Scan lief nicht mit."
  echo "   (Wird von diesem Skript nicht selbst installiert.)"
fi

if [ "${#GEHEIMNIS_DATEIEN[@]}" -gt 0 ] || [ -n "$SCHLUESSEL_TREFFER" ] || [ -n "$MAIL_TREFFER" ] || [ -n "$WERT_TREFFER" ] || [ -n "$GITLEAKS_TREFFER" ]; then
  echo "🔴 Geheimnis-Scan hat Treffer:"
  if [ "${#GEHEIMNIS_DATEIEN[@]}" -gt 0 ]; then
    echo; echo "--- Dateien mit geheimnistragender Endung (${GEHEIMNIS_ENDUNGEN[*]}) ---"
    printf '   %s\n' "${GEHEIMNIS_DATEIEN[@]}"
  fi
  [ -n "$SCHLUESSEL_TREFFER" ] && { echo; echo "--- Schluessel-Muster ---"; printf '%s\n' "$SCHLUESSEL_TREFFER"; }
  [ -n "$MAIL_TREFFER" ] && { echo; echo "--- Mailadressen ---"; printf '%s\n' "$MAIL_TREFFER"; }
  [ -n "$WERT_TREFFER" ] && { echo; echo "--- Werte aus lokalen .env-Dateien ---"; printf '%s\n' "$WERT_TREFFER"; }
  [ -n "$GITLEAKS_TREFFER" ] && { echo; echo "--- gitleaks ---"; printf '%s\n' "$GITLEAKS_TREFFER"; }
  echo
  echo "🔴 GATE B ROT -- Export bricht ab, nichts wird committet." >&2
  exit 1
fi
echo "   sauber: keine geheimnistragenden Dateiendungen, keine Schluessel-Muster,"
echo "   keine unerlaubten Mailadressen, kein Wert aus einer lokalen .env-Datei im Baum."
command -v gitleaks >/dev/null 2>&1 && echo "   gitleaks meldet ebenfalls sauber."

# ------------------------------------------------------------------ Gate c
log "Gate c: LICENSE, README.md, ATTRIBUTIONS.md"
FEHLT=()
[ -f "$ZIEL/LICENSE" ] || FEHLT+=("LICENSE")
[ -f "$ZIEL/README.md" ] || FEHLT+=("README.md")
[ -f "$ZIEL/ATTRIBUTIONS.md" ] || FEHLT+=("ATTRIBUTIONS.md")
if [ -f "$ZIEL/README.md" ] && ! grep -qi "fork" "$ZIEL/README.md"; then
  FEHLT+=("README.md ohne erkennbaren Fork-Hinweis (Wort \"Fork\" fehlt)")
fi
if [ "${#FEHLT[@]}" -gt 0 ]; then
  echo "🔴 GATE C ROT -- fehlt:"
  printf '   - %s\n' "${FEHLT[@]}"
  echo "🔴 Export bricht ab, nichts wird committet." >&2
  exit 1
fi
echo "   LICENSE, README.md (mit Fork-Hinweis) und ATTRIBUTIONS.md liegen."

# ------------------------------------------------------------ Startcommit
# Keine Identitaet hier hardcodieren: der Commit traegt, was `git config
# user.name`/`user.email` auf der aufrufenden Maschine ohnehin liefert (wie
# jeder andere Commit auch). Das haelt eine konkrete Mailadresse aus DIESEM
# Skript heraus, ohne sie irgendwo zu verstecken -- sie steht dort, wo jede
# Git-Identitaet steht: in der Konfiguration, nicht im Quelltext.
log "Alle drei Gates gruen -- Startcommit"
git -C "$ZIEL" add -A
git -C "$ZIEL" commit -q -m "Erster oeffentlicher Stand

Export aus dem privaten Arbeitsbaum (Quell-HEAD $(git -C "$REPO" rev-parse --short HEAD)),
ohne dessen Historie. Diese Historie beginnt hier neu -- die private Historie
davor bleibt privat.

Ausgeschlossen: ${AUSSCHLUSS_PFADE[*]}, alles was .gitignore ohnehin ausschliesst."

echo
echo "   $(git -C "$ZIEL" log -1 --oneline)"
echo "   $(git -C "$ZIEL" status --short | wc -l | tr -d ' ') offene Aenderungen (sollte 0 sein)."

log "Kennzahlen"
echo "   Groesse:      $(du -sh "$ZIEL" | cut -f1)"
echo "   Dateien:      $(find "$ZIEL" -type f -not -path '*/.git/*' | wc -l | tr -d ' ')"
echo "   10 groesste Dateien:"
# `head -10` schliesst die Pipe, sobald es zehn Zeilen hat -- `sort`/`find`
# davor bekommen dann SIGPIPE, sobald sie versuchen, weiter hineinzuschreiben.
# Unter `set -o pipefail` traegt die ganze Pipeline danach Status 141 (128+13)
# weiter, obwohl die Ausgabe laengst vollstaendig und korrekt auf dem
# Bildschirm steht -- das `|| true` haelt das rein kosmetische Ende dieses
# Skripts davon ab, den erfolgreichen Export nachtraeglich als Fehler zu
# meldenden (gemessen 22.09.2026, am ersten Lauf, der ueberhaupt bis hierher
# kam: alle drei Gates waren gruen, der Startcommit lag, und der Exitcode
# haette trotzdem 141 behauptet).
find "$ZIEL" -type f -not -path '*/.git/*' -exec du -h {} + 2>/dev/null | sort -rh | head -10 | sed 's/^/     /' || true

echo
echo "✅ Export liegt unter $ZIEL"
