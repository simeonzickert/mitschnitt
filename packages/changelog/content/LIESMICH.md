# Aenderungsliste dieses Forks

Dieses Verzeichnis wird **nicht mehr gelesen**. Bis zum 01.09.2026 lagen hier
95 Dateien Produktgeschichte des Originals (Release 0.0.x bis 1.4.14,
Stichworte: Anarlog Cloud, Anmeldung, Teilen, Sync, Teams); der Bau haengte
die neueste davon fest ins Programm und die Oberflaeche zeigte sie unter
„Was ist neu" -- inklusive Funktionen, die es in diesem Fork gar nicht mehr
gibt. Eine App, die dem Nutzer die Geschichte eines fremden Produkts als die
eigene vorlegt, ist schlechter als eine ohne Aenderungsliste. Deshalb ist die
fremde Geschichte am 01.09.2026 raus, und der Netzabruf der fremden Chronik
(`raw.githubusercontent.com/fastrepl/anarlog`) ist aus
`apps/desktop/src/changelog/data.ts` entfernt.

## Wo die Aenderungsliste jetzt herkommt

Einzige Quelle ist `CHANGELOG.md` im Repo-Root. Das Vite-Plugin
`apps/desktop/plugins/changelog.ts` liest die App-Version aus
`apps/desktop/package.json` und schneidet den Abschnitt dieser Version aus
`CHANGELOG.md` heraus (dieselben Regeln wie `scripts/changelog-extract.sh`:
Kopfzeile `## X.Y.Z (JJJJ-MM-TT)`, Pflicht-Unterabschnitt `### What's new`,
optional `### Note`). Fehlt der Abschnitt -- etwa bei einem Dev-Bau mit
automatisch hochgezaehlter Patch-Version --, ist `latestVersion` null und die
App zeigt „No changelog available for this version." Das ist Absicht und kein
Defekt.

Format und Pflegepflicht von `CHANGELOG.md` stehen im Kopf von
`scripts/changelog-extract.sh` und im Kopf der Datei selbst. Vor jedem
Release muss der Abschnitt der neuen Version dort stehen, sonst bricht
`scripts/mitschnitt-release.sh` ab.
