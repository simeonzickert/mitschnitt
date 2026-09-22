# Aenderungsliste dieses Forks

Dieses Verzeichnis war bis zum 01.09.2026 mit **95 Dateien Produktgeschichte des
Originals** gefuellt (Release 0.0.x bis 1.4.14, Stichworte: Anarlog Cloud,
Anmeldung, Teilen, Sync, Teams). Der Bau haengt die neueste dieser Dateien fest
ins Programm, und die Oberflaeche hat sie unter „Was ist neu" angezeigt --
inklusive Funktionen, die es in diesem Fork gar nicht mehr gibt. Eine App, die
dem Nutzer die Geschichte eines fremden Produkts als die eigene vorlegt, ist
schlechter als eine ohne Aenderungsliste. Deshalb ist die fremde Geschichte
raus, und der Netzabruf der fremden Chronik (`raw.githubusercontent.com/
fastrepl/anarlog`) ist aus `apps/desktop/src/changelog/data.ts` entfernt.

Solange hier keine `<version>.md` liegt, ist `latestVersion` null und die App
zeigt „No changelog available for this version." Das ist Absicht und kein
Defekt: die Geschichte dieses Forks steht bis auf Weiteres im git-Log.

## Wenn eine eigene Ausgabe herausgeht

Eine Datei pro Version, benannt nach der ausgelieferten Versionsnummer
(`0.2.0.md`), mit diesem Kopf:

```md
---
date: "JJJJ-MM-TT"
summary: "Ein Satz, was sich fuer den Nutzer aendert. Kein Markdown."
---
```

Darunter Ueberschriften und Stichpunkte. Nur was ein Nutzer merkt -- keine
Umbauten im Maschinenraum. Die Anzeige rendert Markdown und zusaetzlich ein
`<banner title="..." variant="warning|info">` fuer Hinweise.
