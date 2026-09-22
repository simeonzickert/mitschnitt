# Fremde Bestandteile in Mitschnitt

Mitschnitt ist ein privater Fork von [anarlog](https://github.com/fastrepl/anarlog).
In der App steckt Arbeit von vielen anderen Leuten. Diese Datei nennt sie und die
Bedingungen, unter denen sie mitgeliefert werden darf.

Die Datei wird mit ins App-Bundle gelegt (`Contents/Resources/licenses/`), zusammen mit
den Lizenztexten, auf die sie verweist. Wer die App weitergibt, gibt sie damit mit.

**Kurzfassung, wenn du wenig Zeit hast:** die Basis ist MIT und unproblematisch. Zwei Dinge
solltest du kennen, bevor du daraus ein Produkt machst. Erstens tragen die zur Laufzeit
geladenen Sprachmodelle eigene Bedingungen (Llama und Gemma sind keine OSI-Lizenzen).
Zweitens fehlt genau einem Modell die Lizenzangabe — dem Parakeet-Batch-Modell, also dem,
das die Transkription tatsächlich macht. Beides steht in Abschnitt 4.

---

## 1. Die Basis

| | |
| --- | --- |
| Projekt | anarlog (früher Hyprnote) |
| Quelle | https://github.com/fastrepl/anarlog |
| Rechteinhaber | Fastrepl, Inc. |
| Lizenz | MIT |
| Lizenztext | [`LICENSE`](LICENSE), im Bundle als `licenses/LICENSE-anarlog-MIT.txt` |

Die MIT-Lizenz verlangt, dass der Urheberrechtshinweis und der Lizenztext bei jeder Kopie
mitgeliefert werden. Genau dafür liegt die `LICENSE` im App-Bundle.

Mitschnitt ist ein eigenständiger Fork und steht in keiner Verbindung zu Fastrepl, Inc.

## 2. Schriften

| Schrift | Rechteinhaber | Lizenz | Wo |
| --- | --- | --- | --- |
| Pretendard | Kil Hyung-jin (mit Anteilen von Adobe Source und Inter) | SIL Open Font License 1.1 | `crates/export-core/fonts/`, per `include_bytes!` fest ins Binary kompiliert (PDF-Export). Eine zweite, derzeit unreferenzierte Kopie liegt in `plugins/export/fonts/`. |
| Cabin Sketch | Impallari Type | SIL Open Font License 1.1 | Windows-Fensterdekoration, `plugins/windows/swift-lib/src/Resources/` |

Beide Lizenztexte liegen neben den Schriftdateien und im App-Bundle
(`licenses/Pretendard-OFL.txt`, `CabinSketch-OFL.txt`).

| Schrift | Rechteinhaber | Lizenz | Wo |
| --- | --- | --- | --- |
| New Computer Modern | GUST e-foundry | GUST Font License (LPPL-artig) | kommt über die typst-Abhängigkeit, nicht als eigene Datei im Repo |

## 3. Modelle, die fest in der App stecken

Diese ONNX-Dateien werden per `include_bytes!` ins Binary kompiliert, sind also in jeder
Kopie der App enthalten.

| Modell | Zweck | Herkunft | Lizenz |
| --- | --- | --- | --- |
| pyannote Segmentation | Sprecher-Segmentierung | pyannote.audio | MIT, [Modellkarte](https://huggingface.co/pyannote/segmentation-3.0) |
| pyannote/WeSpeaker Embedding | Sprecher-Stimmabdruck | pyannote.audio-Familie | MIT, [Modellkarte](https://huggingface.co/pyannote/wespeaker-voxceleb-resnet34-LM) |
| Silero VAD v6.2 | Sprech-/Pausenerkennung | https://github.com/snakers4/silero-vad | MIT |
| DTLN | Rauschunterdrückung (`crates/denoise`) | https://github.com/breizhn/DTLN | MIT |
| DTLN-aec | Echo-Unterdrückung (`crates/aec`) | https://github.com/breizhn/DTLN-aec | MIT |

**Ehrlicher Stand zu DTLN und DTLN-aec:** die Zuordnung ist sehr wahrscheinlich, aber nicht
bewiesen. Die ONNX-Dateien liegen ohne Herkunftsvermerk im Repo, Architektur und
Namensgebung passen zu DTLN, aber es gibt keinen mitgelieferten Beleg. Dasselbe gilt für
das Embedding-Modell in `crates/embedding`, dessen Code sich ausdrücklich an pyannote.audios
WeSpeaker-Implementierung orientiert, ohne die Gewichte zu benennen. Wer die App
veröffentlicht, sollte das vorher klären.

## 3b. Sprachdaten, die fest in der App stecken

| Datei | Zweck | Herkunft | Lizenz |
| --- | --- | --- | --- |
| `crates/vocabulary/wortliste-de.txt` | 38.424 gewöhnliche deutsche Wortformen. Der Wächter, der verhindert, dass der Wörterbuch-Vorschlag zwei Alltagswörter gegeneinander tauscht. | Abgeleitet aus [UD German-GSD](https://github.com/UniversalDependencies/UD_German-GSD) | CC BY-SA 4.0 |

Die Datei wird per `include_str!` fest ins Binary kompiliert, ist also in jeder Kopie der
App enthalten.

**Woraus sie entsteht:** aus den Wortformen und Grundformen der Baumbank, ohne alles, was
dort als Eigenname (`PROPN`), Satzzeichen, Zahl oder Symbol markiert ist. Ein Wort wird
übernommen, wenn es öfter als gewöhnliches Wort denn als Eigenname vorkommt.
Kleingeschrieben, Umlaute zu `ae`/`oe`/`ue` aufgelöst, `ß` zu `ss`.

**Bedingungen der Quelle:** Die Baumbank steht unter der Creative-Commons-Lizenz
Namensnennung – Weitergabe unter gleichen Bedingungen 4.0 International
(<https://creativecommons.org/licenses/by-sa/4.0/>). Die abgeleitete Wortliste steht damit
unter derselben Lizenz. Der übrige Quelltext der App ist davon nicht berührt: die Liste ist
eine beigelegte Datendatei, kein Bestandteil des Programmtexts.

**Nicht genommen und warum** (gemessen am 04.09.2026, weil die Wahl der Quelle über die
Brauchbarkeit des ganzen Wörterbuch-Vorschlags entscheidet):

- Hunspell `de_DE_frami`: GPLv2/GPLv3, also für diese App gesperrt. Enthält außerdem
  Vornamen und Ortsnamen und hätte damit echte Namensvorschläge stillschweigend getötet.
- Häufigkeitslisten aus Untertiteln: enthalten Nachnamen aus dem Korpus, gleiches Problem
  in stärkerer Form.
- UD German-HDT: das Annotationsschema steht unter CC BY-SA 4.0, der zugrundeliegende
  **Text darf aber nur für akademische Zwecke weitergegeben** werden. Für eine App, die
  weitergegeben werden soll, ist das nicht tragfähig.

## 4. Modelle, die zur Laufzeit geladen werden

Diese Modelle sind **nicht** Teil der App. Sie werden erst heruntergeladen, wenn du sie in
den Einstellungen auswählst, und liegen danach in deinem Benutzerordner. Trotzdem gelten
ihre Bedingungen für dich als Nutzer.

| Modell | Lizenz | Anmerkung |
| --- | --- | --- |
| Whisper (ggml, via whisper.cpp) | MIT | Wird direkt von Hugging Face geladen (`ggerganov/whisper.cpp`). |
| NVIDIA Parakeet TDT v3, Batch (`aufklarer/Parakeet-TDT-v3-CoreML-INT8-30s`) | offen, siehe unten | Das Standardmodell für die Transkription fertiger Aufnahmen. CoreML-Umwandlung von [nvidia/parakeet-tdt-0.6b-v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) (CC-BY-4.0). |
| NVIDIA Parakeet EOU 120M, Live (`aufklarer/Parakeet-EOU-120M-CoreML-INT8`) | NVIDIA Open Model License | Das Modell für die Live-Untertitel. Basis: [nvidia/parakeet_realtime_eou_120m-v1](https://huggingface.co/nvidia/parakeet_realtime_eou_120m-v1). |
| pyannote Community-1, Sprechertrennung (`aufklarer/Pyannote-Community-1-CoreML`) | CC-BY-4.0, mit `LICENSE` im Repo | Wird zusammen mit dem Batch-Modell geladen und trennt die Sprecher. Basis: [pyannote/speaker-diarization-community-1](https://huggingface.co/pyannote/speaker-diarization-community-1). |
| Omnilingual ASR CTC 300M (`aufklarer/Omnilingual-ASR-CTC-300M-CoreML-INT8-10s`) | Apache-2.0 | Im Code vorhanden, in der Auswahl nicht angeboten. Basis: `facebook/omniASR-CTC-300M`. |
| Llama 3.2 3B Instruct | Llama 3.2 Community License | **Keine OSI-Lizenz.** Eigene Bedingungen, u. a. Namensnennungs- und Nutzungspflichten. |
| Gemma 3 4B IT | Gemma Terms of Use | **Keine OSI-Lizenz.** Eigene Bedingungen, u. a. Nutzungsbeschränkungen. |

**Merker zur Bezugsquelle:** Llama und Gemma werden direkt von Hugging Face geladen, Whisper
seit dem 31.08.2026 ebenfalls. `hypr-llm-sm` (das Sprachmodell des Ursprungsprojekts) wird
seit dem 02.09.2026 nicht mehr angeboten: es ließ sich nur bitgenau über einen S3-Speicher
laden, der dem Ursprungsprojekt gehört (`hyprnote.s3.us-east-1.amazonaws.com`) — kein
Lizenzproblem, aber eine Leitung, die uns niemand schuldet. Die Fassung auf Hugging Face ist
nicht dieselbe Datei (andere Prüfsumme), und die Prüfsumme wird nicht angepasst, damit es
passt. Details im Kopf von `crates/local-model/src/lib.rs`.

**Die Parakeet-Modelle kommen NICHT mehr von Argmax.** Bis zum 01.09.2026 lud die App drei
Modelle aus `argmaxinc/parakeetkit-pro` und `argmaxinc/whisperkit-pro`. Deren
`LICENSE_NOTICE.txt` sagt wörtlich *„Argmax proprietary and confidential. Under NDA. …
Unauthorized access, copying, use, distribution, and or commercialization … is strictly
prohibited"*; die Repos sind auf Hugging Face zugangsbeschränkt und nur für das Argmax Pro
SDK lizenziert. Das Ursprungsprojekt hat sie umverpackt und über eigenes S3 ausgeliefert.
Dieser gesamte Weg ist entfernt — nicht ersetzt, sondern gestrichen.

**Die eine offene Flanke, ehrlich benannt:** das Batch-Modell
`aufklarer/Parakeet-TDT-v3-CoreML-INT8-30s` hat auf Hugging Face **keine Lizenzangabe und
kein README** (gemessen 01.09.2026, **nachgemessen 04.09.2026: unverändert, die Seite meldet
weiterhin „No model card"**). Das ist eine fehlende Modellkarte, keine
Untersagung — die beiden Geschwister-Fassungen derselben Umwandlung,
`aufklarer/Parakeet-TDT-v3-CoreML-INT8` und `…-iOS-5s`, tragen beide CC-BY-4.0 und
beschreiben dieselbe Architektur (FastConformer, 24 Schichten, 1024 hidden — exakt die
Werte in der `config.json` des 30s-Repos), und das NVIDIA-Ursprungsmodell steht ebenfalls
unter CC-BY-4.0. **Vor einer Veröffentlichung sollte das schriftlich geklärt werden.**

Ein Wechsel auf eine Quelle mit ausgewiesener Lizenz wurde geprüft und verworfen:
`FluidInference/parakeet-tdt-0.6b-v3-coreml` (CC-BY-4.0) liefert einen anders geschnittenen
Rechengraphen — 15-Sekunden-Fenster statt 30, andere Eingabenamen, andere Dateistruktur —
und passt nicht in den Lader von `soniqo/speech-swift`. Die Messung steht im Kopf von
`crates/transcribe-soniqo/src/model.rs`.

`aufklarer` ist übrigens kein fremder Spiegel, sondern die Modell-Heimat der Bibliothek, die
sie lädt: `soniqo/speech-swift` (Apache-2.0). Der Download läuft in deren Swift-Code, nicht
in unserem.

## 5. Fremde Marken und Logos

In der Modell- und Anbieterauswahl sowie bei den Integrationen zeigt die App Logos und
Wortmarken dieser Anbieter:

OpenAI, Meta, NVIDIA, Qwen (Alibaba), Deepgram, Gladia, Cartesia, Rev.ai, Speechmatics,
Soniox, Unsloth, AssemblyAI, pyannote, Obsidian, Slack, Zoom, Microsoft Teams, Google Meet,
Webex, Linear, GitHub, Apple.

Das sind Marken der jeweiligen Inhaber. Sie werden ausschließlich verwendet, um kenntlich zu
machen, welcher Dienst oder welches Modell gemeint ist. Es besteht keine Verbindung zu diesen
Unternehmen, und keines von ihnen unterstützt, prüft oder billigt Mitschnitt.

## 6. Klänge

Der Onboarding-Jingle `plugins/sfx/sounds/bgm.mp3` (aus dem Upstream-Projekt, dort laut
Fundlage mit Suno erzeugt, Rechtelage von außen nicht feststellbar) wurde am 22.09.2026
vollständig entfernt.

Zwei kurze Hinweistöne bleiben: `plugins/sfx/sounds/start_recording.ogg` und
`plugins/sfx/sounds/stop_recording.ogg`. Herkunft: Upstream anarlog, keine eigene
Lizenzangabe.

## 7. Weitere Abhängigkeiten

**Eine Namensgleichheit, die keine ist:** `crates/transcribe-soniqo/swift-lib` zieht über
`soniqo/speech-swift` auch [WhisperKit](https://github.com/argmaxinc/WhisperKit) herein, ein
Paket von Argmax. Das ist **nicht** der proprietäre Pfad, der am 01.09.2026 entfernt wurde:
WhisperKit selbst steht unter **MIT** (`Copyright (c) 2024 argmax, inc.`), es ist die
Bibliothek und nicht die Modelle des Pro-SDK, und speech-swift führt sie ausdrücklich nur zum
Benchmark-Vergleich mit. Wer `argmax` in `Package.resolved` findet, hat also keinen Rest
gefunden.

Rust-Crates und npm-Pakete tragen ihre Lizenzen in `Cargo.lock` und `pnpm-lock.yaml`. Die
vollständige, maschinell erzeugte Liste liegt in
[`apps/desktop/src/about/dritte-lizenzen.json`](apps/desktop/src/about/dritte-lizenzen.json)
(`node scripts/lizenzen-sammeln.mjs`, gemessen 22.09.2026: 1255 fremde Rust-Pakete, 746
fremde npm-Pakete). Die meisten sind MIT/Apache-2.0/BSD/ISC und brauchen nichts weiter als
Namensnennung. Was darüber hinausgeht, steht hier einzeln.

## 7b. Rust- und npm-Pakete mit Bedingungen über Namensnennung hinaus

**MPL-2.0 (Rust, 24 Pakete):** `colored`, `cssparser`/`cssparser-macros`, `dtoa-short`,
`option-ext`, `progenitor`/`progenitor-client`/`progenitor-impl`/`progenitor-macro`,
`selectors`, `smartstring` (MPL-2.0+), sowie die komplette
[Symphonia](https://github.com/pdeljanov/Symphonia)-Familie (`symphonia`,
`symphonia-core`, `symphonia-bundle-flac`, `symphonia-bundle-mp3`, `symphonia-codec-aac`,
`symphonia-codec-adpcm`, `symphonia-codec-alac`, `symphonia-codec-pcm`,
`symphonia-codec-vorbis`, `symphonia-format-caf`, `symphonia-format-isomp4`,
`symphonia-format-mkv`, `symphonia-format-ogg`, `symphonia-format-riff`,
`symphonia-metadata`, `symphonia-utils-xiph`) für Audio-De-/Encoding. MPL ist
dateibasiertes Copyleft: Änderungen an den MPL-Dateien selbst müssten offengelegt werden,
unverändert eingebundene Bibliotheken nicht. Hier ist keine davon verändert.
`htmlescape` trägt zusätzlich MPL-2.0 als eine von drei Alternativlizenzen
(`Apache-2.0 / MIT / MPL-2.0`).

**CDLA-Permissive-2.0 (Rust, 3 Pakete):** `webpki-root-certs`, `webpki-roots` (zwei
Versionen, 0.26.11 und 1.0.7) — Zertifikatsdaten von Mozillas Root-Store, verpackt von
[rustls](https://github.com/rustls/webpki-roots). Permissiv, verlangt aber Namensnennung
der Datenquelle statt nur des Codes.

**Dual-Lizenzen mit einer nicht-OSI-Hälfte (Rust, 2 Pakete):** `ryu` und `whoami` bieten
`Apache-2.0 OR BSL-1.0` (bzw. zusätzlich `OR MIT`) an — beide werden unter Apache-2.0
verwendet, BSL-1.0 (Boost) ist die Alternative, keine Pflicht.

**CC-BY-4.0 (npm, 1 Paket):** `caniuse-lite` (Browserslist-Datenbank) — Datennennung, kein
Code-Copyleft.

**Dual-Lizenzen mit permissiver Wahl (npm, 2 Pakete):** `dompurify`
(`MPL-2.0 OR Apache-2.0`, hier unter Apache-2.0 genutzt) und `json-schema`
(`AFL-2.1 OR BSD-3-Clause`, hier unter BSD-3-Clause genutzt).

**LGPL-3.0 (npm) — entfernt (22.09.2026):** [`pos`](https://github.com/dariusk/pos-js) 0.4.2
(Part-of-Speech-Tagger) stand hier bis 22.09.2026 als tatsächlich genutztes, nicht nur
installiertes Paket, eingebunden über `retext-pos` in `apps/desktop/src/stt/useKeywords.ts`
(`.use(retextPos)`, Stichwort-Extraktion aus Transkripten). `retext-pos` und `pos` sind
ersetzt: `apps/desktop/src/stt/keyword-importance.ts` (Eigenbau, keine fremde Abhängigkeit)
übernimmt das Tagging jetzt über eine Stoppwortliste statt eines englischen
Penn-Treebank-Taggers — gemessen an deutschem Text lieferte `pos` ohnehin keine echte
Wortartanalyse, da praktisch jedes deutsche Wort auf den Lexikon-Standardwert zurückfiel
(Begründung im Dateikopf von `keyword-importance.ts`). Weder `pos` noch `retext-pos` tauchen
mehr in `pnpm-lock.yaml` oder `dritte-lizenzen.json` auf (`pnpm why pos` liefert nichts).

**Ohne Lizenzangabe (4 Pakete, gemessen):**
- `gbnf-validator` 0.1.0 (Rust, git-Abhängigkeit direkt aus
  `github.com/fastrepl/gbnf-validator`) — **landet nicht im ausgelieferten Programm.**
  `crates/gbnf` (der einzige Konsument von `gbnf-validator` im Workspace) hat selbst keinen
  Abhängigen: weder ein anderer Crate noch `apps/desktop/src-tauri` referenziert `anlg-gbnf`.
  Toter Workspace-Member, kein Auslieferungsproblem.
- `@splinetool/runtime` 0.9.526 (npm) — kein Lizenzfeld in `package.json`, kein `LICENSE` im
  Paket, kein öffentliches Repo unter `github.com/splinetool/runtime` (404, Stand
  22.09.2026); vermutlich proprietär, da Spline ein kommerzielles Produkt ist. **Nach
  Quelltext-Prüfung nicht im Frontend-Bundle:** Das Paket kommt ausschließlich als
  `dependencies`-Eintrag von `@lobehub/ui` herein; `apps/desktop/src` importiert nirgends
  aus `@lobehub/ui`, nur aus `@lobehub/icons` (dessen eigene `package.json` `@lobehub/ui`
  nur als leere `peerDependency` führt, ohne Spline/Giscus-Bezug). Ohne echten Bau nicht zu
  100 % auszuschließen, aber die Importkette spricht eindeutig gegen eine Auslieferung.
- `@giscus/react` 3.1.0 (npm) — gleiche Fundlage und gleiche Nicht-Nutzung wie
  `@splinetool/runtime` (identischer Pfad über `@lobehub/ui`).
- `khroma` 2.1.0 (npm) — **das ist eine Lücke im `package.json`, keine fehlende Lizenz:**
  das Paket legt eine eigene `license`-Datei bei, die MIT ausweist
  (`Copyright (c) 2019-present Fabio Spampinato, Andrew Maney`). Über `mermaid` genutzt.

**Eine Ausnahme von diesem Satz ist LAME.** Sie steht in Abschnitt 8, weil sie als einzige
Abhängigkeit im Baum keine Erlaubnis-Lizenz trägt, sondern eine mit einer Bedingung an die
Bauweise.

## 8. libmp3lame (LAME) — die einzige Copyleft-Abhängigkeit

| | |
| --- | --- |
| Bibliothek | LAME 3.100 (`libmp3lame`) |
| Zweck | Aufnahmen als MP3 speichern (`crates/mp3`, genutzt von `audio-norm`, `listener-core`, `listener2-core`) |
| Rechteinhaber | Mark Taylor und das LAME-Projekt |
| Lizenz | **GNU LGPL, Version 2 oder nach Wahl eine spätere** (`include/lame.h`: „version 2 of the License, or (at your option) any later version") |
| Rust-Anbindung | `mp3lame-sys 0.1.11`, **LGPL Version 3** |
| Quelltext | `vendor/mp3lame-sys/lame-3.100/`, unverändert; zusätzlich <https://lame.sourceforge.io/> |
| Lizenztexte | `licenses/LICENSE-LGPL-2.0.txt`, `LICENSE-LGPL-3.0.txt`, `LICENSE-GPL-3.0.txt` — im Bundle unter `Contents/Resources/licenses/` |
| Hinweis für Empfänger | `licenses/LIESMICH-libmp3lame.md`, ebenfalls im Bundle |

**Worin die Bedingung besteht.** Die LGPL erlaubt, die Bibliothek in ein Programm mit
eigener, auch geschlossener Lizenz einzubauen. Sie knüpft das in Paragraf 4 daran, dass der
Empfänger die Bibliothek **austauschen** kann — durch eine eigene, geänderte Fassung.
Dynamisches Linken genügt dafür ausdrücklich.

**Was hier bis zum 22.09.2026 falsch war.** `mp3lame-sys` baut LAME statisch
(`--disable-shared`, `cargo:rustc-link-lib=static=mp3lame`). Gemessen am ausgelieferten
Bundle:

    nm -U Mitschnitt.app/Contents/MacOS/mitschnitt | grep -c " _lame_"   ->  67

67 definierte LAME-Symbole im Hauptprogramm. Austauschen war unmöglich, und die Lizenztexte
lagen nicht bei. Eine Weitergabe wäre in dieser Form nicht zulässig gewesen.

**Was jetzt gilt.** LAME wird auf macOS als eigene Datei gebaut und liegt als
`Contents/Frameworks/libmp3lame.0.dylib` im Bundle; das Hauptprogramm lädt sie unter
`@rpath/libmp3lame.0.dylib`. Derselbe Befehl gibt jetzt `0` aus. Wie man sie ersetzt, steht
in `licenses/LIESMICH-libmp3lame.md`. Der Wächter `scripts/lgpl-gate.sh` prüft das am
gebauten Bundle, damit ein Paketupdate den Zustand nicht still zurückdreht.

**Was am Quelltext der Bibliothek geändert wurde: nichts.** Geändert wurde nur ihre Bauart
(statisch → dynamisch, mpg123-Dekoder mit übersetzt, weil LAMEs eigene Export-Liste ihn
verlangt). Begründung mit den gemessenen Fehlermeldungen im Kopf von
`vendor/mp3lame-sys/build.rs`.

**Entschieden (22.09.2026, Commit `c12de21af2`).** Mit Hardened Runtime lädt macOS nur
Bibliotheken, die mit derselben Team-ID signiert sind wie die App. Ein Empfänger könnte damit
seine eigene Fassung nicht laden, und das Austauschrecht bliebe formal eingeräumt, praktisch
aber versperrt. Dagegen hilft genau ein Recht:
`com.apple.security.cs.disable-library-validation`, seit diesem Commit in
`Entitlements.notar.plist` **gesetzt** — es schwächt die Laufzeit messbar (die App lädt dann
auch fremd signierte Bibliotheken aus ihrem eigenen Bundle) und war deshalb eine bewusste
Entscheidung, keine Nebenwirkung eines Umbaus. Notarisierte Weitergabe ist damit erlaubt.

**Eine Ad-hoc-Neusignatur durch den Empfänger entwertet die Notarisierung.** Das Recht oben
erlaubt, die Dylib gegen eine ANDERS signierte Fassung auszutauschen, ohne dass macOS die App
deshalb ablehnt — es ändert nichts daran, dass eine erneute Signatur des GESAMTEN Bundles
(z.B. durch `codesign --force --deep --sign -` nach dem Austausch, wie in
`licenses/LIESMICH-libmp3lame.md` beschrieben) das angeheftete Notarisierungsticket entwertet.
Gatekeeper fragt dann wieder nach, genau wie bei einer unnotarisierten App — erwartet und in
Ordnung fürs eigene Testen auf dem eigenen Rechner, aber keine Weitergabe an Dritte mehr wert.

**Linux und Windows sind nicht umgestellt.** Dort baut `mp3lame-sys` weiter statisch. Solange
von dort nichts weitergegeben wird, ist das folgenlos; sobald doch, gilt dieser Abschnitt
dort genauso und der Umbau steht noch aus.

---

*Stand: 22. September 2026 (Abschnitt 8 an diesem Tag neu gemessen und angelegt; Modell-Abschnitt am 1. September). Wenn du eine Abhängigkeit hinzufügst, die eigene Bedingungen
trägt, gehört sie hier hinein, bevor die nächste Kopie das Haus verlässt.*
