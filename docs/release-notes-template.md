<!--
Vorlage fuer den Text der GitHub-Release-Seite. scripts/mitschnitt-veroeffentlichen.sh
ersetzt {{VERSION}}, {{DOWNLOADS}}, {{UPDATE_FILES}}, {{NOTE}} und {{CHANGELOG}} und
schreibt das Ergebnis nach <Ausgabeordner>/v<version>/release-notes.md. Dieser
Kommentar wird dabei entfernt. Die Download-Links stehen bewusst ganz oben:
die Release-Seite zeigt alle Anhaenge gleichrangig, Menschen sollen nur die
DMG/EXE nehmen.

{{DOWNLOADS}} wird vom Skript je nach vorhandenen Plattformen erzeugt:
  - macOS, Apple silicon (M1 or newer): Mitschnitt-<v>.dmg
  - macOS, Intel: Mitschnitt-<v>-Intel.dmg
  - Windows 10/11 (64-bit): Mitschnitt_<v>_x64-setup.exe

{{NOTE}} kommt aus dem optionalen "### Note"-Abschnitt der Version in
CHANGELOG.md (scripts/changelog-extract.sh, Feld "note") und steht bewusst
GANZ OBEN, direkt unter der Installationszeile -- fuer Handlungsanweisungen,
die nur fuer DIESE eine Version gelten (Beispiel 0.1.8: "installiere diese
Version einmal von Hand"). Das Skript setzt jede Zeile davon bereits als
Markdown-Blockquote ("> ...") ein. Traegt die Version keinen Note-Abschnitt,
ersetzt das Skript {{NOTE}} durch eine leere Zeichenkette; die Zeile bleibt
dann leer stehen, das ist harmlos. Versions-spezifische Migrationshinweise
gehoeren NUR in den Note-Abschnitt der jeweiligen CHANGELOG.md-Version, nie
fest in diese Vorlage -- sonst klebt z.B. der 0.1.8-Hinweis an jedem
kuenftigen Release, auch wenn ihn dann niemand mehr braucht.

{{CHANGELOG}} kommt aus demselben Skript, Feld "what-new".

{{UPDATE_FILES}} wird vom Skript als kommagetrennte Liste aller Update-Dateien
(latest.json, jedes asset und jedes asset.sig) erzeugt.

Alles hier ist bewusst NUR Englisch (Entscheid 28.09.2026: "GitHub muss bitte
nur auf Englisch. Wir muessen hier nicht Deutsch und Englisch alles
schreiben."). Das gilt fuer diese Vorlage, die daraus gebaute Release-Notiz
und die "notes" in latest.json -- nicht fuer die App-Oberflaeche selbst, die
bleibt zweisprachig.
-->

## Download Mitschnitt {{VERSION}}

{{DOWNLOADS}}

On a Mac: open the `.dmg`, drag **Mitschnitt** into the **Applications** folder, then start it from there. Already installed 0.1.8 or newer? The app updates itself, you don't need to download anything.

{{NOTE}}

### What's new

{{CHANGELOG}}

### Other files

{{UPDATE_FILES}} are for the app's automatic update. **Please don't download them.**
