use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{Stream, StreamExt};
use owhisper_interface::batch_stream::BatchStreamEvent;
use tracing::Instrument;

use anlg_audio_chunking::{AudioChunk, SpeechChunkExt, SpeechChunkingConfig};
use anlg_audio_utils::Source;
use anlg_transcribe_core::TARGET_SAMPLE_RATE;

use super::super::{
    BatchParams, BatchRunMode, BatchRunOutput, format_user_friendly_error, session_span,
};
use crate::{BatchEvent, BatchRuntime};

pub(super) const SONIQO_PARAKEET_MAX_CHUNK_SAMPLES: usize = TARGET_SAMPLE_RATE as usize * 59 / 2;
/// Ab welcher Kanallaenge die Sprechertrennung ausgelassen wird.
///
/// **ACHTUNG, die Bedeutung hat sich am 10.09.2026 geaendert.** Bis dahin war
/// dieser Wert der Absturzschutz und stand bei 120 Minuten. Seitdem wird ein
/// langer Kanal ZERLEGT (siehe [`SONIQO_DIARIZATION_CHUNK_SAMPLES`]), und die
/// groesste einzelne Zuteilung haengt nur noch an der Abschnittslaenge, nicht
/// mehr an der Aufnahmelaenge. Dieser Deckel ist damit kein Absturzschutz
/// mehr, sondern eine Speichergrenze: der GANZE Kanal liegt beim Trennen als
/// `Vec<f32>` im Speicher, unabhaengig davon, in wie vielen Stuecken er
/// gerechnet wird.
///
/// **Korrektur 21.09.2026: von zwoelf auf vier Stunden.** Zwoelf Stunden
/// waren gegriffen, nicht gerechnet. Der Puffer allein betraegt dort
/// 691.200.000 Samples mal 4 Byte = **2,76 GB**, und zwar ZUSAETZLICH zu der
/// am 21.09. gemessenen Spitze von 5,67 GB fuer 122,8 Minuten -- auf einem
/// 24-GB-Rechner, auf dem nebenher die App, das Modell und die Oberflaeche
/// liegen. Vier Stunden kosten als Puffer **0,92 GB** und sind damit
/// vertretbar.
///
/// Was hier gemessen und was gerechnet ist, ausdruecklich getrennt:
/// **gemessen** sind 122,8 Minuten am echten Material (358,1 s, Spitze
/// 5,67 GB, 21.09.2026); **gerechnet** ist der Pufferbedarf laengerer Kanaele
/// (Samples mal 4 Byte). Eine Vier-Stunden-Aufnahme ist nie gelaufen. Der
/// Wert ist die groesste Laenge, deren PUFFER unter einem Gigabyte bleibt --
/// nicht die groesste, die nachweislich durchlaeuft.
///
/// Die folgende Messreihe beschreibt daher den ALTEN, ungeteilten Lauf. Sie
/// steht weiter hier, weil sie begruendet, warum 45 Minuten je Abschnitt
/// konservativ sind -- und weil sie zeigt, was passiert, wenn jemand das
/// Zerlegen wieder ausbaut.
///
/// Gemessen am 07.09.2026 auf dem Rechner des Betreibers (24 GB, `examples/diarize_probe`
/// mit `/usr/bin/time -l`, Kanal 0 des Vor-Ort-Termins, zur Verlaengerung an
/// sich selbst gehaengt):
///
/// | Laenge | Ergebnis | Spitzenspeicher |
/// |---|---|---|
/// | 1 min | laeuft | 0,60 GB |
/// | 10 min | laeuft | 1,56 GB |
/// | 30 min | laeuft | 3,13 GB |
/// | 101 min | laeuft | 5,74 GB |
/// | **120 min** | **laeuft** | **4,82 GB** |
/// | 150 min | **Absturz** | 5,07 GB |
/// | 203 min | **Absturz** | 8,45 GB |
///
/// **WOHER DIESE ZAHLEN STAMMEN, und was sie NICHT beweisen (08.09.2026).**
/// Die Reihe kommt aus `examples/diarize_probe`, gebaut mit
/// `cargo run --release`. Die App wird im Alltag als `--debug` gebaut, und
/// swift-rs hat daran bis zum 08.09. auch die SWIFT-Bauart aufgehaengt --
/// die Messreihe beschrieb also einen Bau, den die App gar nicht hatte.
/// Seit `crates/transcribe-soniqo/build.rs` die Swift-Seite immer in
/// `release` uebersetzt (siehe dort `swift_configuration`), stimmt die
/// Bauart wieder ueberein, und die Reihe gilt fuer den echten Pfad.
///
/// Zwei Unterschiede bleiben und sind mit dieser Reihe NICHT gedeckt:
/// der Probelauf haelt nur die Trennung im Speicher, die App zusaetzlich
/// Parakeet, die Oberflaeche und die Kanaldateien; und die 150/203-Minuten-
/// Abstuerze wurden nie in der App nachgestellt. Wer den Deckel anhebt,
/// misst in der APP nach, nicht im Probelauf.
///
/// Nachgemessen am 08.09. mit Swift in `release`, gleicher Kanal:
/// 101 min laufen durch, 315,7 s, Spitze 6,48 GB -- die 5,74 GB oben sind
/// also eher die untere als die obere Kante desselben Falls.
///
/// Der Absturz ist keine Fehlermeldung, die irgendwo ankommt: CoreML wirft
/// `NSGenericException -- Failed to allocate E5 buffer object` als
/// Objective-C-Ausnahme, und die faengt weder das `do/catch` auf der
/// Swift-Seite noch das `Result` auf der Rust-Seite. `libc++abi` beendet den
/// PROZESS. Ohne diesen Deckel stirbt also die App, wenn jemand einen
/// Ganztagsworkshop transkribieren will.
///
/// 120 Minuten ist die groesste Laenge, die gemessen durchlief. Zwei Dinge
/// stehen dazu ehrlich daneben: es ist die letzte bestandene Messung und
/// nicht ein Wert mit Sicherheitsabstand, und die Grenze haengt nicht am
/// Gesamtspeicher (150 min stirbt bei WENIGER Spitzenspeicher als 101 min
/// durchlaeuft), sondern an einer einzelnen IOSurface-Zuteilung -- unter Last
/// kann sie also frueher liegen. Wer den Wert anhebt, misst vorher neu.
pub(super) const SONIQO_DIARIZATION_MAX_SAMPLES: usize = TARGET_SAMPLE_RATE as usize * 4 * 60 * 60;

/// Laenge EINES Trennungs-Abschnitts.
///
/// **Seit dem 10.09.2026 wird ein langer Kanal zerlegt statt uebergangen.**
/// Anlass war eine echte Raumaufnahme vom selben Tag: 122,8 Minuten, vier
/// Personen an einem Mikrofon -- genau der Fall, in dem die Trennung am
/// meisten wert ist -- und der 120-Minuten-Deckel darueber. Er hat ihn um
/// 2 Minuten und 47 Sekunden gerissen und die komplette Sprecherzuordnung
/// eines Zwei-Stunden-Gespraechs verloren.
///
/// **Warum zerlegen und nicht den Deckel anheben.** Der Deckel war richtig
/// begruendet (die Messreihe steht oben): oberhalb von etwa 150 Minuten
/// stirbt der Prozess an einer CoreML-Zuteilung, die niemand abfangen kann.
/// Wer ihn anhebt, verschiebt diese Klippe nur -- und sie haengt nicht am
/// Gesamtspeicher, sondern an einer einzelnen Zuteilung, liegt unter Last
/// also frueher. Ein zerlegter Lauf haelt die groesste Zuteilung dagegen
/// KONSTANT, unabhaengig davon, wie lang die Aufnahme ist. Die Klippe wird
/// damit unerreichbar statt weiter weg.
///
/// Zerlegen ist ausserdem nicht teurer, sondern billiger: die dominante
/// Stufe (agglomeratives Clustern) waechst quadratisch mit der
/// Abschnittslaenge, zwei halbe Abschnitte kosten also weniger als ein
/// ganzer. Die Messung dazu steht im Commit dieses Laufes.
///
/// 45 Minuten ist bewusst konservativ: es liegt weit unter der letzten
/// bestandenen Messung (120 min) und noch weiter unter der ersten
/// gescheiterten (150 min). Der Preis dafuer ist eine Naht mehr, und Naehte
/// sind die einzige Stelle, an der Sprecher verwechselt werden koennen.
pub(super) const SONIQO_DIARIZATION_CHUNK_SAMPLES: usize = TARGET_SAMPLE_RATE as usize * 45 * 60;

/// Kuerzestes Reststueck, das noch allein getrennt wird.
///
/// Darunter wird es dem vorhergehenden Abschnitt zugeschlagen. Ein
/// Zwei-Minuten-Rest traegt keine belastbare Stimme, und sein Schwerpunkt
/// wuerde beim Zusammenfuehren eher verwechseln als zuordnen.
pub(super) const SONIQO_DIARIZATION_MIN_TAIL_SAMPLES: usize =
    TARGET_SAMPLE_RATE as usize * 5 * 60;
pub(super) const SONIQO_PROGRESS_PLANNED: f64 = 0.05;
/// Ende des Bandes, das der Sprechertrennung gehoert.
///
/// Bis zum 08.09.2026 hatte die Trennung ueberhaupt kein Band: der Balken sprang
/// auf 5 %, und dann geschah minutenlang nichts Sichtbares -- der Prinzipal hat
/// am Abend des 07.09. zweimal abgebrochen, weil eine laufende Trennung von
/// einer haengenden nicht zu unterscheiden war. Die Trennung bekommt jetzt
/// 5 % bis 20 %, die Zerlegung in Bloecke den Rest.
pub(super) const SONIQO_DIARIZATION_PROGRESS_END: f64 = 0.20;
const SONIQO_PROGRESS_RANGE: f64 = SONIQO_PROGRESS_MAX - SONIQO_DIARIZATION_PROGRESS_END;
pub(super) const SONIQO_PROGRESS_MAX: f64 = 0.95;
/// Wie viel schneller als Echtzeit die Sprechertrennung laeuft.
///
/// Gemessen am 08.09.2026 an Kanal 0 der Sitzung vom 07.09. (101 Minuten,
/// Swift in `release`): 315,7 s fuer 6060 s Ton, also 19,2x Echtzeit. Bei
/// kuerzeren Kanaelen ist es mehr (26,3x bei einer Minute), weil der teuerste
/// Schritt quadratisch waechst. Der Wert traegt genau eine
/// Aufgabe -- aus der Kanallaenge eine Erwartung fuer den Fortschrittsbalken zu
/// rechnen. Er ist bewusst vorsichtig (niedriger als gemessen), damit die
/// Schaetzung eher zu langsam als zu schnell laeuft: ein Balken, der vor dem
/// Ende stehen bleibt, ist ehrlicher als einer, der zu frueh voll ist.
const SONIQO_DIARIZATION_REALTIME_FACTOR: f64 = 15.0;
/// Abstand zwischen zwei Fortschritts-Meldungen waehrend der Trennung.
pub(super) const SONIQO_DIARIZATION_HEARTBEAT: Duration = Duration::from_secs(2);
/// Wie weit die Schaetzung innerhalb EINES Kanals hoechstens laufen darf.
///
/// Nie bis 1.0: solange der Kanal laeuft, ist er nicht fertig, und ein Balken,
/// der das Gegenteil behauptet, ist genau die Luege, die hier abgestellt wird.
const SONIQO_DIARIZATION_WITHIN_MAX: f64 = 0.95;
pub(super) const SONIQO_DIRECT_MIC_MIN_RMS: f64 = 0.0008;
pub(super) const MAX_LOCAL_BATCH_CHANNELS: usize = 8;
const SONIQO_SPEECH_REDEMPTION_TIME: Duration = Duration::from_millis(150);
pub(super) const LOCAL_BATCH_CANCELLED: &str = "Local transcription was cancelled.";

#[derive(Debug)]
pub(super) struct ResampledChannelFile {
    pub(super) file: tempfile::NamedTempFile,
    pub(super) sample_count: usize,
    /// Effektivwert (RMS) ueber den GANZEN Kanal. Beantwortet genau eine
    /// Frage: traegt dieser Kanal ueberhaupt Ton, oder ist er tot? Fuer alles
    /// Feinere ist er der falsche Wert -- ein Kanal mit langen Pausen hat
    /// einen niedrigen RMS und traegt trotzdem Sprache.
    pub(super) rms: f64,
}

#[cfg(test)]
pub(super) fn resample_audio_to_channel_files<S>(
    source_path: &str,
    source: S,
) -> std::result::Result<Vec<ResampledChannelFile>, String>
where
    S: Source,
{
    resample_audio_to_channel_files_until(source_path, source, || false)
}

pub(super) fn resample_audio_to_channel_files_until<S, F>(
    source_path: &str,
    source: S,
    mut is_cancelled: F,
) -> std::result::Result<Vec<ResampledChannelFile>, String>
where
    S: Source,
    F: FnMut() -> bool,
{
    if is_cancelled() {
        return Err(LOCAL_BATCH_CANCELLED.to_string());
    }

    let channel_count = u16::from(source.channels()) as usize;
    if channel_count > MAX_LOCAL_BATCH_CHANNELS {
        return Err(format!(
            "Local transcription supports at most {MAX_LOCAL_BATCH_CHANNELS} audio channels; the recording declares {channel_count}."
        ));
    }
    let parent = Path::new(source_path).parent();
    let mut files = (0..channel_count)
        .map(|_| create_channel_tempfile(parent))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writers = files
        .iter()
        .map(|file| {
            file.reopen()
                .map(BufWriter::new)
                .map_err(anlg_audio_utils::Error::from)
                .and_then(|file| hound::WavWriter::new(file, spec).map_err(Into::into))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: anlg_audio_utils::Error| e.to_string())?;
    let mut stereo_difference = 0.0f64;
    let mut stereo_samples = 0usize;
    let mut channel_square_sums = vec![0.0f64; channel_count];

    let info = anlg_audio_utils::for_each_resampled_channel_block::<_, anlg_audio_utils::Error>(
        source,
        TARGET_SAMPLE_RATE,
        |channels| {
            if is_cancelled() {
                return Err(std::io::Error::other(LOCAL_BATCH_CANCELLED).into());
            }
            for ((writer, channel), square_sum) in writers
                .iter_mut()
                .zip(channels)
                .zip(channel_square_sums.iter_mut())
            {
                for sample in *channel {
                    writer.write_sample(*sample)?;
                    *square_sum += f64::from(*sample) * f64::from(*sample);
                }
            }
            if channels.len() == 2 {
                stereo_difference += channels[0]
                    .iter()
                    .zip(channels[1])
                    .map(|(left, right)| f64::from((left - right).abs()))
                    .sum::<f64>();
                stereo_samples += channels[0].len().min(channels[1].len());
            }
            Ok(())
        },
    )
    .map_err(|e| e.to_string())?;

    for writer in writers {
        writer.finalize().map_err(|e| e.to_string())?;
    }

    if is_cancelled() {
        return Err(LOCAL_BATCH_CANCELLED.to_string());
    }

    if files.len() == 2
        && stereo_samples > 0
        && stereo_difference / (stereo_samples as f64) < 0.0005
    {
        files.truncate(1);
    }

    Ok(files
        .into_iter()
        .enumerate()
        .map(|(channel_index, file)| ResampledChannelFile {
            file,
            sample_count: info.frame_count,
            rms: if info.frame_count == 0 {
                0.0
            } else {
                (channel_square_sums[channel_index] / info.frame_count as f64).sqrt()
            },
        })
        .collect())
}

fn create_channel_tempfile(parent: Option<&Path>) -> std::io::Result<tempfile::NamedTempFile> {
    let in_parent = parent.and_then(|parent| {
        tempfile::Builder::new()
            .prefix("anarlog_channel_")
            .suffix(".wav")
            .tempfile_in(parent)
            .ok()
    });
    match in_parent {
        Some(file) => Ok(file),
        None => tempfile::Builder::new()
            .prefix("anarlog_channel_")
            .suffix(".wav")
            .tempfile(),
    }
}

pub(in crate::batch) async fn run_apple_speech_batch(
    runtime: Arc<dyn BatchRuntime>,
    params: BatchParams,
    listen_params: owhisper_interface::ListenParams,
) -> crate::Result<BatchRunOutput> {
    let span = session_span(&params.session_id);

    async {
        let locale = anlg_transcribe_speechanalyzer::resolve_session_locale(
            &listen_params.languages,
        )
        .ok_or_else(|| crate::BatchFailure::DirectRequestFailed {
            provider: "apple-speech".to_string(),
            message:
                "Add this language in System Settings > General > Language & Region to transcribe it with Apple Speech."
                    .to_string(),
        })?;
        let file_path = params.file_path.clone();
        let started_at = Instant::now();

        tracing::info!(
            anarlog.stt.provider.name = "apple-speech",
            anarlog.stt.language = %locale,
            "apple_speech_batch_start"
        );

        let session_id = params.session_id.clone();
        let progress_runtime = runtime.clone();
        let transcribed = tokio::task::spawn_blocking(move || {
            let progress = SoniqoProgressReporter {
                runtime: progress_runtime,
                session_id,
            };
            transcribe_apple_speech_file(&file_path, &locale, Some(&progress))
        })
        .await
        .map_err(|e| crate::BatchFailure::DirectRequestFailed {
            provider: "apple-speech".to_string(),
            message: format!("Apple Speech transcription task failed: {e}"),
        })?
        .map_err(|e| {
            let message = format_user_friendly_error(&e);
            tracing::error!(
                anarlog.stt.provider.name = "apple-speech",
                error = %e,
                anarlog.error.user_message = %message,
                "apple_speech_batch_failed"
            );
            crate::BatchFailure::DirectRequestFailed {
                provider: "apple-speech".to_string(),
                message,
            }
        })?;

        tracing::info!(
            anarlog.stt.provider.name = "apple-speech",
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            transcript.channel_count = transcribed.len(),
            "apple_speech_batch_completed"
        );

        Ok(BatchRunOutput {
            session_id: params.session_id,
            mode: BatchRunMode::Direct,
            response: anlg_transcribe_speechanalyzer::batch_response_from_transcripts(transcribed),
        })
    }
    .instrument(span)
    .await
}

/// Transcribes each recorded channel separately so mic and system audio stay attributable.
fn transcribe_apple_speech_file(
    file_path: &str,
    locale: &str,
    progress: Option<&SoniqoProgressReporter>,
) -> std::result::Result<Vec<anlg_transcribe_speechanalyzer::FileTranscript>, String> {
    ensure_local_batch_running(progress)?;
    let source = anlg_audio_utils::source_from_path(file_path).map_err(|e| e.to_string())?;
    let channels = resample_audio_to_channel_files_until(file_path, source, || {
        local_batch_is_cancelled(progress)
    })?;
    ensure_local_batch_running(progress)?;

    if let Some(progress) = progress {
        progress.emit(SONIQO_PROGRESS_PLANNED);
    }

    let total = channels.len().max(1);
    let mut transcripts = Vec::with_capacity(channels.len());

    for (index, channel) in channels.into_iter().enumerate() {
        ensure_local_batch_running(progress)?;
        let transcript =
            anlg_transcribe_speechanalyzer::transcribe_file(channel.file.path(), locale)
                .map_err(|e| e.to_string())?;
        ensure_local_batch_running(progress)?;
        transcripts.push(transcript);

        if let Some(progress) = progress {
            progress.emit(soniqo_batch_progress(index + 1, total));
        }
    }

    Ok(transcripts)
}

pub(in crate::batch) async fn run_soniqo_batch(
    runtime: Arc<dyn BatchRuntime>,
    params: BatchParams,
    listen_params: owhisper_interface::ListenParams,
) -> crate::Result<BatchRunOutput> {
    let span = session_span(&params.session_id);

    async {
        let model = listen_params
            .model
            .as_deref()
            .ok_or_else(|| crate::BatchFailure::DirectRequestFailed {
                provider: "soniqo".to_string(),
                message: "Missing Soniqo model.".to_string(),
            })?
            .parse::<anlg_transcribe_soniqo::SoniqoModel>()
            .map_err(|e| crate::BatchFailure::DirectRequestFailed {
                provider: "soniqo".to_string(),
                message: e.to_string(),
            })?
            .batch_model();

        let file_path = params.file_path.clone();
        let file_extension = Path::new(&file_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_string();
        let language = listen_params
            .languages
            .first()
            .map(anlg_language::Language::bcp47_code);
        let language_hint = soniqo_language_hint(language.as_deref());
        let num_speakers = listen_params.num_speakers;
        let language_label = language.as_deref().unwrap_or("auto").to_string();
        let language_hint_label = language_hint.as_deref().unwrap_or("auto").to_string();
        let started_at = Instant::now();

        tracing::info!(
            anarlog.stt.provider.name = "soniqo",
            anarlog.stt.model = %model,
            anarlog.stt.language = %language_label,
            anarlog.stt.language_hint = %language_hint_label,
            file.extension = %file_extension,
            "soniqo_batch_start"
        );

        let session_id = params.session_id.clone();
        let async_runtime = tokio::runtime::Handle::current();
        let transcribed = tokio::task::spawn_blocking(move || {
            let progress = SoniqoProgressReporter {
                runtime,
                session_id,
            };
            transcribe_soniqo_file(
                model,
                &file_path,
                language_hint.as_deref(),
                num_speakers,
                Some(&progress),
                &async_runtime,
            )
        })
        .await
        .map_err(|e| {
            tracing::error!(
                anarlog.stt.provider.name = "soniqo",
                anarlog.stt.model = %model,
                error = %e,
                "soniqo_batch_task_join_failed"
            );
            crate::BatchFailure::DirectRequestFailed {
                provider: "soniqo".to_string(),
                message: format!("Soniqo transcription task failed: {e}"),
            }
        })?
        .map_err(|e| {
            let message = format_user_friendly_error(&e);
            tracing::error!(
                anarlog.stt.provider.name = "soniqo",
                anarlog.stt.model = %model,
                error = %e,
                anarlog.error.user_message = %message,
                "soniqo_batch_failed"
            );
            crate::BatchFailure::DirectRequestFailed {
                provider: "soniqo".to_string(),
                message,
            }
        })?;

        tracing::info!(
            anarlog.stt.provider.name = "soniqo",
            anarlog.stt.model = %model,
            elapsed_ms = started_at.elapsed().as_millis() as u64,
            transcript.channel_count = transcribed.len(),
            "soniqo_batch_completed"
        );

        let response = anlg_transcribe_soniqo::batch_response_from_channels(model, transcribed);

        Ok(BatchRunOutput {
            session_id: params.session_id,
            mode: BatchRunMode::Direct,
            response,
        })
    }
    .instrument(span)
    .await
}

fn transcribe_soniqo_file(
    model: anlg_transcribe_soniqo::SoniqoModel,
    file_path: &str,
    language: Option<&str>,
    num_speakers: Option<u32>,
    progress: Option<&SoniqoProgressReporter>,
    async_runtime: &tokio::runtime::Handle,
) -> std::result::Result<Vec<anlg_transcribe_soniqo::FileTranscript>, String> {
    ensure_local_batch_running(progress)?;
    let source = anlg_audio_utils::source_from_path(file_path).map_err(|e| e.to_string())?;
    let channel_count = u16::from(source.channels()).max(1) as usize;
    let sample_rate = u32::from(source.sample_rate());
    let duration_ms = source
        .total_duration()
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64);

    tracing::info!(
        anarlog.stt.provider.name = "soniqo",
        anarlog.stt.model = %model,
        anarlog.stt.language = %language.unwrap_or("auto"),
        audio.channel_count = channel_count,
        audio.sample_rate_hz = sample_rate,
        audio.duration_ms = duration_ms.unwrap_or_default(),
        audio.duration_known = duration_ms.is_some(),
        "soniqo_audio_file_loaded"
    );

    let resample_started_at = Instant::now();
    let channel_files = resample_audio_to_channel_files_until(file_path, source, || {
        local_batch_is_cancelled(progress)
    })?;
    ensure_local_batch_running(progress)?;
    let resampled_sample_count = channel_files
        .iter()
        .map(|channel| channel.sample_count)
        .sum::<usize>();
    tracing::info!(
        anarlog.stt.provider.name = "soniqo",
        anarlog.stt.model = %model,
        elapsed_ms = resample_started_at.elapsed().as_millis() as u64,
        audio.source_sample_rate_hz = sample_rate,
        audio.target_sample_rate_hz = TARGET_SAMPLE_RATE,
        audio.resampled_sample_count = resampled_sample_count,
        "soniqo_audio_resampled"
    );

    tracing::info!(
        anarlog.stt.provider.name = "soniqo",
        anarlog.stt.model = %model,
        audio.source_channel_count = channel_count,
        audio.transcribed_channel_count = channel_files.len(),
        "soniqo_channels_prepared"
    );

    let transcribed_channel_count = channel_files.len();
    let channel_sample_counts = channel_files
        .iter()
        .map(|channel| channel.sample_count)
        .collect::<Vec<_>>();
    let channel_rms = channel_files
        .iter()
        .map(|channel| channel.rms)
        .collect::<Vec<_>>();
    let mut diarization_plans = (0..transcribed_channel_count)
        .map(|channel_index| {
            soniqo_diarization_speaker_count(num_speakers, &channel_rms, channel_index)
        })
        .collect::<Vec<_>>();
    let skipped_for_length =
        soniqo_apply_diarization_limit(&mut diarization_plans, &channel_sample_counts);
    tracing::info!(
        anarlog.stt.provider.name = "soniqo",
        anarlog.stt.model = %model,
        audio.channel_rms = ?channel_rms,
        diarization.speech_channels = ?soniqo_speech_channels(&channel_rms),
        // Seit dem 21.09.2026 steht auch da, WARUM ein Kanal als still galt.
        // Vorher nannte die Zeile nur das Urteil, und ein falsches Urteil war
        // im Nachhinein nicht nachzuvollziehen -- der Fehlalarm, gegen den
        // diese Runde gebaut ist, lag deshalb monatelang unbemerkt in den
        // Protokollen. Deshalb je Kanal ein WORT und nicht nur Zutaten, aus
        // denen der Leser das Urteil selbst zusammenrechnen muesste.
        diarization.channel_voices = ?(0..channel_rms.len())
            .map(|channel_index| soniqo_channel_voice(&channel_rms, channel_index).as_str())
            .collect::<Vec<_>>(),
        diarization.channel_ratios = ?(0..channel_rms.len())
            .map(|channel_index| soniqo_channel_silence_ratio(&channel_rms, channel_index))
            .collect::<Vec<_>>(),
        diarization.silent_below_rms = SONIQO_CHANNEL_SILENT_RMS,
        diarization.silent_below_ratio = SONIQO_CHANNEL_SILENT_RATIO,
        diarization.digital_silence_below_rms = SONIQO_CHANNEL_DIGITAL_SILENCE_RMS,
        // Der Fall, um den es bei der Fehlersuche geht: eine Aufnahme mit
        // zwei Spuren, von denen nur noch eine als sprechend gilt. Dort
        // entscheidet sich, ob der Nutzer mitgezaehlt wird -- und dort waere
        // ein Fehlurteil am teuersten.
        diarization.two_channels_collapsed_to_one = transcribed_channel_count == 2
            && soniqo_speech_channels(&channel_rms).len() == 1,
        diarization.plans = ?diarization_plans,
        "soniqo_diarization_planned"
    );
    if skipped_for_length {
        tracing::warn!(
            anarlog.stt.provider.name = "soniqo",
            anarlog.stt.model = %model,
            audio.duration_seconds = channel_sample_counts.iter().copied().max().unwrap_or_default()
                as f64
                / TARGET_SAMPLE_RATE as f64,
            diarization.max_duration_seconds =
                SONIQO_DIARIZATION_MAX_SAMPLES / TARGET_SAMPLE_RATE as usize,
            "soniqo_diarization_skipped_for_long_recording"
        );
    }
    let diarized_channel_count = diarization_plans
        .iter()
        .filter(|plan| **plan != SoniqoChannelDiarization::Skip)
        .count();
    if let Some(progress) = progress {
        progress.emit(soniqo_diarization_progress(0, diarized_channel_count, 0.0));
    }

    // Die Diarisierung bleibt bewusst sequenziell: sie laedt einen ganzen Kanal
    // als f32-Vektor in den Speicher und ist damit der einzige
    // speicherhungrige Schritt. Nacheinander gehalten bleibt die Speicherspitze
    // die eines EINZELNEN Kanals; parallel waeren es alle zugleich. Wie stark
    // das mit der Laenge waechst, steht bei
    // [`SONIQO_DIARIZATION_MAX_SAMPLES`].
    let mut channel_speaker_segments = Vec::with_capacity(transcribed_channel_count);
    let mut diarized_channels_done = 0usize;
    let mut diarization_failure: Option<String> = None;
    for (channel_index, channel) in channel_files.iter().enumerate() {
        ensure_local_batch_running(progress)?;
        let speaker_count = match diarization_plans[channel_index] {
            SoniqoChannelDiarization::Skip => {
                channel_speaker_segments.push(Vec::new());
                continue;
            }
            SoniqoChannelDiarization::Exact(speaker_count) => Some(speaker_count),
            SoniqoChannelDiarization::Inferred => None,
        };
        let diarized = with_diarization_heartbeat(
            progress,
            diarized_channels_done,
            diarized_channel_count,
            channel.sample_count,
            || {
                diarize_soniqo_channel_file(
                    model,
                    channel_index,
                    channel,
                    speaker_count,
                    progress,
                    diarized_channels_done,
                    diarized_channel_count,
                )
            },
        )?;
        let speaker_segments = diarized.segments;
        if let Some(reason) = diarized.failed_reason {
            diarization_failure = Some(reason);
        }
        diarized_channels_done += 1;
        if let Some(progress) = progress {
            progress.emit(soniqo_diarization_progress(
                diarized_channels_done,
                diarized_channel_count,
                0.0,
            ));
        }
        channel_speaker_segments.push(speaker_segments);
    }
    // Der Uebergang vom Trennungs- ins Zerlegungs-Band wird EINMAL gemeldet,
    // auch wenn gar nichts zu trennen war. Sonst bliebe der Balken bei einem
    // reinen Zwei-Kanal-Gespraech bis zum ersten fertigen Kanal auf 5 % stehen.
    if let Some(progress) = progress {
        progress.emit(soniqo_batch_progress(0, transcribed_channel_count));
    }

    let channel_results = transcribe_soniqo_channels_in_parallel(
        transcribed_channel_count,
        channel_files,
        progress,
        |channel_index, channel| {
            let plan = soniqo_channel_plan(
                model,
                channel_index,
                channel,
                transcribed_channel_count == 2 && channel_index == 0,
            );
            transcribe_soniqo_channel_chunks(model, plan, language, progress, async_runtime)
        },
    )?;

    let mut transcripts = collect_soniqo_channel_transcripts(channel_results)?;
    for (transcript, speaker_segments) in transcripts
        .iter_mut()
        .zip(channel_speaker_segments.into_iter())
    {
        transcript.speaker_segments = speaker_segments;
        // Reist bis in die Antwort-Metadaten und von dort als Hinweis an den
        // Nutzer. Ohne das steht das Auslassen nur im Protokoll, und im
        // Transkript ist es von einem schlechten Ergebnis nicht zu
        // unterscheiden.
        transcript.diarization_skipped_over_seconds = skipped_for_length
            .then(|| SONIQO_DIARIZATION_MAX_SAMPLES as f64 / TARGET_SAMPLE_RATE as f64);
        // Derselbe Weg nach draussen wie beim Auslassen, aber mit eigenem
        // Wortlaut: ausgelassen und fehlgeschlagen sind zwei verschiedene
        // Nachrichten, und beide sind besser als ein leeres Sprecherfeld ohne
        // Erklaerung.
        transcript.diarization_failed = diarization_failure.is_some();
    }
    Ok(transcripts)
}

// Fork-Abweichung (02.09.2026): die Kanaele laufen gleichzeitig statt
// nacheinander.
//
// Vorher lag Kanal 1 still, waehrend Kanal 0 lief -- an des Betreibers Aufnahme
// gemessen (Sitzung b2c3d4e5): Kanal 0 von 0,59 s bis 7,77 s, Kanal 1 erst ab
// 7,77 s bis 15,12 s, keine Ueberlappung.
//
// Was das bringt und was NICHT: die eigentlichen Modellaufrufe koennen NICHT
// parallel laufen. `SoniqoBridge` ist ein Swift-Actor
// (crates/transcribe-soniqo/swift-lib/src/lib.swift:518) und
// `transcribeFileAudio` darin ist synchron, also ohne Aufhaengepunkt -- zwei
// Aufrufe aus zwei Threads reihen sich auf dem Actor-Executor hintereinander
// ein. Was sich ueberlappt, ist alles daneben: das sprachbewusste Zerlegen
// (VAD) und das Schreiben der WAV-Tempdateien. Nach der Buendelung ist das rund
// die Haelfte der Wanduhr, also lohnt es sich -- aber der Gewinn ist gedeckelt,
// und der Deckel liegt in fremdem Code.
pub(super) fn transcribe_soniqo_channels_in_parallel<F>(
    channel_count: usize,
    channels: Vec<ResampledChannelFile>,
    progress: Option<&SoniqoProgressReporter>,
    transcribe_channel: F,
) -> std::result::Result<
    Vec<std::result::Result<anlg_transcribe_soniqo::FileTranscript, String>>,
    String,
>
where
    F: Fn(
            usize,
            ResampledChannelFile,
        ) -> std::result::Result<anlg_transcribe_soniqo::FileTranscript, String>
        + Sync,
{
    let completed_channels = std::sync::Mutex::new(0usize);

    // Achse D des Testplans, seit dem 02.09.2026 entschieden: die Vorgabe ist
    // `true` -- gleichzeitig. Der Zweig hier bleibt die gemessene Gegenprobe
    // (`bench_soniqo --channels sequential`) und wird eigens getestet.
    if !super::tuning::soniqo_tuning().channels_in_parallel {
        let mut results = Vec::with_capacity(channel_count);
        for (channel_index, channel) in channels.into_iter().enumerate() {
            ensure_local_batch_running(progress)?;
            let channel_result = transcribe_channel(channel_index, channel);
            ensure_local_batch_running(progress)?;

            report_channel_completed(&completed_channels, channel_count, progress);

            results.push(channel_result);
        }
        return Ok(results);
    }

    let joined = std::thread::scope(|scope| {
        let handles = channels
            .into_iter()
            .enumerate()
            .map(|(channel_index, channel)| {
                let completed_channels = &completed_channels;
                let transcribe_channel = &transcribe_channel;
                scope.spawn(move || {
                    ensure_local_batch_running(progress)?;
                    let channel_result = transcribe_channel(channel_index, channel);
                    ensure_local_batch_running(progress)?;

                    report_channel_completed(completed_channels, channel_count, progress);

                    Ok(channel_result)
                })
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err("Soniqo channel transcription thread panicked.".to_string())
                })
            })
            .collect::<Vec<_>>()
    });

    // Reihenfolge zaehlt: Kanal 0 muss Kanal 0 bleiben, sonst vertauschen sich
    // Mikrofon und Systemton im Transkript.
    joined.into_iter().collect()
}

/// Zaehlt einen fertigen Kanal und meldet den Fortschritt in EINEM Zug.
///
/// Warum ein Mutex und kein Zaehler-Atomic: zwischen Hochzaehlen und Melden
/// laege sonst ein Fenster, in dem der zweite Kanal seine 100 % losschickt,
/// bevor der erste seine 50 % gemeldet hat -- der Balken liefe rueckwaerts.
/// Der Mutex haelt beides zusammen, damit die Meldungen in der Reihenfolge
/// herausgehen, in der gezaehlt wurde. Auf dem seriellen Zweig kostet das
/// nichts (er zaehlt ohnehin allein).
fn report_channel_completed(
    completed_channels: &std::sync::Mutex<usize>,
    channel_count: usize,
    progress: Option<&SoniqoProgressReporter>,
) {
    let Some(progress) = progress else {
        return;
    };
    let mut completed = completed_channels
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *completed += 1;
    progress.emit(soniqo_batch_progress(*completed, channel_count));
}

/// Laesst waehrend eines Trennungslaufs regelmaessig etwas vom Fortschritt hoeren.
///
/// Die Swift-Seite ist EIN blockierender Aufruf ohne Zwischenmeldung, also gibt
/// es nichts Echtes zu melden -- was hier laeuft, ist eine Schaetzung aus der
/// Uhr gegen eine gemessene Erwartung. Der Unterschied zu einer Luege ist der
/// Deckel: der Balken bleibt vor dem Bandende stehen und springt erst weiter,
/// wenn der Kanal wirklich fertig ist. Das reicht fuer die eine Frage, die der
/// Prinzipal am 07.09. nicht beantworten konnte: laeuft da noch etwas?
pub(super) fn with_diarization_heartbeat<T>(
    progress: Option<&SoniqoProgressReporter>,
    completed_channels: usize,
    total_channels: usize,
    sample_count: usize,
    run: impl FnOnce() -> T,
) -> T {
    with_diarization_heartbeat_every(
        SONIQO_DIARIZATION_HEARTBEAT,
        progress,
        completed_channels,
        total_channels,
        sample_count,
        run,
    )
}

/// Dasselbe mit waehlbarem Takt -- damit der Test nicht zwei Sekunden warten
/// muss, um zu sehen, dass sich ueberhaupt etwas meldet.
pub(super) fn with_diarization_heartbeat_every<T>(
    interval: Duration,
    progress: Option<&SoniqoProgressReporter>,
    completed_channels: usize,
    total_channels: usize,
    sample_count: usize,
    run: impl FnOnce() -> T,
) -> T {
    let Some(progress) = progress else {
        return run();
    };

    let stop = std::sync::atomic::AtomicBool::new(false);
    let expected = soniqo_diarization_expected(sample_count);
    // In kleinen Schritten schlafen, damit der Faden nach dem Ende des Laufs
    // nicht noch sekundenlang haengt.
    let step = (interval / 20).max(Duration::from_millis(5));
    // Der Faden hoert auf, wenn eine von drei Sachen eintritt: der Lauf ist
    // durch, der Prinzipal hat abgebrochen, oder der Lauf ist gestorben. Das
    // dritte deckt der Wachhund weiter unten.
    let heartbeat_should_stop =
        || stop.load(std::sync::atomic::Ordering::Relaxed) || progress.is_cancelled();

    std::thread::scope(|scope| {
        scope.spawn(|| {
            let started_at = Instant::now();
            loop {
                let mut waited = Duration::ZERO;
                while waited < interval {
                    if heartbeat_should_stop() {
                        return;
                    }
                    std::thread::sleep(step);
                    waited += step;
                }
                if heartbeat_should_stop() {
                    return;
                }
                let within = soniqo_diarization_within(started_at.elapsed(), expected);
                progress.emit(soniqo_diarization_progress(
                    completed_channels,
                    total_channels,
                    within,
                ));
            }
        });

        // Der Wachhund MUSS vor `run()` stehen: er setzt die Stopp-Flagge beim
        // Raeumen des Blocks, also auch dann, wenn `run()` panikt. Vorher stand
        // `stop.store(true)` als Zeile HINTER dem Lauf und wurde im Panikfall
        // nie erreicht -- und weil `std::thread::scope` vor dem Weiterreichen
        // der Panik auf alle Faeden wartet, drehte der Herzschlag dann endlos.
        // Kein Absturz, keine Meldung, kein Ende: genau das Bild, wegen dem der
        // Prinzipal am 07.09. zweimal abgebrochen hat, diesmal ohne Ausweg
        // ausser die App zu beenden.
        let _stop_heartbeat = StopHeartbeatOnDrop(&stop);
        run()
    })
}

/// Setzt die Stopp-Flagge des Herzschlags, sobald der Lauf das Feld raeumt.
///
/// Ein Wachhund statt `catch_unwind`, weil er nicht nur die Panik deckt,
/// sondern jeden kuenftigen fruehen Ausstieg aus dem Block (`?`, `return`,
/// `break`) -- und weil er keine `UnwindSafe`-Zusage von `run` verlangt.
struct StopHeartbeatOnDrop<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for StopHeartbeatOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub(super) struct SoniqoProgressReporter {
    pub(super) runtime: Arc<dyn BatchRuntime>,
    pub(super) session_id: String,
}

impl SoniqoProgressReporter {
    fn emit(&self, percentage: f64) {
        self.runtime.emit(BatchEvent::BatchResponseStreamed {
            session_id: self.session_id.clone(),
            event: BatchStreamEvent::Progress {
                percentage,
                partial_text: None,
            },
        });
    }

    fn is_cancelled(&self) -> bool {
        self.runtime.is_cancelled()
    }
}

fn local_batch_is_cancelled(progress: Option<&SoniqoProgressReporter>) -> bool {
    progress.is_some_and(SoniqoProgressReporter::is_cancelled)
}

fn ensure_local_batch_running(
    progress: Option<&SoniqoProgressReporter>,
) -> std::result::Result<(), String> {
    if local_batch_is_cancelled(progress) {
        Err(LOCAL_BATCH_CANCELLED.to_string())
    } else {
        Ok(())
    }
}

struct SoniqoChannelPlan {
    channel_index: usize,
    duration_seconds: f64,
    is_direct_mic: bool,
    chunk_strategy: SoniqoChunkStrategy,
    channel: ResampledChannelFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SoniqoChunkStrategy {
    Fixed { max_samples: usize },
    SpeechAware,
}

pub(super) struct FixedSoniqoFileChunkIterator {
    reader: hound::WavReader<BufReader<File>>,
    _file: tempfile::NamedTempFile,
    max_samples: usize,
    next_start: usize,
    finished: bool,
}

impl FixedSoniqoFileChunkIterator {
    pub(super) fn new(
        file: tempfile::NamedTempFile,
        max_samples: usize,
    ) -> std::result::Result<Self, String> {
        let reader = hound::WavReader::open(file.path()).map_err(|e| e.to_string())?;
        Ok(Self {
            reader,
            _file: file,
            max_samples,
            next_start: 0,
            finished: false,
        })
    }
}

impl Iterator for FixedSoniqoFileChunkIterator {
    type Item = std::result::Result<AudioChunk, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        let samples = match self
            .reader
            .samples::<f32>()
            .take(self.max_samples)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(samples) => samples,
            Err(error) => {
                self.finished = true;
                return Some(Err(error.to_string()));
            }
        };
        if samples.is_empty() {
            self.finished = true;
            return None;
        }
        let sample_start = self.next_start;
        let sample_end = sample_start + samples.len();
        self.next_start = sample_end;
        Some(Ok(AudioChunk {
            samples,
            sample_start,
            sample_end,
        }))
    }
}

struct SpeechSoniqoFileChunkIterator {
    chunks: Pin<Box<dyn Stream<Item = Result<AudioChunk, anlg_audio_chunking::Error>>>>,
    async_runtime: tokio::runtime::Handle,
    _file: tempfile::NamedTempFile,
}

impl SpeechSoniqoFileChunkIterator {
    fn new(
        file: tempfile::NamedTempFile,
        async_runtime: &tokio::runtime::Handle,
    ) -> std::result::Result<Self, String> {
        let source = anlg_audio_utils::source_from_path(file.path()).map_err(|e| e.to_string())?;
        let chunks =
            source.speech_chunks(SpeechChunkingConfig::speech(SONIQO_SPEECH_REDEMPTION_TIME));

        Ok(Self {
            chunks: Box::pin(chunks),
            async_runtime: async_runtime.clone(),
            _file: file,
        })
    }
}

impl Iterator for SpeechSoniqoFileChunkIterator {
    type Item = std::result::Result<AudioChunk, String>;

    fn next(&mut self) -> Option<Self::Item> {
        self.async_runtime
            .block_on(self.chunks.as_mut().next())
            .map(|chunk| chunk.map_err(|error| error.to_string()))
    }
}

enum SoniqoFileChunkIterator {
    Fixed(FixedSoniqoFileChunkIterator),
    SpeechAware(SpeechSoniqoFileChunkIterator),
}

impl SoniqoFileChunkIterator {
    fn new(
        file: tempfile::NamedTempFile,
        strategy: SoniqoChunkStrategy,
        async_runtime: &tokio::runtime::Handle,
    ) -> std::result::Result<Self, String> {
        match strategy {
            SoniqoChunkStrategy::Fixed { max_samples } => {
                FixedSoniqoFileChunkIterator::new(file, max_samples).map(Self::Fixed)
            }
            SoniqoChunkStrategy::SpeechAware => {
                SpeechSoniqoFileChunkIterator::new(file, async_runtime).map(Self::SpeechAware)
            }
        }
    }
}

impl Iterator for SoniqoFileChunkIterator {
    type Item = std::result::Result<AudioChunk, String>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Fixed(chunks) => chunks.next(),
            Self::SpeechAware(chunks) => chunks.next(),
        }
    }
}

// Fork-Abweichung (02.09.2026): mehrere Sprech-Abschnitte gehen in EINEN
// Modellaufruf.
//
// Warum: die Swift-Seite polstert jeden parakeet-batch-Aufruf auf MINDESTENS
// 20 s Audio auf (`normalizedFileTranscriptionAudio`, swift-lib/src/lib.swift,
// `parakeetBatchMinimumChunkSeconds = 20.0`). Ein Sprech-Abschnitt von 1,1 s
// kostet damit dieselbe Rechenzeit wie einer von 20 s. An des Betreibers Lauf vom
// 02.09.2026 gemessen (Sitzung b2c3d4e5, 23,6 min, 2 Kanaele): 414 Aufrufe,
// Median-Audio je Aufruf 1110 ms, Median-Rechenzeit je Aufruf 506 ms
// (Min 470, Max 2909) -- die Zeit haengt praktisch nicht von der Laenge ab.
// 79 % der Aufrufe trugen unter 3 s Ton und zahlten trotzdem den vollen
// 20-s-Preis.
//
// Was hier NICHT passiert: die Zeitachse wird nicht wieder verrastert. Der
// Fehler vom 31.08.2026 (Wortpakete am Fensteranfang statt am Sprechbeginn,
// bis zu 29,5 s Versatz) kam daher, dass EIN Transkript-Abschnitt einen ganzen
// Fensterblock verankerte. Ein Buendel ist deshalb nur eine Aufruf-Einheit,
// keine Transkript-Einheit: jeder Abschnitt behaelt seinen eigenen
// `sample_start`/`sample_end` und wird nach dem Aufruf wieder als eigener
// `FileTranscriptChunk` ausgegeben. Die Anker bleiben bitgenau die von heute.
//
// Der Preis, offen benannt: welches Wort zu welchem Abschnitt gehoert, ist im
// Buendel geschaetzt (nach der geschaetzten Sprechdauer des Textes, siehe
// `split_bundle_text`) statt gemessen. Innerhalb eines Abschnitts waren die
// Wortzeiten ohnehin schon gleichverteilt, die Schaetzung liegt also in
// derselben Groessenordnung wie die bestehende -- aber sie ist neu an der
// GRENZE zwischen zwei Abschnitten. Gemessen bleibt sie erst, wenn der
// Dekoder echte Wortzeiten liefert; bis dahin ist jede Verbesserung hier eine
// bessere Schaetzung, keine Messung.
//
// Die Naht: zwischen zwei Abschnitten liegen `SONIQO_BUNDLE_SEAM_SAMPLES`
// Stille, damit das Modell die Abschnitte nicht zu einem Wort verklebt. Sie
// zaehlt gegen das Fensterbudget und traegt kein Gewicht bei der Aufteilung.
pub(super) const SONIQO_BUNDLE_SEAM_SAMPLES: usize = TARGET_SAMPLE_RATE as usize / 4;

// Zweites Budget: wie weit ein Buendel auf der ZEITACHSE reichen darf.
//
// Warum es das Ton-Budget nicht schon leistet: die sprachbewusste Zerlegung
// wirft die Stille zwischen zwei Abschnitten weg. 29,5 s TON sind deshalb
// beliebig viel ZEIT -- an des Betreibers Gespraech gemessen (tom.mp3, 2 x 23,6 min,
// 608 Abschnitte in 43 Buendeln) im Mittel rund eine Minute je Buendel.
//
// Was daran gefaehrlich ist: `split_bundle_text` teilt die Woerter eines
// Buendels auf die Sprechdauer seiner Mitglieder auf. Faellt ein Wort
// dem falschen Mitglied zu, springt es um die ganze Spanne des Buendels -- und
// die war bis hier ungedeckelt. Das ist dasselbe Fehlerbild wie am 31.08.2026
// ("sieht aus wie Durcheinanderreden"), nur seltener und mit groesserer
// Auslenkung: damals waren es hoechstens 29,5 s, ohne dieses Budget beliebig
// viel.
//
// Die Grenze ist deshalb genau das Fenster, das vor dem 31.08.2026 den Versatz
// deckelte. Zusicherung in einem Satz: ein Wort kann nie weiter verrutschen,
// als es im starren Raster von damals konnte.
//
// Gemessen an tom.mp3 (02.09.2026, Messbank `examples/bench_soniqo.rs`),
// gegen den ungebuendelten Lauf als Bezug, Anker ab 5 gleichen Woertern:
//
//   Spanne        Aufrufe   Wanduhr   Stellen > 10 s   Stellen > 29,5 s   Max
//   unbegrenzt         43     10,5 s               84                 16   48,1 s
//   60 s               53     11,0 s               59                  4   46,7 s
//   45 s               63     11,7 s               44                  2   34,1 s
//   29,5 s             88     14,9 s               11                  0   17,9 s
//   (nicht gebuendelt 608     95,5 s                0                  0    0,0 s)
//
// Der Preis ist damit offen benannt: 88 statt 43 Aufrufe, 14,9 statt 10,5 s.
// Die Buendelung bringt trotzdem den weitaus groessten Teil ihres Gewinns
// (608 -> 88 Aufrufe, 95,5 -> 14,9 s), und erst bei 29,5 s ist die Spalte
// "> 29,5 s" leer -- die Zusicherung ist also nicht nur hergeleitet, sondern
// am echten Material nachgemessen.
pub(super) const SONIQO_BUNDLE_MAX_SPAN_SAMPLES: usize = SONIQO_PARAKEET_MAX_CHUNK_SAMPLES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SoniqoBundleMember {
    pub(super) sample_start: usize,
    pub(super) sample_end: usize,
    pub(super) speech_samples: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SoniqoChunkBundle {
    pub(super) samples: Vec<f32>,
    pub(super) members: Vec<SoniqoBundleMember>,
}

impl SoniqoChunkBundle {
    /// Wo jedes Mitglied IM BUENDEL beginnt, in Abtastwerten. Das Buendel ist
    /// die Aneinanderreihung der Mitglieder mit je einer Naht dazwischen --
    /// derselbe Aufbau, den `SoniqoChunkBundler::push` schreibt. Diese Zahlen
    /// sind der Uebersetzer zwischen der Zeitachse, die das Modell sieht, und
    /// der Zeitachse der Aufnahme.
    fn member_local_starts(&self) -> Vec<usize> {
        let mut starts = Vec::with_capacity(self.members.len());
        let mut cursor = 0usize;
        for (index, member) in self.members.iter().enumerate() {
            if index > 0 {
                cursor += SONIQO_BUNDLE_SEAM_SAMPLES;
            }
            starts.push(cursor);
            cursor += member.speech_samples;
        }
        starts
    }

    fn member_weights(&self) -> Vec<usize> {
        self.members
            .iter()
            .map(|member| member.speech_samples)
            .collect()
    }
}

/// Sammelt aufeinanderfolgende Sprech-Abschnitte, bis das Modellfenster voll
/// ist. Zusicherung: ein Buendel mit mehr als einem Abschnitt bleibt unter
/// `max_samples`. Ein einzelner Abschnitt wird NIE zerschnitten -- passt er
/// allein nicht ins Fenster, geht er allein raus, genau wie vor dieser
/// Aenderung (die Swift-Seite zerlegt ihn dann intern).
pub(super) struct SoniqoChunkBundler {
    max_samples: usize,
    max_span_samples: usize,
    samples: Vec<f32>,
    members: Vec<SoniqoBundleMember>,
}

impl SoniqoChunkBundler {
    pub(super) fn new(max_samples: usize, max_span_samples: usize) -> Self {
        Self {
            max_samples,
            max_span_samples,
            samples: Vec::new(),
            members: Vec::new(),
        }
    }

    /// Nimmt einen Abschnitt auf und gibt das fertige Buendel zurueck, wenn der
    /// Abschnitt nicht mehr hineinpasst -- weder ins Ton-Budget noch in die
    /// erlaubte Zeitspanne (siehe [`SONIQO_BUNDLE_MAX_SPAN_SAMPLES`]).
    pub(super) fn push(&mut self, chunk: AudioChunk) -> Option<SoniqoChunkBundle> {
        let seam = if self.members.is_empty() {
            0
        } else {
            SONIQO_BUNDLE_SEAM_SAMPLES
        };
        let outgrows_audio_budget =
            self.samples.len() + seam + chunk.samples.len() > self.max_samples;
        let outgrows_span_budget = self.members.first().is_some_and(|first| {
            chunk.sample_end.saturating_sub(first.sample_start) > self.max_span_samples
        });
        let flushed = if !self.members.is_empty() && (outgrows_audio_budget || outgrows_span_budget)
        {
            self.take()
        } else {
            None
        };

        if !self.members.is_empty() {
            self.samples
                .extend(std::iter::repeat_n(0.0f32, SONIQO_BUNDLE_SEAM_SAMPLES));
        }
        let speech_samples = chunk.samples.len();
        self.samples.extend_from_slice(&chunk.samples);
        self.members.push(SoniqoBundleMember {
            sample_start: chunk.sample_start,
            sample_end: chunk.sample_end,
            speech_samples,
        });

        flushed
    }

    /// Gibt das angefangene Buendel heraus. Ein zweiter Aufruf liefert `None`.
    pub(super) fn finish(&mut self) -> Option<SoniqoChunkBundle> {
        self.take()
    }

    fn take(&mut self) -> Option<SoniqoChunkBundle> {
        if self.members.is_empty() {
            return None;
        }

        Some(SoniqoChunkBundle {
            samples: std::mem::take(&mut self.samples),
            members: std::mem::take(&mut self.members),
        })
    }
}

pub(super) fn soniqo_bundle_max_samples(strategy: SoniqoChunkStrategy) -> usize {
    // Die Messbank kann die Zielgroesse vorgeben (Achse B des Testplans:
    // 10 s / 20 s / 29,5 s, und 0 s = gar nicht buendeln). Ohne Vorgabe gilt
    // unveraendert das Modellfenster.
    if let Some(max_samples) = super::tuning::soniqo_tuning().bundle_max_samples {
        return max_samples;
    }

    match strategy {
        SoniqoChunkStrategy::Fixed { max_samples } => max_samples,
        SoniqoChunkStrategy::SpeechAware => SONIQO_PARAKEET_MAX_CHUNK_SAMPLES,
    }
}

/// Erlaubte Zeitspanne eines Buendels. Ohne Vorgabe der Messbank gilt
/// [`SONIQO_BUNDLE_MAX_SPAN_SAMPLES`].
///
/// Beim starren Raster (`Fixed`) gibt es keine verworfene Stille -- Tonlaenge
/// und Zeitspanne sind dort dasselbe, das Budget greift also nie. Es bleibt
/// trotzdem gesetzt, damit die Messbank auch dort eine Achse fahren kann.
pub(super) fn soniqo_bundle_max_span_samples() -> usize {
    super::tuning::soniqo_tuning()
        .bundle_max_span_samples
        .unwrap_or(SONIQO_BUNDLE_MAX_SPAN_SAMPLES)
}

/// Zuschlag je Wort in der Sprechdauer-Schaetzung, ausgedrueckt in Zeichen.
///
/// Warum es ihn gibt: ein Wort kostet nicht nur seine Zeichen, sondern auch
/// einen festen Anteil (Ansatz, Absetzen, die kleine Pause davor). Ein Satz aus
/// kurzen Funktionswoertern dauert deshalb laenger, als seine Zeichenzahl
/// vermuten laesst.
///
/// Die Zahl ist gemessen, nicht geraten: an den 227 Sprech-Abschnitten aus
/// des Betreibers `tom.mp3`, deren Wortzeiten NICHT an der 0,4-s-Decke hingen (also
/// deren echte Abschnittsdauer bekannt ist), auf `Dauer = A * Woerter +
/// B * Zeichen` ausgeglichen. Ergebnis A/B = 1,43 Zeichen; der mittlere
/// absolute Fehler der Dauer-Vorhersage liegt bei 2 Zeichen Zuschlag am
/// flachsten (0,442 s gegen 0,451 s ohne Zuschlag und 0,495 s bei reiner
/// Wortzaehlung). Das ist der ganze Grund fuer diese Datei: die reine
/// Wortzaehlung ist die schlechteste der drei Schaetzungen.
const BUNDLE_WORD_OVERHEAD_CHARS: usize = 2;

/// Geschaetzte Sprechdauer eines Wortes in willkuerlichen Einheiten.
fn word_speech_cost(word: &str) -> usize {
    word.chars().count() + BUNDLE_WORD_OVERHEAD_CHARS
}

/// Verteilt den Text eines Buendels wieder auf seine Abschnitte -- anteilig an
/// der Sprechdauer, an Wortgrenzen geschnitten. Bei einem einzigen Abschnitt
/// (und das ist der haeufige Fall am Anfang und Ende eines Kanals) ist das
/// verlustfrei: der ganze Text gehoert ihm.
///
/// Geschnitten wird nach der geschaetzten SPRECHDAUER des Textes, nicht nach
/// der Wortzahl. Der Unterschied ist der Kern der Sache: das Modell liefert
/// einen flachen Text ohne innere Grenzen, geteilt wird also nach Ton --
/// produziert wird aber nach Sprechgeschwindigkeit. Wer nach Wortzahl teilt,
/// schiebt einem langsam sprechenden Abschnitt systematisch zu viele Woerter
/// zu und einem schnellen zu wenige, und jedes falsch zugeteilte Wort springt
/// um die Luecke zwischen den beiden Abschnitten.
pub(super) fn split_bundle_text(text: &str, weights: &[usize]) -> Vec<String> {
    if weights.is_empty() {
        return Vec::new();
    }
    if weights.len() == 1 {
        return vec![text.to_string()];
    }

    let words = text.split_whitespace().collect::<Vec<_>>();
    let total_weight = weights.iter().sum::<usize>();
    if words.is_empty() || total_weight == 0 {
        let mut parts = vec![String::new(); weights.len()];
        if let Some(first) = parts.first_mut() {
            *first = words.join(" ");
        }
        return parts;
    }

    // Aufsummierte Sprechdauer-Schaetzung. `costs[index]` ist der Aufwand der
    // ersten `index` Woerter, `costs[0]` also 0 und der letzte Eintrag der
    // Gesamtaufwand. Ein Wort kostet mindestens den Zuschlag, der Gesamtwert
    // ist bei nicht-leerem Text also immer groesser als 0.
    let mut costs = Vec::with_capacity(words.len() + 1);
    costs.push(0u64);
    for word in &words {
        costs.push(costs[costs.len() - 1] + word_speech_cost(word) as u64);
    }
    let total_cost = costs[words.len()];

    let mut parts = Vec::with_capacity(weights.len());
    let mut cumulative_weight = 0usize;
    let mut previous_boundary = 0usize;
    for (index, weight) in weights.iter().enumerate() {
        cumulative_weight += weight;
        let boundary = if index + 1 == weights.len() {
            words.len()
        } else {
            // Der Abschnitt bekommt den Anteil am Textaufwand, der seinem
            // Anteil an der Sprechdauer entspricht -- an der naechstgelegenen
            // Wortgrenze abgeschnitten, damit er nicht systematisch Woerter an
            // seinen Nachbarn verliert.
            let target = total_cost * cumulative_weight as u64 / total_weight as u64;
            let mut cut = previous_boundary;
            while cut < words.len() && costs[cut] < target {
                cut += 1;
            }
            if cut > previous_boundary && costs[cut] - target > target - costs[cut - 1] {
                cut -= 1;
            }
            cut
        }
        .clamp(previous_boundary, words.len());
        parts.push(words[previous_boundary..boundary].join(" "));
        previous_boundary = boundary;
    }

    parts
}

/// Fork (02.09.2026): dasselbe mit den GEMESSENEN Wortzeiten des Erkenners.
///
/// Der Unterschied zu [`soniqo_bundle_transcript_chunks`] ist nicht die
/// Genauigkeit der Zeitmarken, sondern die Frage, wer die Woerter auf die
/// Abschnitte verteilt: dort raet `split_bundle_text` es aus der geschaetzten
/// Sprechdauer, hier sagt es der Erkenner. Ein Wort faellt dem Abschnitt zu, in
/// dessen Tonbereich es GESPROCHEN wurde.
///
/// Liefert `None`, wenn die Wortliste nicht zum Modelltext passt. Dann ist die
/// Zuordnung nicht belegbar, und der Aufrufer nimmt den alten Weg -- eine
/// halbe Zeitachse waere schlimmer als die alte ganze.
pub(super) fn soniqo_bundle_transcript_chunks_from_words(
    bundle: &SoniqoChunkBundle,
    text: &str,
    words: &[anlg_transcribe_soniqo::TranscriptWord],
) -> Option<Vec<anlg_transcribe_soniqo::FileTranscriptChunk>> {
    if words.is_empty() || bundle.members.is_empty() {
        return None;
    }
    if !word_list_matches_text(text, words) {
        return None;
    }

    use anlg_transcribe_soniqo::MIN_WORD_DURATION_SECONDS;

    let local_starts = bundle.member_local_starts();
    let rate = f64::from(TARGET_SAMPLE_RATE);
    let mut per_member: Vec<Vec<anlg_transcribe_soniqo::TranscriptWord>> =
        vec![Vec::new(); bundle.members.len()];
    // Die Woerter kommen in Sprechreihenfolge. Der Zeiger laeuft nur vorwaerts:
    // ein spaeteres Wort kann keinem frueheren Abschnitt zufallen, auch wenn
    // seine Zeitmarke knapp vor einer Naht liegt.
    let mut member_index = 0usize;

    for word in words {
        let local_start = (word.start_seconds * rate).max(0.0);
        while member_index + 1 < bundle.members.len() {
            let next_start = local_starts[member_index + 1] as f64;
            // Die Naht ist Stille. Wer in ihr landet, gehoert zur naeheren
            // Seite -- deshalb die Mitte der Naht als Grenze.
            let boundary = next_start - SONIQO_BUNDLE_SEAM_SAMPLES as f64 / 2.0;
            if local_start < boundary {
                break;
            }
            member_index += 1;
        }

        let member = &bundle.members[member_index];
        // Die Zeit wird in den Tonbereich IHRES Mitglieds geklemmt. Ohne das
        // konnte ein Wort hinter dem letzten Mitglied mit einem Zeitstempel
        // weit jenseits seines Tons angehaengt werden (zweisekuendiges
        // Mitglied, Wort bei buendel-lokal 30 s = rund 28 s daneben), und ein
        // Wort vor dem ersten landete bei Aufnahmezeit null statt am
        // `sample_start` des ersten. Beides sind Zeiten, fuer die es in der
        // Aufnahme kein Wort gibt -- wer im Abspieler dorthin springt, findet
        // Stille oder einen fremden Satz.
        let member_start_seconds = member.sample_start as f64 / rate;
        let member_end_seconds = member.sample_end as f64 / rate;
        let within = local_start / rate - local_starts[member_index] as f64 / rate;
        // Der Anfang laesst der Mindestdauer Platz, damit das Wort auch dann
        // ganz im Mitglied bleibt, wenn es an dessen Ende geklemmt wird.
        let latest_start_seconds =
            (member_end_seconds - MIN_WORD_DURATION_SECONDS).max(member_start_seconds);
        let start_seconds =
            (member_start_seconds + within).clamp(member_start_seconds, latest_start_seconds);
        // Dieselbe Untergrenze wie im Anbieter: `f64::EPSILON` stand hier
        // vorher und ist rechnerisch wirkungslos -- relativ zu 1,0 und damit
        // ab wenigen Sekunden Aufnahmezeit ein No-op.
        let end_seconds = (start_seconds + (word.end_seconds - word.start_seconds).max(0.0))
            .min(member_end_seconds)
            .max(start_seconds + MIN_WORD_DURATION_SECONDS);

        per_member[member_index].push(anlg_transcribe_soniqo::TranscriptWord {
            text: word.text.clone(),
            start_seconds,
            end_seconds,
            // Die ZEIT wird hier umgerechnet, die SICHERHEIT nicht -- sie
            // gehoert dem Wort, nicht seiner Lage in der Aufnahme.
            confidence: word.confidence,
        });
    }

    Some(
        bundle
            .members
            .iter()
            .zip(per_member)
            .filter_map(|(member, words)| {
                if words.is_empty() {
                    return None;
                }
                let text = words
                    .iter()
                    .map(|word| word.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");

                Some(anlg_transcribe_soniqo::FileTranscriptChunk {
                    text,
                    start_seconds: member.sample_start as f64 / rate,
                    duration_seconds: (member.sample_end - member.sample_start) as f64 / rate,
                    words,
                })
            })
            .collect(),
    )
}

/// Welcher der beiden Wege gilt fuer dieses Buendel?
///
/// Die Entscheidung sitzt in einer eigenen Funktion, damit sie pruefbar ist.
/// Ein Test auf den Wahrheitswert des Schalters allein sagt nichts darueber,
/// WAS der Standard tut -- und genau das ist die Zusicherung, die
/// `SoniqoTuning::PRODUCTION` gibt.
pub(super) struct BundleChunks {
    pub chunks: Vec<anlg_transcribe_soniqo::FileTranscriptChunk>,
    /// Schalter an, aber die Wortliste passte nicht zum Modelltext. Dann geht
    /// es den alten Weg -- laut, nicht still.
    pub fell_back_from_word_timings: bool,
}

pub(super) fn soniqo_chunks_for_bundle(
    bundle: &SoniqoChunkBundle,
    text: &str,
    words: &[anlg_transcribe_soniqo::TranscriptWord],
) -> BundleChunks {
    // Fork (02.09.2026): mit gemessenen Wortzeiten sagt der Erkenner, welcher
    // Abschnitt ein Wort gesprochen hat. Ohne sie -- und ohne den Schalter --
    // bleibt es beim Schaetzen ueber `split_bundle_text`.
    if !super::tuning::soniqo_tuning().word_timings {
        return BundleChunks {
            chunks: soniqo_bundle_transcript_chunks(bundle, text),
            fell_back_from_word_timings: false,
        };
    }

    match soniqo_bundle_transcript_chunks_from_words(bundle, text, words) {
        Some(chunks) => BundleChunks {
            chunks,
            fell_back_from_word_timings: false,
        },
        None => BundleChunks {
            chunks: soniqo_bundle_transcript_chunks(bundle, text),
            fell_back_from_word_timings: true,
        },
    }
}

/// Traegt die Wortliste denselben Text wie das Modell? Verglichen wird ohne
/// Leerraum: WO das Modell trennt, entscheidet seine eigene Wortgruppierung,
/// aber WELCHE Zeichen es gesagt hat, muss uebereinstimmen. Weicht auch nur
/// ein Zeichen ab, ist die Liste keine verlaessliche Zerlegung des Textes.
fn word_list_matches_text(text: &str, words: &[anlg_transcribe_soniqo::TranscriptWord]) -> bool {
    let from_text = text.chars().filter(|c| !c.is_whitespace());
    let from_words = words
        .iter()
        .flat_map(|word| word.text.chars())
        .filter(|c| !c.is_whitespace());
    from_text.eq(from_words)
}

/// Uebersetzt ein transkribiertes Buendel zurueck in Transkript-Abschnitte --
/// einen je Sprech-Abschnitt, mit dessen ORIGINALEN Zeitankern. Das ist die
/// Stelle, an der die Buendelung fuer die Zeitachse folgenlos bleibt.
pub(super) fn soniqo_bundle_transcript_chunks(
    bundle: &SoniqoChunkBundle,
    text: &str,
) -> Vec<anlg_transcribe_soniqo::FileTranscriptChunk> {
    bundle
        .members
        .iter()
        .zip(split_bundle_text(text.trim(), &bundle.member_weights()))
        .filter_map(|(member, member_text)| {
            let member_text = member_text.trim();
            if member_text.is_empty() {
                return None;
            }

            Some(anlg_transcribe_soniqo::FileTranscriptChunk {
                text: member_text.to_string(),
                start_seconds: member.sample_start as f64 / TARGET_SAMPLE_RATE as f64,
                duration_seconds: (member.sample_end - member.sample_start) as f64
                    / TARGET_SAMPLE_RATE as f64,
                words: Vec::new(),
            })
        })
        .collect()
}

pub(super) fn soniqo_language_hint(language: Option<&str>) -> Option<String> {
    let language = language?.trim();
    if language.is_empty() {
        return None;
    }

    language
        .split(['-', '_'])
        .next()
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
}

pub(super) fn soniqo_batch_progress(completed_chunks: usize, total_chunks: usize) -> f64 {
    if total_chunks == 0 {
        return SONIQO_DIARIZATION_PROGRESS_END;
    }

    let ratio = completed_chunks as f64 / total_chunks as f64;
    (SONIQO_DIARIZATION_PROGRESS_END + ratio * SONIQO_PROGRESS_RANGE).min(SONIQO_PROGRESS_MAX)
}

/// Fortschritt WAEHREND der Sprechertrennung, im Band 5 % bis 20 %.
///
/// `within` ist der geschaetzte Anteil des gerade laufenden Kanals. Die
/// Schaetzung kommt aus der Uhr, nicht aus dem Verfahren -- die Swift-Seite
/// meldet zwischendurch nichts. Deshalb ist sie gedeckelt: der Balken erreicht
/// das Bandende erst, wenn ein Kanal wirklich fertig ist.
pub(super) fn soniqo_diarization_progress(
    completed_channels: usize,
    total_channels: usize,
    within: f64,
) -> f64 {
    if total_channels == 0 {
        return SONIQO_PROGRESS_PLANNED;
    }

    let within = within.clamp(0.0, SONIQO_DIARIZATION_WITHIN_MAX);
    let done = (completed_channels as f64 + within) / total_channels as f64;
    let ratio = done.clamp(0.0, 1.0);
    SONIQO_PROGRESS_PLANNED + ratio * (SONIQO_DIARIZATION_PROGRESS_END - SONIQO_PROGRESS_PLANNED)
}

/// Wie lange die Trennung eines Kanals voraussichtlich braucht.
pub(super) fn soniqo_diarization_expected(sample_count: usize) -> Duration {
    let audio_seconds = sample_count as f64 / TARGET_SAMPLE_RATE as f64;
    Duration::from_secs_f64((audio_seconds / SONIQO_DIARIZATION_REALTIME_FACTOR).max(0.001))
}

/// Anteil des laufenden Kanals, geschaetzt aus vergangener gegen erwartete Zeit.
pub(super) fn soniqo_diarization_within(elapsed: Duration, expected: Duration) -> f64 {
    if expected.is_zero() {
        return SONIQO_DIARIZATION_WITHIN_MAX;
    }
    (elapsed.as_secs_f64() / expected.as_secs_f64()).clamp(0.0, SONIQO_DIARIZATION_WITHIN_MAX)
}

/// Was mit EINEM Kanal geschehen soll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SoniqoChannelDiarization {
    /// Nicht trennen. Entweder traegt der Kanal keine Sprache, oder der Kanal
    /// selbst ist bereits die Sprecher-Zuordnung.
    Skip,
    /// Mit vorgegebener Sprecherzahl trennen.
    Exact(usize),
    /// Trennen und die Sprecherzahl vom Verfahren bestimmen lassen.
    Inferred,
}

/// Unter diesem Effektivwert traegt ein Kanal keine Sprache.
///
/// **Angehoben am 21.09.2026 von 0,0001 auf 0,002, und zugleich um eine
/// zweite Bedingung ergaenzt (siehe [`SONIQO_CHANNEL_SILENT_RATIO`]).**
///
/// Der alte Wert war gegen DIGITALE Stille kalibriert: an einer Raumaufnahme
/// gemessen trug der Systemton damals −160,8 dB, also praktisch nichts. Ein
/// Kanal mit leisem GRUNDRAUSCHEN liegt darueber, ohne ein Wort zu tragen --
/// und weil die Planung eine Aufnahme mit zwei tonfuehrenden Kanaelen fuer
/// eine Videokonferenz haelt, bekam genau die Raumaufnahme, fuer die die
/// Trennung gebaut ist, gar keine.
///
/// **Die Messung, die den Wert traegt (21.09.2026).** 157 Aufnahmen aus dem
/// Bestand, je Kanal der Effektivwert gegen die Frage gehalten, ob dieser
/// Kanal spaeter Woerter beigetragen hat -- also gegen die Wahrheit, die zum
/// Planungszeitpunkt fehlt und im Nachhinein vorliegt. 154 leisere Kanaele,
/// davon 128 mit Sprache und 26 ohne.
///
/// Zwei Befunde daraus, beide gegen die naheliegende Loesung:
///
/// 1. **Ein reines Verhaeltnis-Kriterium traegt nicht.** Echte Gegenseiten
///    reichen bis auf 0,0012 des lautesten Kanals herunter, stille bis 0,85
///    hinauf -- die Verteilungen ueberlappen ueber fast den ganzen Bereich.
/// 2. **Ein reiner Effektivwert traegt auch nicht.** Die leiseste echte
///    Gegenseite liegt bei 0,00021, der lauteste stille Kanal bei 0,0427.
///
/// Erst BEIDE Bedingungen zusammen schneiden sauber. Innerhalb des Bandes
/// "hoechstens 6 % des lautesten Kanals" enden die stillen Kanaele bei
/// 0,00101 und der naechste echte beginnt bei 0,00384 -- dazwischen liegt
/// nichts. 0,002 sitzt mittig in dieser Luecke, mit Faktor 2 Abstand nach
/// unten und Faktor 1,9 nach oben.
///
/// Wirkung, gemessen: 19 der 26 stillen Kanaele werden erkannt (vorher 9),
/// 4 der 128 echten sind betroffen. Die sieben, die bleiben, sind mit
/// Lautstaerke nicht zu fassen: sie liegen bei 0,0012 bis 0,0427 und
/// zugleich ueber dem Verhaeltnis-Band. Dafuer braeuchte es eine Aussage
/// ueber SPRACHE statt ueber Pegel.
///
/// **Die Fehlerrichtung ist bewusst gewaehlt und asymmetrisch.** Wird ein
/// echter Kanal faelschlich verworfen, bleibt genau ein Kanal uebrig, und
/// der bekommt eine Trennung, die er vorher nicht hatte; der verworfene wird
/// weiterhin vollstaendig TRANSKRIBIERT (diese Schwelle beruehrt die
/// Transkription nicht, siehe [`SONIQO_DIRECT_MIC_MIN_RMS`]) und behaelt
/// seine eigenen Woerter. Es geht also nichts verloren ausser einer
/// Sprechertrennung auf einem Kanal, auf dem ohnehin meist ein einzelner
/// Mensch sitzt. Wird dagegen Rauschen faelschlich behalten, bekommt
/// NIEMAND eine Trennung -- das war der Normalfall und der eigentliche
/// Schaden. Alle vier betroffenen echten Kanaele der Messung sind
/// Mikrofonkanaele von Videokonferenzen mit wenig Redeanteil.
///
/// Nicht zu verwechseln mit [`SONIQO_DIRECT_MIC_MIN_RMS`] (0,0008): das ist
/// das Stille-Gatter je ABSCHNITT vor dem Erkenner, hier geht es um den ganzen
/// Kanal.
pub(super) const SONIQO_CHANNEL_SILENT_RMS: f64 = 0.002;

/// Zweite Bedingung: wie leise ein Kanal gegenueber dem lautesten sein muss,
/// um als still zu gelten.
///
/// Ohne sie wuerde der angehobene Effektivwert auch bei einer durchgehend
/// leise aufgenommenen Videokonferenz zuschlagen, in der BEIDE Seiten unter
/// 0,002 liegen. Die Bedingung bindet das Urteil an die Aufnahme selbst
/// statt an einen absoluten Pegel: 6 % des lautesten Kanals.
///
/// Gemessen liegt die leiseste echte Gegenseite, die dieses Band ueberhaupt
/// erreicht, bei 0,0012 -- sie faellt durch den Effektivwert, nicht durch
/// das Verhaeltnis. Umgekehrt beginnen die echten Gegenseiten oberhalb des
/// Bandes bei 0,0778, also mit deutlichem Abstand.
pub(super) const SONIQO_CHANNEL_SILENT_RATIO: f64 = 0.06;

/// Unterhalb dieses Wertes ist ein Kanal IMMER still, egal was daneben liegt.
///
/// Das ist der alte Wert von [`SONIQO_CHANNEL_SILENT_RMS`] und bleibt als
/// eigene Stufe stehen, weil die Verhaeltnis-Bedingung genau einen Fall
/// nicht fassen kann: liegen BEIDE Kanaele gleich tief, ist das Verhaeltnis
/// des leiseren zum lautesten 1,0 und damit weit ueber dem Band -- eine
/// Aufnahme aus zwei toten Kanaelen haette so zwei sprechende bekommen.
///
/// Gemessen traegt digitale Stille 9,1e-9 und der leiseste je gemessene
/// echte Kanal 0,00021, also das 23.000-fache. Zwischen beiden ist reichlich
/// Platz fuer diese Grenze.
///
/// **Eine bewusst in Kauf genommene Verschiebung um einen Punkt.** Bis zum
/// 21.09.2026 lautete die Bedingung "sprechend, wenn `rms > 0,0001`", ein
/// Kanal mit EXAKT diesem Wert galt also als still. Jetzt heisst es "still,
/// wenn `rms < 0,0001`", derselbe Kanal gilt also als sprechend. Der
/// Unterschied betrifft genau einen Punkt einer stetigen Groesse: ein
/// Effektivwert ueber Millionen Abtastwerten trifft ihn nicht, und selbst
/// wenn, waere die Folge eine Trennung mehr statt einer weniger -- die
/// Richtung, die dieser Runde ohnehin zugrunde liegt. Kein Code dagegen; es
/// steht hier, damit niemand es spaeter fuer ein Versehen haelt.
pub(super) const SONIQO_CHANNEL_DIGITAL_SILENCE_RMS: f64 = 0.0001;

/// Welche Kanaele tragen Sprache?
///
/// Ein Kanal gilt als still, wenn er BEIDE Bedingungen erfuellt: absolut
/// leiser als [`SONIQO_CHANNEL_SILENT_RMS`] und zugleich hoechstens
/// [`SONIQO_CHANNEL_SILENT_RATIO`] des lautesten Kanals. Die Begruendung mit
/// der Messung steht bei den beiden Konstanten.
pub(super) fn soniqo_speech_channels(channel_rms: &[f64]) -> Vec<usize> {
    channel_rms
        .iter()
        .enumerate()
        .filter(|(channel_index, _)| !soniqo_channel_is_silent(channel_rms, *channel_index))
        .map(|(channel_index, _)| channel_index)
        .collect()
}

/// Gilt dieser Kanal als still -- und warum?
///
/// Gibt zusaetzlich das gemessene Verhaeltnis zurueck, damit die
/// Planungs-Protokollzeile nicht nur das Urteil nennt, sondern auch die Zahl
/// dahinter. Ohne sie ist ein falsches Urteil im Nachhinein nicht
/// nachzuvollziehen, und genau das hat diesen Fehler monatelang verdeckt.
pub(super) fn soniqo_channel_silence_ratio(channel_rms: &[f64], channel_index: usize) -> f64 {
    debug_assert!(
        channel_index < channel_rms.len(),
        "Kanal {channel_index} gibt es nicht; die Liste hat {} Eintraege",
        channel_rms.len()
    );
    let rms = channel_rms.get(channel_index).copied().unwrap_or_default();
    let loudest = channel_rms
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(0.0f64, |highest, value| highest.max(value));
    if loudest <= 0.0 || !rms.is_finite() {
        0.0
    } else {
        rms / loudest
    }
}

/// Wie sicher ist die Einstufung eines Kanals?
///
/// Der Unterschied ist nicht kosmetisch: er entscheidet, wie hart auf dem
/// verbleibenden Kanal geplant werden darf. Bei digitaler Stille ist die
/// Lage eindeutig, bei einer nur RELATIVEN Einstufung ist sie eine gut
/// begruendete Vermutung -- und auf eine Vermutung hin wird keine
/// Sprecherzahl erzwungen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SoniqoChannelVoice {
    /// Kein verwertbarer Pegel oder digitale Stille. Sicher.
    DigitallySilent,
    /// Leise UND deutlich leiser als der Nachbar. Wahrscheinlich Rauschen,
    /// aber nicht sicher.
    RelativelySilent,
    /// Traegt Sprache.
    Speaking,
}

impl SoniqoChannelVoice {
    pub(super) fn is_silent(self) -> bool {
        !matches!(self, Self::Speaking)
    }

    /// Ein Wort fuers Protokoll.
    ///
    /// Die Planungszeile nannte bisher nur Zutaten (Pegel, Schwellen) und
    /// ueberliess das Zusammenrechnen dem Leser. Genau daran ist der
    /// Fehlalarm monatelang vorbeigelaufen: die Zahlen standen da, das Urteil
    /// stand da, aber nicht, welche Bedingung es getragen hat.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::DigitallySilent => "digital",
            Self::RelativelySilent => "relativ",
            Self::Speaking => "spricht",
        }
    }
}

pub(super) fn soniqo_channel_voice(
    channel_rms: &[f64],
    channel_index: usize,
) -> SoniqoChannelVoice {
    let rms = channel_rms.get(channel_index).copied().unwrap_or_default();
    // Ein Wert, der keine Zahl ist, ist keine Stimme. Ohne diese Zeile gilt
    // NaN als sprechend, weil JEDER Vergleich mit NaN falsch ist -- ein
    // kaputter Pegel haette also die Trennung der ganzen Aufnahme verhindert,
    // und zwar still.
    if !rms.is_finite() {
        return SoniqoChannelVoice::DigitallySilent;
    }
    // Erste Stufe: so leise, dass die Frage nach dem Nachbarn sich nicht
    // stellt. Faengt auch den Fall, in dem BEIDE Kanaele tot sind und das
    // Verhaeltnis deshalb 1,0 betraegt.
    if rms < SONIQO_CHANNEL_DIGITAL_SILENCE_RMS {
        return SoniqoChannelVoice::DigitallySilent;
    }
    // Zweite Stufe: leise UND deutlich leiser als der Rest der Aufnahme.
    if rms < SONIQO_CHANNEL_SILENT_RMS
        && soniqo_channel_silence_ratio(channel_rms, channel_index) < SONIQO_CHANNEL_SILENT_RATIO
    {
        return SoniqoChannelVoice::RelativelySilent;
    }
    SoniqoChannelVoice::Speaking
}

pub(super) fn soniqo_channel_is_silent(channel_rms: &[f64], channel_index: usize) -> bool {
    soniqo_channel_voice(channel_rms, channel_index).is_silent()
}

/// Auf welchem Kanal wird getrennt, und mit welcher Sprecherzahl?
///
/// Fork (07.09.2026). Vorher hing das allein am Kanal-INDEX: bei zwei Kanaelen
/// wurde ausschliesslich Kanal 1 (Systemton) getrennt, Kanal 0 (Mikrofon) fiel
/// durch. Dahinter stand die Annahme, am Mikrofon sitze immer nur der Nutzer
/// und alle anderen kaemen ueber den Rechner. Bei einer Aufnahme im Raum ist
/// das genau umgekehrt -- dort ist der Systemton digitale Stille und das
/// Mikrofon traegt alles. Gemessen wird deshalb, welcher Kanal Ton fuehrt,
/// statt es aus seiner Nummer zu schliessen.
///
/// Die Entscheidung faellt an der ZAHL der tonfuehrenden Kanaele, nicht an
/// ihrer Nummer -- sonst waere die alte Annahme nur durch eine neue ersetzt,
/// die den anderen Einzelfall trifft:
///
/// - **Mehrere Kanaele tragen Ton** = Regelfall Videokonferenz. Der Kanal IST
///   die Sprecher-Zuordnung und damit die verlaesslichere Information. Hier
///   bleibt alles wortgleich wie vorher.
/// - **Genau ein Kanal traegt Ton** = Raum- oder Mono-Aufnahme. Nur dort wird
///   getrennt, und zwar auf der VOLLEN Sprecherzahl: das alte `total - 1`
///   zog den Nutzer ab, weil er auf dem anderen Kanal sass. Bei einer
///   Raumaufnahme sitzt er mit am Tisch.
/// - **Kein Kanal traegt Ton**: nichts zu trennen.
pub(super) fn soniqo_diarization_speaker_count(
    num_speakers: Option<u32>,
    channel_rms: &[f64],
    channel_index: usize,
) -> SoniqoChannelDiarization {
    // Die RELATIVE Still-Regel (leise und deutlich leiser als der Nachbar)
    // gilt nur fuer eine oder zwei Spuren. Sie ist an genau zwei Spuren
    // gemessen und an genau zwei gedacht: "deutlich leiser als der lauteste"
    // heisst bei drei Kanaelen etwas anderes als bei zweien, und im Bestand
    // gibt es solche Aufnahmen nicht.
    //
    // Die DIGITALE Stille gilt dagegen weiter fuer alle Kanalzahlen -- sie
    // galt schon vor dem 21.09.2026 und ist keine Vermutung, sondern die
    // Feststellung, dass da nichts ist. Sie hier mit wegzunehmen war ein
    // Fehler dieser Runde: eine Dreikanal-Aufnahme mit zwei toten Spuren
    // bekam dadurch gar keine Trennung mehr, obwohl sie vorher eine bekam.
    let speech_channels = if channel_rms.len() <= 2 {
        soniqo_speech_channels(channel_rms)
    } else {
        (0..channel_rms.len())
            .filter(|channel_index| {
                soniqo_channel_voice(channel_rms, *channel_index)
                    != SoniqoChannelVoice::DigitallySilent
            })
            .collect()
    };
    let requested = num_speakers.and_then(|count| usize::try_from(count).ok());

    match speech_channels.as_slice() {
        [] => SoniqoChannelDiarization::Skip,
        [only_channel] => {
            if channel_index != *only_channel {
                return SoniqoChannelDiarization::Skip;
            }
            // Wie hart hier geplant werden darf, haengt daran, wie SICHER
            // der andere Kanal als still gilt -- und seit dem 21.09.2026 kann
            // ein Zwei-Kanal-Fall ueberhaupt erst auf einen Kanal fallen.
            // Vorher war dieser Zweig nur fuer echte Ein-Kanal-Aufnahmen
            // erreichbar, und die Frage stellte sich nicht.
            //
            // Die Teilnehmerzahl aus der Oberflaeche SCHLIESST DEN NUTZER EIN
            // (`useRunBatch.ts` haengt `selfHumanId` an).
            let other_channel = match (channel_rms.len(), *only_channel) {
                (2, 0) => Some((1usize, soniqo_channel_voice(channel_rms, 1))),
                (2, 1) => Some((0usize, soniqo_channel_voice(channel_rms, 0))),
                _ => None,
            };

            match other_channel {
                // Der andere Kanal ist nur RELATIV still: wahrscheinlich eine
                // Raumaufnahme, aber nicht sicher. Auf eine Vermutung hin wird
                // keine Sprecherzahl erzwungen.
                Some((_, SoniqoChannelVoice::RelativelySilent)) if *only_channel == 0 => {
                    // Sitzt auf dem Mikrofon doch nur der Nutzer, findet die
                    // freie Trennung in aller Regel EINE Stimme, und seine
                    // Selbst-Zuordnung bleibt erhalten -- die Anzeige nimmt
                    // einen Kanal erst bei mehreren Sprecherindizes in die
                    // Mehrsprecher-Behandlung. Ein erzwungenes `Exact(n >= 2)`
                    // zerlegt ihn dagegen garantiert, und dann steht der
                    // Nutzer im Transkript als "Speaker 1/2".
                    //
                    // Ein Sprecher zu viel im echten Raumfall ist von Hand zu
                    // beheben. Ein Nutzer ohne Namen ist es nicht.
                    SoniqoChannelDiarization::Inferred
                }
                Some((_, SoniqoChannelVoice::RelativelySilent)) => {
                    // Uebrig ist der Systemton. Der Nutzer sitzt per
                    // Konstruktion nicht darauf -- er spricht ins Mikrofon,
                    // das gerade als still eingestuft wurde. Also genau das
                    // Verhalten, das dieser Kanal im Zwei-Kanal-Fall bisher
                    // hatte: Teilnehmerzahl minus Nutzer, und ohne Vorgabe
                    // gar nichts.
                    match requested.map(|total| total.saturating_sub(1)) {
                        Some(count) if count >= 2 => SoniqoChannelDiarization::Exact(count),
                        _ => SoniqoChannelDiarization::Skip,
                    }
                }
                // Der andere Kanal ist digital still, oder es gibt gar keinen
                // zweiten: sichere Lage, Verhalten wie vor dieser Aenderung.
                _ => match requested {
                    Some(total) if total >= 2 => SoniqoChannelDiarization::Exact(total),
                    // Auch ohne Teilnehmerliste wird getrennt. Sie ist eine
                    // Praezisierung, keine Bedingung.
                    _ => SoniqoChannelDiarization::Inferred,
                },
            }
        }
        _ => {
            let Some(total) = requested else {
                return SoniqoChannelDiarization::Skip;
            };
            let count = match channel_rms.len() {
                2 if channel_index == 1 => total.saturating_sub(1),
                _ => return SoniqoChannelDiarization::Skip,
            };
            if count >= 2 {
                SoniqoChannelDiarization::Exact(count)
            } else {
                SoniqoChannelDiarization::Skip
            }
        }
    }
}

pub(super) fn collect_soniqo_channel_transcripts<I>(
    transcripts: I,
) -> std::result::Result<Vec<anlg_transcribe_soniqo::FileTranscript>, String>
where
    I: IntoIterator<Item = std::result::Result<anlg_transcribe_soniqo::FileTranscript, String>>,
{
    let mut output = Vec::new();
    let mut successful_channels = 0usize;
    let mut failed_channels = 0usize;

    for transcript in transcripts {
        match transcript {
            Ok(transcript) => {
                successful_channels += 1;
                output.push(transcript);
            }
            Err(error) => {
                failed_channels += 1;
                tracing::warn!(
                    anarlog.stt.provider.name = "soniqo",
                    error = %error,
                    "soniqo_channel_transcription_failed"
                );
                output.push(anlg_transcribe_soniqo::FileTranscript::new(
                    String::new(),
                    0.05,
                ));
            }
        }
    }

    if successful_channels == 0 && failed_channels > 0 {
        return Err(format!(
            "Soniqo failed to transcribe all {failed_channels} audio channel(s)."
        ));
    }

    Ok(output)
}

fn soniqo_channel_plan(
    model: anlg_transcribe_soniqo::SoniqoModel,
    channel_index: usize,
    channel: ResampledChannelFile,
    is_direct_mic: bool,
) -> SoniqoChannelPlan {
    let duration_seconds = channel.sample_count as f64 / TARGET_SAMPLE_RATE as f64;
    let sample_count = channel.sample_count;
    let chunk_strategy = soniqo_chunk_strategy(model);
    let (strategy_label, chunk_count) = match chunk_strategy {
        SoniqoChunkStrategy::Fixed { max_samples } => {
            ("fixed", Some(sample_count.div_ceil(max_samples)))
        }
        SoniqoChunkStrategy::SpeechAware => ("speech-aware", None),
    };
    tracing::info!(
        anarlog.stt.provider.name = "soniqo",
        anarlog.stt.model = %model,
        channel.index = channel_index,
        channel.duration_seconds = duration_seconds,
        channel.sample_count = sample_count,
        chunk.strategy = strategy_label,
        chunk.count = chunk_count.unwrap_or_default(),
        chunk.count_known = chunk_count.is_some(),
        "soniqo_channel_chunked"
    );

    SoniqoChannelPlan {
        channel_index,
        duration_seconds,
        is_direct_mic,
        chunk_strategy,
        channel,
    }
}

// Fork-Abweichung (31.08.2026): ParakeetBatch bekommt dieselbe sprachbewusste
// Zerlegung wie jedes andere Modell. Upstream schnitt hier in STARRE Fenster von
// 29,5 s, und `batch_words_from_chunks` verankert jedes Wortpaket am
// FENSTERANFANG statt am Sprechbeginn -- ein Wort kann damit bis zu 29,5 s zu
// frueh stehen.
//
// An echtem Material gemessen (des Betreibers 20-Minuten-Teammeeting, Session
// c3d4e5f6): ALLE 30 Blockanfaenge beider Kanaele sind exakte Vielfache von
// 29,5 s; im produktiven anarlog ebenso alle 153. Der Schaden ist nicht
// kosmetisch: bei 383,5 s sagt der Systemton "Dann darfst du, Mads" und das
// Mikrofon "Danke Bo ..." -- eine Uebergabe, die im selben Kasten landet und
// im Transkript wie Durcheinanderreden aussieht. Genau das machte das
// Transkript fuer den Prinzipal unbrauchbar.
//
// Warum das sicher ist: der VAD-Chunker deckelt bei max_chunk_duration = 25 s
// (crates/audio-chunking/src/vad/session.rs:31) und bleibt damit unter dem
// 29,5-s-Fenster, das das Parakeet-Modell selbst verlangt
// (SONIQO_PARAKEET_MAX_CHUNK_SAMPLES). Die Modellgrenze wird also eingehalten,
// nur die Schnittpunkte liegen jetzt an Sprechpausen.
//
// Was das NICHT loest: innerhalb eines Chunks bleiben die Wortzeiten
// gleichverteilt (0,4 s je Wort). Fuer Lesen und Zuordnen reicht das, fuer
// sekundengenaues Anspringen nicht. Echte Wortzeiten liefert erst der
// TDT-Dekoder von speech-swift, der sie berechnet und verwirft -- bewusst
// nicht angefasst (Fremdcode, exact-Pin, ein bis zwei Tage plus dauerhafte
// Wartungslast).
//
// Upstream-Naehe: diese Datei war bis hier byte-identisch mit desktop_v1.4.14.
// Bei kuenftigen Spruengen ist soniqo_chunk_strategy eine Konfliktflaeche --
// die Fixed-Variante bleibt deshalb im Enum erhalten, damit ein Merge nichts
// wegwirft, was Upstream noch braucht.
//
// Die Messbank (Testplan 02.09.2026) kann die Zerlegung ueber
// `super::tuning::set_soniqo_tuning` umstellen. Ohne diesen Aufruf -- und die
// App ruft ihn nie -- kommt hier unveraendert `SpeechAware` heraus.
pub(super) fn soniqo_chunk_strategy(
    _model: anlg_transcribe_soniqo::SoniqoModel,
) -> SoniqoChunkStrategy {
    match super::tuning::soniqo_tuning().chunking {
        super::tuning::SoniqoChunkingMode::SpeechAware => SoniqoChunkStrategy::SpeechAware,
        super::tuning::SoniqoChunkingMode::Fixed { max_samples } => {
            SoniqoChunkStrategy::Fixed { max_samples }
        }
    }
}

/// Das Ergebnis EINES getrennten Kanals.
///
/// `failed_reason` traegt den Grund, wenn die Trennung fehlschlug und das
/// Transkript trotzdem weitergeht. Vorher war dieser Fall vom Erfolg nicht zu
/// unterscheiden: der Mensch bekam ein Transkript ohne Sprecher und ohne ein
/// einziges Zeichen dafuer, dass der Schritt ueberhaupt gelaufen war.
pub(super) struct ChannelDiarization {
    pub(super) segments: Vec<anlg_transcribe_soniqo::DiarizationSegment>,
    pub(super) failed_reason: Option<String>,
}

fn diarize_soniqo_channel_file(
    model: anlg_transcribe_soniqo::SoniqoModel,
    channel_index: usize,
    channel: &ResampledChannelFile,
    speaker_count: Option<usize>,
    progress: Option<&SoniqoProgressReporter>,
    diarized_channels_done: usize,
    diarized_channel_count: usize,
) -> std::result::Result<ChannelDiarization, String> {
    ensure_local_batch_running(progress)?;
    ensure_soniqo_diarization_within_limit(channel.sample_count)?;
    let mut reader = hound::WavReader::open(channel.file.path()).map_err(|e| e.to_string())?;
    let samples = reader
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    ensure_local_batch_running(progress)?;

    let started_at = Instant::now();
    let plan = anlg_transcribe_soniqo::ChunkPlan::new(
        SONIQO_DIARIZATION_CHUNK_SAMPLES,
        SONIQO_DIARIZATION_MIN_TAIL_SAMPLES,
    )
    .map_err(|error| error.to_string())?;

    // Zwischen den Abschnitten passieren zwei Dinge, die vorher gar nicht
    // passierten: der Abbruch wird gehoert, und der Balken bekommt etwas
    // Echtes zu melden. Vorher lief ein abgebrochener Zwei-Stunden-Lauf alle
    // uebrigen Abschnitte zu Ende, und die Anzeige war bis zum Schluss eine
    // reine Uhr-Schaetzung.
    struct ChunkReporter<'a> {
        progress: Option<&'a SoniqoProgressReporter>,
        channel_index: usize,
        diarized_channels_done: usize,
        diarized_channel_count: usize,
    }

    impl anlg_transcribe_soniqo::ChunkObserver for ChunkReporter<'_> {
        fn should_continue(&mut self, finished_chunks: usize, total_chunks: usize) -> bool {
            if local_batch_is_cancelled(self.progress) {
                return false;
            }
            if let Some(progress) = self.progress
                && total_chunks > 1
            {
                progress.emit(soniqo_diarization_progress(
                    self.diarized_channels_done,
                    self.diarized_channel_count,
                    (finished_chunks as f64 / total_chunks as f64)
                        .min(SONIQO_DIARIZATION_WITHIN_MAX),
                ));
            }
            true
        }

        fn speaker_count_differs(
            &mut self,
            expected: usize,
            found: usize,
            highest_remaining_similarity: Option<f32>,
        ) {
            // Nur ins Protokoll, bewusst kein Hinweis an den Nutzer: eine
            // abweichende Sprecherzahl ist kein Fehler, sondern eine Aussage
            // des Verfahrens ueber die Aufnahme. Wer sie korrigieren will,
            // legt im Transkript zwei Etiketten zusammen -- das ist billiger
            // und ehrlicher, als sie hier zu erzwingen (Begruendung bei
            // `diarize_chunks_with`).
            tracing::info!(
                anarlog.stt.provider.name = "soniqo",
                channel.index = self.channel_index,
                diarization.speakers_expected = expected,
                diarization.speakers_found = found,
                diarization.highest_remaining_similarity = ?highest_remaining_similarity,
                "soniqo_speaker_count_differs_from_hint"
            );
        }
    }

    let mut observer = ChunkReporter {
        progress,
        channel_index,
        diarized_channels_done,
        diarized_channel_count,
    };

    // Eine gescheiterte Trennung darf das TRANSKRIPT nicht kosten. Ohne diese
    // Glaettung schluege ein Fehler bis in den Stapellauf durch, und der Mensch
    // stuende statt ohne Sprecher ganz ohne Text da -- ein schlechterer Tausch
    // als der, den wir hier gerade abschaffen. Der Fehler wird protokolliert,
    // nicht verschluckt -- und seit dem 21.09.2026 auch SICHTBAR gemeldet
    // (siehe Rueckgabewert).
    let mut failed_reason = None;
    let segments = match anlg_transcribe_soniqo::diarize_samples_chunked(
        model,
        &samples,
        speaker_count,
        plan,
        &mut observer,
    ) {
        Ok(segments) => segments,
        Err(error) => {
            let message = error.to_string();
            // Ein Abbruch ist kein Fehlschlag. Wer abbricht, braucht keinen
            // Hinweis darueber, dass das Abgebrochene nicht fertig wurde.
            if message.contains(anlg_transcribe_soniqo::CHUNKED_DIARIZATION_CANCELLED) {
                return Err(LOCAL_BATCH_CANCELLED.to_string());
            }
            tracing::warn!(
                anarlog.stt.provider.name = "soniqo",
                anarlog.stt.model = %model,
                channel.index = channel_index,
                channel.seconds = samples.len() as f64 / TARGET_SAMPLE_RATE as f64,
                elapsed_ms = started_at.elapsed().as_millis() as u64,
                error = %message,
                "soniqo_channel_diarization_failed"
            );
            failed_reason = Some(message);
            Vec::new()
        }
    };
    ensure_local_batch_running(progress)?;
    tracing::info!(
        anarlog.stt.provider.name = "soniqo",
        anarlog.stt.model = %model,
        channel.index = channel_index,
        channel.seconds = samples.len() as f64 / TARGET_SAMPLE_RATE as f64,
        diarization.chunked = samples.len() > SONIQO_DIARIZATION_CHUNK_SAMPLES,
        speaker.count = ?speaker_count,
        diarization.speaker_count = segments
            .iter()
            .map(|segment| segment.speaker_index)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        segment.count = segments.len(),
        elapsed_ms = started_at.elapsed().as_millis() as u64,
        // Steht bewusst im Protokoll: eine langsame Trennung ist fast immer
        // eine Trennung in der falschen Bauart.
        swift.configuration = anlg_transcribe_soniqo::SWIFT_CONFIGURATION,
        "soniqo_channel_diarization_completed"
    );
    Ok(ChannelDiarization {
        segments,
        failed_reason,
    })
}

pub(super) fn ensure_soniqo_diarization_within_limit(
    sample_count: usize,
) -> std::result::Result<(), String> {
    if sample_count <= SONIQO_DIARIZATION_MAX_SAMPLES {
        return Ok(());
    }
    // Die Minutenzahl wird aus dem Deckel GERECHNET, nicht danebengeschrieben:
    // die alte Fassung sagte fuer immer "10 minutes", egal was oben stand.
    let max_minutes = SONIQO_DIARIZATION_MAX_SAMPLES / TARGET_SAMPLE_RATE as usize / 60;
    Err(format!(
        "Soniqo speaker diarization is limited to recordings up to {max_minutes} minutes to prevent excessive memory use."
    ))
}

/// Streicht jede geplante Trennung, deren Kanal ueber dem Deckel liegt.
///
/// Gibt `true` zurueck, wenn dabei mindestens eine Trennung wegfaellt -- die
/// Unterscheidung zwischen "war nie geplant" und "wurde gestrichen" ist der
/// ganze Zweck des Rueckgabewerts: nur der zweite Fall ist eine Nachricht an
/// den Nutzer.
pub(super) fn soniqo_apply_diarization_limit(
    plans: &mut [SoniqoChannelDiarization],
    channel_sample_counts: &[usize],
) -> bool {
    let mut skipped = false;
    for (channel_index, plan) in plans.iter_mut().enumerate() {
        if *plan == SoniqoChannelDiarization::Skip {
            continue;
        }
        let sample_count = channel_sample_counts
            .get(channel_index)
            .copied()
            .unwrap_or_default();
        if sample_count > SONIQO_DIARIZATION_MAX_SAMPLES {
            *plan = SoniqoChannelDiarization::Skip;
            skipped = true;
        }
    }
    skipped
}

fn transcribe_soniqo_channel_chunks(
    model: anlg_transcribe_soniqo::SoniqoModel,
    plan: SoniqoChannelPlan,
    language: Option<&str>,
    progress: Option<&SoniqoProgressReporter>,
    async_runtime: &tokio::runtime::Handle,
) -> std::result::Result<anlg_transcribe_soniqo::FileTranscript, String> {
    transcribe_soniqo_channel_chunks_with(
        model,
        plan,
        language,
        progress,
        async_runtime,
        transcribe_soniqo_samples,
    )
}

fn transcribe_soniqo_channel_chunks_with<F>(
    model: anlg_transcribe_soniqo::SoniqoModel,
    plan: SoniqoChannelPlan,
    language: Option<&str>,
    progress: Option<&SoniqoProgressReporter>,
    async_runtime: &tokio::runtime::Handle,
    mut transcribe: F,
) -> std::result::Result<anlg_transcribe_soniqo::FileTranscript, String>
where
    F: FnMut(
        anlg_transcribe_soniqo::SoniqoModel,
        &[f32],
        Option<&str>,
    ) -> std::result::Result<anlg_transcribe_soniqo::FileTranscript, String>,
{
    let mut transcript_chunks = Vec::new();
    let mut successful_chunks = 0usize;
    let mut failed_chunks = 0usize;
    let SoniqoChannelPlan {
        channel_index,
        duration_seconds,
        is_direct_mic,
        chunk_strategy,
        channel,
    } = plan;
    let mut chunks = SoniqoFileChunkIterator::new(channel.file, chunk_strategy, async_runtime)?;
    let mut bundler = SoniqoChunkBundler::new(
        soniqo_bundle_max_samples(chunk_strategy),
        soniqo_bundle_max_span_samples(),
    );
    let mut next_chunk_index = 0usize;
    let mut next_bundle_index = 0usize;
    let mut chunks_exhausted = false;

    loop {
        ensure_local_batch_running(progress)?;
        let bundle = if chunks_exhausted {
            match bundler.finish() {
                Some(bundle) => bundle,
                None => break,
            }
        } else {
            let Some(chunk) = chunks.next() else {
                chunks_exhausted = true;
                continue;
            };
            ensure_local_batch_running(progress)?;
            let chunk = chunk?;
            let chunk_index = next_chunk_index;
            next_chunk_index += 1;
            // Stille-Gate. `None` gibt es nur ueber die Messbank (Achse E);
            // die App laeuft immer mit `Some(SONIQO_DIRECT_MIC_MIN_RMS)`.
            let minimum_rms = super::tuning::soniqo_tuning().direct_mic_min_rms;
            let chunk_rms = audio_rms(&chunk.samples);
            if let Some(minimum_rms) = minimum_rms
                && is_direct_mic
                && chunk_rms < minimum_rms
            {
                tracing::info!(
                    anarlog.stt.provider.name = "soniqo",
                    anarlog.stt.model = %model,
                    channel.index = channel_index,
                    chunk.index = chunk_index,
                    audio.rms = chunk_rms,
                    audio.minimum_rms = minimum_rms,
                    "soniqo_direct_mic_chunk_skipped"
                );
                continue;
            }
            match bundler.push(chunk) {
                Some(bundle) => bundle,
                None => continue,
            }
        };

        let bundle_index = next_bundle_index;
        next_bundle_index += 1;
        let bundle_duration_ms = bundle.samples.len() * 1000 / TARGET_SAMPLE_RATE as usize;
        let chunk_started_at = Instant::now();
        tracing::info!(
            anarlog.stt.provider.name = "soniqo",
            anarlog.stt.model = %model,
            channel.index = channel_index,
            chunk.index = bundle_index,
            chunk.member_count = bundle.members.len(),
            chunk.sample_start = bundle
                .members
                .first()
                .map(|member| member.sample_start)
                .unwrap_or_default(),
            chunk.sample_end = bundle
                .members
                .last()
                .map(|member| member.sample_end)
                .unwrap_or_default(),
            chunk.sample_count = bundle.samples.len(),
            chunk.duration_ms = bundle_duration_ms,
            "soniqo_chunk_native_inference_start"
        );

        let transcribed = transcribe(model, &bundle.samples, language);
        ensure_local_batch_running(progress)?;
        let (text, words) = match transcribed {
            Ok(transcript) => {
                successful_chunks += 1;
                (transcript.text, transcript.words)
            }
            Err(e) => {
                failed_chunks += 1;
                tracing::warn!(
                    anarlog.stt.provider.name = "soniqo",
                    anarlog.stt.model = %model,
                    channel.index = channel_index,
                    chunk.index = bundle_index,
                    chunk.member_count = bundle.members.len(),
                    elapsed_ms = chunk_started_at.elapsed().as_millis() as u64,
                    error = %e,
                    "soniqo_chunk_native_inference_failed"
                );
                continue;
            }
        };

        tracing::info!(
            anarlog.stt.provider.name = "soniqo",
            anarlog.stt.model = %model,
            channel.index = channel_index,
            chunk.index = bundle_index,
            chunk.member_count = bundle.members.len(),
            elapsed_ms = chunk_started_at.elapsed().as_millis() as u64,
            transcript.text_chars = text.chars().count(),
            "soniqo_chunk_native_inference_completed"
        );

        let decided = soniqo_chunks_for_bundle(&bundle, &text, &words);
        if decided.fell_back_from_word_timings {
            tracing::warn!(
                anarlog.stt.provider.name = "soniqo",
                anarlog.stt.model = %model,
                channel.index = channel_index,
                chunk.index = bundle_index,
                transcript.word_count = words.len(),
                transcript.text_chars = text.chars().count(),
                "soniqo_word_timings_unusable_falling_back"
            );
        }
        transcript_chunks.extend(decided.chunks);
    }

    if successful_chunks == 0 && failed_chunks > 0 {
        return Err(format!(
            "Soniqo failed to transcribe all {failed_chunks} chunk(s) for channel {channel_index}."
        ));
    }

    if failed_chunks > 0 {
        tracing::warn!(
            anarlog.stt.provider.name = "soniqo",
            anarlog.stt.model = %model,
            channel.index = channel_index,
            chunk.success_count = successful_chunks,
            chunk.failed_count = failed_chunks,
            "soniqo_channel_completed_with_chunk_failures"
        );
    }

    if transcript_chunks.is_empty() {
        return Ok(anlg_transcribe_soniqo::FileTranscript::new(
            String::new(),
            duration_seconds,
        ));
    }

    Ok(anlg_transcribe_soniqo::FileTranscript::from_chunks(
        transcript_chunks,
        duration_seconds,
    ))
}

pub(super) fn audio_rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_of_squares = samples
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum::<f64>();
    (sum_of_squares / samples.len() as f64).sqrt()
}

fn transcribe_soniqo_samples(
    model: anlg_transcribe_soniqo::SoniqoModel,
    samples: &[f32],
    language: Option<&str>,
) -> std::result::Result<anlg_transcribe_soniqo::FileTranscript, String> {
    let file = tempfile::Builder::new()
        .prefix("soniqo_channel_")
        .suffix(".wav")
        .tempfile()
        .map_err(|e| e.to_string())?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    {
        let mut writer = hound::WavWriter::create(file.path(), spec).map_err(|e| e.to_string())?;
        for sample in samples {
            writer.write_sample(*sample).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
    }

    anlg_transcribe_soniqo::transcribe_file(model, file.path(), language).map_err(|e| e.to_string())
}

#[cfg(test)]
mod cancellation_tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    struct TestRuntime {
        cancelled: Arc<AtomicBool>,
    }

    impl BatchRuntime for TestRuntime {
        fn emit(&self, _event: BatchEvent) {}

        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::Acquire)
        }
    }

    #[tokio::test]
    async fn cancellation_after_native_inference_skips_remaining_chunks() {
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let mut writer = hound::WavWriter::create(
            file.path(),
            hound::WavSpec {
                channels: 1,
                sample_rate: TARGET_SAMPLE_RATE,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for sample in [0.1f32, 0.2, 0.3] {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let progress = SoniqoProgressReporter {
            runtime: Arc::new(TestRuntime {
                cancelled: cancelled.clone(),
            }),
            session_id: "cancel-test".to_string(),
        };
        let plan = SoniqoChannelPlan {
            channel_index: 0,
            duration_seconds: 3.0 / TARGET_SAMPLE_RATE as f64,
            is_direct_mic: false,
            chunk_strategy: SoniqoChunkStrategy::Fixed { max_samples: 1 },
            channel: ResampledChannelFile {
                file,
                sample_count: 3,
                rms: 0.0,
            },
        };
        let mut calls = 0usize;

        let error = transcribe_soniqo_channel_chunks_with(
            anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch,
            plan,
            None,
            Some(&progress),
            &tokio::runtime::Handle::current(),
            |_, _, _| {
                calls += 1;
                cancelled.store(true, Ordering::Release);
                Ok(anlg_transcribe_soniqo::FileTranscript::new(
                    "first".to_string(),
                    1.0,
                ))
            },
        )
        .expect_err("cancellation should stop before the next chunk");

        assert_eq!(calls, 1);
        assert_eq!(error, LOCAL_BATCH_CANCELLED);
    }
}
