//! Sprechertrennung fuer eine FERTIGE Aufnahme nachholen (10.09.2026).
//!
//! **Wofuer das da ist.** Eine echte Raumaufnahme vom 10.09. hat ihre
//! Sprecherzuordnung verloren, weil er zwei Minuten und 47 Sekunden ueber dem
//! damaligen 120-Minuten-Deckel lag. Die 17.191 Woerter liegen unversehrt in
//! der Datenbank, nur `speaker_hints_json` ist leer. Neu zu transkribieren
//! waere Verschwendung: die Woerter sind richtig, es fehlt nur, WER sie
//! gesagt hat.
//!
//! Dieses Werkzeug rechnet genau das nach -- Ton lesen, trennen (zerlegt,
//! ueber `diarize_samples_chunked`), jedes Wort seinem Sprecher zuordnen -- und
//! schreibt die fertigen Hinweise als SQL heraus. **Es fasst keine Datenbank
//! an.** Das Einspielen ist ein eigener, sichtbarer Schritt; wer eine
//! Sitzung veraendert, soll das selbst tun und vorher hinsehen.
//!
//! Die Zuordnung kommt aus `anlg_transcribe_soniqo::speaker_for_word` und
//! danach durch `smooth_speakers`, also aus denselben Funktionen wie beim
//! Stapellauf. Zwei Kopien dieser Regeln wuerden auseinanderlaufen, und der
//! Unterschied waere ein Wort, das je nach Weg einem anderen Menschen
//! gehoert.
//!
//! # Wie man es benutzt
//!
//! **Zuerst die App beenden** -- sie haelt Transkripte im Speicher und
//! schreibt sie unter einer eigenen Zaehlung zurueck. Solange sie laeuft,
//! kann sich der Stand zwischen Export und Einspielen aendern.
//!
//! Dann beide Werte lesen, die das Werkzeug braucht. Sie gehoeren zusammen:
//! die Woerter sind die Rechengrundlage, die Revision ist der Beweis, dass
//! sie beim Einspielen noch gilt.
//!
//! ```text
//! DB=~/Library/Application\ Support/media.zickert.mitschnitt/app.db
//! ID=<transcript-id>
//!
//! sqlite3 "$DB" "SELECT content_revision FROM transcripts WHERE id='$ID';"
//! sqlite3 "$DB" "SELECT words_json FROM transcripts WHERE id='$ID';" > woerter.json
//! ```
//!
//! Zwei Befehle statt einem, und das ist Absicht: ein Einzeiler, der beides
//! in einem Zug liest, waere nur dann besser, wenn sich der Stand dazwischen
//! aendern koennte -- und genau das schliesst die beendete App aus. Ein
//! fragiler Einzeiler, der Konsistenz nur vortaeuscht, waere schlechter als
//! zwei ehrliche Zeilen.
//!
//! ```text
//! cargo run --release --example nachtraegliche_sprecher -- \
//!     --audio <sitzung>/audio_mic.wav \
//!     --kanal 0 \
//!     --woerter woerter.json \
//!     --transkript $ID \
//!     --revision <die gelesene Zahl> \
//!     > hinweise.sql
//! ```
//!
//! Das erzeugte SQL prueft die Revision und hebt sie um eins. Passt sie
//! nicht, passiert nichts und die Diagnose daneben sagt, warum.
//!
//! # Was am 21.09.2026 dazukam, und warum
//!
//! Drei Dinge, die das Werkzeug vorher unsicher machten:
//!
//! 1. **`content_revision` wird mitgezaehlt.** Die App schreibt Transkripte
//!    unter einer optimistischen Sperre (`apps/desktop/src/stt/queries.ts`:
//!    `UPDATE ... SET content_revision = content_revision + 1 WHERE
//!    content_revision = ?`). Ein Schreibvorgang, der die Zaehlung NICHT
//!    hochzaehlt, ist fuer die App unsichtbar: sie haelt ihren gelesenen
//!    Stand weiter fuer aktuell und ueberschreibt die nachgeholten Hinweise
//!    beim naechsten Speichern spurlos.
//! 2. **Die Abtastrate wird geprueft.** Das Verfahren rechnet fest mit
//!    16 kHz. Eine 44,1- oder 48-kHz-Datei haette klaglos falsche Zeiten
//!    ergeben -- jede Sprechergrenze um den Faktor der Ratendifferenz
//!    verschoben.
//! 3. **Das `UPDATE` ist an `content_revision` gebunden** (`--revision`,
//!    Pflicht). Ohne das schreibt ein spaeterer Lauf Hinweise auf Wort-IDs,
//!    die es nach einer Neutranskription nicht mehr gibt -- und sperrt sich
//!    danach durch die eigene Nichtleer-Bedingung selbst aus. Die Revision
//!    ist derselbe Riegel, den die App fuer sich selbst benutzt; eine
//!    Bindung an Laenge und Wort-IDs war der schwaechere Ersatz dafuer.

use std::collections::BTreeMap;
use std::time::Instant;

/// Die Abtastrate, mit der die Trennung rechnet.
const ERWARTETE_ABTASTRATE: u32 = anlg_transcribe_soniqo::DIARIZATION_SAMPLE_RATE;

/// Ab dieser Laenge wird zerlegt -- und ab hier wirkt `--sprecher` nicht mehr.
fn plan_chunk_samples() -> usize {
    anlg_transcribe_soniqo::ChunkPlan::default().chunk_samples()
}

#[derive(serde::Deserialize)]
struct StoredWord {
    id: String,
    #[serde(default)]
    start_ms: i64,
    #[serde(default)]
    end_ms: i64,
    #[serde(default)]
    channel: i32,
}

fn main() {
    let mut audio: Option<String> = None;
    let mut words_path: Option<String> = None;
    let mut transcript_id: Option<String> = None;
    let mut channel: i32 = 0;
    let mut exact_speakers: Option<usize> = None;
    let mut revision: Option<i64> = None;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--audio" => audio = args.next(),
            "--woerter" => words_path = args.next(),
            "--transkript" => transcript_id = args.next(),
            "--kanal" => {
                channel = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--kanal braucht eine Zahl")
            }
            "--sprecher" => {
                exact_speakers = Some(
                    args.next()
                        .and_then(|v| v.parse().ok())
                        .expect("--sprecher braucht eine Zahl"),
                )
            }
            "--revision" => {
                revision = Some(
                    args.next()
                        .and_then(|v| v.parse().ok())
                        .expect("--revision braucht eine Zahl"),
                )
            }
            other => panic!("unbekannter Schalter: {other}"),
        }
    }

    let audio = audio.expect("--audio fehlt");
    let words_path = words_path.expect("--woerter fehlt");
    let transcript_id = transcript_id.expect("--transkript fehlt");

    let revision = revision.expect(
        "--revision fehlt. Sie ist der eigentliche Riegel gegen ein Einspielen auf einen \
         veraenderten Stand und wird zusammen mit den Woertern gelesen (siehe Kopf).",
    );

    let words_raw = std::fs::read_to_string(&words_path).expect("Woerterdatei nicht lesbar");
    let words: Vec<StoredWord> =
        serde_json::from_str(&words_raw).expect("Woerterdatei ist kein gueltiges JSON");
    assert!(!words.is_empty(), "die Woerterdatei ist leer");
    // Die Laenge in BYTES, und zwar ohne den Zeilenumbruch, den sqlite3 beim
    // Export anhaengt. Beides ist gemessen, nicht vermutet (21.09.2026 an
    // einer echten 122,8-Minuten-Aufnahme):
    //
    //   length(words_json)                  3.444.739   <- ZEICHEN
    //   length(CAST(words_json AS BLOB))    3.445.735   <- Bytes
    //   Dateigroesse des Exports            3.445.736   <- Bytes + "\n"
    //
    // Die erste Fassung dieser Bindung verglich die Dateigroesse mit
    // `length(words_json)` und lag damit um 997 daneben: SQLite zaehlt bei
    // TEXT Zeichen, Rust zaehlt Bytes, und 498 Umlaute im deutschen
    // Transkript machen den Unterschied. Das UPDATE endete zuverlaessig mit
    // null geaenderten Zeilen, und die Diagnose sagte nur "unklar".
    let words_bytes = words_raw.trim_end().len();

    let mut reader = hound::WavReader::open(&audio).expect("WAV nicht lesbar");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1, "erwartet EINEN Kanal");
    // Die Trennung rechnet fest mit 16 kHz (`DIARIZATION_SAMPLE_RATE`). Eine
    // andere Rate wuerde nicht auffallen, sondern still alle Zeiten
    // verschieben -- also lieber hier abbrechen.
    assert_eq!(
        spec.sample_rate, ERWARTETE_ABTASTRATE,
        "diese Datei hat {} Hz, die Trennung rechnet mit {ERWARTETE_ABTASTRATE} Hz. \
         Vorher umwandeln, zum Beispiel: \
         ffmpeg -i <datei> -af \"pan=mono|c0=c0\" -c:a pcm_f32le -ar 16000 kanal0.wav",
        spec.sample_rate
    );
    let sample_rate = spec.sample_rate as usize;
    let samples = reader
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .expect("Samples nicht lesbar");

    eprintln!(
        "Ton      {:.1} min ({} Samples bei {} Hz)",
        samples.len() as f64 / sample_rate as f64 / 60.0,
        samples.len(),
        sample_rate
    );
    eprintln!(
        "Woerter  {} gesamt, davon Kanal {channel}: {}",
        words.len(),
        words.iter().filter(|w| w.channel == channel).count()
    );
    if let Some(exact) = exact_speakers {
        // `--sprecher` ist die ECHTE Zahl der Menschen, die auf diesem Kanal
        // zu hoeren sind -- bei einer Raumaufnahme also inklusive der Person,
        // die aufnimmt. Das Werkzeug fragt bewusst nicht die App-Zaehlung:
        // `expected_speakers_per_channel` zaehlt nur die FERNEN Teilnehmer und
        // liegt bei einer Raumaufnahme deshalb systematisch zu niedrig.
        //
        // Wirkung, je nach Laenge: unterhalb der Abschnittslaenge (45 min)
        // reicht die Zahl an die Bibliothek durch und wird dort eingehalten.
        // Darueber wird sie nicht erzwungen, aber seit dem 22.09.2026 auch
        // nicht mehr nur gemeldet: `resolve_surplus_speakers` loest
        // ueberzaehlige Gruppen auf, soweit sie erkennbar Rest-Cluster sind.
        // Was die Schutzbedingung nicht erfuellt, bleibt stehen und wird
        // gemeldet.
        let zerlegt = samples.len() > plan_chunk_samples();
        eprintln!(
            "Vorgabe  {exact} Sprecher ({})",
            if zerlegt {
                "Rest-Cluster werden aufgeloest, echte Stimmen bleiben -- Aufnahme ist \
                 laenger als ein Abschnitt"
            } else {
                "wird eingehalten -- Aufnahme passt in einen Abschnitt"
            }
        );
    }

    let started_at = Instant::now();
    let plan = anlg_transcribe_soniqo::ChunkPlan::default();
    struct Fortschritt;
    impl anlg_transcribe_soniqo::ChunkObserver for Fortschritt {
        fn should_continue(&mut self, finished: usize, total: usize) -> bool {
            if finished > 0 {
                eprintln!("  Abschnitt {finished}/{total} fertig");
            }
            true
        }
        fn speaker_count_differs(
            &mut self,
            expected: usize,
            found: usize,
            highest_remaining_similarity: Option<f32>,
        ) {
            eprintln!("Sprecherzahl weicht ab: vorgegeben {expected}, gefunden {found}.");
            match highest_remaining_similarity {
                Some(similarity) => eprintln!(
                    "  Hoechste Aehnlichkeit zwischen zwei gefundenen Stimmen: {similarity:+.4} \
                     (ab +0,8000 gelten zwei als derselbe Mensch)."
                ),
                None => eprintln!("  Nur eine Stimme gefunden, nichts zu vergleichen."),
            }
            eprintln!(
                "  Die Zahl wird NICHT erzwungen. Zwei Etiketten im Transkript \
                 zusammenzulegen ist reparierbar, zwei Menschen zusammenzulegen nicht."
            );
        }
    }
    let mut observer = Fortschritt;
    let segments = anlg_transcribe_soniqo::diarize_samples_chunked(
        anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch,
        &samples,
        exact_speakers,
        plan,
        &mut observer,
    )
    .expect("Trennung fehlgeschlagen");
    eprintln!(
        "Trennung {:.1} s, {} Abschnitte",
        started_at.elapsed().as_secs_f64(),
        segments.len()
    );

    // Erst zuordnen, dann glaetten -- genau wie der Stapellauf
    // (`batch_words_from_chunks`). Ohne den zweiten Schritt bekaeme eine
    // nachgeholte Aufnahme eine andere Zuordnung als eine frisch
    // transkribierte, und der Unterschied waere ein Wort, das je nach Weg
    // einem anderen Menschen gehoert.
    let kanal_woerter: Vec<&StoredWord> =
        words.iter().filter(|w| w.channel == channel).collect();
    let mut geglaettet: Vec<anlg_transcribe_soniqo::SmoothingWord> = kanal_woerter
        .iter()
        .map(|word| anlg_transcribe_soniqo::SmoothingWord {
            start_seconds: word.start_ms as f64 / 1000.0,
            end_seconds: word.end_ms as f64 / 1000.0,
            speaker: anlg_transcribe_soniqo::speaker_for_word(
                &segments,
                word.start_ms as f64 / 1000.0,
                word.end_ms as f64 / 1000.0,
            ),
        })
        .collect();
    let umgehaengt = anlg_transcribe_soniqo::smooth_speakers(&mut geglaettet);
    eprintln!("Glaettung {umgehaengt} Woerter umgehaengt");

    let mut per_speaker = BTreeMap::<usize, usize>::new();
    let mut unassigned = 0usize;
    let mut rows = Vec::new();

    for (word, smoothed) in kanal_woerter.iter().zip(geglaettet.iter()) {
        match smoothed.speaker {
            Some(speaker_index) => {
                *per_speaker.entry(speaker_index).or_default() += 1;
                // `value` ist im Bestand eine JSON-Zeichenkette IN einem
                // JSON-Feld -- nicht flach machen, sonst liest die Oberflaeche
                // den Hinweis nicht.
                let value = serde_json::to_string(&serde_json::json!({
                    "provider": "soniqo",
                    "channel": channel,
                    "speaker_index": speaker_index,
                }))
                .expect("value nicht serialisierbar");
                rows.push(serde_json::json!({
                    // Dieselbe Form, die die App heute selbst erzeugt
                    // (`apps/desktop/src/stt/utils.ts`: `${wordId}:provider_speaker_index`).
                    // Ausgewertet wird sie nirgends -- gefiltert wird ueber
                    // `type` und `word_id` --, aber sie ist bestimmt statt
                    // zufaellig: zwei Laeufe ueber dieselben Woerter ergeben
                    // dieselben Zeilen statt neuer Dubletten.
                    "id": format!("{}:provider_speaker_index", word.id),
                    "word_id": word.id,
                    "type": "provider_speaker_index",
                    "value": value,
                }));
            }
            None => unassigned += 1,
        }
    }

    eprintln!("Zugeordnet:");
    for (speaker, count) in &per_speaker {
        eprintln!("  Sprecher {speaker}: {count} Woerter");
    }
    if unassigned > 0 {
        eprintln!("  OHNE Sprecher: {unassigned} Woerter");
    }

    // Erste und letzte Wort-ID standen hier bis zum 21.09.2026 zusaetzlich in
    // der Bedingung. Sie sind raus: neben `content_revision` tragen sie
    // nichts mehr bei -- jede Aenderung an den Woertern laeuft durch die App
    // und hebt die Revision -- und eine Bedingung, die nichts sichert, laesst
    // die anderen staerker aussehen, als sie sind.
    let hints = serde_json::to_string(&rows).expect("Hinweise nicht serialisierbar");

    println!("-- {} Hinweise fuer Transkript {transcript_id}, Kanal {channel}", rows.len());
    println!("--");
    println!("-- WICHTIG: die App vorher BEENDEN. Sie haelt Transkripte im Speicher und");
    println!("-- schreibt sie unter einer eigenen Zaehlung zurueck; ein Schreibvorgang");
    println!("-- neben der laufenden App kann von ihr wieder ueberschrieben werden.");
    println!("--");
    println!("--");
    println!("-- WENN DIESES SQL AN EINER KOPIE GERECHNET WURDE: die Revision unten");
    println!("-- ({revision}) stammt aus dem Stand, der beim Export vorlag. Vor dem");
    println!("-- Einspielen in die ECHTE Datenbank einmal nachlesen --");
    println!("--   sqlite3 \"$DB\" \"SELECT content_revision FROM transcripts WHERE id='{}';\"", transcript_id.replace('\'', "''"));
    println!("-- -- und wenn dort eine andere Zahl steht, hat die App zwischenzeitlich");
    println!("-- geschrieben: dann neu exportieren und neu rechnen, nicht die Zahl von");
    println!("-- Hand anpassen. Die Hinweise haengen an den Woertern, die beim Rechnen");
    println!("-- vorlagen.");
    println!("--");
    println!("-- Die WHERE-Bedingung ist absichtlich streng. Sie verlangt:");
    println!("--   * dieselbe Transkript-ID");
    println!("--   * eine noch LEERE Sprecherspalte (eine vorhandene, erst recht eine von");
    println!("--     Hand korrigierte Zuordnung wird nie ueberschrieben)");
    println!("--   * GENAU die Revision, aus der diese Hinweise gerechnet wurden. Das ist");
    println!("--     derselbe Riegel, den die App selbst benutzt: jede ihrer Schreibungen");
    println!("--     zaehlt content_revision unter 'WHERE content_revision = ?' hoch. Hat");
    println!("--     die App seit dem Export EINMAL geschrieben, passt die Zahl nicht mehr");
    println!("--     und es passiert nichts.");
    println!("--   * dieselbe Wortmenge in Bytes und Anzahl (billiger zweiter Riegel)");
    println!(
        "UPDATE transcripts SET speaker_hints_json = '{}', content_revision = {}, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = '{}' AND speaker_hints_json = '[]' AND deleted_at IS NULL AND content_revision = {} AND length(CAST(words_json AS BLOB)) = {} AND json_array_length(words_json) = {};",
        hints.replace('\'', "''"),
        revision + 1,
        transcript_id.replace('\'', "''"),
        revision,
        words_bytes,
        words.len(),
    );
    println!("--");
    println!("-- EINE Zeile Ergebnis. Der Erfolgsfall steht bewusst IN derselben Abfrage:");
    println!("-- die Diagnose darunter prueft den Zustand NACH dem UPDATE, und nach einem");
    println!("-- geglueckten Einspielen steht dort natuerlich eine Zuordnung. Als eigene");
    println!("-- Abfrage meldete sie deshalb auch im Erfolgsfall 'da stand schon eine");
    println!("-- Zuordnung' -- eine Meldung, die genau dann falsch klingt, wenn alles");
    println!("-- richtig gelaufen ist.");
    println!(
        "SELECT CASE WHEN changes() = 1 THEN 'eingespielt, 1 Zeile geaendert' WHEN NOT EXISTS (SELECT 1 FROM transcripts WHERE id = '{}') THEN 'nichts getan: Transkript-ID gibt es nicht' WHEN EXISTS (SELECT 1 FROM transcripts WHERE id = '{}' AND deleted_at IS NOT NULL) THEN 'nichts getan: Transkript ist geloescht' WHEN EXISTS (SELECT 1 FROM transcripts WHERE id = '{}' AND speaker_hints_json <> '[]') THEN 'nichts getan: da stand schon eine Zuordnung' WHEN EXISTS (SELECT 1 FROM transcripts WHERE id = '{}' AND content_revision <> {}) THEN 'nichts getan: die App hat seit dem Export geschrieben (Revision passt nicht) -- neu exportieren und neu rechnen' WHEN EXISTS (SELECT 1 FROM transcripts WHERE id = '{}' AND json_array_length(words_json) <> {}) THEN 'nichts getan: die Woerter haben sich geaendert (neu transkribiert?)' ELSE 'nichts getan: unklar -- von Hand nachsehen' END AS ergebnis;",
        transcript_id.replace('\'', "''"),
        transcript_id.replace('\'', "''"),
        transcript_id.replace('\'', "''"),
        transcript_id.replace('\'', "''"),
        revision,
        transcript_id.replace('\'', "''"),
        words.len(),
    );
}
