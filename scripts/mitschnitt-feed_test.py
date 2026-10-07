#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/mitschnitt-feed_test.py
# Tests fuer mitschnitt-feed.py per subprocess.

import base64
import json
import os
import subprocess
import sys
import tempfile
import unittest

# Pfad zum Skript relativ zu dieser Testdatei
SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
SCRIPT_PATH = os.path.join(SCRIPT_DIR, "mitschnitt-feed.py")


def _baue_fake_minisign_pubkey(key_id_hex):
    """Erzeuge einen gueltigen Fake-minisign-pubkey (base64 einer 2-Zeilen-Textdatei)
    mit frei waehlbarer Key-ID (8 Bytes, hex, Grossbuchstaben, in umgekehrter Reihenfolge wie minisign).
    Gibt den base64-String zurueck.
    """
    # key_id_hex ist die Darstellung, die minisign anzeigt (Bytes in umgekehrter Reihenfolge)
    key_id_bytes = bytes.fromhex(key_id_hex)[::-1]  # 8 Bytes in little-endian
    # Algorithmus 2 Bytes + Key-ID 8 Bytes + Key 32 Bytes (dummy)
    raw = b"Ed" + key_id_bytes + b"\x00" * 32
    text = (
        "untrusted comment: minisign public key "
        + key_id_hex
        + "\n"
        + base64.b64encode(raw).decode("ascii")
        + "\n"
    )
    return base64.b64encode(text.encode("utf-8")).decode("ascii")


def _baue_fake_minisign_signatur(key_id_hex):
    """Erzeuge eine gueltige Fake-minisign-Signatur (base64 einer 4-Zeilen-Textdatei)
    mit frei waehlbarer Key-ID.
    Gibt den base64-String zurueck.
    """
    key_id_bytes = bytes.fromhex(key_id_hex)[::-1]  # 8 Bytes in little-endian
    # Algorithmus 2 Bytes + Key-ID 8 Bytes + Signatur 64 Bytes (dummy)
    raw = b"Ed" + key_id_bytes + b"\x00" * 64
    sig_line = base64.b64encode(raw).decode("ascii")
    # Textdatei: 4 Zeilen
    text = (
        "untrusted comment: fake signature\n"
        + sig_line
        + "\n"
        + "trusted comment: fake\n"
        + base64.b64encode(b"\x00" * 64).decode("ascii")
        + "\n"
    )
    return base64.b64encode(text.encode("utf-8")).decode("ascii")


class TestMitschnittFeed(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.ausgabe = self.temp_dir.name
        # Standard-Fake-Key-ID fuer Tests (8 Bytes, hex, gross, umgekehrte Reihenfolge)
        self.test_key_id_hex = "AABBCCDDEEFF0011"
        self.test_pubkey_b64 = _baue_fake_minisign_pubkey(self.test_key_id_hex)

    def tearDown(self):
        self.temp_dir.cleanup()

    def _run_baustein(self, args_list, expect_failure=False):
        """Fuehre 'baustein' Befehl aus. Gib (returncode, stdout, stderr) zurueck."""
        cmd = [sys.executable, SCRIPT_PATH, "baustein"] + args_list
        result = subprocess.run(cmd, capture_output=True, text=True)
        if expect_failure:
            self.assertNotEqual(
                result.returncode,
                0,
                msg=f"Erwarteter Fehler, aber Erfolg: {result.stdout}",
            )
        else:
            self.assertEqual(result.returncode, 0, msg=f"Fehler: {result.stderr}")
        return result

    def _run_zusammenfuehren(self, args_list, expect_failure=False):
        cmd = [sys.executable, SCRIPT_PATH, "zusammenfuehren"] + args_list
        result = subprocess.run(cmd, capture_output=True, text=True)
        if expect_failure:
            self.assertNotEqual(
                result.returncode,
                0,
                msg=f"Erwarteter Fehler, aber Erfolg: {result.stdout}",
            )
        else:
            self.assertEqual(result.returncode, 0, msg=f"Fehler: {result.stderr}")
        return result

    def _create_asset_file(self, name, content=b"dummy"):
        path = os.path.join(self.temp_dir.name, name)
        with open(path, "wb") as f:
            f.write(content)
        return path

    def _create_sig_file(self, content="dummy_signature"):
        path = os.path.join(self.temp_dir.name, "test.sig")
        with open(path, "w") as f:
            f.write(content)
        return path

    def _create_baustein(
        self,
        plattform,
        version="0.1.9",
        asset_name=None,
        sig_content=None,
        asset_content=b"asset",
        url=None,
        notes="Test",
        pub_date="2026-09-28T18:00:00Z",
        testschluessel=False,
    ):
        if asset_name is None:
            if plattform == "darwin-aarch64":
                asset_name = "Mitschnitt.app.tar.gz"
            elif plattform == "darwin-x86_64":
                asset_name = "Mitschnitt-x86_64.app.tar.gz"
            elif plattform == "windows-x86_64":
                asset_name = "Mitschnitt_0.1.9_x64-setup.exe"
        asset_path = self._create_asset_file(asset_name, asset_content)
        # Standard-Signatur: Fake-minisign-Format mit Test-Key-ID
        if sig_content is None:
            sig_content = _baue_fake_minisign_signatur(self.test_key_id_hex)
        sig_path = self._create_sig_file(sig_content)
        if url is None:
            url = (
                f"https://github.com/simeonzickert/mitschnitt/releases/download/"
                f"v{version}/{asset_name}"
            )
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            version,
            "--plattform",
            plattform,
            "--sig",
            sig_path,
            "--datei",
            asset_path,
            "--url",
            url,
            "--notes",
            notes,
            "--pub-date",
            pub_date,
        ]
        if testschluessel:
            args.append("--testschluessel")
        return self._run_baustein(args)

    # ----- Tests -----

    def test_gluecksfall_drei_plattformen(self):
        """Gluecksfall: drei Plattformen, zusammenfuehren, latest.json und stdout pruefen."""
        self._create_baustein("darwin-aarch64")
        self._create_baustein("darwin-x86_64")
        self._create_baustein("windows-x86_64")

        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64]
        )
        # Pruefe stdout
        lines = result.stdout.strip().split("\n")
        self.assertEqual(len(lines), 3)
        expected_lines = [
            "darwin-aarch64\tMitschnitt.app.tar.gz\t5",
            "darwin-x86_64\tMitschnitt-x86_64.app.tar.gz\t5",
            "windows-x86_64\tMitschnitt_0.1.9_x64-setup.exe\t5",
        ]
        self.assertEqual(lines, expected_lines)

        # Pruefe latest.json
        latest_path = os.path.join(self.ausgabe, "latest.json")
        with open(latest_path, "r") as f:
            latest = json.load(f)
        self.assertEqual(latest["version"], "0.1.9")
        self.assertIn("darwin-aarch64", latest["platforms"])
        self.assertIn("darwin-x86_64", latest["platforms"])
        self.assertIn("windows-x86_64", latest["platforms"])
        for plat in ["darwin-aarch64", "darwin-x86_64", "windows-x86_64"]:
            self.assertIn("signature", latest["platforms"][plat])
            self.assertIn("url", latest["platforms"][plat])
        self.assertEqual(latest["notes"], "Test")
        self.assertEqual(latest["pub_date"], "2026-09-28T18:00:00Z")

    def test_ungueltiger_plattform_key(self):
        """Baustein mit ungueltigem Plattform-Key -> Fehler."""
        asset_path = self._create_asset_file("test.exe")
        sig_path = self._create_sig_file()
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            "0.1.9",
            "--plattform",
            "linux-x86_64",
            "--sig",
            sig_path,
            "--datei",
            asset_path,
            "--url",
            "https://github.com/simeonzickert/mitschnitt/releases/download/v0.1.9/test.exe",
            "--notes",
            "Test",
            "--pub-date",
            "2026-09-28T18:00:00Z",
        ]
        result = self._run_baustein(args, expect_failure=True)
        self.assertIn("FEHLER", result.stderr)

    def test_ungueltige_version(self):
        """Baustein mit ungueltiger Version -> Fehler."""
        asset_path = self._create_asset_file("test.exe")
        sig_path = self._create_sig_file()
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            "0.1",
            "--plattform",
            "windows-x86_64",
            "--sig",
            sig_path,
            "--datei",
            asset_path,
            "--url",
            "https://github.com/simeonzickert/mitschnitt/releases/download/v0.1/test.exe",
            "--notes",
            "Test",
            "--pub-date",
            "2026-09-28T18:00:00Z",
        ]
        result = self._run_baustein(args, expect_failure=True)
        self.assertIn("FEHLER", result.stderr)

    def test_sig_datei_fehlt(self):
        """Baustein mit fehlender Sig-Datei -> Fehler."""
        asset_path = self._create_asset_file("test.exe")
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            "0.1.9",
            "--plattform",
            "windows-x86_64",
            "--sig",
            "/nonexistent.sig",
            "--datei",
            asset_path,
            "--url",
            "https://github.com/simeonzickert/mitschnitt/releases/download/v0.1.9/test.exe",
            "--notes",
            "Test",
            "--pub-date",
            "2026-09-28T18:00:00Z",
        ]
        result = self._run_baustein(args, expect_failure=True)
        self.assertIn("FEHLER", result.stderr)

    def test_sig_datei_leer(self):
        """Baustein mit leerer Sig-Datei -> Fehler."""
        asset_path = self._create_asset_file("test.exe")
        sig_path = self._create_sig_file("")  # leer
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            "0.1.9",
            "--plattform",
            "windows-x86_64",
            "--sig",
            sig_path,
            "--datei",
            asset_path,
            "--url",
            "https://github.com/simeonzickert/mitschnitt/releases/download/v0.1.9/test.exe",
            "--notes",
            "Test",
            "--pub-date",
            "2026-09-28T18:00:00Z",
        ]
        result = self._run_baustein(args, expect_failure=True)
        self.assertIn("FEHLER", result.stderr)

    def test_asset_datei_fehlt(self):
        """Baustein mit fehlender Asset-Datei -> Fehler."""
        sig_path = self._create_sig_file()
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            "0.1.9",
            "--plattform",
            "windows-x86_64",
            "--sig",
            sig_path,
            "--datei",
            "/nonexistent.exe",
            "--url",
            "https://github.com/simeonzickert/mitschnitt/releases/download/v0.1.9/test.exe",
            "--notes",
            "Test",
            "--pub-date",
            "2026-09-28T18:00:00Z",
        ]
        result = self._run_baustein(args, expect_failure=True)
        self.assertIn("FEHLER", result.stderr)

    def test_asset_datei_leer(self):
        """Baustein mit leerer Asset-Datei -> Fehler."""
        asset_path = self._create_asset_file("test.exe", b"")
        sig_path = self._create_sig_file()
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            "0.1.9",
            "--plattform",
            "windows-x86_64",
            "--sig",
            sig_path,
            "--datei",
            asset_path,
            "--url",
            "https://github.com/simeonzickert/mitschnitt/releases/download/v0.1.9/test.exe",
            "--notes",
            "Test",
            "--pub-date",
            "2026-09-28T18:00:00Z",
        ]
        result = self._run_baustein(args, expect_failure=True)
        self.assertIn("FEHLER", result.stderr)

    def test_url_falscher_prefix(self):
        """Baustein mit URL, die nicht mit dem erwarteten Prefix beginnt -> Fehler."""
        asset_path = self._create_asset_file("test.exe")
        sig_path = self._create_sig_file()
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            "0.1.9",
            "--plattform",
            "windows-x86_64",
            "--sig",
            sig_path,
            "--datei",
            asset_path,
            "--url",
            "https://other.com/test.exe",
            "--notes",
            "Test",
            "--pub-date",
            "2026-09-28T18:00:00Z",
        ]
        result = self._run_baustein(args, expect_failure=True)
        self.assertIn("FEHLER", result.stderr)

    def test_url_falsches_ende(self):
        """Baustein mit URL, die nicht mit /<asset> endet -> Fehler."""
        asset_path = self._create_asset_file("test.exe")
        sig_path = self._create_sig_file()
        args = [
            "--ausgabe",
            self.ausgabe,
            "--version",
            "0.1.9",
            "--plattform",
            "windows-x86_64",
            "--sig",
            sig_path,
            "--datei",
            asset_path,
            "--url",
            "https://github.com/simeonzickert/mitschnitt/releases/download/v0.1.9/other.exe",
            "--notes",
            "Test",
            "--pub-date",
            "2026-09-28T18:00:00Z",
        ]
        result = self._run_baustein(args, expect_failure=True)
        self.assertIn("FEHLER", result.stderr)

    def test_zusammenfuehren_keine_bausteine(self):
        """Zusammenfuehren ohne Bausteine -> Fehler."""
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe], expect_failure=True
        )
        self.assertIn("FEHLER", result.stderr)

    def test_zusammenfuehren_unterschiedliche_versionen(self):
        """Zusammenfuehren mit Bausteinen unterschiedlicher Version -> Fehler."""
        self._create_baustein("darwin-aarch64", version="0.1.9")
        self._create_baustein("darwin-x86_64", version="0.2.0")
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)
        self.assertIn("0.1.9", result.stderr)
        self.assertIn("0.2.0", result.stderr)

    def test_zusammenfuehren_plattform_feld_falsch(self):
        """Baustein-Datei mit falschem plattform-Feld -> Fehler."""
        data = {
            "version": "0.1.9",
            "plattform": "windows-x86_64",  # Dateiname sagt darwin-aarch64
            "signature": _baue_fake_minisign_signatur(self.test_key_id_hex),
            "url": "https://github.com/simeonzickert/mitschnitt/releases/download/v0.1.9/test.exe",
            "asset": "test.exe",
            "asset_bytes": 5,
            "notes": "Test",
            "pub_date": "2026-09-28T18:00:00Z",
        }
        datei_name = "plattform-darwin-aarch64.json"
        with open(os.path.join(self.ausgabe, datei_name), "w") as f:
            json.dump(data, f)
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)

    def test_zusammenfuehren_kaputtes_json(self):
        """Baustein-Datei mit ungueltigem JSON -> Fehler."""
        datei_name = "plattform-darwin-aarch64.json"
        with open(os.path.join(self.ausgabe, datei_name), "w") as f:
            f.write("{defekt}")
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)

    def test_zusammenfuehren_doppeltes_asset(self):
        """Zwei Plattformen mit gleichem Asset-Namen -> Fehler."""
        self._create_baustein("darwin-aarch64", asset_name="same.exe")
        self._create_baustein("darwin-x86_64", asset_name="same.exe")
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)

    def test_zusammenfuehren_erwarte_fehlt(self):
        """Erwartete Plattform fehlt -> Fehler."""
        self._create_baustein("darwin-aarch64")
        self._create_baustein("darwin-x86_64")
        result = self._run_zusammenfuehren(
            [
                "--ausgabe",
                self.ausgabe,
                "--erwarte",
                "darwin-aarch64,darwin-x86_64,windows-x86_64",
                "--pubkey",
                self.test_pubkey_b64,
            ],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)
        self.assertIn("windows-x86_64", result.stderr)

    def test_zusammenfuehren_erwarte_erfolg(self):
        """Erwartete Plattformen sind vorhanden -> Erfolg."""
        self._create_baustein("darwin-aarch64")
        self._create_baustein("darwin-x86_64")
        self._create_baustein("windows-x86_64")
        result = self._run_zusammenfuehren(
            [
                "--ausgabe",
                self.ausgabe,
                "--erwarte",
                "darwin-aarch64,darwin-x86_64,windows-x86_64",
                "--pubkey",
                self.test_pubkey_b64,
            ]
        )
        self.assertEqual(result.returncode, 0)

    def test_darwin_aarch64_falsches_asset(self):
        """darwin-aarch64 mit falschem Asset -> Fehler."""
        self._create_baustein("darwin-aarch64", asset_name="falsch.tar.gz")
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)

    def test_neubau_ueberschreibt_baustein(self):
        """Neubau eines Bausteins ueberschreibt vorhandenen."""
        # Ersten Baustein anlegen
        self._create_baustein(
            "darwin-aarch64", notes="alt", pub_date="2026-09-27T18:00:00Z"
        )
        # Zweiten Baustein mit gleicher Plattform, anderen Daten
        self._create_baustein(
            "darwin-aarch64", notes="neu", pub_date="2026-09-28T18:00:00Z"
        )
        # Pruefen, dass nur eine Datei existiert und die neuen Daten enthaelt
        baustein_datei = os.path.join(self.ausgabe, "plattform-darwin-aarch64.json")
        with open(baustein_datei, "r") as f:
            data = json.load(f)
        self.assertEqual(data["notes"], "neu")
        self.assertEqual(data["pub_date"], "2026-09-28T18:00:00Z")

    def test_notes_vom_juengsten_pub_date(self):
        """notes und pub_date in latest.json vom Baustein mit juengstem pub_date."""
        self._create_baustein(
            "darwin-aarch64", notes="aelteste", pub_date="2026-09-27T18:00:00Z"
        )
        self._create_baustein(
            "darwin-x86_64", notes="mittlere", pub_date="2026-09-28T18:00:00Z"
        )
        self._create_baustein(
            "windows-x86_64", notes="juengste", pub_date="2026-09-29T18:00:00Z"
        )
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64]
        )
        latest_path = os.path.join(self.ausgabe, "latest.json")
        with open(latest_path, "r") as f:
            latest = json.load(f)
        self.assertEqual(latest["notes"], "juengste")
        self.assertEqual(latest["pub_date"], "2026-09-29T18:00:00Z")

    # ----- Neue Tests fuer Schluessel-Pruefung und Testschluessel -----

    def test_key_id_passt(self):
        """Gluecksfall: Key-ID der Signatur passt zum Pubkey."""
        self._create_baustein("darwin-aarch64")
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64]
        )
        self.assertEqual(result.returncode, 0)

    def test_key_id_fremd(self):
        """Abbruch bei fremder Key-ID."""
        andere_key_id = "1122334455667788"
        falsche_sig = _baue_fake_minisign_signatur(andere_key_id)
        self._create_baustein("darwin-aarch64", sig_content=falsche_sig)
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)
        self.assertIn(self.test_key_id_hex, result.stderr)
        self.assertIn(andere_key_id, result.stderr)

    def test_kaputtes_signatur_format(self):
        """Abbruch bei kaputtem Signaturformat (zu kurze base64)."""
        # Kaputte Signatur: nur 10 Bytes nach Dekodierung
        kaputte_sig = base64.b64encode(b"x" * 10).decode("ascii")
        self._create_baustein("darwin-aarch64", sig_content=kaputte_sig)
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)

    def test_pubkey_fehlt(self):
        """Abbruch bei fehlendem pubkey (weder --pubkey noch tauri.conf.json)."""
        self._create_baustein("darwin-aarch64")
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)
        self.assertIn("Pubkey", result.stderr)

    def test_testschluessel_baustein_bricht_ab(self):
        """Baustein mit testschluessel: true bricht zusammenfuehren ab."""
        self._create_baustein("darwin-aarch64", testschluessel=True)
        result = self._run_zusammenfuehren(
            ["--ausgabe", self.ausgabe, "--pubkey", self.test_pubkey_b64],
            expect_failure=True,
        )
        self.assertIn("FEHLER", result.stderr)
        self.assertIn("Testschluessel", result.stderr)

    def test_testschluessel_mit_erlaube(self):
        """Mit --erlaube-testschluessel geht Testschluessel-Baustein durch mit Warnung."""
        self._create_baustein("darwin-aarch64", testschluessel=True)
        result = self._run_zusammenfuehren(
            [
                "--ausgabe",
                self.ausgabe,
                "--pubkey",
                self.test_pubkey_b64,
                "--erlaube-testschluessel",
            ]
        )
        self.assertEqual(result.returncode, 0)
        self.assertIn("WARNUNG", result.stderr)
        self.assertIn("Testschluessel", result.stderr)


if __name__ == "__main__":
    unittest.main()
