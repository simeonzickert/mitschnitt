#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/mitschnitt-feed.py
# Beschreibung: CLI zum Erstellen von Plattform-Bausteinen und Zusammenfuehren zu latest.json fuer Tauri-v2-Updater.
# Kommentare auf Deutsch ohne Umlaute (ae/oe/ue/ss).

import argparse
import base64
import json
import os
import re
import sys
import tempfile

ERLAUBTE_PLATTFORMEN = {"darwin-aarch64", "darwin-x86_64", "windows-x86_64"}
VERSION_REGEX = re.compile(r"^\d+\.\d+\.\d+$")


def _lade_pubkey_aus_tauri_conf(script_dir):
    """Lese pubkey aus tauri.conf.json relativ zum Repo-Root (Elternordner von scripts/)."""
    repo_root = os.path.dirname(script_dir)  # Eltern von scripts/
    conf_path = os.path.join(repo_root, "apps", "desktop", "src-tauri", "tauri.conf.json")
    if not os.path.isfile(conf_path):
        return None
    try:
        with open(conf_path, "r", encoding="utf-8") as f:
            conf = json.load(f)
    except (json.JSONDecodeError, OSError):
        return None
    # Pfad: plugins -> updater -> pubkey
    pubkey = conf.get("plugins", {}).get("updater", {}).get("pubkey")
    if pubkey and isinstance(pubkey, str) and pubkey.strip():
        return pubkey.strip()
    return None


def _extrahiere_key_id_aus_pubkey(pubkey_b64):
    """Dekodiere einen minisign-pubkey (base64 einer 2-Zeilen-Textdatei) und gib (key_id_bytes, key_id_hex) zurueck.
    key_id_bytes: 8 Bytes in der Reihenfolge wie in der Datei (Big-Endian? minisign speichert little-endian).
    Wir geben die Bytes so zurueck, wie sie aus der base64 kommen (Algorithmus 2B + Key-ID 8B + Key 32B).
    """
    # Zwei Ebenen: der Tauri-Wert ist base64 einer Textdatei ("untrusted
    # comment: ..." + eine base64-Zeile); erst diese zweite Zeile dekodiert zu
    # den 42 Bytes. Nur einmal zu dekodieren liefert den Dateitext (gemessen
    # 28.09.2026 am echten pubkey: 114 statt 42 Bytes).
    try:
        text = base64.b64decode(pubkey_b64).decode("utf-8")
        zeilen = [z.strip() for z in text.splitlines() if z.strip()]
        if len(zeilen) < 2 or not zeilen[0].startswith("untrusted comment:"):
            raise ValueError("kein minisign-pubkey-Text")
        raw = base64.b64decode(zeilen[1])
    except (base64.binascii.Error, ValueError, UnicodeDecodeError):
        print("FEHLER: Pubkey ist kein gueltiger minisign-pubkey (base64 einer Textdatei)", file=sys.stderr)
        sys.exit(1)
    if len(raw) != 42:
        print(
            f"FEHLER: Pubkey muss 42 Bytes haben (2+8+32), hat {len(raw)} Bytes",
            file=sys.stderr,
        )
        sys.exit(1)
    algo = raw[:2]
    if algo != b"Ed":
        print(
            f"FEHLER: Pubkey-Algorithmus ist nicht 'Ed', sondern {algo!r}",
            file=sys.stderr,
        )
        sys.exit(1)
    key_id_bytes = raw[2:10]  # 8 Bytes, minisign little-endian
    # Hex-Darstellung: Bytes in umgekehrter Reihenfolge wie minisign sie anzeigt
    key_id_hex = key_id_bytes[::-1].hex().upper()
    return key_id_bytes, key_id_hex


def _extrahiere_key_id_aus_signatur(sig_b64):
    """Dekodiere eine minisign-Signatur (base64 einer 4-Zeilen-Textdatei) und gib (key_id_bytes, key_id_hex) zurueck.
    Die zweite Zeile der Textdatei ist die eigentliche base64-Signatur.
    """
    try:
        sig_text = base64.b64decode(sig_b64).decode("utf-8")
    except (base64.binascii.Error, ValueError, UnicodeDecodeError):
        print("FEHLER: Signatur ist kein gueltiger Base64-String (Text)", file=sys.stderr)
        sys.exit(1)
    lines = sig_text.strip().split("\n")
    if len(lines) < 2:
        print("FEHLER: Signatur-Text hat weniger als 2 Zeilen", file=sys.stderr)
        sys.exit(1)
    # Zweite Zeile ist die base64-Signatur
    sig_line = lines[1].strip()
    try:
        sig_raw = base64.b64decode(sig_line)
    except (base64.binascii.Error, ValueError):
        print("FEHLER: Signatur-Zeile ist kein gueltiger Base64-String", file=sys.stderr)
        sys.exit(1)
    if len(sig_raw) != 74:
        print(
            f"FEHLER: Signatur muss 74 Bytes haben (2+8+64), hat {len(sig_raw)} Bytes",
            file=sys.stderr,
        )
        sys.exit(1)
    algo = sig_raw[:2]
    if algo not in (b"Ed", b"ED"):
        print(
            f"FEHLER: Signatur-Algorithmus ist nicht 'Ed'/'ED', sondern {algo!r}",
            file=sys.stderr,
        )
        sys.exit(1)
    key_id_bytes = sig_raw[2:10]
    key_id_hex = key_id_bytes[::-1].hex().upper()
    return key_id_bytes, key_id_hex


def _pruefe_baustein_signatur(data, pubkey_b64, plattform_key, erlaube_testschluessel):
    """Pruefe die Signatur eines Bausteins: Key-ID muss passen, kein Testschluessel.
    Gibt None zurueck bei Erfolg, sonst sys.exit(1).
    """
    # Testschluessel-Pruefung
    if data.get("testschluessel") is True:
        if erlaube_testschluessel:
            print(
                f"WARNUNG: Baustein fuer {plattform_key} ist mit Testschluessel signiert, "
                f"ueberspringe Key-ID-Pruefung",
                file=sys.stderr,
            )
            return  # Ueberspringe Key-ID-Pruefung
        else:
            print(
                f"FEHLER: Baustein fuer {plattform_key} ist mit Testschluessel signiert, "
                f"nicht veroeffentlichbar",
                file=sys.stderr,
            )
            sys.exit(1)

    # Key-ID aus pubkey extrahieren
    pub_key_id_bytes, pub_key_id_hex = _extrahiere_key_id_aus_pubkey(pubkey_b64)

    # Signatur aus dem Baustein holen
    sig_value = data.get("signature")
    if not sig_value:
        print(
            f"FEHLER: Baustein fuer {plattform_key} hat keine Signatur",
            file=sys.stderr,
        )
        sys.exit(1)

    sig_key_id_bytes, sig_key_id_hex = _extrahiere_key_id_aus_signatur(sig_value)

    if pub_key_id_bytes != sig_key_id_bytes:
        print(
            f"FEHLER: Key-ID der Signatur fuer {plattform_key} "
            f"({sig_key_id_hex}) stimmt nicht mit Pubkey-Key-ID "
            f"({pub_key_id_hex}) ueberein",
            file=sys.stderr,
        )
        sys.exit(1)


def baustein(args):
    # Plattform-Key validieren
    if args.plattform not in ERLAUBTE_PLATTFORMEN:
        print(
            f"FEHLER: Ungueltiger Plattform-Key '{args.plattform}'. "
            f"Erlaubt: {', '.join(sorted(ERLAUBTE_PLATTFORMEN))}",
            file=sys.stderr,
        )
        sys.exit(1)

    # Version validieren
    if not VERSION_REGEX.match(args.version):
        print(
            f"FEHLER: Version '{args.version}' entspricht nicht dem Muster X.Y.Z",
            file=sys.stderr,
        )
        sys.exit(1)

    # Signatur-Datei pruefen
    if not os.path.isfile(args.sig):
        print(f"FEHLER: Signatur-Datei '{args.sig}' existiert nicht", file=sys.stderr)
        sys.exit(1)
    if os.path.getsize(args.sig) == 0:
        print(f"FEHLER: Signatur-Datei '{args.sig}' ist leer", file=sys.stderr)
        sys.exit(1)

    # Asset-Datei pruefen
    if not os.path.isfile(args.datei):
        print(f"FEHLER: Asset-Datei '{args.datei}' existiert nicht", file=sys.stderr)
        sys.exit(1)
    if os.path.getsize(args.datei) == 0:
        print(f"FEHLER: Asset-Datei '{args.datei}' ist leer", file=sys.stderr)
        sys.exit(1)

    asset_name = os.path.basename(args.datei)
    expected_url_prefix = (
        f"https://github.com/simeonzickert/mitschnitt/releases/download/v{args.version}/"
    )
    # URL-Prefix validieren
    if not args.url.startswith(expected_url_prefix):
        print(
            f"FEHLER: URL muss mit '{expected_url_prefix}' beginnen",
            file=sys.stderr,
        )
        sys.exit(1)
    # URL-Ende validieren
    if not args.url.endswith(f"/{asset_name}"):
        print(
            f"FEHLER: URL muss mit '/{asset_name}' enden",
            file=sys.stderr,
        )
        sys.exit(1)

    # Signatur einlesen und trimmen
    with open(args.sig, "r", encoding="utf-8") as f:
        signature = f.read().strip()

    asset_bytes = os.path.getsize(args.datei)

    data = {
        "version": args.version,
        "plattform": args.plattform,
        "signature": signature,
        "url": args.url,
        "asset": asset_name,
        "asset_bytes": asset_bytes,
        "notes": args.notes,
        "pub_date": args.pub_date,
    }

    # Testschluessel-Markierung
    if args.testschluessel:
        data["testschluessel"] = True

    out_filename = f"plattform-{args.plattform}.json"
    out_path = os.path.join(args.ausgabe, out_filename)

    # Atomar schreiben: temp-Datei, dann os.replace
    os.makedirs(args.ausgabe, exist_ok=True)
    fd, tmp_path = tempfile.mkstemp(dir=args.ausgabe, suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
            f.write("\n")
        os.replace(tmp_path, out_path)
    except BaseException:
        os.unlink(tmp_path)
        raise


def zusammenfuehren(args):
    ausgabe_dir = args.ausgabe
    if not os.path.isdir(ausgabe_dir):
        print(
            f"FEHLER: Ausgabeverzeichnis '{ausgabe_dir}' existiert nicht",
            file=sys.stderr,
        )
        sys.exit(1)

    # Pubkey ermitteln: entweder von Kommandozeile oder aus tauri.conf.json
    pubkey_b64 = args.pubkey
    if not pubkey_b64:
        # Skript-Pfad ermitteln, um Repo-Root zu finden
        script_dir = os.path.dirname(os.path.abspath(__file__))
        pubkey_b64 = _lade_pubkey_aus_tauri_conf(script_dir)
    if not pubkey_b64:
        print(
            "FEHLER: Kein Pubkey angegeben (--pubkey) und kein pubkey in tauri.conf.json gefunden",
            file=sys.stderr,
        )
        sys.exit(1)

    # Alle plattform-*.json Dateien einsammeln
    baustein_dateien = sorted(
        f
        for f in os.listdir(ausgabe_dir)
        if f.startswith("plattform-") and f.endswith(".json")
    )

    if not baustein_dateien:
        print("FEHLER: Keine Baustein-Dateien gefunden", file=sys.stderr)
        sys.exit(1)

    bausteine = []
    versionen = set()
    asset_zu_plattform = {}  # asset -> plattform

    for datei_name in baustein_dateien:
        # Plattform-Key aus Dateinamen extrahieren: plattform-<key>.json
        key = datei_name[len("plattform-"):-len(".json")]
        if key not in ERLAUBTE_PLATTFORMEN:
            print(
                f"FEHLER: Datei '{datei_name}' enthaelt ungueltigen "
                f"Plattform-Key '{key}'",
                file=sys.stderr,
            )
            sys.exit(1)

        datei_pfad = os.path.join(ausgabe_dir, datei_name)
        try:
            with open(datei_pfad, "r", encoding="utf-8") as f:
                data = json.load(f)
        except (json.JSONDecodeError, OSError) as e:
            print(
                f"FEHLER: Kann Baustein-Datei '{datei_name}' nicht lesen: {e}",
                file=sys.stderr,
            )
            sys.exit(1)

        if not isinstance(data, dict):
            print(
                f"FEHLER: Baustein-Datei '{datei_name}' enthaelt kein JSON-Objekt",
                file=sys.stderr,
            )
            sys.exit(1)

        # plattform-Feld muss mit Dateinamen-Key uebereinstimmen
        if data.get("plattform") != key:
            print(
                f"FEHLER: In Datei '{datei_name}' stimmt das Feld 'plattform' "
                f"('{data.get('plattform')}') nicht mit dem "
                f"Dateinamen-Key '{key}' ueberein",
                file=sys.stderr,
            )
            sys.exit(1)

        # Version muss vorhanden und gueltig sein
        version = data.get("version")
        if not version or not VERSION_REGEX.match(version):
            print(
                f"FEHLER: Ungueltige oder fehlende Version in Baustein "
                f"'{datei_name}'",
                file=sys.stderr,
            )
            sys.exit(1)
        versionen.add(version)

        # Asset muss vorhanden sein
        asset = data.get("asset")
        if not asset:
            print(
                f"FEHLER: Fehlendes Asset in Baustein '{datei_name}'",
                file=sys.stderr,
            )
            sys.exit(1)

        # Doppelte Asset-Namen verhindern
        if asset in asset_zu_plattform:
            andere_plattform = asset_zu_plattform[asset]
            print(
                f"FEHLER: Asset '{asset}' kommt in zwei Plattformen vor: "
                f"'{andere_plattform}' und '{key}'",
                file=sys.stderr,
            )
            sys.exit(1)
        asset_zu_plattform[asset] = key

        # Spezielle Regel fuer darwin-aarch64
        if key == "darwin-aarch64" and asset != "Mitschnitt.app.tar.gz":
            print(
                f"FEHLER: Plattform 'darwin-aarch64' muss Asset "
                f"'Mitschnitt.app.tar.gz' haben, aktuell '{asset}'",
                file=sys.stderr,
            )
            sys.exit(1)

        # Signaturpruefung (Key-ID)
        _pruefe_baustein_signatur(data, pubkey_b64, key, args.erlaube_testschluessel)

        bausteine.append(data)

    # Alle Bausteine muessen dieselbe Version haben
    if len(versionen) != 1:
        versionen_str = ", ".join(sorted(versionen))
        print(
            f"FEHLER: Unterschiedliche Versionen in Bausteinen gefunden: "
            f"{versionen_str}",
            file=sys.stderr,
        )
        sys.exit(1)

    version = versionen.pop()

    # Erwartete Plattformen pruefen
    if args.erwarte:
        erwartete = set(args.erwarte.split(","))
        vorhandene = {b["plattform"] for b in bausteine}
        fehlende = erwartete - vorhandene
        if fehlende:
            print(
                f"FEHLER: Erwartete Plattformen fehlen: "
                f"{', '.join(sorted(fehlende))}",
                file=sys.stderr,
            )
            sys.exit(1)

    # notes und pub_date vom Baustein mit dem juengsten pub_date
    juengster = max(bausteine, key=lambda b: b.get("pub_date", ""))
    notes = juengster.get("notes", "")
    pub_date = juengster.get("pub_date", "")

    # platforms-Dict bauen
    platforms = {}
    for b in bausteine:
        key = b["plattform"]
        platforms[key] = {
            "signature": b["signature"],
            "url": b["url"],
        }

    latest = {
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": platforms,
    }

    # latest.json atomar schreiben
    out_path = os.path.join(ausgabe_dir, "latest.json")
    fd, tmp_path = tempfile.mkstemp(dir=ausgabe_dir, suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(latest, f, indent=2, ensure_ascii=False)
            f.write("\n")
        os.replace(tmp_path, out_path)
    except BaseException:
        os.unlink(tmp_path)
        raise

    # stdout: pro Plattform eine Zeile, sortiert nach Key
    for b in sorted(bausteine, key=lambda x: x["plattform"]):
        print(f"{b['plattform']}\t{b['asset']}\t{b['asset_bytes']}")


def main():
    parser = argparse.ArgumentParser(
        description="Mitschnitt Feed - Bausteine und Zusammenfuehrung"
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    # baustein
    bp = subparsers.add_parser("baustein", help="Erstelle einen Plattform-Baustein")
    bp.add_argument("--ausgabe", required=True, help="Ausgabeverzeichnis")
    bp.add_argument("--version", required=True, help="Version X.Y.Z")
    bp.add_argument("--plattform", required=True, help="Plattform-Key")
    bp.add_argument("--sig", required=True, help="Pfad zur .sig-Datei")
    bp.add_argument("--datei", required=True, help="Pfad zur Asset-Datei")
    bp.add_argument("--url", required=True, help="Download-URL")
    bp.add_argument("--notes", required=True, help="Release-Notes")
    bp.add_argument("--pub-date", required=True, help="Veroeffentlichungsdatum (ISO-8601)")
    bp.add_argument("--testschluessel", action="store_true",
                    help="Markiere Baustein als mit Testschluessel signiert")
    bp.set_defaults(func=baustein)

    # zusammenfuehren
    zp = subparsers.add_parser(
        "zusammenfuehren", help="Fuehre Bausteine zu latest.json zusammen"
    )
    zp.add_argument("--ausgabe", required=True, help="Ausgabeverzeichnis mit Baustein-Dateien")
    zp.add_argument(
        "--erwarte",
        help="Komma-separierte Liste erwarteter Plattform-Keys",
    )
    zp.add_argument("--pubkey", help="Base64-Pubkey (minisign-Format)")
    zp.add_argument("--erlaube-testschluessel", action="store_true",
                    help="Erlaube Bausteine mit Testschluessel (nur fuer Probelaeufe)")
    zp.set_defaults(func=zusammenfuehren)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
