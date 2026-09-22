use std::path::{Path, PathBuf};
use std::time::Instant;

mod chunked;
mod error;
mod model;
mod platform;
mod responses;
#[cfg(test)]
mod streaming_offline;
/// Nur zum Testen eingebunden: das Urteil des Bauart-Waechters aus `build.rs`.
/// Ohne diese Zeile laeuft seine Logik in keinem einzigen Test.
#[cfg(test)]
mod swift_build_check;
mod types;
#[cfg(test)]
mod word_frames;

/// In welcher Bauart die gelinkte Swift-Bibliothek uebersetzt wurde.
///
/// `release` oder `debug` -- gesetzt vom Bauskript (`build.rs`,
/// `swift_configuration`). Steht hier, damit die Frage "laeuft die
/// Sprechertrennung gerade optimiert?" ABLESBAR ist statt geraten: der
/// Unterschied ist am 101-Minuten-Kanal der zwischen fuenf und sechzig
/// Minuten. Ausserhalb von macOS gibt es keine Swift-Seite.
pub const SWIFT_CONFIGURATION: &str = match option_env!("SONIQO_SWIFT_CONFIGURATION") {
    Some(configuration) => configuration,
    None => "unbekannt",
};

pub use chunked::{
    CHUNKED_DIARIZATION_CANCELLED, ChunkObserver, ChunkPlan, DEFAULT_CHUNK_SAMPLES,
    DEFAULT_MIN_TAIL_SAMPLES, DIARIZATION_SAMPLE_RATE, EMBEDDING_DIMENSION, IgnoreChunkProgress,
    MAX_CHUNK_SAMPLES, diarize_samples_chunked,
};
pub use error::{Error, Result};
pub use model::{
    LOCAL_BASE_URL, SoniqoModel, is_local_base_url, is_loopback_http_base_url,
    local_model_from_request,
};
pub use responses::{
    MIN_WORD_DURATION_SECONDS, SmoothingWord, batch_response_from_channels,
    batch_response_from_text, smooth_speakers, speaker_for_word, stream_response_from_text,
};
pub use types::{
    Diarization, DiarizationSegment, FileTranscript, FileTranscriptChunk, LivePartial,
    ModelDownloadState, TranscriptSource, TranscriptWord,
};

fn ensure_supported_platform(model: SoniqoModel) -> Result<()> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(Error::UnsupportedPlatform);
    }

    if model.requires_macos_15() && !model::macos_version_at_least(15) {
        return Err(Error::RequiresMacOs15(model));
    }

    Ok(())
}

pub fn model_cache_dir(model: SoniqoModel) -> Result<PathBuf> {
    ensure_supported_platform(model)?;
    platform::model_cache_dir(model)
}

pub fn model_download_state(model: SoniqoModel) -> Result<ModelDownloadState> {
    ensure_supported_platform(model)?;
    platform::model_download_state(model)
}

pub fn start_model_download(model: SoniqoModel) -> Result<()> {
    ensure_supported_platform(model)?;
    platform::start_model_download(model)
}

pub fn reset_model(model: SoniqoModel) -> Result<()> {
    ensure_supported_platform(model)?;
    platform::reset_model(model)
}

pub fn is_model_downloaded(model: SoniqoModel) -> Result<bool> {
    Ok(model_download_state(model)?.status == "ready")
}

pub fn is_model_downloading(model: SoniqoModel) -> Result<bool> {
    Ok(model_download_state(model)?.status == "downloading")
}

pub fn delete_model(model: SoniqoModel) -> Result<()> {
    reset_model(model)?;

    let cache_dir = model_cache_dir(model)?;
    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir).map_err(Error::Delete)?;
    }
    if model == SoniqoModel::ParakeetBatch {
        let cache_dir = platform::diarization_cache_dir()?;
        if cache_dir.exists() {
            std::fs::remove_dir_all(&cache_dir).map_err(Error::Delete)?;
        }
    }

    Ok(())
}

pub fn transcribe_file(
    model: SoniqoModel,
    path: impl AsRef<Path>,
    language: Option<&str>,
) -> Result<FileTranscript> {
    ensure_supported_platform(model)?;

    let path = path.as_ref();
    let language = language.unwrap_or_default();
    let language_label = if language.is_empty() {
        "auto"
    } else {
        language
    };
    let file_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    let started_at = Instant::now();

    tracing::info!(
        anarlog.stt.provider.name = "soniqo",
        anarlog.stt.model = %model,
        anarlog.stt.language = %language_label,
        file.extension = %file_extension,
        "soniqo_native_file_transcription_start"
    );

    let result = platform::transcribe_file(model, path, language);
    let elapsed_ms = started_at.elapsed().as_millis() as u64;

    match &result {
        Ok(transcript) => {
            tracing::info!(
                anarlog.stt.provider.name = "soniqo",
                anarlog.stt.model = %model,
                elapsed_ms,
                transcript.duration_seconds = transcript.duration_seconds,
                transcript.text_chars = transcript.text.chars().count(),
                "soniqo_native_file_transcription_completed"
            );
        }
        Err(error) => {
            tracing::error!(
                anarlog.stt.provider.name = "soniqo",
                anarlog.stt.model = %model,
                elapsed_ms,
                error = %error,
                "soniqo_native_file_transcription_failed"
            );
        }
    }

    result
}

/// Trennt Sprecher und gibt nur die Abschnitte zurueck.
///
/// Fuer alles, was einen Kanal in EINEM Stueck trennt. Wer in Abschnitten
/// trennt, braucht [`diarize_samples_with_embeddings`] -- ohne die
/// Stimm-Schwerpunkte laesst sich Abschnitt zwei nicht an Abschnitt eins
/// anschliessen.
pub fn diarize_samples(
    model: SoniqoModel,
    samples: &[f32],
    exact_speakers: Option<usize>,
) -> Result<Vec<DiarizationSegment>> {
    Ok(diarize_samples_with_embeddings(model, samples, exact_speakers)?.segments)
}

/// Trennt Sprecher und gibt zusaetzlich je Sprecher den Stimm-Schwerpunkt
/// zurueck.
///
/// **Laengenschutz (21.09.2026).** Auch dieser Weg haelt sich an
/// [`MAX_CHUNK_SAMPLES`]. Bis hierher war er die offene Flanke: der zerlegte
/// Lauf war sorgfaeltig gedeckelt, waehrend nebenan zwei oeffentliche
/// Funktionen einen beliebig langen Kanal in EINEN CoreML-Aufruf schicken
/// konnten -- also genau in die Zuteilung, die den Prozess beendet. Ein
/// Absturzschutz, an dem man vorbeigehen kann, ist keiner.
///
/// Wer ueber der Grenze messen WILL (und nur dazu), nimmt
/// [`diarize_samples_unguarded_for_measurement`].
pub fn diarize_samples_with_embeddings(
    model: SoniqoModel,
    samples: &[f32],
    exact_speakers: Option<usize>,
) -> Result<Diarization> {
    if samples.len() > chunked::MAX_CHUNK_SAMPLES {
        return Err(Error::Bridge(format!(
            "a single diarization call is limited to {} samples ({} minutes); this call \
             passed {} ({:.1} minutes). Use diarize_samples_chunked for long channels.",
            chunked::MAX_CHUNK_SAMPLES,
            chunked::MAX_CHUNK_SAMPLES / chunked::DIARIZATION_SAMPLE_RATE as usize / 60,
            samples.len(),
            samples.len() as f64 / f64::from(chunked::DIARIZATION_SAMPLE_RATE) / 60.0
        )));
    }
    diarize_samples_unguarded_for_measurement(model, samples, exact_speakers)
}

/// Derselbe Aufruf OHNE Laengenschutz -- ausschliesslich fuer Messbaenke.
///
/// `examples/diarize_probe.rs` misst, wo der Prozess stirbt; ein Schutz waere
/// dort genau das, was die Messung verhindert. Der Name sagt bewusst laut,
/// was er ist, damit er nicht versehentlich im Produktivpfad landet.
pub fn diarize_samples_unguarded_for_measurement(
    model: SoniqoModel,
    samples: &[f32],
    exact_speakers: Option<usize>,
) -> Result<Diarization> {
    ensure_supported_platform(model)?;
    if model.batch_model() != SoniqoModel::ParakeetBatch {
        return Err(Error::Bridge(format!(
            "{} does not support speaker diarization",
            model.display_name()
        )));
    }
    // `None` = Sprecherzahl selbst bestimmen. Nur eine VORGEGEBENE Zahl muss
    // mindestens zwei sein -- "genau ein Sprecher" waere keine Trennung.
    if exact_speakers.is_some_and(|count| count < 2) {
        return Err(Error::Bridge(
            "speaker diarization requires at least two speakers".to_string(),
        ));
    }

    platform::diarize_samples(model.batch_model(), samples, exact_speakers)
}

pub struct LiveTranscriptionSession {
    model: SoniqoModel,
    session_token: String,
    stopped: bool,
}

impl LiveTranscriptionSession {
    pub fn start(model: SoniqoModel) -> Result<Self> {
        ensure_supported_platform(model)?;

        if !model.supports_live() {
            return Err(Error::Bridge(format!(
                "{} does not support realtime transcription",
                model.display_name()
            )));
        }

        let session_token = platform::live_start(model).map_err(|error| {
            tracing::error!(
                anarlog.stt.provider.name = "soniqo",
                anarlog.stt.model = %model,
                error = %error,
                "soniqo_native_live_start_failed"
            );
            error
        })?;
        Ok(Self {
            model,
            session_token,
            stopped: false,
        })
    }

    pub fn append(
        &mut self,
        source: TranscriptSource,
        samples: &[f32],
    ) -> Result<Vec<LivePartial>> {
        platform::live_append(&self.session_token, source, samples)
    }

    pub fn finalize(&mut self, source: TranscriptSource) -> Result<Vec<LivePartial>> {
        platform::live_finalize(&self.session_token, source)
    }

    pub fn model(&self) -> SoniqoModel {
        self.model
    }

    pub fn stop(mut self) -> Result<()> {
        self.stopped = true;
        platform::live_stop(&self.session_token)
    }
}

impl Drop for LiveTranscriptionSession {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = platform::live_stop(&self.session_token);
        }
    }
}

#[cfg(test)]
mod tests;
