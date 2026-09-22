//! Messbank fuer die Sprechertrennung (07.09.2026).
//!
//! Trennt EINEN Kanal einer Aufnahme und schreibt heraus, was dabei
//! herauskam: Sprecherzahl, Abschnitte, Redeanteile, Rechenzeit. Fasst weder
//! die Datenbank noch `~/Library/Application Support/` an.
//!
//! Zweck ist die Beantwortung von zwei Fragen, die sich nicht aus dem Code
//! ablesen lassen:
//!
//! 1. Traegt die selbst bestimmte Sprecherzahl (`--speakers auto`) dasselbe
//!    Ergebnis wie eine vorgegebene?
//! 2. Wie waechst der Speicherbedarf mit der Laenge? Der Deckel in
//!    `SONIQO_DIARIZATION_MAX_SAMPLES` ist genau dafuer da, und ohne Messung
//!    ist er eine geratene Zahl.
//!
//! Fuer die Speicherfrage von aussen messen, der Prozess misst sich nicht
//! selbst:
//!
//! ```text
//! /usr/bin/time -l cargo run --release --example diarize_probe -- \
//!     --audio /pfad/audio_mic.wav --minutes 30 --speakers auto
//! ```
//!
//! Erwartet eine einkanalige WAV-Datei mit 32-bit-Gleitkomma bei 16 kHz --
//! genau das, was die App je Kanal ablegt.

use std::time::Instant;

fn main() {
    let mut audio: Option<String> = None;
    let mut minutes: f64 = 0.0;
    let mut speakers: Option<usize> = None;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--audio" => audio = args.next(),
            "--minutes" => {
                minutes = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .expect("--minutes braucht eine Zahl")
            }
            "--speakers" => {
                let value = args.next().expect("--speakers braucht auto oder eine Zahl");
                speakers = if value == "auto" {
                    None
                } else {
                    Some(
                        value
                            .parse()
                            .expect("--speakers braucht auto oder eine Zahl"),
                    )
                };
            }
            other => panic!("unbekannter Schalter: {other}"),
        }
    }

    let audio = audio.expect("--audio fehlt");
    let mut reader = hound::WavReader::open(&audio).expect("WAV nicht lesbar");
    let spec = reader.spec();
    assert_eq!(spec.channels, 1, "erwartet EINEN Kanal");
    assert_eq!(spec.sample_format, hound::SampleFormat::Float);

    let mut samples = reader
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .expect("Samples nicht lesbar");
    let sample_rate = spec.sample_rate as usize;
    if minutes > 0.0 {
        let wanted = (minutes * 60.0 * sample_rate as f64) as usize;
        samples.truncate(wanted.min(samples.len()));
    }
    let seconds = samples.len() as f64 / sample_rate as f64;

    let square_sum: f64 = samples
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum();
    let rms = (square_sum / samples.len().max(1) as f64).sqrt();

    println!("datei          {audio}");
    println!("laenge         {seconds:.1} s ({:.1} min)", seconds / 60.0);
    println!("samples        {}", samples.len());
    println!("effektivwert   {rms:.9}  ({:.1} dB)", 20.0 * rms.log10());
    println!(
        "sprecherzahl   {}",
        speakers.map_or_else(|| "auto".to_string(), |count| count.to_string())
    );

    let started_at = Instant::now();
    // Bewusst der Weg OHNE Laengenschutz: diese Messbank misst genau das
    // Verhalten oberhalb der sicheren Grenze -- wo der Prozess stirbt und bei
    // welchem Spitzenspeicher. Ein Schutz waere hier das, was die Messung
    // verhindert. Ueberall sonst gilt `diarize_samples_chunked`.
    let segments = anlg_transcribe_soniqo::diarize_samples_unguarded_for_measurement(
        anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch,
        &samples,
        speakers,
    )
    .map(|diarization| diarization.segments);
    let elapsed = started_at.elapsed();

    match segments {
        Ok(segments) => {
            let mut seconds_by_speaker = std::collections::BTreeMap::<usize, f64>::new();
            for segment in &segments {
                *seconds_by_speaker.entry(segment.speaker_index).or_default() +=
                    segment.end_seconds - segment.start_seconds;
            }
            let total: f64 = seconds_by_speaker.values().sum();

            println!("---");
            println!("rechenzeit     {:.1} s", elapsed.as_secs_f64());
            println!(
                "echtzeitfaktor {:.1}x",
                seconds / elapsed.as_secs_f64().max(f64::EPSILON)
            );
            println!("abschnitte     {}", segments.len());
            println!("sprecher       {}", seconds_by_speaker.len());
            for (speaker_index, spoken) in &seconds_by_speaker {
                println!(
                    "  sprecher {speaker_index}   {spoken:8.1} s   {:5.1} %",
                    100.0 * spoken / total.max(f64::EPSILON)
                );
            }
            println!(
                "sprechanteil   {:.1} % der Aufnahme",
                100.0 * total / seconds
            );
        }
        Err(error) => {
            println!("---");
            println!("rechenzeit     {:.1} s", elapsed.as_secs_f64());
            println!("FEHLER         {error}");
            std::process::exit(1);
        }
    }
}
