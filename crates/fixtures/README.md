# fixtures — selbst erzeugte Testtoene

Ersatz fuer `crates/data`. Der Vorgaenger hielt 64 Dateien fremder Aufnahmen,
77 MB, **ohne jede Herkunfts- oder Lizenzangabe** — genau deshalb ist er am
01.09.2026 geloescht worden. Damit fiel das Testnetz von elf Crates weg, unter
anderem in der Ecke, in der am 31.08. der 29,5-Sekunden-Rasterfehler steckte.

Dieses Crate schliesst die Luecke, ohne die Lizenzfrage zurueckzuholen: **alles
hier entsteht auf dem eigenen Rechner** aus den macOS-Systemstimmen und
synthetischen Wandlungen derselben Quelle. Nichts ist aufgenommen, nichts
uebernommen.

## Neu erzeugen

```sh
crates/fixtures/scripts/generate.sh
```

Braucht macOS (`say`, `afconvert`) und `ffmpeg`. Der Ablauf im Klartext:

1. `say -v Anna -o audio/speech-de.wav --data-format=LEI16@16000 "<deutscher Satz>"`
   und dasselbe mit `-v Samantha` fuer Englisch. Ergebnis: 16 kHz, mono, 16 bit.
2. Die Formatmatrix (`mp3 mp4 m4a aac ogg flac aiff caf`) entsteht per `ffmpeg`
   bzw. `afconvert` **aus der einen deutschen WAV-Datei** — nicht aus je einer
   eigenen Aufnahme, sonst waere nichts mehr vergleichbar.
3. `speech-stereo.mp3` legt die deutsche Spur nach links und die englische nach
   rechts (`amerge`). Zwei *gleiche* Kanaele wuerden auch dann noch stereo
   aussehen, wenn ein Kodierer still auf mono zusammenfaellt.
4. Die Abtastraten-Leiter `rate-8000 … rate-48000` sind je 1,5 Sekunden
   derselben Quelle in sechs Raten.

Die genauen Saetze stehen im Skript. Wer sie aendert, aendert die Dateien —
die Tests haengen an Form und Rate, nicht am Wortlaut.

## Was hier bewusst NICHT liegt

Alles, wofuer echte menschliche Sprache noetig waere. Synthetische Stimmen
tragen Formate, Container, Raten, Kanaele und Blockgrenzen; sie tragen **keine**
Aussage ueber Erkennungsguete, Sprecher-Einbettungen oder den Abstand zu einer
fremden Referenzmessung. Diese Tests mit angepassten Erwartungswerten
wiederzubeleben hiesse, einen Test zu bauen, der seine eigene Ausgabe bestaetigt.
Sie bleiben geloescht; welche das sind, steht in der Commit-Nachricht zu
`73ecb6fa11` und in der zu dieser Wiederherstellung.

## Platzbudget

Unter 5 MB, gemessen von einem eigenen Test (`the_whole_set_stays_far_below_five_megabytes`).
Stand der Einrichtung: rund 2,0 MB.
