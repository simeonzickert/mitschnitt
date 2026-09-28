//! Cloud-Transkription je Kanal (Fork, ZICK-330, 26.09.2026).
//!
//! Warum es diesen Weg gibt: bis hierher ging die Stereo-Aufnahme (Kanal 0 =
//! Mikrofon, Kanal 1 = Systemton) als EINE Datei an den Cloud-Anbieter. Fast
//! alle Adapter liefern genau einen Kanal zurueck, und die Anzeige macht aus
//! Kanal 0 den Nutzer selbst. Gemessen am Bestand (26.09.2026): alle sechs
//! Cloud-Transkripte (`gpt-transcribe` ueber OpenAI und OpenRouter) tragen
//! ausschliesslich Kanal 0 und keine Wortzeiten. Ein 1:1-Gespraech las sich
//! damit als Monolog des Nutzers.
//!
//! Warum nicht einfach die ganze Kanaldatei einzeln hochladen: ohne Wortzeiten
//! beginnen dann beide Kanaele bei 0 s mit erfundenen Zeiten, und die Anzeige
//! sortiert kanaluebergreifend nach Startzeit -- Wort fuer Wort abwechselnd.
//!
//! Was dieser Weg stattdessen tut, genau wie der lokale (`local.rs`): jeden
//! Kanal einzeln zerlegen, stille Kanaele ueberspringen, die Sprech-Abschnitte
//! zu Paketen buendeln, EIN Aufruf je Paket, und den Text danach auf die
//! Abschnitte zurueckverteilen. Die Zeitanker sind die echten
//! Abschnittsstarts; Wortzeiten des Anbieters werden genutzt, wo sie zum Text
//! passen.
//!
//! Paketgroesse (Nachtrag 28.09.2026, `bundle_max_samples`): liefert der
//! Adapter fuer das gewaehlte Modell ECHTE Wortzeiten (belegt, siehe
//! `BatchSttAdapter::wants_word_timestamps`), buendelt dieser Weg bis zu
//! ~5 Minuten statt der alten ~30 s -- ohne diese Zusage bleibt es bei den
//! ~30 s, weil dann nur die geschaetzte Sprechdauer auf die Abschnitte
//! zurueckverteilt, und ein zu grosses Paket den Fehler auf viele Abschnitte
//! streut statt auf einen.
//!
//! Wann er NICHT greift (Rueckgabe `None`, der Aufrufer nimmt den alten Weg):
//! Mono-Aufnahme, zwei gleiche Kanaele, oder hoechstens ein Kanal mit Sprache
//! (Raumaufnahme). Dort ist die ganze Datei richtig, und die Sprechertrennung
//! des Anbieters bleibt erhalten.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::{StreamExt, TryStreamExt};
use owhisper_client::{BatchSttAdapter, BatchUploadLimit};
use owhisper_interface::batch_stream::BatchStreamEvent;
use tracing::Instrument;

use anlg_audio_chunking::AudioChunk;
use anlg_transcribe_core::TARGET_SAMPLE_RATE;
use anlg_transcribe_soniqo::{FileTranscript, FileTranscriptChunk, SoniqoModel, TranscriptWord};

use super::super::upload::audio_duration;
use super::super::{
    BatchParams, BatchRunMode, BatchRunOutput, format_user_friendly_error, session_span,
};
use super::direct::{direct_batch_timeout_for_audio, transcribe_once};
use super::local::soniqo_diarization_slot;
use super::local::{
    SONIQO_BUNDLE_MAX_SPAN_SAMPLES, SONIQO_DIRECT_MIC_MIN_RMS, SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
    SoniqoChannelVoice, SoniqoChunkBundle, SoniqoChunkBundler, SpeechSoniqoFileChunkIterator,
    audio_rms, resample_audio_to_channel_files_until, soniqo_bundle_transcript_chunks,
    soniqo_bundle_transcript_chunks_from_words, soniqo_channel_is_silent, soniqo_channel_voice,
};
use super::system_diarization::{
    SKIPPED_INSUFFICIENT_MEMORY, SYSTEM_DIARIZATION_MIN_MEMORY_BYTES, StopOnDrop,
    SystemDiarizationJob, SystemDiarizer, copy_channel_file, physical_memory_bytes,
    plan_system_diarization, read_channel_samples, system_diarizer,
};
use crate::{BatchEvent, BatchRuntime};

/// Hoechstens so viele Anfragen gleichzeitig. Genug, damit eine Stunde nicht
/// minutenlang seriell laeuft, wenig genug fuer die Ratenlimits der Anbieter.
pub(super) const CLOUD_CHANNEL_CONCURRENCY: usize = 4;
/// Wiederholungen bei einer Ratenbegrenzung, mit Wartezeit 2, 4, 8, 16 s.
pub(super) const RATE_LIMIT_RETRIES: u32 = 4;
const RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(2);

const PROGRESS_PREPARED: f64 = 0.10;
const PROGRESS_MAX: f64 = 0.95;

pub(super) struct ChannelBundleJob {
    pub(super) channel_index: usize,
    pub(super) path: PathBuf,
    /// Das Buendel ohne Ton (der liegt in `path`); gebraucht werden nur die
    /// Mitglieder mit ihren Zeitankern.
    pub(super) bundle: SoniqoChunkBundle,
}

pub(super) struct PreparedChannels {
    _dir: tempfile::TempDir,
    pub(super) channel_count: usize,
    pub(super) duration_seconds: f64,
    pub(super) jobs: Vec<ChannelBundleJob>,
    /// Geplante lokale Trennung des Systemkanals (`system_diarization.rs`).
    pub(super) system_diarization: Option<SystemDiarizationJob>,
    /// Warum eine geplante Trennung nicht laeuft (Hinweis-Code), sonst `None`.
    pub(super) system_diarization_skipped: Option<&'static str>,
}

pub(super) enum Preparation {
    Split(PreparedChannels),
    /// Mono, gleiche Kanaele oder hoechstens ein Kanal mit Sprache.
    KeepWholeFile,
}

/// Paketgroesse bei ECHTEN Wortzeiten des Anbieters (Nachtrag 28.09.2026,
/// ZICK-330): 5 Minuten statt des lokalen ~30-s-Modellfensters.
///
/// Das ~30-s-Fenster gehoert dem LOKALEN Parakeet-Modell und der geschaetzten
/// Sprechdauer-Verteilung (`split_bundle_text`) -- fuer den Cloud-Weg mit
/// echten Wortzeiten ist beides irrelevant, das Modell bekommt die
/// Zeitmarken vom Anbieter selbst zurueck (`soniqo_bundle_transcript_chunks_from_words`).
/// Es gilt nur noch: sicher unter jeder bekannten Anbietergrenze bleiben
/// (`bundle_max_samples` unten prueft zusaetzlich `BatchUploadLimit`, bei
/// OpenRouter 25 MB / 10 min, siehe `AdapterKind::batch_upload_limit`).
///
/// Belegt am echten Einstieg (Test
/// `deutlich_weniger_pakete_bei_echten_wortzeiten` unten): dieselbe
/// Sprechzeit ergibt mit Wortzeit-Zusage ein Bruchteil der Pakete des alten
/// ~30-s-Fensters.
pub(super) const CLOUD_WORD_TIMESTAMP_BUNDLE_MAX_SAMPLES: usize =
    TARGET_SAMPLE_RATE as usize * 5 * 60;

/// Ein mono-WAV mit 16 Bit/Sample (`write_mono_wav`): 2 Byte je Abtastwert,
/// der 44-Byte-Kopf ist gegenueber Minuten an Ton vernachlaessigbar.
const CLOUD_BUNDLE_BYTES_PER_SAMPLE: u64 = 2;

/// Wie gross darf ein Paket sein? Ohne belegte Wortzeit-Zusage das Fenster
/// des lokalen Weges (29,5 s), mit Zusage [`CLOUD_WORD_TIMESTAMP_BUNDLE_MAX_SAMPLES`]
/// -- in jedem Fall nie laenger und nie groesser, als der Anbieter pro
/// Anfrage annimmt (`BatchUploadLimit` deckt sowohl Zeitspanne als auch
/// Byte-Obergrenze ab, mit demselben 5-%-Sicherheitsabstand wie zuvor).
pub(super) fn bundle_max_samples(
    limit: Option<BatchUploadLimit>,
    wants_word_timestamps: bool,
) -> usize {
    let provider_max_duration = limit
        .map(|limit| {
            (limit.max_duration.as_secs_f64() * f64::from(TARGET_SAMPLE_RATE) * 0.95) as usize
        })
        .unwrap_or(usize::MAX);
    let provider_max_bytes = limit
        .map(|limit| {
            (limit.max_bytes as f64 * 0.95 / CLOUD_BUNDLE_BYTES_PER_SAMPLE as f64) as usize
        })
        .unwrap_or(usize::MAX);
    let provider_max = provider_max_duration.min(provider_max_bytes);

    let local_max = if wants_word_timestamps {
        CLOUD_WORD_TIMESTAMP_BUNDLE_MAX_SAMPLES
    } else {
        SONIQO_PARAKEET_MAX_CHUNK_SAMPLES.min(SONIQO_BUNDLE_MAX_SPAN_SAMPLES)
    };

    local_max.min(provider_max).max(TARGET_SAMPLE_RATE as usize)
}

/// Traegt dieser Kanal Sprache? Dieselbe Regel wie der lokale Weg: bei ein
/// oder zwei Kanaelen die relative Stille-Regel, darueber nur digitale Stille.
fn channel_speaks(channel_rms: &[f64], channel_index: usize) -> bool {
    if channel_rms.len() <= 2 {
        !soniqo_channel_is_silent(channel_rms, channel_index)
    } else {
        soniqo_channel_voice(channel_rms, channel_index) != SoniqoChannelVoice::DigitallySilent
    }
}

/// Zerlegt die Aufnahme in Kanaele und Pakete. Blockierend, gehoert in
/// `spawn_blocking`.
pub(super) fn prepare_channel_bundles(
    file_path: &str,
    max_samples: usize,
    async_runtime: &tokio::runtime::Handle,
    mut is_cancelled: impl FnMut() -> bool,
    mut on_bundle: impl FnMut(),
    diarize_system: Option<Option<u32>>,
) -> Result<Preparation, String> {
    let source = anlg_audio_utils::source_from_path(file_path).map_err(|e| e.to_string())?;
    let files = resample_audio_to_channel_files_until(file_path, source, &mut is_cancelled)?;
    let Some(speaking) = speaking_channels_to_split(&files)? else {
        return Ok(Preparation::KeepWholeFile);
    };

    // Vor dem Zerlegen lesen: danach gehoert die Kanal-Datei dem Buendler.
    // Scheitert das Lesen, gibt es eben keine Trennung, der Lauf geht weiter.
    let mut system_diarization_skipped = None;
    let system_diarization = diarize_system
        .and_then(|num_speakers| plan_system_diarization(&files, num_speakers))
        .and_then(|(channel_index, speaker_count)| {
            if physical_memory_bytes() < SYSTEM_DIARIZATION_MIN_MEMORY_BYTES {
                tracing::info!("cloud_system_diarization_skipped_insufficient_memory");
                system_diarization_skipped = Some(SKIPPED_INSUFFICIENT_MEMORY);
                return None;
            }
            match copy_channel_file(&files[channel_index]) {
                Ok(file) => Some(SystemDiarizationJob {
                    channel_index,
                    file,
                    speaker_count,
                }),
                Err(error) => {
                    tracing::warn!(%error, "cloud_system_diarization_copy_failed");
                    None
                }
            }
        });

    let channel_count = files.len();
    let duration_seconds = files[0].sample_count as f64 / f64::from(TARGET_SAMPLE_RATE);
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut jobs = Vec::new();

    for (channel_index, channel) in files.into_iter().enumerate() {
        if !speaking.contains(&channel_index) {
            continue;
        }
        let chunks = SpeechSoniqoFileChunkIterator::new(channel.file, async_runtime)?;
        let mut bundler = GatedBundler::new(max_samples);
        let mut write = |bundle: SoniqoChunkBundle| -> Result<(), String> {
            let path = dir
                .path()
                .join(format!("channel-{channel_index}-{:05}.wav", jobs.len()));
            write_mono_wav(&path, &bundle.samples)?;
            jobs.push(ChannelBundleJob {
                channel_index,
                path,
                bundle: SoniqoChunkBundle {
                    samples: Vec::new(),
                    members: bundle.members,
                },
            });
            on_bundle();
            Ok(())
        };

        for chunk in chunks {
            if is_cancelled() {
                return Err(TRANSCRIPTION_CANCELLED.to_string());
            }
            if let Some(bundle) = bundler.push(chunk?) {
                write(bundle)?;
            }
        }
        if let Some(bundle) = bundler.finish() {
            write(bundle)?;
        }
    }

    Ok(Preparation::Split(PreparedChannels {
        _dir: dir,
        channel_count,
        duration_seconds,
        jobs,
        system_diarization,
        system_diarization_skipped,
    }))
}

/// Die eine Entscheidung, ob eine Aufnahme zerlegt wird. `None`: ganze Datei
/// (Mono, zwei gleiche Kanaele, hoechstens ein sprechender Kanal, oder zwei
/// Kanaele mit derselben Quelle). Sonst die sprechenden Kanaele.
fn speaking_channels_to_split(
    files: &[super::local::ResampledChannelFile],
) -> Result<Option<Vec<usize>>, String> {
    if files.len() < 2 {
        return Ok(None);
    }

    let channel_rms = files.iter().map(|file| file.rms).collect::<Vec<_>>();
    let speaking = (0..files.len())
        .filter(|index| channel_speaks(&channel_rms, *index))
        .collect::<Vec<_>>();
    if speaking.len() < 2 {
        tracing::info!(
            channel_rms = ?channel_rms,
            speaking = speaking.len(),
            "cloud_channel_split_skipped_single_speaking_channel"
        );
        return Ok(None);
    }

    // Eine importierte Stereo-Datei (iPhone-Memo, Zoom-Recorder) hat zwei
    // sprechende Kanaele, die DIESELBE Quelle tragen. Zerlegt stuende jede
    // Aussage doppelt im Transkript und kostete doppelt. Eine Markierung
    // "importiert" gibt es an dieser Stelle nicht (der Import legt die Datei
    // als gewoehnliche Sitzungs-Aufnahme ab, `fs-sync-core/src/audio`), also
    // entscheidet der Ton selbst.
    if let Some(correlation) = channel_source_correlation(&files[speaking[0]], &files[speaking[1]])?
        && correlation >= SAME_SOURCE_CORRELATION
    {
        tracing::info!(
            correlation,
            "cloud_channel_split_skipped_correlated_channels"
        );
        return Ok(None);
    }

    Ok(Some(speaking))
}

/// Wuerde der Kanal-Weg diese Aufnahme zerlegen? Dieselbe Entscheidung wie
/// beim Lauf selbst, ohne Pakete und ohne Anfrage. Blockierend.
pub(in crate::batch) fn channel_split_would_apply(file_path: &str) -> Result<bool, String> {
    let source = anlg_audio_utils::source_from_path(file_path).map_err(|e| e.to_string())?;
    let files = resample_audio_to_channel_files_until(file_path, source, || false)?;
    Ok(speaking_channels_to_split(&files)?.is_some())
}

/// Ab dieser Korrelation tragen zwei Kanaele dieselbe Quelle.
///
/// Eine eigene Aufnahme hat links das Mikrofon und rechts den digitalen
/// Systemton: zwei verschiedene Signale, die hoechstens ueber Uebersprechen
/// (Lautsprecher ins Mikrofon, dann mit Raumhall und Ausgabe-Latenz von
/// meist deutlich mehr als 10 ms) aehnlich werden. Eine Stereo-Aufnahme EINES
/// Raums (zwei Mikrofone wenige Zentimeter auseinander) liegt bei derselben
/// Verschiebung von unter einer Millisekunde nahe 1. Die Schwelle liegt
/// dazwischen; der Testfall unten misst beide Seiten.
pub(super) const SAME_SOURCE_CORRELATION: f64 = 0.6;
/// Laenge eines Probe-Fensters (0,5 s).
const CORRELATION_WINDOW: usize = TARGET_SAMPLE_RATE as usize / 2;
/// Groesste gepruefte Verschiebung zwischen den Kanaelen (10 ms). Deckt den
/// Abstand zweier Mikrofone eines Geraets, NICHT die Latenz eines
/// Lautsprechers -- genau das trennt die beiden Faelle.
const CORRELATION_MAX_LAG: usize = TARGET_SAMPLE_RATE as usize / 100;
/// So viele Fenster, gleichmaessig ueber die Aufnahme verteilt.
const CORRELATION_PROBES: usize = 60;
/// Unter so vielen Fenstern, in denen BEIDE Kanaele Ton tragen, gibt es kein
/// Urteil. Ein Gespraech mit Sprecherwechsel hat kaum solche Fenster.
const CORRELATION_MIN_WINDOWS: usize = 3;

/// Median der hoechsten normierten Kreuzkorrelation zweier Kanaele ueber die
/// Fenster, in denen beide Ton tragen. `None`: zu wenige solche Fenster.
pub(super) fn channel_source_correlation(
    left: &super::local::ResampledChannelFile,
    right: &super::local::ResampledChannelFile,
) -> Result<Option<f64>, String> {
    let samples = left.sample_count.min(right.sample_count);
    if samples < CORRELATION_WINDOW {
        return Ok(None);
    }
    let mut left_reader = hound::WavReader::open(left.file.path()).map_err(|e| e.to_string())?;
    let mut right_reader = hound::WavReader::open(right.file.path()).map_err(|e| e.to_string())?;

    let probes = CORRELATION_PROBES.min(samples / CORRELATION_WINDOW).max(1);
    let span = samples - CORRELATION_WINDOW;
    let mut scores = Vec::new();
    for probe in 0..probes {
        let start = if probes == 1 {
            0
        } else {
            span * probe / (probes - 1)
        };
        let a = read_float_window(&mut left_reader, start)?;
        let b = read_float_window(&mut right_reader, start)?;
        if audio_rms(&a) < SONIQO_DIRECT_MIC_MIN_RMS || audio_rms(&b) < SONIQO_DIRECT_MIC_MIN_RMS {
            continue;
        }
        scores.push(max_normalized_correlation(&a, &b, CORRELATION_MAX_LAG));
    }

    if scores.len() < CORRELATION_MIN_WINDOWS {
        return Ok(None);
    }
    scores.sort_by(f64::total_cmp);
    Ok(Some(scores[scores.len() / 2]))
}

fn read_float_window(
    reader: &mut hound::WavReader<std::io::BufReader<std::fs::File>>,
    start: usize,
) -> Result<Vec<f32>, String> {
    reader.seek(start as u32).map_err(|e| e.to_string())?;
    reader
        .samples::<f32>()
        .take(CORRELATION_WINDOW)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// Hoechster Betrag der normierten Kreuzkorrelation ueber +-`max_lag`
/// Samples. Betrag, weil ein verpolter Kanal dieselbe Quelle bleibt.
pub(super) fn max_normalized_correlation(a: &[f32], b: &[f32], max_lag: usize) -> f64 {
    let len = a.len().min(b.len());
    if len <= 2 * max_lag {
        return 0.0;
    }
    // Mittelwertbereinigt (Forge m7): ein Gleichanteil (Brummen, Versatz eines
    // Wandlers) auf beiden Kanaelen saehe sonst nach "derselben Quelle" aus.
    let mean =
        |samples: &[f32]| samples[..len].iter().map(|x| f64::from(*x)).sum::<f64>() / len as f64;
    let (mean_a, mean_b) = (mean(a), mean(b));
    let core = max_lag..len - max_lag;
    let mut best = 0.0f64;
    for lag in 0..=2 * max_lag {
        let (mut dot, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
        for index in core.clone() {
            let x = f64::from(a[index]) - mean_a;
            let y = f64::from(b[index + lag - max_lag]) - mean_b;
            dot += x * y;
            aa += x * x;
            bb += y * y;
        }
        if aa > 0.0 && bb > 0.0 {
            best = best.max(dot.abs() / (aa * bb).sqrt());
        }
    }
    best
}

/// Der Buendler des lokalen Weges mit Lautstaerke-Gate davor.
///
/// Gate wie im lokalen Weg (`SONIQO_DIRECT_MIC_MIN_RMS`, 0,0008; gemessen am
/// 31.08.2026: Stille ~0,00012, Sprache ~0,059). Modelle der Whisper-Familie,
/// `gpt-transcribe` eingeschlossen, erfinden in fast stillen Abschnitten Text
/// ("Vielen Dank."). Ein solcher Abschnitt geht gar nicht erst raus -- hier
/// auf JEDEM Kanal, nicht nur dem Mikrofon: der Anbieter halluziniert auf dem
/// Systemton genauso. Ein Paket, das nur aus solchen Abschnitten bestuende,
/// entsteht damit nie, also auch keine Anfrage dafuer.
pub(super) struct GatedBundler {
    inner: SoniqoChunkBundler,
}

impl GatedBundler {
    pub(super) fn new(max_samples: usize) -> Self {
        Self {
            inner: SoniqoChunkBundler::new(max_samples, max_samples),
        }
    }

    pub(super) fn push(&mut self, chunk: AudioChunk) -> Option<SoniqoChunkBundle> {
        if audio_rms(&chunk.samples) < SONIQO_DIRECT_MIC_MIN_RMS {
            return None;
        }
        self.inner.push(chunk)
    }

    pub(super) fn finish(&mut self) -> Option<SoniqoChunkBundle> {
        self.inner.finish()
    }
}

fn write_mono_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        writer.write_sample(value).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())
}

/// Die beiden Abbruch-Meldungen der Vorbereitung (Zerlegen hier, Resampling
/// im lokalen Weg).
fn is_cancel_message(error: &str) -> bool {
    error == TRANSCRIPTION_CANCELLED || error == super::local::LOCAL_BATCH_CANCELLED
}

const TRANSCRIPTION_CANCELLED: &str = "Transcription was cancelled.";

/// Was ein Paket zurueckbrachte.
pub(super) struct BundleResult {
    pub(super) chunks: Vec<FileTranscriptChunk>,
    /// Sprache ging raus (das Paket hat das Lautstaerke-Gate passiert), aber
    /// auch nach einer Wiederholung kam kein Text zurueck.
    pub(super) empty: bool,
}

/// Hoechstens dieser Anteil leerer Sprachpakete wird hingenommen (Forge M2,
/// Entscheid 27.09.2026). Ein einzelnes leeres Paket kann legitim sein --
/// Husten oder Atmen ueber dem Gate --, viele leere Pakete sind es nicht: dann
/// fehlt ein Teil des Gespraechs, und ein Lauf, der das als Erfolg meldet,
/// ersetzt das bisherige Transkript durch ein luechenhaftes.
pub(super) const MAX_EMPTY_BUNDLE_SHARE: f64 = 0.2;

pub(super) fn empty_share_too_high(empty: usize, total: usize) -> bool {
    empty as f64 > MAX_EMPTY_BUNDLE_SHARE * total as f64
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct RejectedChannel {
    pub(super) channel_index: usize,
    pub(super) empty: usize,
    pub(super) total: usize,
}

/// Die Leerquote gilt JE KANAL, nicht ueber alle Pakete (Forge-Nachpruefung
/// 27.09.2026): Mikrofon mit einem Sprachpaket, Systemton mit sechs -- bleibt
/// das Mikrofonpaket leer, waeren es global nur 1 von 7 (14 %), und Kanal 0
/// verschwaende komplett. Deshalb: ein Kanal, der Sprache trug, darf nie ganz
/// leer enden, und je Kanal hoechstens `MAX_EMPTY_BUNDLE_SHARE`.
pub(super) fn rejected_by_empty_bundles(bundles: &[(usize, bool)]) -> Option<RejectedChannel> {
    let mut per_channel = std::collections::BTreeMap::<usize, (usize, usize)>::new();
    for (channel_index, empty) in bundles {
        let entry = per_channel.entry(*channel_index).or_default();
        entry.1 += 1;
        if *empty {
            entry.0 += 1;
        }
    }
    per_channel
        .into_iter()
        .find(|(_, (empty, total))| {
            *empty > 0 && (*empty == *total || empty_share_too_high(*empty, *total))
        })
        .map(|(channel_index, (empty, total))| {
            tracing::warn!(
                channel_index,
                empty,
                total,
                "cloud_channel_split_empty_bundles"
            );
            RejectedChannel {
                channel_index,
                empty,
                total,
            }
        })
}

/// Ein Aufruf mit Wiederholung bei Ratenbegrenzung.
async fn request_bundle<A: BatchSttAdapter>(
    provider: &str,
    bundle_params: &BatchParams,
    listen_params: &owhisper_interface::ListenParams,
    backoff: Duration,
) -> crate::Result<owhisper_interface::batch::Response> {
    // Ein Paket ist hoechstens ~30 s (ohne Wortzeit-Zusage) oder ~5 min (mit)
    // und liegt in jedem Fall unter der Grenze des Anbieters
    // (`bundle_max_samples`), also genau EIN Aufruf ohne Zerlegen.
    let timeout = direct_batch_timeout_for_audio(audio_duration(&bundle_params.file_path));
    let mut attempt = 0u32;
    loop {
        match transcribe_once::<A>(bundle_params, listen_params.clone(), timeout).await {
            Ok(response) => return Ok(response),
            Err(error) if error.is_rate_limited() && attempt < RATE_LIMIT_RETRIES => {
                let wait = backoff * 2u32.pow(attempt);
                tracing::warn!(
                    attempt,
                    wait_ms = wait.as_millis() as u64,
                    "cloud_channel_bundle_rate_limited"
                );
                tokio::time::sleep(wait).await;
                attempt += 1;
            }
            Err(error) => return Err(error.into_batch_error(provider, timeout)),
        }
    }
}

/// Ein Paket hochladen und seinen Text auf die Abschnitte zurueckverteilen.
///
/// Leerer Text oder eine Antwort ganz ohne Kanal/Alternative wird EINMAL
/// wiederholt. Fehlt die Struktur danach immer noch, ist das ein
/// Protokollfehler und der Lauf scheitert laut; bleibt nur der Text leer,
/// zaehlt das Paket als leer (`BundleResult::empty`).
pub(super) async fn transcribe_bundle<A: BatchSttAdapter>(
    provider: &str,
    params: &BatchParams,
    listen_params: &owhisper_interface::ListenParams,
    job: &ChannelBundleJob,
    backoff: Duration,
) -> crate::Result<BundleResult> {
    let mut bundle_params = params.clone();
    bundle_params.file_path = job.path.to_string_lossy().into_owned();

    let mut alternative = None;
    for attempt in 0..2 {
        let response =
            request_bundle::<A>(provider, &bundle_params, listen_params, backoff).await?;
        let found = response
            .results
            .channels
            .into_iter()
            .next()
            .and_then(|channel| channel.alternatives.into_iter().next());
        match found {
            Some(found) if !found.transcript.trim().is_empty() => {
                alternative = Some(found);
                break;
            }
            Some(found) => {
                tracing::warn!(attempt, "cloud_channel_bundle_empty_text");
                alternative = Some(found);
            }
            None => {
                tracing::warn!(attempt, "cloud_channel_bundle_without_channel");
                alternative = None;
            }
        }
    }

    let Some(alternative) = alternative else {
        return Err(direct_failure(
            provider,
            "The transcription provider returned a response without any transcript.".to_string(),
        )
        .into());
    };

    let text = alternative.transcript.trim();
    if text.is_empty() {
        return Ok(BundleResult {
            chunks: Vec::new(),
            empty: true,
        });
    }
    let words = alternative
        .words
        .iter()
        .map(|word| TranscriptWord {
            text: word
                .punctuated_word
                .clone()
                .unwrap_or_else(|| word.word.clone()),
            start_seconds: word.start,
            end_seconds: word.end,
            confidence: word.measured_confidence,
        })
        .collect::<Vec<_>>();

    // Wortzeiten des Anbieters, wo sie zum Text passen; sonst nach Sprechanteil.
    Ok(BundleResult {
        chunks: soniqo_bundle_transcript_chunks_from_words(&job.bundle, text, &words)
            .unwrap_or_else(|| soniqo_bundle_transcript_chunks(&job.bundle, text)),
        empty: false,
    })
}

fn direct_failure(provider: &str, message: String) -> crate::BatchFailure {
    crate::BatchFailure::DirectRequestFailed {
        provider: provider.to_string(),
        message,
    }
}

fn emit_progress(runtime: &Arc<dyn BatchRuntime>, session_id: &str, percentage: f64) {
    runtime.emit(BatchEvent::BatchResponseStreamed {
        session_id: session_id.to_string(),
        event: BatchStreamEvent::Progress {
            percentage,
            partial_text: None,
        },
    });
}

/// Alles, was ein Paket-Aufruf braucht, an einer Stelle -- BESITZEND.
///
/// Mit geliehenen Feldern (und davor mit einem async-Block, der Referenzen auf
/// Closures einfing) konnte der Compiler den Future nicht als `Send` fuer alle
/// Lebensdauern belegen, und `tokio::spawn` in
/// `plugins/transcription/src/listener2/ext.rs` baute nicht
/// ("implementation of `Send` is not general enough"). Besitzende Daten hinter
/// einem `Arc` machen jeden Paket-Future `'static`.
struct BundleRun {
    provider: String,
    runtime: Arc<dyn BatchRuntime>,
    session_id: String,
    params: BatchParams,
    bundle_params: owhisper_interface::ListenParams,
    backoff: Duration,
    completed: AtomicUsize,
    total: usize,
}

impl BundleRun {
    async fn run<A: BatchSttAdapter>(
        self: Arc<Self>,
        job: ChannelBundleJob,
    ) -> crate::Result<(usize, BundleResult)> {
        if self.runtime.is_cancelled() {
            return Err(direct_failure(&self.provider, TRANSCRIPTION_CANCELLED.to_string()).into());
        }
        let result = transcribe_bundle::<A>(
            &self.provider,
            &self.params,
            &self.bundle_params,
            &job,
            self.backoff,
        )
        .await?;
        let done = self.completed.fetch_add(1, Ordering::SeqCst) + 1;
        emit_progress(
            &self.runtime,
            &self.session_id,
            PROGRESS_PREPARED
                + (PROGRESS_MAX - PROGRESS_PREPARED) * done as f64 / self.total.max(1) as f64,
        );
        Ok((job.channel_index, result))
    }
}

type SystemDiarizationHandle = tokio::task::JoinHandle<(
    usize,
    Result<Vec<anlg_transcribe_soniqo::DiarizationSegment>, String>,
)>;

enum SystemDiarizationOutcome {
    Segments(usize, Vec<anlg_transcribe_soniqo::DiarizationSegment>),
    Failed(usize),
}

fn spawn_system_diarization(
    diarizer: Arc<dyn SystemDiarizer>,
    job: SystemDiarizationJob,
    runtime: Arc<dyn BatchRuntime>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> SystemDiarizationHandle {
    tokio::task::spawn_blocking(move || {
        let keep_going = || !runtime.is_cancelled() && !stop.load(Ordering::SeqCst);
        // Wartet, bis keine andere Soniqo-Trennung mehr laeuft (lokal oder
        // Cloud). Erst danach wird gelesen.
        let _slot = soniqo_diarization_slot();
        if !keep_going() {
            return (job.channel_index, Err(TRANSCRIPTION_CANCELLED.to_string()));
        }
        let started = std::time::Instant::now();
        let result = read_channel_samples(job.file.path()).and_then(|samples| {
            let mut keep_going = keep_going;
            let result = diarizer.diarize(&samples, job.speaker_count, &mut keep_going);
            tracing::info!(
                channel.index = job.channel_index,
                channel.seconds = samples.len() as f64 / f64::from(TARGET_SAMPLE_RATE),
                speaker.count = ?job.speaker_count,
                ok = result.is_ok(),
                elapsed_ms = started.elapsed().as_millis() as u64,
                "cloud_system_diarization_finished"
            );
            result
        });
        (job.channel_index, result)
    })
}

/// Wartet auf die Trennung und meldet dabei alle 2 s ein Lebenszeichen: das
/// Hochladen ist meist schneller fertig, und ohne Ereignis bricht der
/// Leerlauf-Waechter der App einen OpenAI-Lauf nach 60 s ab.
async fn await_system_diarization(
    mut handle: SystemDiarizationHandle,
    runtime: &Arc<dyn BatchRuntime>,
    session_id: &str,
) -> SystemDiarizationOutcome {
    let mut heartbeat = tokio::time::interval(Duration::from_secs(2));
    let joined = loop {
        tokio::select! {
            joined = &mut handle => break joined,
            _ = heartbeat.tick() => emit_progress(runtime, session_id, PROGRESS_MAX),
        }
    };
    match joined {
        Ok((channel_index, Ok(segments))) => {
            SystemDiarizationOutcome::Segments(channel_index, segments)
        }
        Ok((channel_index, Err(error))) => {
            tracing::warn!(%error, "cloud_system_diarization_failed_keeping_one_counterpart");
            SystemDiarizationOutcome::Failed(channel_index)
        }
        Err(error) => {
            tracing::warn!(%error, "cloud_system_diarization_panicked_keeping_one_counterpart");
            SystemDiarizationOutcome::Failed(1)
        }
    }
}

/// Was der Kanal-Weg ergab.
#[derive(Debug)]
pub(in crate::batch) enum ChannelSplitOutcome {
    /// Getrennt transkribiert.
    Done(BatchRunOutput),
    /// Passt nicht, und das ist richtig so (Mono, Raumaufnahme, eine Quelle):
    /// die ganze Datei, wie bisher.
    WholeFile,
    /// Haette zerlegt werden sollen, aber die Vorbereitung scheiterte. Der
    /// Aufrufer nimmt die ganze Datei, legt die Woerter aber auf den
    /// gemischten Kanal und setzt diesen Hinweis-Code (Forge M3).
    Fallback(&'static str),
}

/// Hinweis-Code in `channel_split_fallback`.
pub(in crate::batch) const FALLBACK_PREPARATION_FAILED: &str = "preparation_failed";

/// Einstieg. `WholeFile`/`Fallback` heissen: dieser Weg passt nicht, der Aufrufer nimmt den
/// alten (ganze Datei).
pub(in crate::batch) async fn run_channel_split_batch<A: BatchSttAdapter>(
    provider: &str,
    runtime: Arc<dyn BatchRuntime>,
    params: &BatchParams,
    listen_params: &owhisper_interface::ListenParams,
    limit: Option<BatchUploadLimit>,
) -> crate::Result<ChannelSplitOutcome> {
    run_channel_split_batch_with_backoff::<A>(
        provider,
        runtime,
        params,
        listen_params,
        limit,
        RATE_LIMIT_BACKOFF,
    )
    .await
}

pub(super) async fn run_channel_split_batch_with_backoff<A: BatchSttAdapter>(
    provider: &str,
    runtime: Arc<dyn BatchRuntime>,
    params: &BatchParams,
    listen_params: &owhisper_interface::ListenParams,
    limit: Option<BatchUploadLimit>,
    backoff: Duration,
) -> crate::Result<ChannelSplitOutcome> {
    let span = session_span(&params.session_id);
    let session_id = params.session_id.clone();

    async {
        let file_path = params.file_path.clone();
        let wants_word_timestamps = A::default().wants_word_timestamps(listen_params.model.as_deref());
        let max_samples = bundle_max_samples(limit, wants_word_timestamps);
        let async_runtime = tokio::runtime::Handle::current();
        let prep_runtime = runtime.clone();
        let prep_session_id = session_id.clone();
        let diarizer = system_diarizer();
        let prep_diarizer = diarizer.clone();
        let num_speakers = params.num_speakers;
        let prepared = tokio::task::spawn_blocking(move || {
            // Fragt Soniqo (Swift) nach dem Modell, deshalb hier im Blocking-Teil.
            let diarize_system = prep_diarizer.available().then_some(num_speakers);
            let prep_progress =
                |percentage: f64| emit_progress(&prep_runtime, &prep_session_id, percentage);
            // Lebenszeichen waehrend der Zerlegung. Der Leerlauf-Waechter der
            // App bricht einen OpenAI-Lauf nach 60 s ohne Ereignis ab
            // (`plugins/transcription/src/listener2/ext.rs`), und Dekodieren
            // plus Sprach-Erkennung einer langen Aufnahme dauert laenger.
            // Die Gesamtzahl der Pakete ist hier noch unbekannt, deshalb
            // waechst der Wert nur asymptotisch gegen die Marke.
            let prepared_bundles = std::cell::Cell::new(0usize);
            let last_emit = std::cell::Cell::new(std::time::Instant::now());
            let heartbeat = |force: bool| {
                if force || last_emit.get().elapsed() >= Duration::from_secs(2) {
                    let count = prepared_bundles.get() as f64;
                    prep_progress(PROGRESS_PREPARED * count / (count + 20.0));
                    last_emit.set(std::time::Instant::now());
                }
            };
            prepare_channel_bundles(
                &file_path,
                max_samples,
                &async_runtime,
                || {
                    heartbeat(false);
                    prep_runtime.is_cancelled()
                },
                || {
                    prepared_bundles.set(prepared_bundles.get() + 1);
                    heartbeat(true);
                },
                diarize_system,
            )
        })
        .await;

        // Scheitert die Vorbereitung (Dekodieren, Resampling, mehr als acht
        // Kanaele, Probe), ist das kein Grund, den Lauf abzubrechen: die ganze
        // Datei geht dann wie frueher an den Anbieter. Nur ein Abbruch durch
        // den Nutzer bleibt ein Abbruch.
        let prepared = match prepared {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(error)) if runtime.is_cancelled() || is_cancel_message(&error) => {
                return Err(direct_failure(provider, format_user_friendly_error(&error)).into());
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "cloud_channel_split_preparation_failed_using_whole_file");
                return Ok(ChannelSplitOutcome::Fallback(FALLBACK_PREPARATION_FAILED));
            }
            Err(error) => {
                tracing::warn!(%error, "cloud_channel_split_preparation_panicked_using_whole_file");
                return Ok(ChannelSplitOutcome::Fallback(FALLBACK_PREPARATION_FAILED));
            }
        };

        let Preparation::Split(prepared) = prepared else {
            return Ok(ChannelSplitOutcome::WholeFile);
        };

        let mut bundle_params = listen_params.clone();
        bundle_params.channels = 1;
        // Je Paket wuerde ein Anbieter seine Sprecher neu nummerieren; ueber
        // Pakete hinweg waeren die Nummern nicht vergleichbar. Der Kanal ist
        // hier die Sprecher-Zuordnung.
        bundle_params.num_speakers = None;
        bundle_params.min_speakers = None;
        bundle_params.max_speakers = None;

        let total = prepared.jobs.len();
        tracing::info!(
            channels = prepared.channel_count,
            bundles = total,
            concurrency = CLOUD_CHANNEL_CONCURRENCY,
            "cloud_channel_split_start"
        );
        emit_progress(&runtime, &session_id, PROGRESS_PREPARED);

        let run = Arc::new(BundleRun {
            provider: provider.to_string(),
            runtime: runtime.clone(),
            session_id: session_id.clone(),
            params: params.clone(),
            bundle_params,
            backoff,
            completed: AtomicUsize::new(0),
            total,
        });
        // Die Paketdateien liegen in `prepared._dir`; das bleibt bis zum Ende
        // dieser Funktion am Leben, die Jobs selbst wandern in die Futures.
        let mut prepared = prepared;
        let jobs = std::mem::take(&mut prepared.jobs);

        // Die lokale Trennung des Systemkanals laeuft GLEICHZEITIG mit dem
        // Hochladen. Scheitert das Hochladen, haelt `_stop` sie am naechsten
        // Abschnitt an.
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let _stop = StopOnDrop(stop.clone());
        let diarization = prepared.system_diarization.take().map(|job| {
            spawn_system_diarization(diarizer.clone(), job, runtime.clone(), stop.clone())
        });

        let results =
            futures_util::stream::iter(jobs.into_iter().map(|job| run.clone().run::<A>(job)))
                .buffered(CLOUD_CHANNEL_CONCURRENCY)
                .try_collect::<Vec<_>>()
                .await;
        // Scheitert das Hochladen, wird die Trennung angehalten und ABGEWARTET:
        // den laufenden CoreML-Abschnitt kann niemand unterbrechen, und die
        // Sitzung soll erst frei sein, wenn er zurueck ist. Sein Ergebnis
        // wird verworfen.
        let results = match results {
            Ok(results) => results,
            Err(error) => {
                if let Some(diarization) = diarization {
                    stop.store(true, Ordering::SeqCst);
                    let _ = await_system_diarization(diarization, &runtime, &session_id).await;
                }
                return Err(error);
            }
        };

        let mut per_channel = (0..prepared.channel_count)
            .map(|_| Vec::<FileTranscriptChunk>::new())
            .collect::<Vec<_>>();
        let empties = results
            .iter()
            .map(|(channel_index, result)| (*channel_index, result.empty))
            .collect::<Vec<_>>();
        if let Some(rejected) = rejected_by_empty_bundles(&empties) {
            if let Some(diarization) = diarization {
                stop.store(true, Ordering::SeqCst);
                let _ = await_system_diarization(diarization, &runtime, &session_id).await;
            }
            return Err(direct_failure(
                provider,
                format!(
                    "The transcription provider returned no text for {} of {} speech segments on one channel. Nothing was replaced; please try again or choose another provider.",
                    rejected.empty, rejected.total
                ),
            )
            .into());
        }
        for (channel_index, result) in results {
            per_channel[channel_index].extend(result.chunks);
        }
        let mut transcripts = per_channel
            .into_iter()
            .map(|chunks| {
                if chunks.is_empty() {
                    FileTranscript::new(String::new(), prepared.duration_seconds)
                } else {
                    FileTranscript::from_chunks(chunks, prepared.duration_seconds)
                }
            })
            .collect::<Vec<_>>();

        if let Some(diarization) = diarization {
            let outcome = await_system_diarization(diarization, &runtime, &session_id).await;
            if runtime.is_cancelled() {
                return Err(direct_failure(provider, TRANSCRIPTION_CANCELLED.to_string()).into());
            }
            match outcome {
                SystemDiarizationOutcome::Segments(channel_index, segments) => {
                    if let Some(transcript) = transcripts.get_mut(channel_index) {
                        transcript.speaker_segments = segments;
                    }
                }
                // Fehlgeschlagen: ein Gegenueber wie bisher, dazu derselbe
                // Hinweis wie im lokalen Weg. Kein Abbruch.
                SystemDiarizationOutcome::Failed(channel_index) => {
                    if let Some(transcript) = transcripts.get_mut(channel_index) {
                        transcript.diarization_failed = true;
                    }
                }
            }
        }

        // Dieselbe Umwandlung wie der lokale Weg -- damit Kanalindex,
        // Abschnittsanker und `timing_source` exakt so ankommen wie dort. Die
        // Modellangabe wird danach auf den Cloud-Anbieter umgeschrieben.
        let mut response = anlg_transcribe_soniqo::batch_response_from_channels(
            SoniqoModel::ParakeetBatch,
            transcripts,
        );
        if let Some(object) = response.metadata.as_object_mut() {
            object.insert(
                "model_info".to_string(),
                serde_json::json!({
                    "name": listen_params.model.clone().unwrap_or_default(),
                    "version": "",
                    "arch": provider,
                }),
            );
            object.insert("channel_split".to_string(), serde_json::json!(true));
            if let Some(reason) = prepared.system_diarization_skipped {
                object.insert(
                    "system_diarization_skipped".to_string(),
                    serde_json::json!(reason),
                );
            }
        }

        Ok(ChannelSplitOutcome::Done(BatchRunOutput {
            session_id: session_id.clone(),
            mode: BatchRunMode::Direct,
            response,
        }))
    }
    .instrument(span)
    .await
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::path::Path;
    use std::pin::Pin;
    use std::sync::Mutex;

    use owhisper_interface::batch::{Alternatives, Channel, Response, Results};

    use super::super::local::SONIQO_BUNDLE_SEAM_SAMPLES;
    use super::*;

    /// Die Tests teilen sich den Mitschnitt des Fake-Anbieters.
    static SERIAL: Mutex<()> = Mutex::new(());
    static UPLOADS: Mutex<Vec<Upload>> = Mutex::new(Vec::new());
    /// So viele Anfragen scheitern noch mit 429, bevor der Fake antwortet.
    static RATE_LIMITED_CALLS: Mutex<u32> = Mutex::new(0);
    /// So viele Anfragen liefern noch HTTP 200 mit leerem Text.
    static EMPTY_CALLS: Mutex<u32> = Mutex::new(0);
    /// Antworten ohne jeden Kanal (Protokollfehler).
    static NO_CHANNELS: Mutex<bool> = Mutex::new(false);

    #[derive(Debug, Clone)]
    struct Upload {
        channels: u16,
        seconds: f64,
    }

    /// Fake wie `gpt-transcribe`: nur Text, keine Wortzeiten. Kein Netz.
    #[derive(Clone, Default)]
    struct TextOnlyAdapter;

    impl BatchSttAdapter for TextOnlyAdapter {
        fn is_supported_languages(
            &self,
            _languages: &[anlg_language::Language],
            _model: Option<&str>,
        ) -> bool {
            true
        }

        fn transcribe_file<'a, P: AsRef<Path> + Send + 'a>(
            &'a self,
            _client: &'a reqwest_middleware::ClientWithMiddleware,
            _api_base: &'a str,
            _api_key: &'a str,
            _params: &'a owhisper_interface::ListenParams,
            file_path: P,
        ) -> Pin<
            Box<
                dyn Future<Output = std::result::Result<Response, owhisper_client::Error>>
                    + Send
                    + 'a,
            >,
        > {
            let path = file_path.as_ref().to_path_buf();
            Box::pin(async move {
                {
                    let mut remaining = RATE_LIMITED_CALLS.lock().unwrap();
                    if *remaining > 0 {
                        *remaining -= 1;
                        return Err(owhisper_client::Error::UnexpectedStatus {
                            status: reqwest_middleware::reqwest::StatusCode::TOO_MANY_REQUESTS,
                            body: "rate limit".to_string(),
                        });
                    }
                }
                let reader = hound::WavReader::open(&path).unwrap();
                let spec = reader.spec();
                UPLOADS.lock().unwrap().push(Upload {
                    channels: spec.channels,
                    seconds: reader.duration() as f64 / f64::from(spec.sample_rate),
                });
                if *NO_CHANNELS.lock().unwrap() {
                    return Ok(Response {
                        metadata: serde_json::json!({}),
                        results: Results {
                            channels: Vec::new(),
                        },
                    });
                }
                let text = {
                    let mut empty = EMPTY_CALLS.lock().unwrap();
                    if *empty > 0 {
                        *empty -= 1;
                        String::new()
                    } else {
                        "eins zwei drei vier".to_string()
                    }
                };
                Ok(Response {
                    metadata: serde_json::json!({}),
                    results: Results {
                        channels: vec![Channel {
                            alternatives: vec![Alternatives {
                                transcript: text,
                                confidence: 1.0,
                                words: Vec::new(),
                            }],
                        }],
                    },
                })
            })
        }
    }

    struct SilentRuntime;

    impl BatchRuntime for SilentRuntime {
        fn emit(&self, _event: BatchEvent) {}
    }

    fn read_mono(path: &str) -> Vec<f32> {
        let mut reader = hound::WavReader::open(path).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, TARGET_SAMPLE_RATE);
        reader
            .samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / f32::from(i16::MAX))
            .collect()
    }

    /// Stereo wie eine echte Aufnahme: links Mikrofon, rechts Systemton.
    /// Links spricht zuerst, dann 1 s Pause, dann spricht rechts.
    fn write_dialog(path: &Path, right_speaks: bool) -> (f64, f64) {
        let left = read_mono(anlg_fixtures::SPEECH_DE_WAV);
        let right = read_mono(anlg_fixtures::SPEECH_EN_WAV);
        let gap = TARGET_SAMPLE_RATE as usize;
        let total = left.len() + gap + right.len();
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for index in 0..total {
            let l = left.get(index).copied().unwrap_or(0.0);
            let r = if right_speaks && index >= left.len() + gap {
                right[index - left.len() - gap]
            } else {
                0.0
            };
            for value in [l, r] {
                writer
                    .write_sample((value * f32::from(i16::MAX)) as i16)
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
        let rate = f64::from(TARGET_SAMPLE_RATE);
        (left.len() as f64 / rate, (left.len() + gap) as f64 / rate)
    }

    fn params(path: &Path) -> BatchParams {
        BatchParams {
            session_id: "channel-split-test".to_string(),
            provider: super::super::super::BatchProvider::OpenAI,
            file_path: path.to_string_lossy().into_owned(),
            model: Some("gpt-transcribe".to_string()),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test".to_string(),
            languages: vec![anlg_language::ISO639::De.into()],
            keywords: vec![],
            vocabulary: vec![],
            num_speakers: Some(2),
            min_speakers: None,
            max_speakers: None,
        }
    }

    fn listen_params() -> owhisper_interface::ListenParams {
        owhisper_interface::ListenParams {
            channels: 2,
            sample_rate: TARGET_SAMPLE_RATE,
            num_speakers: Some(2),
            ..Default::default()
        }
    }

    async fn run(path: &Path) -> crate::Result<Option<BatchRunOutput>> {
        run_outcome(path).await.map(|outcome| match outcome {
            ChannelSplitOutcome::Done(output) => Some(output),
            _ => None,
        })
    }

    async fn run_outcome(path: &Path) -> crate::Result<ChannelSplitOutcome> {
        run_channel_split_batch_with_backoff::<TextOnlyAdapter>(
            "openai",
            Arc::new(SilentRuntime),
            &params(path),
            &listen_params(),
            None,
            Duration::from_millis(1),
        )
        .await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn beide_kanaele_kommen_getrennt_und_zeitlich_richtig_an() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        UPLOADS.lock().unwrap().clear();
        *RATE_LIMITED_CALLS.lock().unwrap() = 0;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let (left_end, right_start) = write_dialog(file.path(), true);

        let output = run(file.path())
            .await
            .unwrap()
            .expect("zwei sprechende Kanaele muessen den Kanal-Weg nehmen");
        let channels = &output.response.results.channels;
        assert_eq!(channels.len(), 2, "Mikrofon und Systemton getrennt");

        let mic = &channels[0].alternatives[0].words;
        let remote = &channels[1].alternatives[0].words;
        assert!(!mic.is_empty(), "Kanal 0 ohne Woerter");
        assert!(!remote.is_empty(), "Kanal 1 ohne Woerter");
        assert!(mic.iter().all(|word| word.channel == 0));
        assert!(remote.iter().all(|word| word.channel == 1));
        // Echte Abschnittszeiten statt "alles ab 0 s": links redet vorher,
        // rechts nachher. Toleranz fuer die Sprach-Erkennung an den Raendern.
        let mic_end = mic.iter().map(|word| word.end).fold(0.0, f64::max);
        let remote_start = remote
            .iter()
            .map(|word| word.start)
            .fold(f64::MAX, f64::min);
        assert!(
            mic_end <= left_end + 0.5,
            "Mikrofon endet bei {mic_end}, Sprache bei {left_end}"
        );
        assert!(
            remote_start >= right_start - 0.5,
            "Systemton beginnt bei {remote_start}, Sprache bei {right_start}"
        );
        assert_eq!(
            output.response.metadata["timing_source"],
            "provider_segment_interpolated"
        );

        let uploads = UPLOADS.lock().unwrap().clone();
        assert!(
            uploads.len() >= 2,
            "mindestens ein Paket je Kanal: {uploads:?}"
        );
        for upload in &uploads {
            assert_eq!(upload.channels, 1, "Paket nicht mono");
            assert!(upload.seconds <= 29.6, "Paket zu lang: {upload:?}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stiller_kanal_erzeugt_keine_anfrage_und_laesst_den_alten_weg() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        UPLOADS.lock().unwrap().clear();
        *RATE_LIMITED_CALLS.lock().unwrap() = 0;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), false);

        let output = run(file.path()).await.unwrap();

        assert!(
            output.is_none(),
            "Raumaufnahme: ganze Datei, Trennung des Anbieters bleibt"
        );
        assert!(UPLOADS.lock().unwrap().is_empty(), "keine Paket-Anfrage");
    }

    /// Stereo EINES Raums, wie ein iPhone-Memo: beide Kanaele tragen dieselbe
    /// Sprache, rechts leiser, 3 Samples spaeter und mit eigenem Rauschen
    /// (sonst faengt schon die Gleichheits-Pruefung beim Zerlegen den Fall).
    fn write_correlated_stereo(path: &Path) {
        let speech = read_mono(anlg_fixtures::SPEECH_DE_WAV);
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        let mut noise_state = 12345u32;
        for index in 0..speech.len() {
            noise_state = noise_state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let noise = ((noise_state >> 16) as f32 / 32_768.0 - 1.0) * 0.003;
            let l = speech[index];
            let r = 0.7 * speech[index.saturating_sub(3)] + noise;
            for value in [l, r] {
                writer
                    .write_sample((value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn importierte_stereo_datei_einer_quelle_bleibt_eine_datei() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        UPLOADS.lock().unwrap().clear();
        *RATE_LIMITED_CALLS.lock().unwrap() = 0;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_correlated_stereo(file.path());

        let output = run(file.path()).await.unwrap();

        assert!(
            output.is_none(),
            "dieselbe Quelle auf beiden Kanaelen: alter Weg, sonst steht alles doppelt"
        );
        assert!(UPLOADS.lock().unwrap().is_empty(), "keine Paket-Anfrage");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scheiternde_vorbereitung_nimmt_den_alten_weg_statt_abzubrechen() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        UPLOADS.lock().unwrap().clear();
        *RATE_LIMITED_CALLS.lock().unwrap() = 0;
        // Zehn Kanaele: das Zerlegen lehnt mehr als acht ab.
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let speech = read_mono(anlg_fixtures::SPEECH_DE_WAV);
        let spec = hound::WavSpec {
            channels: 10,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(file.path(), spec).unwrap();
        for sample in speech.iter().take(TARGET_SAMPLE_RATE as usize * 2) {
            for _ in 0..10 {
                writer
                    .write_sample((sample * f32::from(i16::MAX)) as i16)
                    .unwrap();
            }
        }
        writer.finalize().unwrap();

        let output = run_outcome(file.path()).await;

        assert!(
            matches!(
                output,
                Ok(ChannelSplitOutcome::Fallback(FALLBACK_PREPARATION_FAILED))
            ),
            "Vorbereitung gescheitert: alter Weg mit Hinweis, kein Abbruch: {output:?}"
        );
    }

    /// Forge M3 am echten Einstieg: scheitert die Vorbereitung (hier: zehn
    /// Kanaele), geht die ganze Datei wie frueher raus -- aber die Woerter
    /// liegen auf dem gemischten Kanal statt auf Kanal 0 = Nutzer, und der
    /// Grund steht in den Metadaten.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rueckfall_legt_woerter_auf_den_gemischten_kanal() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, requests) = fake_groq_with(true).await;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let speech = read_mono(anlg_fixtures::SPEECH_DE_WAV);
        let mut writer = hound::WavWriter::create(
            file.path(),
            hound::WavSpec {
                channels: 10,
                sample_rate: TARGET_SAMPLE_RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for sample in speech.iter().take(TARGET_SAMPLE_RATE as usize * 2) {
            for _ in 0..10 {
                writer
                    .write_sample((sample * f32::from(i16::MAX)) as i16)
                    .unwrap();
            }
        }
        writer.finalize().unwrap();

        let output =
            crate::batch::run_batch(Arc::new(SilentRuntime), groq_params(file.path(), base_url))
                .await
                .unwrap();

        assert_eq!(
            requests.load(Ordering::SeqCst),
            1,
            "ganze Datei, eine Anfrage"
        );
        assert_eq!(
            output.response.metadata["channel_split_fallback"],
            "preparation_failed"
        );
        let channels = &output.response.results.channels;
        assert_eq!(channels.len(), 3, "Position 2 = gemischter Kanal");
        assert!(
            channels[0].alternatives.is_empty() && channels[1].alternatives.is_empty(),
            "Mikrofon und Systemton leer, sonst nimmt die Anzeige Kanal 0 als Nutzer"
        );
        let words = channels[2].alternatives[0].words.iter().collect::<Vec<_>>();
        assert!(!words.is_empty());
        assert!(
            words.iter().all(|word| word.channel == 2),
            "gemischter Kanal statt Nutzer: {:?}",
            words.iter().map(|word| word.channel).collect::<Vec<_>>()
        );
    }

    struct CancelledRuntime;

    impl BatchRuntime for CancelledRuntime {
        fn emit(&self, _event: BatchEvent) {}
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn abbruch_durch_den_nutzer_bleibt_ein_abbruch() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        UPLOADS.lock().unwrap().clear();
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);

        let output = run_channel_split_batch_with_backoff::<TextOnlyAdapter>(
            "openai",
            Arc::new(CancelledRuntime),
            &params(file.path()),
            &listen_params(),
            None,
            Duration::from_millis(1),
        )
        .await;

        assert!(
            output.is_err(),
            "Abbruch darf nicht still zur ganzen Datei werden"
        );
        assert!(UPLOADS.lock().unwrap().is_empty());
    }

    #[test]
    fn korrelation_trennt_eine_quelle_von_zwei_quellen() {
        let de = read_mono(anlg_fixtures::SPEECH_DE_WAV);
        let en = read_mono(anlg_fixtures::SPEECH_EN_WAV);
        let window = CORRELATION_WINDOW;
        let start = de.len() / 3;
        let a = &de[start..start + window];
        let shifted = de[start - 3..start - 3 + window]
            .iter()
            .map(|sample| -0.6 * sample)
            .collect::<Vec<_>>();
        let same = max_normalized_correlation(a, &shifted, CORRELATION_MAX_LAG);
        assert!(same > 0.95, "eine Quelle, verpolt und verschoben: {same}");

        // Zwei Quellen mit demselben Gleichanteil bleiben zwei Quellen.
        let offset_a = a.iter().map(|x| x + 0.3).collect::<Vec<_>>();
        let offset_b = en[en.len() / 3..en.len() / 3 + window]
            .iter()
            .map(|x| x + 0.3)
            .collect::<Vec<_>>();
        let with_offset = max_normalized_correlation(&offset_a, &offset_b, CORRELATION_MAX_LAG);
        assert!(
            with_offset < SAME_SOURCE_CORRELATION,
            "Gleichanteil macht keine Quelle: {with_offset}"
        );

        let other = &en[en.len() / 3..en.len() / 3 + window];
        let different = max_normalized_correlation(a, other, CORRELATION_MAX_LAG);
        assert!(
            different < SAME_SOURCE_CORRELATION,
            "zwei Quellen: {different}"
        );
    }

    fn reset_fake() {
        UPLOADS.lock().unwrap().clear();
        *RATE_LIMITED_CALLS.lock().unwrap() = 0;
        *EMPTY_CALLS.lock().unwrap() = 0;
        *NO_CHANNELS.lock().unwrap() = false;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn leeres_paket_wird_einmal_wiederholt() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset_fake();
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);
        *EMPTY_CALLS.lock().unwrap() = 1;

        let output = run(file.path()).await.unwrap().unwrap();
        reset_fake();

        let words = output
            .response
            .results
            .channels
            .iter()
            .map(|channel| channel.alternatives[0].words.len())
            .collect::<Vec<_>>();
        assert!(words.iter().all(|count| *count > 0), "{words:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dauerhaft_leere_pakete_lassen_den_lauf_scheitern() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset_fake();
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);
        *EMPTY_CALLS.lock().unwrap() = 1_000;

        let error = run(file.path()).await.unwrap_err();
        reset_fake();

        assert!(error.to_string().contains("returned no text"), "{error}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn antwort_ohne_kanal_ist_ein_protokollfehler() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset_fake();
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);
        *NO_CHANNELS.lock().unwrap() = true;

        let error = run(file.path()).await.unwrap_err();
        reset_fake();

        assert!(
            error.to_string().contains("without any transcript"),
            "{error}"
        );
    }

    #[test]
    fn ein_kanal_darf_nicht_komplett_leer_enden() {
        // Das Szenario der Nachpruefung: Mikrofon 1 Paket (leer), Systemton 6.
        let mut bundles = vec![(0usize, true)];
        bundles.extend((0..6).map(|_| (1usize, false)));
        assert_eq!(
            rejected_by_empty_bundles(&bundles),
            Some(RejectedChannel {
                channel_index: 0,
                empty: 1,
                total: 1
            })
        );

        // Je Kanal: 2 von 5 leer auf dem Systemton sind zu viel, auch wenn
        // es ueber alle Pakete nur 2 von 15 waeren.
        let mut bundles = (0..10).map(|_| (0usize, false)).collect::<Vec<_>>();
        bundles.extend([(1, true), (1, true), (1, false), (1, false), (1, false)]);
        assert_eq!(
            rejected_by_empty_bundles(&bundles).map(|r| r.channel_index),
            Some(1)
        );

        // Ein leeres Paket unter zehn auf einem Kanal geht durch.
        let mut bundles = (0..9).map(|_| (0usize, false)).collect::<Vec<_>>();
        bundles.push((0, true));
        bundles.extend((0..3).map(|_| (1usize, false)));
        assert_eq!(rejected_by_empty_bundles(&bundles), None);
    }

    #[test]
    fn wenige_leere_pakete_sind_erlaubt() {
        assert!(!empty_share_too_high(0, 10));
        assert!(!empty_share_too_high(2, 10), "20 % gehen noch");
        assert!(empty_share_too_high(3, 10));
        assert!(empty_share_too_high(1, 1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ratenbegrenzung_wird_wiederholt() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        UPLOADS.lock().unwrap().clear();
        *RATE_LIMITED_CALLS.lock().unwrap() = 2;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);

        let output = run(file.path()).await.unwrap().unwrap();

        assert_eq!(output.response.results.channels.len(), 2);
        assert_eq!(*RATE_LIMITED_CALLS.lock().unwrap(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dauerhafte_ratenbegrenzung_bricht_den_lauf_laut_ab() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        UPLOADS.lock().unwrap().clear();
        *RATE_LIMITED_CALLS.lock().unwrap() = 1_000;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);

        let error = run(file.path()).await.unwrap_err();
        *RATE_LIMITED_CALLS.lock().unwrap() = 0;

        assert!(
            error.to_string().contains("Rate limit exceeded"),
            "kein halbes Transkript, sondern ein Fehler: {error}"
        );
    }

    #[test]
    fn ratenbegrenzung_wird_am_statuscode_erkannt() {
        use super::super::direct::DirectAttemptError;
        use reqwest_middleware::reqwest::StatusCode;

        let status = |status: StatusCode, body: &str| {
            DirectAttemptError::Provider(owhisper_client::Error::UnexpectedStatus {
                status,
                body: body.to_string(),
            })
        };
        assert!(status(StatusCode::TOO_MANY_REQUESTS, "").is_rate_limited());
        // Der Statuscode gewinnt gegen den Text: ein Serverfehler, dessen
        // Meldung "rate limit" erwaehnt, ist keine Ratenbegrenzung.
        assert!(
            !status(StatusCode::INTERNAL_SERVER_ERROR, "rate limit backend down").is_rate_limited()
        );
        // Ohne Statuscode bleibt die Meldung der Weg.
        assert!(
            DirectAttemptError::Provider(owhisper_client::Error::AudioProcessing(
                "HTTP 429 Too Many Requests".to_string()
            ))
            .is_rate_limited()
        );
        assert!(!DirectAttemptError::TimedOut.is_rate_limited());
    }

    /// Ein OpenAI-kompatibler Anbieter (Groq) auf 127.0.0.1: zaehlt Anfragen,
    /// liefert reinen Text. So laeuft der Test ueber den ECHTEN Einstieg
    /// `run_batch` mit dem echten Adapter, ohne Netz und ohne Kosten.
    async fn fake_groq() -> (String, Arc<AtomicUsize>) {
        fake_groq_with(false).await
    }

    /// `with_words`: die Antwort traegt Wortzeiten (0 bis 1,6 s), sonst nur
    /// Text wie `gpt-transcribe`.
    async fn fake_groq_with(with_words: bool) -> (String, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let app = axum::Router::new().route(
            "/audio/transcriptions",
            axum::routing::post(move |_body: axum::body::Bytes| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    if !with_words {
                        return axum::Json(serde_json::json!({ "text": "eins zwei drei vier" }));
                    }
                    axum::Json(serde_json::json!({
                        "text": "eins zwei drei vier",
                        "words": [
                            { "word": "eins", "start": 0.0, "end": 0.4 },
                            { "word": "zwei", "start": 0.4, "end": 0.8 },
                            { "word": "drei", "start": 0.8, "end": 1.2 },
                            { "word": "vier", "start": 1.2, "end": 1.6 }
                        ]
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), count)
    }

    fn groq_params(path: &Path, base_url: String) -> BatchParams {
        BatchParams {
            provider: super::super::super::BatchProvider::Groq,
            base_url,
            model: None,
            ..params(path)
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn echter_einstieg_trennt_stereo_und_laesst_mono() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, requests) = fake_groq().await;

        let stereo = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(stereo.path(), true);
        let output = crate::batch::run_batch(
            Arc::new(SilentRuntime),
            groq_params(stereo.path(), base_url.clone()),
        )
        .await
        .unwrap();
        assert_eq!(
            output.response.results.channels.len(),
            2,
            "Stereo mit zwei Sprechern ueber run_batch: Mikrofon und Systemton getrennt"
        );
        assert_eq!(output.response.metadata["channel_split"], true);
        assert!(
            requests.load(Ordering::SeqCst) >= 2,
            "mindestens ein Paket je Kanal"
        );

        // Gegenprobe: Mono geht als EINE Datei den alten Weg.
        requests.store(0, Ordering::SeqCst);
        let mono = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        std::fs::copy(anlg_fixtures::SPEECH_DE_WAV, mono.path()).unwrap();
        let output =
            crate::batch::run_batch(Arc::new(SilentRuntime), groq_params(mono.path(), base_url))
                .await
                .unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 1, "Mono: eine Anfrage");
        assert!(output.response.metadata.get("channel_split").is_none());
    }

    /// Ein OpenRouter-Fake, der sieht, ob `response_format`/
    /// `timestamp_granularities[]` tatsaechlich im Formular stecken (ZICK-330
    /// Nachtrag, 28.09.2026). Liefert echte, ungleichmaessige Wortzeiten nur
    /// dann zurueck; sonst reinen Text wie `openai/gpt-transcribe`.
    async fn fake_openrouter_with_words() -> (String, Arc<AtomicUsize>, Arc<Mutex<Vec<bool>>>) {
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let saw_format = Arc::new(Mutex::new(Vec::<bool>::new()));
        let saw_format_handler = saw_format.clone();
        let app = axum::Router::new().route(
            "/audio/transcriptions",
            axum::routing::post(move |body: axum::body::Bytes| {
                let counter = counter.clone();
                let saw_format = saw_format_handler.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    let form = String::from_utf8_lossy(&body);
                    let requested_words = form.contains("name=\"response_format\"")
                        && form.contains("verbose_json")
                        && form.contains("timestamp_granularities");
                    saw_format.lock().unwrap().push(requested_words);
                    if !requested_words {
                        return axum::Json(serde_json::json!({ "text": "eins zwei drei vier" }));
                    }
                    // Bewusst INNERHALB der ersten ~0,4 s (bundle-lokal): mit
                    // vier Woertern bis 1,6 s griff bei den kurzen
                    // Fixture-Kanaelen bereits eine echte Naht zwischen zwei
                    // Sprech-Abschnitten DESSELBEN Buendels (VAD-Pause in
                    // SPEECH_DE/EN_WAV) -- dann verteilt
                    // `soniqo_bundle_transcript_chunks_from_words` auf zwei
                    // Mitglieder, deren reale Anker fuer diesen Test nicht
                    // zusammenpassen. Drei Woerter mit einer echten, ungleich
                    // grossen Pause (eins-zwei buendig, zwei-drei mit 0,06-s-
                    // Luecke) reichen, um echte statt interpolierte Zeiten zu
                    // belegen, ohne diese Naht zu treffen.
                    axum::Json(serde_json::json!({
                        "text": "eins zwei drei",
                        "words": [
                            { "word": "eins", "start": 0.0, "end": 0.12 },
                            { "word": "zwei", "start": 0.12, "end": 0.22 },
                            { "word": "drei", "start": 0.28, "end": 0.40 }
                        ]
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), count, saw_format)
    }

    fn openrouter_params(path: &Path, base_url: String, model: &str) -> BatchParams {
        BatchParams {
            provider: super::super::super::BatchProvider::OpenRouter,
            base_url,
            model: Some(model.to_string()),
            ..params(path)
        }
    }

    /// Beide Kanaele sprechen GLEICHZEITIG (echtes Gegenredeprodukt), anders
    /// als `write_dialog` mit seiner Pause zwischen Mikrofon und Systemton.
    fn write_overlapping_dialog(path: &Path) {
        let left = read_mono(anlg_fixtures::SPEECH_DE_WAV);
        let right = read_mono(anlg_fixtures::SPEECH_EN_WAV);
        let total = left.len().max(right.len());
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for index in 0..total {
            let l = left.get(index).copied().unwrap_or(0.0);
            let r = right.get(index).copied().unwrap_or(0.0);
            for value in [l, r] {
                writer
                    .write_sample((value * f32::from(i16::MAX)) as i16)
                    .unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    /// Ein Modell aus der belegten Liste (`openrouter/mod.rs`): verbose_json
    /// wird angefragt, und die echten Wortzeiten kommen ueber
    /// `soniqo_bundle_transcript_chunks_from_words` bis zur Antwort durch --
    /// auch wenn beide Kanaele gleichzeitig sprechen. "provider_word" statt
    /// "provider_segment_interpolated" ist der Beleg dafuer; die zweite
    /// Zusicherung ist, dass die Wortfolge je Kanal nicht durcheinandergerissen
    /// wird (keine erfundene Verzahnung ueber Kanalgrenzen).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn openrouter_mit_wortzeit_modell_nutzt_echte_zeiten_bei_ueberlappender_sprache() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, requests, saw_format) = fake_openrouter_with_words().await;

        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_overlapping_dialog(file.path());

        let output = crate::batch::run_batch(
            Arc::new(SilentRuntime),
            openrouter_params(file.path(), base_url, "google/gemini-3.5-transcribe"),
        )
        .await
        .unwrap();

        assert_eq!(
            output.response.results.channels.len(),
            2,
            "ueberlappende Sprache: beide Kanaele trotzdem getrennt"
        );
        assert_eq!(
            output.response.metadata["timing_source"], "provider_word",
            "echte Wortzeiten des Anbieters, keine geschaetzten"
        );

        for channel in &output.response.results.channels {
            let words = &channel.alternatives[0].words;
            assert_eq!(words.len(), 3, "alle drei Woerter des Pakets: {words:?}");
            let mut previous_end = -1.0;
            for word in words {
                assert!(
                    word.start >= previous_end - 0.001,
                    "Wortfolge im Kanal durcheinander: {word:?} nach Ende {previous_end}"
                );
                previous_end = word.end;
            }
            // Echte, UNGLEICHE Zeiten statt gleichverteilter Schaetzung: "eins"
            // und "zwei" liegen buendig hintereinander, zwischen "zwei" und
            // "drei" liegt eine echte, deutlich groessere Pause -- exakt die
            // Lueckenstruktur der Anbieter-Antwort, nicht platt interpoliert.
            let gap_eins_zwei = words[1].start - words[0].end;
            let gap_zwei_drei = words[2].start - words[1].end;
            assert!(
                gap_eins_zwei.abs() < 0.01,
                "eins/zwei sollten buendig sein: {words:?}"
            );
            assert!(
                gap_zwei_drei > 0.03,
                "die echte Pause zwischen zwei/drei muss erhalten bleiben: {words:?}"
            );
        }
        assert!(
            requests.load(Ordering::SeqCst) >= 2,
            "mindestens ein Paket je Kanal"
        );
        assert!(
            saw_format.lock().unwrap().iter().all(|seen| *seen),
            "jede Anfrage fuer ein Wortzeit-Modell muss verbose_json anfordern"
        );
    }

    /// Ein Modell AUSSERHALB der Liste (wie `openai/gpt-transcribe`, HTTP 400
    /// auf `verbose_json` gemessen): kein `response_format` im Formular, alter
    /// Weg mit geschaetzten Zeiten -- und explizit nur EIN Aufruf je Paket,
    /// kein zweiter (teurer) Rueckfall-Versuch.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn openrouter_ohne_wortzeit_modell_bleibt_beim_alten_weg_ein_aufruf_je_paket() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, requests, saw_format) = fake_openrouter_with_words().await;

        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);

        let output = crate::batch::run_batch(
            Arc::new(SilentRuntime),
            openrouter_params(file.path(), base_url, "openai/gpt-transcribe"),
        )
        .await
        .unwrap();

        assert_eq!(
            output.response.metadata["timing_source"], "provider_segment_interpolated",
            "ohne belegte Wortzeit-Zusage bleibt es beim geschaetzten Weg"
        );
        let calls = requests.load(Ordering::SeqCst);
        let seen = saw_format.lock().unwrap().clone();
        assert_eq!(
            seen.len(),
            calls as usize,
            "ein HTTP-Aufruf je Paket, kein zweiter Rueckfall-Versuch auf verbose_json"
        );
        assert!(
            seen.iter().all(|seen| !seen),
            "ein Modell ausserhalb der Liste darf nie verbose_json anfragen"
        );
    }

    // --- Grosse Pakete bei echten Wortzeiten (ZICK-330 Nachtrag, 28.09.2026) ---

    /// Drei Sprech-Abschnitte je Kanal, 40 s auseinander -- eine Luecke
    /// groesser als das alte 29,5-s-Fenster, aber winzig gegen die neuen
    /// 5 Minuten. Links DE, rechts EN (verschiedene Quellen, sonst greift die
    /// Gleiche-Quelle-Erkennung). Gibt je Kanal die drei (Start, Ende)-Zeiten
    /// zurueck, an denen die Bursts PLATZIERT wurden (die reale, von der VAD
    /// erkannte Sprechzeit kann davon um bis zu ~0,5 s abweichen, siehe
    /// `beide_kanaele_kommen_getrennt_und_zeitlich_richtig_an` oben).
    fn write_three_bursts(path: &Path) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) {
        let left_clip = read_mono(anlg_fixtures::SPEECH_DE_WAV);
        let right_clip = read_mono(anlg_fixtures::SPEECH_EN_WAV);
        let rate = f64::from(TARGET_SAMPLE_RATE);
        let starts = [0.0f64, 40.0, 80.0];
        let longest_clip = left_clip.len().max(right_clip.len());
        let total = (starts[2] * rate) as usize + longest_clip + TARGET_SAMPLE_RATE as usize;

        let mut left = vec![0.0f32; total];
        let mut right = vec![0.0f32; total];
        let mut left_times = Vec::with_capacity(starts.len());
        let mut right_times = Vec::with_capacity(starts.len());
        for &start in &starts {
            let start_sample = (start * rate) as usize;
            left[start_sample..start_sample + left_clip.len()].copy_from_slice(&left_clip);
            left_times.push((start, start + left_clip.len() as f64 / rate));
            right[start_sample..start_sample + right_clip.len()].copy_from_slice(&right_clip);
            right_times.push((start, start + right_clip.len() as f64 / rate));
        }

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: TARGET_SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for index in 0..total {
            for value in [left[index], right[index]] {
                writer
                    .write_sample((value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
                    .unwrap();
            }
        }
        writer.finalize().unwrap();

        (left_times, right_times)
    }

    async fn prepared_channels(path: &str, max_samples: usize) -> PreparedChannels {
        let handle = tokio::runtime::Handle::current();
        let path = path.to_string();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_channel_bundles(&path, max_samples, &handle, || false, || {}, None)
        })
        .await
        .unwrap()
        .unwrap();
        match prepared {
            Preparation::Split(prepared) => prepared,
            Preparation::KeepWholeFile => panic!("erwarte den Kanal-Weg, nicht die ganze Datei"),
        }
    }

    /// Direkt an `prepare_channel_bundles` gemessen (kein Netz, kein Zufall):
    /// dieselbe Aufnahme braucht mit Wortzeit-Zusage deutlich weniger Pakete
    /// als mit dem alten ~30-s-Fenster -- und jedes neue Paket bleibt unter
    /// der 25-MB-Grenze von OpenRouter.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deutlich_weniger_pakete_bei_echten_wortzeiten() {
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_three_bursts(file.path());
        let path = file.path().to_str().unwrap();

        let old = prepared_channels(path, bundle_max_samples(None, false)).await;
        let new = prepared_channels(path, bundle_max_samples(None, true)).await;

        assert_eq!(
            old.jobs.len(),
            6,
            "drei Abschnitte je Kanal, je 40 s auseinander (> 29,5 s): drei Pakete je Kanal am alten Fenster: {:?}",
            old.jobs
                .iter()
                .map(|job| job.channel_index)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            new.jobs.len(),
            2,
            "alle drei Abschnitte je Kanal passen in EIN Paket (5-Minuten-Fenster)"
        );
        assert!(
            old.jobs.len() >= new.jobs.len() * 3,
            "deutlich weniger Pakete: alt {}, neu {}",
            old.jobs.len(),
            new.jobs.len()
        );

        for job in &new.jobs {
            let bytes = std::fs::metadata(&job.path).unwrap().len();
            assert!(
                bytes < 25 * 1024 * 1024,
                "Paket ({bytes} Byte, Kanal {}) muss unter der 25-MB-Grenze von OpenRouter bleiben",
                job.channel_index
            );
        }
    }

    /// Zeitachse: innerhalb EINES grossen Pakets mit mehreren Abschnitten
    /// (drei Bursts, je 40 s auseinander -- weit ausserhalb dessen, was das
    /// alte Fenster je in EIN Paket gepackt haette) landet jedes Wort auf dem
    /// ECHTEN Anker seines Abschnitts, nicht auf der Buendel-lokalen Zeit des
    /// Anbieters. Nutzt die REAL gemessenen Mitgliedsgrenzen aus
    /// `prepare_channel_bundles` -- kein geschaetzter Zeitpunkt, kein Rateraten
    /// ueber VAD-Feinheiten.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wortzeit_paket_verankert_mehrere_abschnitte_richtig() {
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_three_bursts(file.path());
        let path = file.path().to_str().unwrap();

        let prepared = prepared_channels(path, bundle_max_samples(None, true)).await;
        assert_eq!(prepared.jobs.len(), 2, "ein Paket je Kanal");

        let job = prepared
            .jobs
            .iter()
            .find(|job| job.channel_index == 0)
            .expect("Mikrofonkanal muss ein Paket haben");
        assert!(
            job.bundle.members.len() >= 3,
            "drei 40-s-auseinanderliegende Abschnitte muessen mindestens drei Mitglieder ergeben: {}",
            job.bundle.members.len()
        );

        // Dieselbe Kumulierung wie `SoniqoChunkBundle::member_local_starts`
        // (dort privat) -- Buendel-lokale Startzeit jedes Mitglieds.
        let rate = f64::from(TARGET_SAMPLE_RATE);
        let mut local_starts = Vec::with_capacity(job.bundle.members.len());
        let mut cursor = 0usize;
        for (index, member) in job.bundle.members.iter().enumerate() {
            if index > 0 {
                cursor += SONIQO_BUNDLE_SEAM_SAMPLES;
            }
            local_starts.push(cursor);
            cursor += member.speech_samples;
        }

        let first = job.bundle.members.first().unwrap();
        let last = job.bundle.members.last().unwrap();
        let last_index = job.bundle.members.len() - 1;
        let words = vec![
            TranscriptWord {
                text: "eins".to_string(),
                start_seconds: local_starts[0] as f64 / rate + 0.05,
                end_seconds: local_starts[0] as f64 / rate + 0.15,
                confidence: None,
            },
            TranscriptWord {
                text: "zwei".to_string(),
                start_seconds: local_starts[last_index] as f64 / rate + 0.05,
                end_seconds: local_starts[last_index] as f64 / rate + 0.15,
                confidence: None,
            },
        ];

        let chunks = soniqo_bundle_transcript_chunks_from_words(&job.bundle, "eins zwei", &words)
            .expect("die Wortliste passt zum Text");

        assert_eq!(chunks.len(), 2, "je ein Abschnitt fuer 'eins' und 'zwei'");
        assert!(
            (chunks[0].start_seconds - first.sample_start as f64 / rate).abs() < 1e-6,
            "'eins' muss auf dem ECHTEN Anker des ersten Mitglieds landen: {} vs {}",
            chunks[0].start_seconds,
            first.sample_start as f64 / rate
        );
        assert!(
            (chunks[1].start_seconds - last.sample_start as f64 / rate).abs() < 1e-6,
            "'zwei' muss auf dem ECHTEN Anker des letzten Mitglieds landen, nicht auf der Buendel-lokalen Zeit: {} vs {}",
            chunks[1].start_seconds,
            last.sample_start as f64 / rate
        );
        // Der Beleg fuer die Naht: der Abstand zwischen den beiden Ankern ist
        // die echte Luecke der Aufnahme (mindestens die 40 s minus eine
        // Burst-Laenge), nicht die kleine Buendel-Naht (0,25 s).
        assert!(
            chunks[1].start_seconds - chunks[0].start_seconds > 30.0,
            "Abstand muss die echte Aufnahmeluecke sein, nicht die Buendel-Naht: {} vs {}",
            chunks[0].start_seconds,
            chunks[1].start_seconds
        );
    }

    /// Derselbe Fall (drei Bursts, 40 s auseinander) am ECHTEN Einstieg
    /// (`run_batch` mit Fake-OpenRouter-Server): weniger Anfragen als Pakete
    /// am alten Fenster, `timing_source` bleibt `provider_word`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn echter_einstieg_buendelt_drei_abschnitte_in_ein_paket_je_kanal() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, requests, saw_format) = fake_openrouter_with_words().await;

        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_three_bursts(file.path());

        let output = crate::batch::run_batch(
            Arc::new(SilentRuntime),
            openrouter_params(file.path(), base_url, "google/gemini-3.5-transcribe"),
        )
        .await
        .unwrap();

        assert_eq!(
            requests.load(Ordering::SeqCst),
            2,
            "ein Paket je Kanal statt drei -- die alten 29,5-s-Pakete haetten je \
             Kanal drei Anfragen gebraucht (drei Abschnitte, 40 s auseinander)"
        );
        assert_eq!(
            output.response.metadata["timing_source"], "provider_word",
            "echte Wortzeiten des Anbieters"
        );
        assert!(
            saw_format.lock().unwrap().iter().all(|seen| *seen),
            "beide Pakete fragen verbose_json an"
        );
    }

    /// Die App startet `run_batch` mit `tokio::spawn`
    /// (`plugins/transcription/src/listener2/ext.rs`); ohne `Send` baut sie
    /// nicht. Dieser Test baut nur, wenn der Future `Send` ist.
    #[test]
    fn run_batch_future_ist_send() {
        fn assert_send<T: Send>(_: T) {}
        let path = Path::new("/nicht/vorhanden.wav");
        assert_send(crate::batch::run_batch(
            Arc::new(SilentRuntime),
            params(path),
        ));
    }

    /// Traegt den Hinweis an alten Cloud-Transkripten: nur wenn ein neuer Lauf
    /// wirklich getrennt zurueckkaeme.
    #[test]
    fn hinweis_nur_wenn_neu_transkribieren_wirklich_trennt() {
        let available = crate::batch::cloud_channel_split_available;
        let dialog = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(dialog.path(), true);
        let dialog = dialog.path().to_str().unwrap();

        assert_eq!(
            available("openai", Some("gpt-transcribe"), dialog),
            Ok(true)
        );
        assert_eq!(available("openrouter", None, dialog), Ok(true));
        // Anbieter mit eigener Trennung, Diarize-Modell, lokale Wege: nie.
        assert_eq!(available("soniox", None, dialog), Ok(false));
        assert_eq!(
            available("openai", Some("gpt-4o-transcribe-diarize"), dialog),
            Ok(false)
        );
        assert_eq!(available("soniqo", None, dialog), Ok(false));
        assert_eq!(available("unbekannt", None, dialog), Ok(false));

        // Raumaufnahme (Systemton still) und importiertes Stereo einer Quelle.
        let room = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(room.path(), false);
        assert_eq!(
            available("openai", None, room.path().to_str().unwrap()),
            Ok(false)
        );
        let imported = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_correlated_stereo(imported.path());
        assert_eq!(
            available("openai", None, imported.path().to_str().unwrap()),
            Ok(false)
        );
    }

    /// Fake-Trenner: zwei Stimmen, geteilt in der Mitte der Sprache auf dem
    /// Systemkanal; oder er wirft. Merkt sich, was er bekam.
    struct FakeDiarizer {
        split_at_seconds: f64,
        fail: bool,
        seen: Mutex<Vec<(usize, Option<usize>)>>,
    }

    impl super::super::system_diarization::SystemDiarizer for FakeDiarizer {
        fn available(&self) -> bool {
            true
        }

        fn diarize(
            &self,
            samples: &[f32],
            speaker_count: Option<usize>,
            _keep_going: &mut dyn FnMut() -> bool,
        ) -> std::result::Result<Vec<anlg_transcribe_soniqo::DiarizationSegment>, String> {
            self.seen
                .lock()
                .unwrap()
                .push((samples.len(), speaker_count));
            if self.fail {
                return Err("Modell fehlt".to_string());
            }
            let end = samples.len() as f64 / f64::from(TARGET_SAMPLE_RATE);
            Ok(vec![
                anlg_transcribe_soniqo::DiarizationSegment {
                    start_seconds: 0.0,
                    end_seconds: self.split_at_seconds,
                    speaker_index: 0,
                },
                anlg_transcribe_soniqo::DiarizationSegment {
                    start_seconds: self.split_at_seconds,
                    end_seconds: end,
                    speaker_index: 1,
                },
            ])
        }
    }

    /// Setzt den Fake fuer die Dauer eines Tests und raeumt danach auf.
    struct DiarizerGuard;
    impl DiarizerGuard {
        fn set(fake: Arc<FakeDiarizer>) -> Self {
            super::super::system_diarization::set_test_diarizer(Some(fake));
            Self
        }
    }
    impl Drop for DiarizerGuard {
        fn drop(&mut self) {
            super::super::system_diarization::set_test_diarizer(None);
        }
    }

    fn remote_speakers(output: &BatchRunOutput) -> std::collections::BTreeSet<Option<usize>> {
        output.response.results.channels[1].alternatives[0]
            .words
            .iter()
            .map(|word| word.speaker)
            .collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gruppen_call_trennt_die_gegenueber_auf_dem_systemkanal() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, _requests) = fake_groq().await;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let (_, right_start) = write_dialog(file.path(), true);
        let right_len =
            read_mono(anlg_fixtures::SPEECH_EN_WAV).len() as f64 / f64::from(TARGET_SAMPLE_RATE);
        let fake = Arc::new(FakeDiarizer {
            split_at_seconds: right_start + right_len / 2.0,
            fail: false,
            seen: Mutex::new(Vec::new()),
        });
        let _guard = DiarizerGuard::set(fake.clone());

        // Vier Teilnehmer: Nutzer plus drei Gegenueber.
        let params = BatchParams {
            num_speakers: Some(4),
            ..groq_params(file.path(), base_url)
        };
        let output = crate::batch::run_batch(Arc::new(SilentRuntime), params)
            .await
            .unwrap();

        let seen = fake.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "genau ein Trennungslauf: {seen:?}");
        let total = hound::WavReader::open(file.path()).unwrap().duration() as usize;
        assert_eq!(
            seen[0].0, total,
            "die GANZE Systemkanal-Datei, nicht ein Paket"
        );
        assert_eq!(seen[0].1, Some(3), "Teilnehmer minus Nutzer");

        let speakers = remote_speakers(&output);
        assert!(
            speakers.contains(&Some(0)) && speakers.contains(&Some(1)),
            "zwei Gegenueber auf Kanal 1: {speakers:?}"
        );
        assert!(
            output.response.results.channels[0].alternatives[0]
                .words
                .iter()
                .all(|word| word.channel == 0),
            "Mikrofon bleibt der Nutzer"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scheiternde_trennung_laesst_ein_gegenueber_und_keinen_fehler() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, _requests) = fake_groq().await;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);
        let fake = Arc::new(FakeDiarizer {
            split_at_seconds: 0.0,
            fail: true,
            seen: Mutex::new(Vec::new()),
        });
        let _guard = DiarizerGuard::set(fake.clone());

        let params = BatchParams {
            num_speakers: Some(4),
            ..groq_params(file.path(), base_url)
        };
        let output = crate::batch::run_batch(Arc::new(SilentRuntime), params)
            .await
            .expect("eine gescheiterte Trennung darf den Lauf nicht kosten");

        assert_eq!(
            fake.seen.lock().unwrap().len(),
            1,
            "Trennung wurde versucht"
        );
        assert_eq!(output.response.results.channels.len(), 2);
        assert!(
            remote_speakers(&output).len() <= 1,
            "ein Gegenueber wie bisher: {:?}",
            remote_speakers(&output)
        );
    }

    /// Fake, der misst, wie viele Trennungen gleichzeitig laufen.
    struct OverlapDiarizer {
        active: std::sync::atomic::AtomicUsize,
        max_active: std::sync::atomic::AtomicUsize,
    }

    impl super::super::system_diarization::SystemDiarizer for OverlapDiarizer {
        fn available(&self) -> bool {
            true
        }

        fn diarize(
            &self,
            _samples: &[f32],
            _speaker_count: Option<usize>,
            _keep_going: &mut dyn FnMut() -> bool,
        ) -> std::result::Result<Vec<anlg_transcribe_soniqo::DiarizationSegment>, String> {
            let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(150));
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(Vec::new())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn hoechstens_eine_trennung_gleichzeitig() {
        // Wie eine resampelte Kanal-Datei: mono, 32-bit float, 16 kHz.
        let dialog = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let mut writer = hound::WavWriter::create(
            dialog.path(),
            hound::WavSpec {
                channels: 1,
                sample_rate: TARGET_SAMPLE_RATE,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for _ in 0..TARGET_SAMPLE_RATE {
            writer.write_sample(0.1f32).unwrap();
        }
        writer.finalize().unwrap();
        let fake = Arc::new(OverlapDiarizer {
            active: std::sync::atomic::AtomicUsize::new(0),
            max_active: std::sync::atomic::AtomicUsize::new(0),
        });
        let handles = (0..3)
            .map(|_| {
                let copy = tempfile::NamedTempFile::new().unwrap();
                std::fs::copy(dialog.path(), copy.path()).unwrap();
                spawn_system_diarization(
                    fake.clone(),
                    SystemDiarizationJob {
                        channel_index: 1,
                        file: copy,
                        speaker_count: Some(2),
                    },
                    Arc::new(SilentRuntime),
                    Arc::new(std::sync::atomic::AtomicBool::new(false)),
                )
            })
            .collect::<Vec<_>>();
        for handle in handles {
            let (_, result) = handle.await.unwrap();
            assert!(result.is_ok(), "{result:?}");
        }
        assert_eq!(fake.max_active.load(Ordering::SeqCst), 1);
    }

    struct MemoryGuard;
    impl MemoryGuard {
        fn set(bytes: u64) -> Self {
            super::super::system_diarization::set_test_memory_bytes(Some(bytes));
            Self
        }
    }
    impl Drop for MemoryGuard {
        fn drop(&mut self) {
            super::super::system_diarization::set_test_memory_bytes(None);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unter_16_gb_keine_trennung_aber_ein_hinweis() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, _requests) = fake_groq().await;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);
        let fake = Arc::new(FakeDiarizer {
            split_at_seconds: 1.0,
            fail: false,
            seen: Mutex::new(Vec::new()),
        });
        let _guard = DiarizerGuard::set(fake.clone());
        let _memory = MemoryGuard::set(8 * 1024 * 1024 * 1024);

        let params = BatchParams {
            num_speakers: Some(4),
            ..groq_params(file.path(), base_url)
        };
        let output = crate::batch::run_batch(Arc::new(SilentRuntime), params)
            .await
            .unwrap();

        assert!(fake.seen.lock().unwrap().is_empty(), "8 GB: keine Trennung");
        assert_eq!(output.response.results.channels.len(), 2);
        assert_eq!(
            output.response.metadata["system_diarization_skipped"],
            "insufficient_memory"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn eins_zu_eins_wird_nie_getrennt() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (base_url, _requests) = fake_groq().await;
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        write_dialog(file.path(), true);
        let fake = Arc::new(FakeDiarizer {
            split_at_seconds: 1.0,
            fail: false,
            seen: Mutex::new(Vec::new()),
        });
        let _guard = DiarizerGuard::set(fake.clone());

        // Zwei Teilnehmer (Nutzer + einer) und ohne Teilnehmerliste: keine
        // Trennung, sonst wuerde aus einer Stimme zwei.
        for num_speakers in [Some(2), None] {
            let params = BatchParams {
                num_speakers,
                ..groq_params(file.path(), base_url.clone())
            };
            crate::batch::run_batch(Arc::new(SilentRuntime), params)
                .await
                .unwrap();
        }
        assert!(fake.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn stilles_paket_erzeugt_keine_anfrage() {
        let rate = TARGET_SAMPLE_RATE as usize;
        let mut bundler = GatedBundler::new(bundle_max_samples(None, false));
        // Reine Stille: das Modell wuerde "Vielen Dank." erfinden.
        assert!(
            bundler
                .push(AudioChunk {
                    samples: vec![0.0; rate],
                    sample_start: 0,
                    sample_end: rate,
                })
                .is_none()
        );
        // Knapp unter der Schwelle (Stille gemessen ~0,00012).
        assert!(
            bundler
                .push(AudioChunk {
                    samples: vec![0.0005; rate],
                    sample_start: rate,
                    sample_end: 2 * rate,
                })
                .is_none()
        );
        assert!(
            bundler.finish().is_none(),
            "ein Paket aus Stille darf nicht entstehen"
        );

        assert!(
            bundler
                .push(AudioChunk {
                    samples: vec![0.05; rate],
                    sample_start: 2 * rate,
                    sample_end: 3 * rate,
                })
                .is_none()
        );
        let bundle = bundler.finish().expect("Sprache ergibt ein Paket");
        assert_eq!(bundle.members.len(), 1);
        assert_eq!(bundle.members[0].sample_start, 2 * rate);
    }

    #[test]
    fn paket_bleibt_unter_der_grenze_des_anbieters() {
        let zai = BatchUploadLimit {
            max_bytes: u64::MAX,
            max_duration: Duration::from_secs(25),
        };
        assert!(bundle_max_samples(Some(zai), false) < 25 * TARGET_SAMPLE_RATE as usize);
        // Ohne Zusage aendert eine Wortzeit-Anfrage nichts: das lokale Fenster
        // bleibt die engere Grenze.
        assert_eq!(
            bundle_max_samples(None, false),
            SONIQO_PARAKEET_MAX_CHUNK_SAMPLES
        );

        // Mit Zusage: 5 Minuten, aber weiterhin sicher unter der Anbieter-
        // Zeitspanne UND -Byte-Grenze -- hier an den echten OpenRouter-Werten
        // (`AdapterKind::batch_upload_limit`, 25 MB / 10 min).
        let samples = bundle_max_samples(None, true);
        assert_eq!(samples, CLOUD_WORD_TIMESTAMP_BUNDLE_MAX_SAMPLES);
        assert!(
            samples as f64 / TARGET_SAMPLE_RATE as f64 > 29.5,
            "deutlich groesser als der alte Deckel: {samples}"
        );
        assert!(
            (samples as f64 / TARGET_SAMPLE_RATE as f64 - 300.0).abs() < 1.0,
            "rund 5 Minuten: {samples}"
        );

        let openrouter_limit = owhisper_client::AdapterKind::OpenRouter
            .batch_upload_limit(Some("google/gemini-3.5-transcribe"))
            .expect("OpenRouter hat eine Obergrenze");
        let bounded = bundle_max_samples(Some(openrouter_limit), true);
        let worst_case_bytes = bounded as u64 * 2; // 16-Bit-PCM-Mono, siehe `write_mono_wav`.
        assert!(
            worst_case_bytes < openrouter_limit.max_bytes,
            "Paket ({worst_case_bytes} Byte) muss unter der 25-MB-Grenze bleiben"
        );
        assert!(
            Duration::from_secs_f64(bounded as f64 / TARGET_SAMPLE_RATE as f64)
                < openrouter_limit.max_duration,
            "Paket muss unter der 10-Minuten-Zeitspanne bleiben"
        );
        // Bei diesen konkreten Werten sticht weiterhin unser eigener 5-Minuten-
        // Deckel, nicht die (grosszuegigere) Anbietergrenze.
        assert_eq!(bounded, CLOUD_WORD_TIMESTAMP_BUNDLE_MAX_SAMPLES);

        // Eine enge Byte-Grenze (kleiner als unser 5-Minuten-Deckel in Bytes)
        // muss trotzdem greifen -- nicht nur die Zeitspanne.
        let tight_bytes = BatchUploadLimit {
            max_bytes: 4 * 1024 * 1024,
            max_duration: Duration::from_secs(20 * 60),
        };
        let tight = bundle_max_samples(Some(tight_bytes), true);
        assert!(
            tight < CLOUD_WORD_TIMESTAMP_BUNDLE_MAX_SAMPLES,
            "die engere Byte-Grenze muss den 5-Minuten-Deckel unterbieten: {tight}"
        );
        assert!((tight as u64 * 2) < tight_bytes.max_bytes);
    }

    #[test]
    fn kanal_weg_nur_fuer_anbieter_ohne_eigene_sprecher() {
        use owhisper_client::AdapterKind;
        let splits = super::super::adapter_splits_channels;
        let params =
            |model: Option<&str>, num_speakers: Option<u32>| owhisper_interface::ListenParams {
                model: model.map(str::to_string),
                num_speakers,
                ..Default::default()
            };

        // Drin: Text-Anbieter ohne Sprecher, mit und ohne Sprecherzahl.
        for kind in [
            AdapterKind::OpenAI,
            AdapterKind::OpenRouter,
            AdapterKind::Groq,
            AdapterKind::Together,
            AdapterKind::SiliconFlow,
            AdapterKind::Zai,
            AdapterKind::Fireworks,
            AdapterKind::Cohere,
            AdapterKind::AquaVoice,
            AdapterKind::AwsTranscribe,
        ] {
            assert!(
                splits(&kind, &params(Some("gpt-transcribe"), Some(3))),
                "{kind} fehlt"
            );
            assert!(
                splits(&kind, &params(None, None)),
                "{kind} ohne Modell fehlt"
            );
        }

        // xAI nur ohne Sprecherzahl (sonst fordert es selbst Diarisierung an).
        assert!(splits(&AdapterKind::Xai, &params(None, None)));
        assert!(!splits(&AdapterKind::Xai, &params(None, Some(3))));
        assert!(!splits(
            &AdapterKind::Xai,
            &owhisper_interface::ListenParams {
                max_speakers: Some(4),
                ..Default::default()
            }
        ));

        // Draussen: eigene Mehrkanal-Spur oder eigene Sprechertrennung.
        for kind in [
            AdapterKind::Deepgram,
            AdapterKind::Soniox,
            AdapterKind::Pyannote,
            AdapterKind::Cartesia,
            AdapterKind::Gladia,
            AdapterKind::ElevenLabs,
            AdapterKind::AssemblyAI,
            AdapterKind::Speechmatics,
            AdapterKind::AzureSpeech,
            AdapterKind::GoogleCloud,
            AdapterKind::GoogleGenerativeAi,
            AdapterKind::Mistral,
            AdapterKind::RevAi,
            AdapterKind::LocalServer,
            AdapterKind::DashScope,
            AdapterKind::Anarlog,
        ] {
            assert!(
                !splits(&kind, &params(None, None)),
                "{kind} darf nicht zerlegt werden"
            );
        }

        // Ein Diarize-Modell liefert Sprecher, auch ueber OpenAI/OpenRouter.
        assert!(!splits(
            &AdapterKind::OpenAI,
            &params(Some("gpt-4o-transcribe-diarize"), None)
        ));
        assert!(!splits(
            &AdapterKind::OpenRouter,
            &params(Some("openai/gpt-4o-transcribe-diarize"), None)
        ));
    }
}
