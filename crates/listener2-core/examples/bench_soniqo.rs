//! Messbank fuer die lokale Transkription (Testplan 02.09.2026, Stufe 0).
//!
//! Transkribiert EINE Audiodatei mit EINER Konfiguration und legt drei Dateien
//! ab: das Transkript (textlich vergleichbar), die Kennzahlen als JSON und die
//! Wortliste mit Zeitmarken (Grundlage fuer Zeitversatz-Vergleiche).
//! Schreibt nichts in die App-Datenbank und nichts unter
//! `~/Library/Application Support/`.
//!
//! Aufruf:
//!
//! ```text
//! cargo run --release --example bench_soniqo -- \
//!     --audio /pfad/zu/tom.mp3 \
//!     --out-dir /pfad/zum/ausgabeordner \
//!     --label heute-standard
//! ```
//!
//! Achsen (Abschnitt 3 des Testplans), alle als Flags:
//!
//! | Achse | Flag | Vorgabe |
//! |---|---|---|
//! | A Modell | `--model soniqo-parakeet-batch` | `soniqo-parakeet-batch` |
//! | B Zerlegung | `--chunking speech\|fixed` | `speech` |
//! | B Zielgroesse | `--bundle-seconds 29.5` (`0` = nicht buendeln) | Modellfenster (29,5 s) |
//! | B Zeitspanne | `--bundle-span-seconds 29.5` | 29,5 s |
//! | C Ueberlappung | -- | nicht steuerbar, siehe unten |
//! | D Kanaele | `--channels sequential\|parallel` | `parallel` (Produktionsstand) |
//! | E Stille-Gate | `--silence-gate on\|off` bzw. `--silence-rms 0.0008` | `on`, 0,0008 |
//! | H Wortzeiten | `--word-timings on\|off` | `on` (Produktionsstand seit 02.09.2026) |
//! | F Sprache | `--language de` / `--language auto` | `de` |
//! | G Stichwortliste | `--vocabulary <datei>` (mehrfach), `--vocabulary-stages` | leer = aus |
//!
//!
//! **Achse G (Stichwortliste)** schaltet den Nachlauf aus
//! [`anlg_vocabulary`] zu. Ohne `--vocabulary` laeuft er gar nicht -- das ist
//! der Vorher-Lauf. `--vocabulary-stages alias|alias+klang|alle` (Vorgabe
//! `alias`, wie im Betrieb) schaltet die Klangstufen zu, damit sich messen
//! laesst, welche Stufe traegt und welche Schaden anrichtet. Jede einzelne Ersetzung
//! steht mit Vorher/Nachher/Stufe in der Kennzahlendatei, sonst waere die
//! Fehlgriff-Zahl eine Behauptung statt einer Zahl. Darueber steht
//! `nachlauf_gelaufen`: ohne dieses Feld waeren "null Ersetzungen" und "der
//! Nachlauf lief gar nicht" dieselbe Ausgabe.
//!
//! **Achse C (Ueberlappung) ist NICHT steuerbar** und wird bewusst nicht
//! eingebaut: im Fork existiert keine Ueberlappung an den Schnitten. Der
//! Buendler legt zwischen zwei Sprech-Abschnitten eine Viertelsekunde STILLE
//! (`SONIQO_BUNDLE_SEAM_SAMPLES`), damit das Modell nicht verklebt -- das ist
//! das Gegenteil einer Ueberlappung. Eine echte Ueberlappung braeuchte
//! zusaetzlich eine Zusammenfuehrung (LCS ueber die doppelten Woerter), also
//! einen Umbau am Produktivcode. Das ist ein Bau, kein Messschalter.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use listener2_core::{
    BatchEvent, BatchParams, BatchProvider, BatchRuntime, SoniqoChunkingMode, SoniqoTuning,
    run_batch, set_soniqo_tuning,
};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

const TARGET_SAMPLE_RATE: f64 = 16_000.0;

// --- Konfiguration ----------------------------------------------------------

#[derive(Debug, Clone)]
struct Config {
    audio: PathBuf,
    out_dir: PathBuf,
    label: String,
    provider: String,
    model: String,
    base_url: String,
    chunking: String,
    fixed_seconds: f64,
    bundle_seconds: Option<f64>,
    bundle_span_seconds: Option<f64>,
    channels_parallel: bool,
    /// Achse H: gemessene Wortzeiten aus dem Erkenner statt Gleichverteilung.
    word_timings: bool,
    silence_rms: Option<f64>,
    language: Option<String>,
    /// Dateien mit der eingetragenen Stichwortliste. Leer = der Nachlauf laeuft nicht.
    vocabulary_files: Vec<PathBuf>,
    vocabulary_stages: String,
    /// Teilnehmerzahl wie aus der Teilnehmerliste der App. `None` = die
    /// Sprecherzahl wird selbst bestimmt. Ohne diesen Schalter liesse sich der
    /// Fall des Prinzipals (Raumaufnahme mit vier Personen) hier nicht
    /// nachstellen.
    speakers: Option<u32>,
}

impl Config {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut audio = None;
        let mut out_dir = None;
        let mut label = None;
        let mut provider = "soniqo".to_string();
        let mut model = "soniqo-parakeet-batch".to_string();
        let mut base_url = String::new();
        let mut chunking = "speech".to_string();
        let mut fixed_seconds = 29.5;
        let mut bundle_seconds = None;
        let mut bundle_span_seconds = None;
        // Folgt dem Produktionsstand: seit dem 02.09.2026 laufen die Kanaele
        // gleichzeitig. `--channels sequential` misst weiter die Gegenprobe.
        let mut channels_parallel = SoniqoTuning::PRODUCTION.channels_in_parallel;
        let mut word_timings = SoniqoTuning::PRODUCTION.word_timings;
        let mut silence_rms = Some(0.0008);
        let mut language = Some("de".to_string());
        let mut vocabulary_files: Vec<PathBuf> = Vec::new();
        let mut vocabulary_stages = "alias".to_string();
        let mut speakers: Option<u32> = None;

        let mut index = 0usize;
        while index < args.len() {
            let flag = args[index].as_str();
            let mut value = || -> Result<String, String> {
                index += 1;
                args.get(index)
                    .cloned()
                    .ok_or_else(|| format!("{flag} braucht einen Wert"))
            };
            match flag {
                "--audio" => audio = Some(PathBuf::from(value()?)),
                "--speakers" => {
                    let raw = value()?;
                    speakers = if raw == "auto" {
                        None
                    } else {
                        Some(raw.parse().map_err(|_| "--speakers: auto oder eine Zahl")?)
                    };
                }
                "--out-dir" => out_dir = Some(PathBuf::from(value()?)),
                "--label" => label = Some(value()?),
                "--provider" => provider = value()?,
                "--model" => model = value()?,
                "--base-url" => base_url = value()?,
                "--chunking" => chunking = value()?,
                "--fixed-seconds" => {
                    fixed_seconds = value()?
                        .parse()
                        .map_err(|_| "--fixed-seconds: keine Zahl")?
                }
                "--bundle-seconds" => {
                    bundle_seconds = Some(
                        value()?
                            .parse()
                            .map_err(|_| "--bundle-seconds: keine Zahl")?,
                    )
                }
                "--bundle-span-seconds" => {
                    bundle_span_seconds = Some(
                        value()?
                            .parse()
                            .map_err(|_| "--bundle-span-seconds: keine Zahl")?,
                    )
                }
                "--word-timings" => {
                    let value = value()?;
                    match value.as_str() {
                        "on" => word_timings = true,
                        "off" => word_timings = false,
                        other => return Err(format!("--word-timings: on|off, nicht {other}")),
                    }
                }
                "--silence-gate" => {
                    let raw = value()?;
                    match raw.as_str() {
                        "on" => silence_rms = silence_rms.or(Some(0.0008)),
                        "off" => silence_rms = None,
                        other => return Err(format!("--silence-gate: on|off, nicht {other}")),
                    }
                }
                "--silence-rms" => {
                    silence_rms = Some(value()?.parse().map_err(|_| "--silence-rms: keine Zahl")?)
                }
                "--channels" => {
                    let raw = value()?;
                    match raw.as_str() {
                        "sequential" => channels_parallel = false,
                        "parallel" => channels_parallel = true,
                        other => {
                            return Err(format!("--channels: sequential|parallel, nicht {other}"));
                        }
                    }
                }
                "--language" => {
                    let raw = value()?;
                    language = (raw != "auto").then_some(raw);
                }
                "--vocabulary" => vocabulary_files.push(PathBuf::from(value()?)),
                "--vocabulary-stages" => vocabulary_stages = value()?,
                "--help" | "-h" => return Err(usage()),
                other => return Err(format!("unbekanntes Flag: {other}\n\n{}", usage())),
            }
            index += 1;
        }

        let audio = audio.ok_or_else(|| format!("--audio fehlt\n\n{}", usage()))?;
        let out_dir = out_dir.ok_or_else(|| format!("--out-dir fehlt\n\n{}", usage()))?;
        if !matches!(chunking.as_str(), "speech" | "fixed") {
            return Err(format!("--chunking: speech|fixed, nicht {chunking}"));
        }
        if !matches!(vocabulary_stages.as_str(), "alias" | "alias+klang" | "alle") {
            return Err(format!(
                "--vocabulary-stages: alias|alias+klang|alle, nicht {vocabulary_stages}"
            ));
        }

        Ok(Self {
            label: label.unwrap_or_else(|| default_label(&chunking, bundle_seconds)),
            audio,
            out_dir,
            provider,
            model,
            base_url,
            chunking,
            fixed_seconds,
            bundle_seconds,
            bundle_span_seconds,
            channels_parallel,
            word_timings,
            silence_rms,
            language,
            vocabulary_files,
            vocabulary_stages,
            speakers,
        })
    }

    /// Liest die Stichwortlisten und legt sie aneinander. Mehrere `--vocabulary`
    /// sind der Regelfall: der Betreiber haelt die richtigen Schreibweisen und die
    /// bekannten Verhoerungen in zwei Dateien.
    fn vocabulary_lines(&self) -> Result<Vec<String>, String> {
        let mut lines = Vec::new();
        for path in &self.vocabulary_files {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("--vocabulary {}: {error}", path.display()))?;
            lines.extend(text.lines().map(str::to_string));
        }
        Ok(lines)
    }

    /// Welche Stufen des Nachlaufs laufen duerfen.
    fn vocabulary_options(&self) -> anlg_vocabulary::Options {
        let default = anlg_vocabulary::Options::default();
        // Jeder Zweig setzt BEIDE Felder. Ein Zweig, der sich auf die Vorgabe
        // verlaesst, tut stillschweigend nichts mehr, sobald sich die Vorgabe
        // aendert.
        match self.vocabulary_stages.as_str() {
            "alias+klang" => anlg_vocabulary::Options {
                alias_phonetics: true,
                canonical_phonetics: false,
                ..default
            },
            "alle" => anlg_vocabulary::Options {
                alias_phonetics: true,
                canonical_phonetics: true,
                ..default
            },
            _ => anlg_vocabulary::Options {
                alias_phonetics: false,
                canonical_phonetics: false,
                ..default
            },
        }
    }

    fn tuning(&self) -> SoniqoTuning {
        let mut tuning = SoniqoTuning {
            channels_in_parallel: self.channels_parallel,
            word_timings: self.word_timings,
            direct_mic_min_rms: self.silence_rms,
            ..SoniqoTuning::PRODUCTION
        };
        if self.chunking == "fixed" {
            tuning = tuning.with_fixed_chunking(self.fixed_seconds);
        } else {
            tuning.chunking = SoniqoChunkingMode::SpeechAware;
        }
        if let Some(seconds) = self.bundle_seconds {
            tuning = tuning.with_bundle_seconds(seconds);
        }
        if let Some(seconds) = self.bundle_span_seconds {
            tuning = tuning.with_bundle_span_seconds(seconds);
        }
        tuning
    }

    /// Vollstaendige Konfiguration fuer die Kennzahlendatei -- damit ein Lauf
    /// aus seiner eigenen Ausgabe heraus wiederholbar ist.
    fn as_json(&self) -> serde_json::Value {
        let tuning = self.tuning();
        serde_json::json!({
            "label": self.label,
            "audio": self.audio.display().to_string(),
            "provider": self.provider,
            "model": self.model,
            "base_url": self.base_url,
            "chunking": self.chunking,
            "fixed_seconds": (self.chunking == "fixed").then_some(self.fixed_seconds),
            "bundle_seconds": match tuning.bundle_max_samples {
                Some(samples) => serde_json::json!(samples as f64 / TARGET_SAMPLE_RATE),
                None => serde_json::json!("modellfenster-29.5s"),
            },
            "bundle_span_seconds": match tuning.bundle_max_span_samples {
                Some(samples) => serde_json::json!(samples as f64 / TARGET_SAMPLE_RATE),
                None => serde_json::json!("unbegrenzt"),
            },
            "overlap_seconds": 0.0,
            "channels": if self.channels_parallel { "parallel" } else { "sequential" },
            "word_timings": self.word_timings,
            "silence_gate": self.silence_rms.is_some(),
            "silence_rms": self.silence_rms,
            "language": self.language.clone().unwrap_or_else(|| "auto".to_string()),
            "vocabulary_files": self
                .vocabulary_files
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
            "vocabulary_stages": match self.vocabulary_files.is_empty() {
                true => "aus".to_string(),
                false => self.vocabulary_stages.clone(),
            },
        })
    }
}

fn default_label(chunking: &str, bundle_seconds: Option<f64>) -> String {
    match bundle_seconds {
        Some(seconds) => format!("{chunking}-{seconds}s"),
        None => format!("{chunking}-modellfenster"),
    }
}

fn usage() -> String {
    "bench_soniqo -- Messbank fuer die lokale Transkription\n\
     \n\
     Pflicht:\n  \
       --audio <datei>          Audiodatei (Kopie, nie das Original)\n  \
       --out-dir <ordner>       Zielordner fuer Transkript + Kennzahlen\n\
     \n\
     Optional:\n  \
       --label <name>           Name des Laufs (Dateipraefix)\n  \
       --speakers auto|<zahl>   Teilnehmerzahl wie in der App (Vorgabe: auto)\n  \
       --provider <p>           soniqo (Vorgabe) | whispercpp | applespeech\n  \
       --model <m>              soniqo-parakeet-batch (Vorgabe)\n  \
       --base-url <url>         nur fuer whispercpp (laufender Server noetig)\n  \
       --chunking speech|fixed  Vorgabe speech\n  \
       --fixed-seconds <s>      Fensterlaenge bei --chunking fixed (29.5)\n  \
       --bundle-seconds <s>     Zielgroesse je Modellaufruf; 0 = nicht buendeln\n  \
       --bundle-span-seconds <s>  Erlaubte Zeitspanne je Buendel (29.5)\n  \
       --channels sequential|parallel   Vorgabe parallel\n  \
       --word-timings on|off    Vorgabe off\n  \
       --silence-gate on|off    Vorgabe on\n  \
       --silence-rms <wert>     Schwelle des Gates (0.0008)\n  \
       --language de|auto       Vorgabe de\n  \
       --vocabulary <datei>     Stichwortliste; mehrfach erlaubt, ohne = Nachlauf aus\n  \
       --vocabulary-stages <s>  alias | alias+klang | alle (Vorgabe alias)"
        .to_string()
}

// --- Kennzahlen -------------------------------------------------------------

#[derive(Default)]
struct CallStats {
    /// Tonlaenge je Modellaufruf in ms, aus `soniqo_chunk_native_inference_start`.
    audio_ms: Vec<u64>,
    /// Rechenzeit je Aufruf in ms, aus `..._completed`.
    compute_ms: Vec<u64>,
    /// Aufrufe, die leeren Text zurueckgaben (`transcript.text_chars == 0`).
    empty_calls: usize,
    completed_calls: usize,
    failed_calls: usize,
    /// Abschnitte, die das Stille-Gate vor dem Modell aussortiert hat.
    gate_skipped: usize,
    /// Sprech-Abschnitte je Aufruf (`chunk.member_count`).
    members_per_call: Vec<u64>,
    /// Jede einzelne Ersetzung des Stichwort-Nachlaufs, in der Reihenfolge, in
    /// der sie passiert ist. Ohne diese Liste laesst sich die Fehlgriff-Zahl
    /// nicht bilden -- man muesste sie glauben.
    replacements: Vec<(String, String, String)>,
    /// Hat der Nachlauf ueberhaupt stattgefunden? Ohne diese Zahl ist "0
    /// Ersetzungen" nicht von "nie gelaufen" zu unterscheiden -- und genau das
    /// war am 02.09.2026 der Fall: `tom.mp3` ergab 0, und nichts im Protokoll
    /// belegte, dass ueberhaupt jemand nachgesehen hatte.
    vocabulary_passes: usize,
}

#[derive(Default)]
struct EventFields {
    strings: BTreeMap<&'static str, String>,
    numbers: BTreeMap<&'static str, f64>,
    message: String,
}

impl Visit for EventFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}").trim_matches('"').to_string();
        } else {
            self.strings.insert(field.name(), format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.strings.insert(field.name(), value.to_string());
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.numbers.insert(field.name(), value as f64);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.numbers.insert(field.name(), value as f64);
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.numbers.insert(field.name(), value);
    }
}

/// Zaehlt mit, was die Produktion ohnehin protokolliert. Bewusst KEIN Eingriff
/// in den Messgegenstand: die Ereignisse und ihre Felder stehen seit dem
/// 02.09.2026 so im Code, die Bank liest sie nur mit.
struct StatsLayer(Arc<Mutex<CallStats>>);

impl<S: Subscriber> Layer<S> for StatsLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = EventFields::default();
        event.record(&mut fields);
        let mut stats = self.0.lock().unwrap_or_else(|error| error.into_inner());
        match fields.message.as_str() {
            "soniqo_chunk_native_inference_start" => {
                if let Some(duration) = fields.numbers.get("chunk.duration_ms") {
                    stats.audio_ms.push(*duration as u64);
                }
                if let Some(members) = fields.numbers.get("chunk.member_count") {
                    stats.members_per_call.push(*members as u64);
                }
            }
            "soniqo_chunk_native_inference_completed" => {
                stats.completed_calls += 1;
                if let Some(elapsed) = fields.numbers.get("elapsed_ms") {
                    stats.compute_ms.push(*elapsed as u64);
                }
                if fields.numbers.get("transcript.text_chars").copied() == Some(0.0) {
                    stats.empty_calls += 1;
                }
            }
            "soniqo_chunk_native_inference_failed" => {
                stats.failed_calls += 1;
                if let Some(elapsed) = fields.numbers.get("elapsed_ms") {
                    stats.compute_ms.push(*elapsed as u64);
                }
            }
            "soniqo_direct_mic_chunk_skipped" => stats.gate_skipped += 1,
            "vocabulary_pass_completed" => stats.vocabulary_passes += 1,
            "vocabulary_replacement" => {
                let field = |name| {
                    fields
                        .strings
                        .get(name)
                        .map(|value| value.trim_matches('"').to_string())
                        .unwrap_or_default()
                };
                stats.replacements.push((
                    field("anarlog.vocabulary.before"),
                    field("anarlog.vocabulary.after"),
                    field("anarlog.vocabulary.kind"),
                ));
            }
            _ => {}
        }
    }
}

/// Wie viele Ersetzungen je Stufe -- die Aufteilung sagt, ob der Gewinn aus der
/// sicheren oder aus der riskanten Stufe kommt.
fn replacements_per_kind(replacements: &[(String, String, String)]) -> serde_json::Value {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, _, kind) in replacements {
        *counts.entry(kind.as_str()).or_default() += 1;
    }
    serde_json::json!(counts)
}

fn summary(values: &[u64]) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::json!({ "count": 0 });
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2
    } else {
        sorted[sorted.len() / 2]
    };
    serde_json::json!({
        "count": sorted.len(),
        "min": sorted[0],
        "median": median,
        "max": sorted[sorted.len() - 1],
        "sum": sorted.iter().sum::<u64>(),
    })
}

// --- Transkript -------------------------------------------------------------

struct Line {
    start: f64,
    end: f64,
    channel: i32,
    text: String,
}

/// Baut aus den Woertern kanalweise Zeilen und mischt sie chronologisch --
/// dieselbe Ansicht, die ein Mensch im Abspieler sieht. Eine neue Zeile beginnt,
/// wenn im selben Kanal mehr als `GAP_SECONDS` Pause liegt.
fn transcript_lines(response: &owhisper_interface::batch::Response) -> Vec<Line> {
    const GAP_SECONDS: f64 = 1.0;

    let mut lines: Vec<Line> = Vec::new();
    for (channel_index, channel) in response.results.channels.iter().enumerate() {
        let Some(alternative) = channel.alternatives.first() else {
            continue;
        };
        let channel_label = i32::try_from(channel_index).unwrap_or_default();
        let mut current: Option<Line> = None;
        for word in &alternative.words {
            let text = word.punctuated_word.as_deref().unwrap_or(&word.word);
            match current.as_mut() {
                Some(line) if word.start - line.end <= GAP_SECONDS => {
                    line.text.push(' ');
                    line.text.push_str(text);
                    line.end = word.end;
                }
                _ => {
                    if let Some(line) = current.take() {
                        lines.push(line);
                    }
                    current = Some(Line {
                        start: word.start,
                        end: word.end,
                        channel: channel_label,
                        text: text.to_string(),
                    });
                }
            }
        }
        if let Some(line) = current.take() {
            lines.push(line);
        }
    }

    lines.sort_by(|left, right| {
        left.start
            .total_cmp(&right.start)
            .then(left.channel.cmp(&right.channel))
    });
    lines
}

fn timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        total / 3600,
        (total % 3600) / 60,
        total % 60,
        ((seconds.max(0.0) - total as f64) * 1000.0) as u64
    )
}

fn render_transcript(config: &Config, lines: &[Line]) -> String {
    let mut out = format!(
        "# Messlauf {}\n\n_Datei: {}_\n_Konfiguration: {}_\n\n",
        config.label,
        config.audio.display(),
        serde_json::to_string(&config.as_json()).unwrap_or_default()
    );
    for line in lines {
        out.push_str(&format!(
            "[{}] [Kanal {}] {}\n",
            timestamp(line.start),
            line.channel,
            line.text.trim()
        ));
    }
    out
}

/// Wort fuer Wort mit seiner Zeitmarke -- die Grundlage jeder Aussage darueber,
/// wie weit eine Textstelle zwischen zwei Laeufen wandert. Aus den Zeilen laesst
/// sich das NICHT zurueckrechnen: eine Zeile kann eine Minute Rede tragen, und
/// jedes Wort ihren Anfang zu geben, erzeugt einen Messfehler in genau der
/// Groessenordnung, die hier untersucht wird.
fn word_table(response: &owhisper_interface::batch::Response) -> serde_json::Value {
    let mut rows = Vec::new();
    for (channel_index, channel) in response.results.channels.iter().enumerate() {
        let Some(alternative) = channel.alternatives.first() else {
            continue;
        };
        for word in &alternative.words {
            rows.push(serde_json::json!({
                "kanal": channel_index,
                "start": word.start,
                "ende": word.end,
                "wort": word.punctuated_word.as_deref().unwrap_or(&word.word),
                // Fork (03.09.2026): die Sicherheit des Erkenners -- und
                // daneben, ob sie GEMESSEN ist.
                //
                // Der Kommentar an dieser Stelle behauptete, `sicherheit`
                // allein entscheide, ob die Zahl echt oder eine harte 1,0 ist.
                // Das kann sie nicht: `confidence` ist auf dem Draht ein
                // nackter `f64`, und jeder fehlende Wert steht dort als 1,0.
                // Genau die beiden Faelle, die man auseinanderhalten will,
                // sehen in diesem Feld gleich aus.
                "sicherheit": word.confidence,
                "gemessen": word.measured_confidence.is_some(),
            }));
        }
    }
    serde_json::Value::Array(rows)
}

fn word_and_char_counts(response: &owhisper_interface::batch::Response) -> (usize, usize) {
    response
        .results
        .channels
        .iter()
        .filter_map(|channel| channel.alternatives.first())
        .fold((0, 0), |(words, chars), alternative| {
            (
                words + alternative.transcript.split_whitespace().count(),
                chars + alternative.transcript.chars().count(),
            )
        })
}

// --- Lauf -------------------------------------------------------------------

/// Schreibt jede Fortschritts-Meldung mit Uhrzeit nach stderr.
///
/// Vorher hiess dieser Typ `Silent` und warf alles weg. Damit war die Frage
/// "bewegt sich der Balken waehrend der Sprechertrennung?" an der Messbank
/// nicht zu beantworten -- und genau die ist am 07.09.2026 teuer geworden.
struct ProgressLog {
    started_at: Instant,
    values: Mutex<Vec<(f64, f64)>>,
}

impl ProgressLog {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            values: Mutex::new(Vec::new()),
        }
    }

    /// (Sekunde seit Start, Anteil) je Meldung, in der Reihenfolge des Eingangs.
    fn values(&self) -> Vec<(f64, f64)> {
        self.values
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl BatchRuntime for ProgressLog {
    fn emit(&self, event: BatchEvent) {
        if let BatchEvent::BatchResponseStreamed {
            event: owhisper_interface::batch_stream::BatchStreamEvent::Progress { percentage, .. },
            ..
        } = event
        {
            let at = self.started_at.elapsed().as_secs_f64();
            self.values
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push((at, percentage));
            eprintln!("FORTSCHRITT {at:8.1}s  {:5.1} %", percentage * 100.0);
        }
    }

    fn is_cancelled(&self) -> bool {
        false
    }
}

#[tokio::main]
async fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let config = match Config::parse(&args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let stats = Arc::new(Mutex::new(CallStats::default()));
    tracing_subscriber::registry()
        .with(StatsLayer(stats.clone()))
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .with_filter(tracing_subscriber::EnvFilter::new(
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
                )),
        )
        .init();

    set_soniqo_tuning(config.tuning());

    let provider = match config.provider.parse::<BatchProvider>() {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!("--provider {}: {error}", config.provider);
            std::process::exit(2);
        }
    };
    let languages = match config.language.as_deref() {
        Some(code) => match code.parse::<anlg_language::Language>() {
            Ok(language) => vec![language],
            Err(error) => {
                eprintln!("--language {code}: {error}");
                std::process::exit(2);
            }
        },
        None => vec![],
    };

    if let Err(error) = std::fs::create_dir_all(&config.out_dir) {
        eprintln!("--out-dir {}: {error}", config.out_dir.display());
        std::process::exit(2);
    }

    let vocabulary = match config.vocabulary_lines() {
        Ok(lines) => lines,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    // Der Nachlauf laeuft in `run_batch` unter seinen eigenen Vorgaben. Damit
    // `--vocabulary-stages` hier ueberhaupt etwas bedeutet, muss die Bank sie
    // setzen -- sonst misst sie stumm immer dieselbe Konfiguration.
    listener2_core::set_vocabulary_options(config.vocabulary_options());

    let progress = Arc::new(ProgressLog::new());
    let started = Instant::now();
    let output = run_batch(
        progress.clone(),
        BatchParams {
            session_id: format!("bench-{}", config.label),
            provider,
            file_path: config.audio.display().to_string(),
            model: Some(config.model.clone()),
            base_url: config.base_url.clone(),
            api_key: String::new(),
            languages,
            keywords: vec![],
            vocabulary,
            num_speakers: config.speakers,
            min_speakers: None,
            max_speakers: None,
        },
    )
    .await;
    let wall_ms = started.elapsed().as_millis() as u64;

    // Die laengste Stille zwischen zwei Meldungen ist die Zahl, die zaehlt:
    // sie sagt, wie lange der Balken im schlimmsten Fall stillsteht.
    let progress_values = progress.values();
    let longest_silence = progress_values
        .windows(2)
        .map(|pair| pair[1].0 - pair[0].0)
        .fold(0.0_f64, f64::max);
    eprintln!(
        "FORTSCHRITT gesamt: {} Meldungen, laengste Stille {:.1} s",
        progress_values.len(),
        longest_silence
    );

    let response = match output {
        Ok(output) => output.response,
        Err(error) => {
            eprintln!("FEHLGESCHLAGEN nach {wall_ms} ms: {error}");
            std::process::exit(1);
        }
    };

    let lines = transcript_lines(&response);
    let (word_count, char_count) = word_and_char_counts(&response);
    let stats = stats.lock().unwrap_or_else(|error| error.into_inner());

    let transcript_path = config
        .out_dir
        .join(format!("{}.transkript.md", config.label));
    let metrics_path = config
        .out_dir
        .join(format!("{}.kennzahlen.json", config.label));
    let words_path = config
        .out_dir
        .join(format!("{}.woerter.json", config.label));

    let metrics = serde_json::json!({
        "label": config.label,
        "konfiguration": config.as_json(),
        "zeit": {
            "wanduhr_ms": wall_ms,
            "modellzeit_ms": stats.compute_ms.iter().sum::<u64>(),
        },
        "aufrufe": {
            "gesamt": stats.completed_calls + stats.failed_calls,
            "erfolgreich": stats.completed_calls,
            "fehlgeschlagen": stats.failed_calls,
            "ohne_text": stats.empty_calls,
            "ohne_text_anteil": if stats.completed_calls == 0 {
                0.0
            } else {
                stats.empty_calls as f64 / stats.completed_calls as f64
            },
            "vom_stille_gate_uebersprungen": stats.gate_skipped,
        },
        "audio_ms_je_aufruf": summary(&stats.audio_ms),
        "rechenzeit_ms_je_aufruf": summary(&stats.compute_ms),
        "abschnitte_je_aufruf": summary(&stats.members_per_call),
        "transkript": {
            "woerter": word_count,
            "zeichen": char_count,
            "zeilen": lines.len(),
            "kanaele": response.results.channels.len(),
        },
        "stichwortliste": {
            // Zuerst: hat der Nachlauf stattgefunden? Erst danach ist die
            // Ersetzungszahl darunter ueberhaupt lesbar. Ohne Liste ist die
            // Antwort erwartungsgemaess "nein" -- das ist der Vorher-Lauf.
            "nachlauf_gelaufen": stats.vocabulary_passes > 0,
            "ersetzungen": stats.replacements.len(),
            "je_stufe": replacements_per_kind(&stats.replacements),
            // Vollstaendig, nicht als Auszug: die Fehlgriff-Zahl entsteht durch
            // Lesen dieser Liste, also muss sie ganz dastehen.
            "liste": stats
                .replacements
                .iter()
                .map(|(before, after, kind)| {
                    serde_json::json!({ "vorher": before, "nachher": after, "stufe": kind })
                })
                .collect::<Vec<_>>(),
        },
    });

    if let Err(error) = std::fs::write(&transcript_path, render_transcript(&config, &lines)) {
        eprintln!("Transkript nicht geschrieben: {error}");
        std::process::exit(1);
    }
    if let Err(error) = std::fs::write(
        &words_path,
        serde_json::to_string(&word_table(&response)).unwrap_or_default(),
    ) {
        eprintln!("Wortliste nicht geschrieben: {error}");
        std::process::exit(1);
    }
    let metrics_text = serde_json::to_string_pretty(&metrics).unwrap_or_default();
    if let Err(error) = std::fs::write(&metrics_path, format!("{metrics_text}\n")) {
        eprintln!("Kennzahlen nicht geschrieben: {error}");
        std::process::exit(1);
    }

    println!("{metrics_text}");
    println!("\nTranskript : {}", short(&transcript_path));
    println!("Kennzahlen : {}", short(&metrics_path));
    println!("Wortliste  : {}", short(&words_path));
}

fn short(path: &Path) -> String {
    path.display().to_string()
}
