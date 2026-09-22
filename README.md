<div align="center">

  <img width="110" src="apps/desktop/src-tauri/icons/stable/128x128@2x.png" alt="Mitschnitt" />

  <h1>Mitschnitt</h1>

  <p>
    <b>Meeting-Mitschnitt, der auf deinem Rechner bleibt.</b>
  </p>

</div>

<br />

## Was das hier ist

Mitschnitt ist ein **privater Fork von [anarlog](https://github.com/fastrepl/anarlog)**
(Fastrepl, Inc., MIT-lizenziert). Es ist kein offizielles Produkt von Fastrepl, steht in
keiner Verbindung zu ihnen, und Fastrepl unterstützt oder prüft diesen Fork nicht. Fehler
hier sind unsere, nicht ihre. Fragen zu diesem Fork gehen nicht an das Original-Projekt.

Die App nimmt Meetings auf deinem Rechner auf, schreibt sie dort mit und legt alles in eine
lokale SQLite-Datenbank plus normale Dateien daneben. Kein Bot sitzt im Call, nichts muss in
eine Cloud.

Der Fork existiert, weil die Original-App zwei Sachen tat, die im Alltag stören: sie schickte
Telemetrie los, und sie setzte einen Account voraus, wo keiner nötig ist. Beides ist raus.

## Was wir geändert haben

- **Eigene Marke und eigene Identität.** Anderer Name, anderes Icon, eigener
  Bundle-Bezeichner (`media.zickert.mitschnitt`), eigenes URL-Schema.
- **Eigener Datenordner.** Der Fork fasst die Daten einer parallel installierten
  Original-App nicht an. Beide können nebeneinander laufen.
- **Kein Telemetrie-Zwang, kein Cloud-Zwang.** Der Telemetrie-Sender ist entfernt, nicht nur
  abgeschaltet. Account, Sync und Onboarding-Login sind raus. Cloud-Anbieter für
  Transkription und Sprachmodelle kannst du weiter einrichten, wenn du willst; du musst
  nicht.
- **Markdown-Spiegel.** Jedes Gespräch bekommt zusätzlich zur Datenbank einen lesbaren
  Ordner mit Markdown und Audio. Einweg: die Datenbank bleibt der Maschinenraum, der
  Spiegel ist der garantierte Ausgang.
- **Einstellungen exportieren und importieren.** Als eine Datei, mit Vorschau vor dem
  Import und ohne dass Zugangsdaten dabei verloren gehen.
- **Aufbewahrung.** Du kannst eine Frist setzen, nach der alte Aufnahmen verschwinden. Sie
  löscht nichts, bevor du sie einmal ausdrücklich scharf gestellt hast.
- **Reparierte Zeitachse.** Der Batch-Weg setzte Wortpakete auf ein starres
  29,5-Sekunden-Raster statt an den Sprechbeginn. Das ist behoben.
- **Drei lokale Transkriptionsmodelle zur Wahl:** Parakeet (schnell, trifft Eigennamen gut),
  Whisper large-v3-turbo (genauer, deutlich langsamer), plus die Anbieter, die du selbst
  einträgst.

Alles andere kommt weiter aus dem Upstream und wird von dort nachgezogen.

## Repository

| Pfad | Was da liegt |
| --- | --- |
| `apps/desktop` | Die App: Tauri v2, React/TypeScript vorn, Rust hinten |
| `plugins/*` | Tauri-Erweiterungen: lokale Transkription, Datenbank, Kalender, Export, Benachrichtigungen |
| `crates/*` | Rust-Bibliotheken: Audioaufnahme, Transkription, Sprechertrennung, Speicher |
| `packages/*` | Geteilte TypeScript-Pakete: Editor, Datenbank, UI |

## Selbst bauen

Du brauchst Node.js 22 oder neuer, pnpm 11.1.1, Rust 1.94.0 und die
[Tauri-v2-Systemabhängigkeiten](https://v2.tauri.app/start/prerequisites/).

```bash
pnpm install --frozen-lockfile
pnpm exec turbo dev:desktop
```

Für eine signierte, installierbare Fassung auf dem Mac gibt es
`scripts/mitschnitt-deploy.sh`. Das Skript baut, signiert mit der Developer-ID und bricht ab,
wenn die Signatur doch ad-hoc wäre oder das Bundle nicht aus diesem Lauf stammt. Ohne das
Skript kommt das Berechtigungs-Problem bei jedem Neubau zurück.

## Lizenz

Der Code steht unter der **MIT-Lizenz**, Copyright Fastrepl, Inc., siehe
[`LICENSE`](LICENSE). Die MIT-Lizenz verlangt, dass dieser Hinweis bei jeder Kopie
mitgeht; deshalb liegt die Lizenzdatei auch im fertigen App-Bundle unter
`Contents/Resources/licenses/`.

In der App steckt außerdem Arbeit von anderen: Schriften, Modelle, fremde Logos. Wer das
alles ist und unter welchen Bedingungen es mitgeliefert wird, steht in
[`ATTRIBUTIONS.md`](ATTRIBUTIONS.md). Ein Punkt lohnt einen Blick, bevor du daraus etwas
Kommerzielles machst: die Sprachmodelle Llama, Gemma und Parakeet tragen eigene
Bedingungen, die keine OSI-Lizenzen sind.

Die Sync-Bibliothek unter der Elastic License ist am 01.09.2026 mit der Cloud-Schicht
ausgebaut worden. Dieser Fork liefert keine Binärdateien mehr aus, die eine Lizenz von
SQLite Cloud, Inc. verlangen.
