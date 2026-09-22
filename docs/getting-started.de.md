# Mitschnitt: Anleitung für den ersten Start

Mitschnitt nimmt Gespräche auf und schreibt sie mit. Alles läuft auf deinem eigenen
Rechner.

Du brauchst einen Mac mit Apple-Chip (M1 oder neuer) und **macOS 15 oder neuer**. Auf
älteren Fassungen startet die App nicht.

---

## 1. Die App zum ersten Mal öffnen

1. Lade `Mitschnitt-<version>-notarisiert.zip` von der
   [aktuellen Version](https://github.com/simeonzickert/mitschnitt/releases/latest) und
   entpacke die Datei.
2. Zieh **Mitschnitt** in deinen Ordner **Programme**.
3. Doppelklick auf die App.

Die App ist von Apple notarisiert, macOS öffnet sie deshalb ohne Warnung. Spätere
Versionen installiert sie selbst (siehe Abschnitt 5).

---

## 2. Was die App beim ersten Start von dir will

Beim ersten Start führt dich die App durch ein paar Schritte. Sie fragt dabei nach
Berechtigungen. Jede wird erst abgefragt, wenn du selbst darauf klickst — es passiert
nichts von allein.

1. **Mikrofon** — damit deine eigene Stimme aufgenommen wird.
2. **Systemton** — damit die Stimmen der anderen aus dem Videogespräch aufgenommen werden.
   Ohne das hörst du im Mitschnitt nur dich selbst.
3. **Bedienungshilfen** — damit die App merkt, wann ein Videogespräch läuft. Hier führt
   dich ein kleiner Assistent durch die Systemeinstellungen.
4. **Kalender** — freiwillig. Damit bekommen Aufnahmen automatisch den Titel und die
   Teilnehmer des Termins. Du kannst den Schritt überspringen.

Danach legt die App eine Beispielsitzung namens „Welcome to Mitschnitt" an, damit du
siehst, wie das Ergebnis aussieht.

---

## 3. Das Modell für die Mitschrift

Beim ersten Start lädt die App das Modell für die Mitschrift (Parakeet, rund 660 MB).
Der Knopf zum Öffnen der App kommt erst, wenn es da ist. Die Modelle stecken nicht in
der App, weil sie zu groß dafür sind.

Weitere Modelle findest du unter **Einstellungen → Transcription**: Modell wählen,
**Download** klicken. Was es gibt:

| Modell | Größe | Wofür |
| --- | --- | --- |
| Parakeet (Batch) | rund 660 MB | Die Mitschrift nach dem Gespräch. Der übliche Weg. |
| Parakeet (Streaming) | rund 125 MB | Untertitel live während des Gesprächs. |
| Whisper large-v3-turbo | rund 830 MB | Langsamer, aber besser bei Namen und Satzzeichen. |

Dazu kommen rund 35 MB für die Sprechertrennung, also dafür, dass im Text steht, wer
gerade spricht.

Der Download läuft über eine Leitung mit gut 50 Mbit in ein bis zwei Minuten, bei
langsamerem Netz entsprechend länger. Du siehst währenddessen einen Fortschritt in
Prozent. Geht dabei etwas schief, erscheint oben rechts eine Meldung.

**Wenn du diese Meldung wegklickst, siehst du den Grund nicht mehr** — die Zeile springt
dann einfach zurück auf den Download-Knopf. Probier es in dem Fall noch einmal.

---

## 4. Zusammenfassungen

Die Mitschrift läuft komplett auf deinem Rechner. Für die **Zusammenfassung** brauchst du
zusätzlich ein Sprachmodell, und da gibt es nur diese Wege:

- **Apple Intelligence** — der einzige Weg ohne Konto und ohne zusätzliche Software.
  Voraussetzung: macOS 26 oder neuer, ein Mac der Apple Intelligence unterstützt, und
  Apple Intelligence muss in den Systemeinstellungen eingeschaltet sein. Zu finden unter
  **Einstellungen → Intelligence**. Das Ganze ist als Versuch gekennzeichnet und
  funktioniert am besten bei kürzeren Gesprächen.
- **Ein eigener Zugang** bei einem Anbieter wie OpenAI, Anthropic oder Google. Dafür
  trägst du unter **Einstellungen → Intelligence** deinen eigenen Schlüssel ein. Achtung:
  In diesem Fall wird der Text des Gesprächs an diesen Anbieter geschickt.
- **Ollama oder LM Studio**, falls du so etwas ohnehin auf dem Rechner hast.

Findet die App kein Modell, zeigt sie dir statt der Zusammenfassung einen Hinweis mit
einem Knopf in die richtigen Einstellungen. Die Aufnahme und die Mitschrift funktionieren
davon unabhängig weiter.

---

## 5. Was die App nicht tut

- **Sie schickt weder Ton noch Text irgendwohin**, solange du bei den lokalen Modellen
  bleibst. Aufnahmen, Mitschriften und Zusammenfassungen liegen in deinem Benutzerordner.
- **Es gibt keine Anmeldung, kein Konto und kein Abo.** Wenn dich irgendwo etwas nach
  einem Konto fragt, ist das ein Fehler — sag Bescheid.
- **Es gibt keine Absturzberichte und keine Nutzungsstatistik.** Es ist kein Dienst dafür
  eingebaut.

Drei Ausnahmen, damit es ehrlich bleibt:

- Beim Laden eines Modells (Schritt 3) holt die App die Datei aus dem Netz, von
  Hugging Face.
- Beim Start und danach etwa alle 30 Minuten fragt die App bei GitHub
  (`github.com/simeonzickert/mitschnitt/releases`) nach, ob es eine neuere Version
  gibt. Dabei geht nur die Anfrage raus, keine Aufnahme und kein Text. Findet sie
  beim Start eine neue Version, lädt sie sie, installiert sie und startet einmal neu;
  findet sie später eine, zeigt sie einen Hinweis mit Knopf. Während einer Aufnahme
  wird nie aktualisiert, das wartet bis nach dem Meeting.
- Wenn du unter „Intelligence" selbst einen Anbieter mit eigenem Schlüssel einträgst,
  geht der Text des Gesprächs an diesen Anbieter. Das entscheidest du.

---

## 6. Wo die Lizenzen liegen

Die App baut auf der Arbeit vieler anderer Leute auf, und deren Bedingungen werden
mitgeliefert.

- **In der App**: Einstellungen → **About**. Dort stehen die Herkunft, die verwendeten
  Modelle mit ihren Lizenzen und die vollständige Liste aller Bausteine.
- **Als Dateien**: Rechtsklick auf Mitschnitt im Ordner Programme →
  **Paketinhalt zeigen** → `Contents/Resources/licenses/`.

---

## 7. Wenn etwas nicht geht

Sag dem Betreiber Bescheid, am besten mit diesen drei Angaben:

1. Was du gemacht hast, als es passierte.
2. Was du erwartet hast und was stattdessen passierte.
3. Deine macOS-Version (  → Über diesen Mac).

Ein Bildschirmfoto hilft fast immer.
