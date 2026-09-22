#!/usr/bin/env bash
# audit-gate.sh — deterministisches Abhängigkeits-Gate vor jedem Bau/Deploy.
#
# WARUM (2026-09-12): Ein zweiter Prüfer teilt die blinden Flecken des ersten;
# ein Scanner nicht. Dieses Skript ist der Scanner: `pnpm audit` über die
# JavaScript-Seite und `cargo audit` über die Rust-Seite. Es ist ein RATCHET,
# kein Alles-oder-nichts: bekannte, geerbte Advisories stehen in
# scripts/audit-baseline.txt und blockieren nicht; jeder NEUE Treffer blockiert.
# So kann der Deploy heute laufen (Stand 12.09.: 15 High in Prod-Abhängigkeiten
# aus dem Original) und jeder Zuwachs wird laut.
#
# ZWEI SCHICHTEN (Opus-Review 12.09.): Prod-Abhängigkeiten sind das Gate (Exit 1
# bei Neuem). Der volle Baum inkl. Dev-Werkzeuge wird zusätzlich GEMELDET, nicht
# geblockt — vite & Co. bündeln das Frontend, sitzen also in der Vertrauenskette,
# aber nicht im ausgelieferten Bundle.
#
# EIN LEERER SCAN IST KEIN GRÜNER SCAN (R8, Opus-Review 12.09., Blocker): jeder
# Scanner muss sein JSON mit dem erwarteten Schlüssel liefern, sonst bricht das
# Gate mit UNGEMESSEN ab. Findet der Scan weniger als die Baseline kennt, ist das
# eine Warnung (Abhängigkeit repariert ODER Scan halb blind), nie stilles Grün.
#
# Aufrufe:
#   scripts/audit-gate.sh              Gate: Exit 0 = nur Bekanntes, Exit 1 = neue Treffer, Exit 2 = ungemessen
#   scripts/audit-gate.sh --baseline   schreibt den aktuellen Stand als Baseline (nur größer/gleich, sonst --force)
#   scripts/audit-gate.sh --baseline --force
#   scripts/audit-gate.sh --liste      zeigt alle Treffer inkl. bekannter (Prod + voller Baum)
#
# Umgebung: MITSCHNITT_AUDIT_SKIP=1 überspringt das Gate mit lauter Meldung (Notausgang,
# nie der Standard).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE="$REPO_ROOT/scripts/audit-baseline.txt"
MODE="${1:-gate}"
FORCE="${2:-}"
case "$MODE" in gate|--baseline|--liste) ;; *) echo "FEHLER: unbekannter Aufruf '$MODE' (gate | --baseline [--force] | --liste)" >&2; exit 2 ;; esac

if [ "${MITSCHNITT_AUDIT_SKIP:-0}" = "1" ]; then
  echo "⚠️  AUDIT-GATE ÜBERSPRUNGEN (MITSCHNITT_AUDIT_SKIP=1). Das ist ein Notausgang, kein Zustand." >&2
  exit 0
fi

command -v pnpm >/dev/null || { echo "FEHLER: pnpm fehlt — UNGEMESSEN" >&2; exit 2; }
command -v python3 >/dev/null || { echo "FEHLER: python3 fehlt — UNGEMESSEN" >&2; exit 2; }
if ! cargo audit --version >/dev/null 2>&1; then
  echo "FEHLER: cargo-audit fehlt — Rust-Seite UNGEMESSEN. Installieren: cargo install cargo-audit --locked" >&2
  exit 2
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

# --- pnpm: Parser verlangt den Schlüssel "advisories"; fehlt er, ist der Scan kaputt. ---
# pnpm audit endet mit Exit 1, sobald es Treffer gibt; das JSON ist trotzdem vollständig.
# Ein Netzfehler liefert KEIN JSON — das fängt der Parser, nicht der Exit-Code.
pnpm_scan() { # $1 = "--prod" | ""  $2 = out-file
  pnpm --dir "$REPO_ROOT" audit $1 --json >"$TMP/pnpm-raw.json" 2>"$TMP/pnpm.err" || true
  python3 - "$TMP/pnpm-raw.json" >"$2" 2>/dev/null <<'PY' || { echo "FEHLER: pnpm audit lieferte kein auswertbares JSON — UNGEMESSEN:" >&2; head -c 400 "$TMP/pnpm.err" >&2; echo >&2; exit 2; }
import json, sys
d = json.load(open(sys.argv[1]))
if "advisories" not in d:
    raise SystemExit("kein advisories-Schluessel")
for a in d["advisories"].values():
    if a.get("severity") not in ("high", "critical"):
        continue
    ghsa = a.get("github_advisory_id") or f"npm-{a.get('id')}"
    print(f"js\t{ghsa}\t{a['severity']}\t{a.get('module_name','?')}")
PY
}
pnpm_scan "--prod" "$TMP/pnpm-prod.ids"
pnpm_scan ""       "$TMP/pnpm-all.ids"

# --- cargo: Parser verlangt "vulnerabilities". ---
( cd "$REPO_ROOT" && cargo audit --json >"$TMP/cargo-raw.json" 2>"$TMP/cargo.err" ) || true
python3 - "$TMP/cargo-raw.json" >"$TMP/cargo.ids" 2>/dev/null <<'PY' || { echo "FEHLER: cargo audit lieferte kein auswertbares JSON — UNGEMESSEN:" >&2; head -c 400 "$TMP/cargo.err" >&2; echo >&2; exit 2; }
import json, sys
d = json.load(open(sys.argv[1]))
if "vulnerabilities" not in d:
    raise SystemExit("kein vulnerabilities-Schluessel")
for v in d["vulnerabilities"].get("list", []):
    adv = v.get("advisory", {})
    print(f"rust\t{adv.get('id','?')}\t{(adv.get('cvss') or 'n/a')}\t{v.get('package',{}).get('name','?')}")
PY

cat "$TMP/pnpm-prod.ids" "$TMP/cargo.ids" | sort -u >"$TMP/prod.ids"
cat "$TMP/pnpm-all.ids"  "$TMP/cargo.ids" | sort -u >"$TMP/all.ids"
TOTAL=$(wc -l <"$TMP/prod.ids" | tr -d ' ')
TOTAL_ALL=$(wc -l <"$TMP/all.ids" | tr -d ' ')

# Baseline lesen (Kommentare raus; eine leere Datei ist erlaubt und darf grep nicht sterben lassen).
touch "$BASELINE"
{ grep -v '^#' "$BASELINE" || true; } | cut -f1,2 | sort -u >"$TMP/known.ids"
KNOWN_TOTAL=$(wc -l <"$TMP/known.ids" | tr -d ' ')

case "$MODE" in
  --baseline)
    if [ "$TOTAL" -lt "$KNOWN_TOTAL" ] && [ "$FORCE" != "--force" ]; then
      echo "FEHLER: neue Baseline ($TOTAL) wäre kleiner als die bestehende ($KNOWN_TOTAL). Entweder ist eine Abhängigkeit wirklich repariert (dann --force), oder der Scan ist halb blind." >&2
      exit 2
    fi
    { echo "# audit-baseline.txt — bekannte, bewusst geduldete Advisories (Ratchet-Stand, nur Prod-Abhängigkeiten)."
      echo "# Neu schreiben nur mit Absicht: scripts/audit-gate.sh --baseline [--force], dann committen."
      echo "# Stand: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
      cut -f1,2 "$TMP/prod.ids"; } >"$BASELINE"
    echo "Baseline geschrieben: $TOTAL Advisories -> $BASELINE"
    exit 0 ;;
  --liste)
    echo "Prod (Gate): $TOTAL"; cat "$TMP/prod.ids"
    echo; echo "Voller Baum inkl. Dev (nur Meldung): $TOTAL_ALL"; comm -13 "$TMP/prod.ids" "$TMP/all.ids"
    exit 0 ;;
esac

cut -f1,2 "$TMP/prod.ids" | sort -u >"$TMP/now.ids"
NEW=$(comm -13 "$TMP/known.ids" "$TMP/now.ids" || true)
KNOWN_COUNT=$(comm -12 "$TMP/known.ids" "$TMP/now.ids" | wc -l | tr -d ' ')
GONE=$(comm -23 "$TMP/known.ids" "$TMP/now.ids" | wc -l | tr -d ' ')
DEV_ONLY=$(comm -13 "$TMP/prod.ids" "$TMP/all.ids" | wc -l | tr -d ' ')

if [ -n "$NEW" ]; then
  echo "❌ AUDIT-GATE: neue High/Critical-Advisories in Prod-Abhängigkeiten:" >&2
  while IFS=$'\t' read -r eco id; do
    awk -F'\t' -v e="$eco" -v i="$id" '$1==e && $2==i' "$TMP/prod.ids" >&2
  done <<<"$NEW"
  echo "Beheben (Update/Override), dann erneut laufen lassen. Bewusst dulden: scripts/audit-gate.sh --baseline + Commit." >&2
  exit 1
fi
if [ "$GONE" -gt 0 ]; then
  echo "⚠️  AUDIT-GATE: $GONE Baseline-Einträge werden nicht mehr gefunden — entweder repariert (Baseline mit --baseline --force nachziehen) oder der Scan sieht weniger als vorher." >&2
fi
echo "✅ AUDIT-GATE: keine neuen Advisories in Prod ($KNOWN_COUNT bekannte, $TOTAL gesamt). Voller Baum inkl. Dev: $DEV_ONLY weitere, nur gemeldet."
