<!--
Vorlage fuer den Text der GitHub-Release-Seite. scripts/mitschnitt-release.sh
ersetzt {{VERSION}}, {{DMG_URL}}, {{DMG_NAME}} und {{CHANGELOG}} und schreibt das
Ergebnis nach <Ausgabeordner>/v<version>/release-notes.md. Dieser Kommentar wird
dabei entfernt. Der Download-Link auf die DMG steht bewusst ganz oben: die
Release-Seite zeigt alle Anhaenge gleichrangig, Menschen sollen nur die DMG nehmen.
-->
## [Download Mitschnitt {{VERSION}} for macOS ({{DMG_NAME}})]({{DMG_URL}})

**English:** Open the `.dmg`, drag **Mitschnitt** into the **Applications** folder, then start it from there. Apple silicon (M1 or newer) is required. Already installed? The app updates itself, you don't need to download anything.

**Deutsch:** Die `.dmg` öffnen, **Mitschnitt** in den Ordner **Programme** ziehen und von dort starten. Nötig ist ein Mac mit Apple-Chip (M1 oder neuer). Schon installiert? Die App aktualisiert sich selbst, du musst nichts herunterladen.

### What's new / Was ist neu

{{CHANGELOG}}

### Other files / Die übrigen Dateien

`latest.json`, `Mitschnitt.app.tar.gz` and `Mitschnitt.app.tar.gz.sig` are for the app's automatic update. **Please don't download them.**

`latest.json`, `Mitschnitt.app.tar.gz` und `Mitschnitt.app.tar.gz.sig` sind für die automatische Aktualisierung der App. **Bitte nicht herunterladen.**
