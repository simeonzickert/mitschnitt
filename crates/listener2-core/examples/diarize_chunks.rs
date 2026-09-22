//! Messbank fuer die ZERLEGTE Sprechertrennung (10.09.2026).
//!
//! Beantwortet die eine Frage, die der Zerlegung zugrunde liegt und sich
//! nicht aus dem Code ablesen laesst: **erkennt man denselben Menschen in
//! zwei getrennt gerechneten Abschnitten an seinem Stimm-Schwerpunkt wieder,
//! und mit welchem Abstand zu fremden Stimmen?**
//!
//! Ohne diese Zahl ist die Schwelle in `diarization_stitch` geraten. Mit ihr
//! ist sie belegt -- oder widerlegt.
//!
//! Der Kosinus wird hier bewusst NOCH EINMAL gerechnet statt aus dem Crate
//! geholt: diese Datei ist die unabhaengige Messung, und eine Messung, die
//! die zu pruefende Funktion selbst benutzt, prueft nur sich.
//!
//! ```text
//! /usr/bin/time -l cargo run --release --example diarize_chunks -- \
//!     --audio /pfad/audio_mic.wav --chunk-minutes 45
//! ```

use std::time::Instant;

fn cosine(left: &[f32], right: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut ln = 0.0f64;
    let mut rn = 0.0f64;
    for (a, b) in left.iter().zip(right.iter()) {
        dot += f64::from(*a) * f64::from(*b);
        ln += f64::from(*a) * f64::from(*a);
        rn += f64::from(*b) * f64::from(*b);
    }
    if ln <= 0.0 || rn <= 0.0 {
        return f64::NAN;
    }
    dot / (ln.sqrt() * rn.sqrt())
}

fn main() {
    let mut audio: Option<String> = None;
    let mut chunk_minutes: f64 = 45.0;
    let mut minutes: f64 = 0.0;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--audio" => audio = args.next(),
            "--chunk-minutes" => {
                chunk_minutes = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--chunk-minutes braucht eine Zahl")
            }
            "--minutes" => {
                minutes = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .expect("--minutes braucht eine Zahl")
            }
            other => panic!("unbekannter Schalter: {other}"),
        }
    }

    let audio = audio.expect("--audio fehlt");
    let mut reader = hound::WavReader::open(&audio).expect("WAV nicht lesbar");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1, "erwartet EINEN Kanal");
    let sample_rate = spec.sample_rate as usize;
    let mut samples = reader
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .expect("Samples nicht lesbar");
    if minutes > 0.0 {
        samples.truncate(((minutes * 60.0 * sample_rate as f64) as usize).min(samples.len()));
    }

    let chunk_samples = (chunk_minutes * 60.0 * sample_rate as f64) as usize;
    let mut bounds = Vec::new();
    let mut start = 0usize;
    while start < samples.len() {
        let end = (start + chunk_samples).min(samples.len());
        bounds.push((start, end));
        start = end;
    }

    println!("datei          {audio}");
    println!(
        "laenge         {:.1} min",
        samples.len() as f64 / sample_rate as f64 / 60.0
    );
    println!("abschnitte     {} a {chunk_minutes} min", bounds.len());
    println!();

    let mut all_embeddings: Vec<(usize, usize, Vec<f32>)> = Vec::new();
    let mut total_elapsed = 0.0f64;
    let mut peak_chunk_elapsed = 0.0f64;

    for (chunk_index, (start, end)) in bounds.iter().copied().enumerate() {
        let started_at = Instant::now();
        // Bewusst der Weg OHNE Laengenschutz, wie in `diarize_probe`: diese
        // Messbank soll auch Abschnittslaengen oberhalb der sicheren Grenze
        // durchrechnen koennen -- das ist ihr Zweck. Ueber
        // `diarize_samples_with_embeddings` wuerde `--chunk-minutes 90` am
        // Waechter scheitern, statt die Frage zu beantworten, die gestellt
        // wurde.
        let result = anlg_transcribe_soniqo::diarize_samples_unguarded_for_measurement(
            anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch,
            &samples[start..end],
            None,
        );
        let elapsed = started_at.elapsed().as_secs_f64();
        total_elapsed += elapsed;
        peak_chunk_elapsed = peak_chunk_elapsed.max(elapsed);

        match result {
            Ok(diarization) => {
                let seconds = (end - start) as f64 / sample_rate as f64;
                let mut spoken = std::collections::BTreeMap::<usize, f64>::new();
                for segment in &diarization.segments {
                    *spoken.entry(segment.speaker_index).or_default() +=
                        segment.end_seconds - segment.start_seconds;
                }
                println!(
                    "Abschnitt {chunk_index}: {:.1} min, {:.1} s Rechenzeit ({:.1}x), \
                     {} Sprecher, {} Abschnitte, {} Schwerpunkte",
                    seconds / 60.0,
                    elapsed,
                    seconds / elapsed.max(f64::EPSILON),
                    spoken.len(),
                    diarization.segments.len(),
                    diarization.speaker_embeddings.len(),
                );
                for (speaker, secs) in &spoken {
                    println!("   sprecher {speaker}: {secs:8.1} s");
                }
                for (speaker_index, embedding) in
                    diarization.speaker_embeddings.iter().enumerate()
                {
                    all_embeddings.push((chunk_index, speaker_index, embedding.clone()));
                }
            }
            Err(error) => {
                println!("Abschnitt {chunk_index}: FEHLER nach {elapsed:.1} s -- {error}");
            }
        }
        println!();
    }

    println!("rechenzeit gesamt   {total_elapsed:.1} s");
    println!("laengster abschnitt {peak_chunk_elapsed:.1} s");
    println!();

    if all_embeddings.is_empty() {
        println!("KEINE Schwerpunkte -- die Bruecke liefert sie nicht.");
        std::process::exit(1);
    }
    println!(
        "Schwerpunkt-Laenge  {}",
        all_embeddings.first().map(|e| e.2.len()).unwrap_or(0)
    );
    println!();

    // Die eigentliche Aussage: wie sehen die Aehnlichkeiten ZWISCHEN
    // Abschnitten aus? Innerhalb eines Abschnitts sind es verschiedene
    // Menschen (das Verfahren hat sie ja gerade getrennt) -- diese Werte sind
    // die Gegenprobe und muessen NIEDRIGER liegen als die besten
    // abschnittsuebergreifenden Paare, sonst traegt das Verfahren nicht.
    println!("== Aehnlichkeiten ==");
    println!("(A/s = Abschnitt/Sprecher)");
    let mut across: Vec<f64> = Vec::new();
    let mut within: Vec<f64> = Vec::new();
    for (i, (chunk_a, speaker_a, emb_a)) in all_embeddings.iter().enumerate() {
        for (chunk_b, speaker_b, emb_b) in all_embeddings.iter().skip(i + 1) {
            let similarity = cosine(emb_a, emb_b);
            if chunk_a == chunk_b {
                within.push(similarity);
            } else {
                across.push(similarity);
                println!(
                    "  {chunk_a}/{speaker_a} <-> {chunk_b}/{speaker_b}   {similarity:+.4}"
                );
            }
        }
    }

    println!();
    let summarize = |label: &str, values: &mut Vec<f64>| {
        if values.is_empty() {
            println!("{label}: keine Paare");
            return;
        }
        values.sort_by(|a, b| b.total_cmp(a));
        println!(
            "{label}: hoechste {:+.4}   niedrigste {:+.4}   n={}",
            values.first().unwrap(),
            values.last().unwrap(),
            values.len()
        );
    };
    summarize("zwischen Abschnitten ", &mut across);
    summarize("innerhalb Abschnitt  ", &mut within);
}
