//! Mehrere Gegenueber auf dem Systemton trennen (ZICK-330 Nachzug, Luecke 1).
//!
//! Der Kanal-Weg (`channel_split.rs`) trennt Mikrofon und Systemton, aber alle
//! Gegenueber eines Gruppen-Calls liegen auf dem Systemton unter EINEM
//! Sprecher: die Anbieter-Nummern je ~30-s-Paket waeren nicht vergleichbar,
//! und die Positivliste nimmt ohnehin nur Anbieter ohne eigene Trennung.
//!
//! Hier laeuft deshalb nach dem Hochladen die LOKALE Trennung ueber die GANZE
//! Systemkanal-Datei -- dieselbe wie im lokalen Weg (Soniqo, `local.rs`), mit
//! denselben Regeln:
//! - Sprecherzahl nach `soniqo_diarization_speaker_count`: Teilnehmer minus
//!   Nutzer, mindestens zwei, sonst keine Trennung. Ein 1:1-Gespraech oder
//!   eine Sitzung ohne Teilnehmerliste wird also nie getrennt -- genau der
//!   Schutz gegen "aus einer Stimme werden zwei".
//! - Laengendeckel `SONIQO_DIARIZATION_MAX_SAMPLES`, lange Kanaele werden in
//!   Abschnitten getrennt (`diarize_samples_chunked`).
//! - Die Zuordnung Wort -> Sprecher ueber die groesste Zeitueberlappung
//!   macht `batch_response_from_channels` (`speaker_for_word`), wie lokal.
//!
//! Nur macOS (Apple Silicon): Soniqo ist Swift/CoreML. Unter Windows meldet
//! der Trenner "nicht verfuegbar", und das Ergebnis ist wie vorher ein
//! Gegenueber. Eine plattformneutrale Trennung braucht Clustering fuer
//! `crates/pyannote-local` und kommt mit ZICK-332 (Raummikrofon-Modus).
//!
//! Kein Download im Cloud-Lauf: das Trennungsmodell wird nur benutzt, wenn
//! Parakeet (samt Trennungsmodell) schon auf der Platte liegt. Sonst laedt
//! Soniqo beim ersten Aufruf mehrere hundert MB -- ungefragt waere das der
//! falsche Tausch fuer jemanden, der gerade die Cloud gewaehlt hat.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anlg_transcribe_soniqo::DiarizationSegment;

use super::local::{
    ResampledChannelFile, SONIQO_DIARIZATION_CHUNK_SAMPLES, SONIQO_DIARIZATION_MIN_TAIL_SAMPLES,
    SoniqoChannelDiarization, ensure_soniqo_diarization_within_limit,
    soniqo_apply_diarization_limit, soniqo_diarization_speaker_count,
};

/// Die Naht zur eigentlichen Trennung; im Test ein Fake.
pub(super) trait SystemDiarizer: Send + Sync {
    /// Laeuft eine Trennung ueberhaupt, ohne etwas herunterzuladen?
    fn available(&self) -> bool;

    /// Trennt die ganze Kanal-Datei. `keep_going` wird zwischen Abschnitten
    /// gerufen; `false` bricht ab.
    fn diarize(
        &self,
        samples: &[f32],
        speaker_count: Option<usize>,
        keep_going: &mut dyn FnMut() -> bool,
    ) -> Result<Vec<DiarizationSegment>, String>;
}

pub(super) struct SoniqoSystemDiarizer;

impl SystemDiarizer for SoniqoSystemDiarizer {
    fn available(&self) -> bool {
        anlg_transcribe_soniqo::model_download_state(
            anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch,
        )
        .is_ok_and(|state| state.status == "ready")
    }

    fn diarize(
        &self,
        samples: &[f32],
        speaker_count: Option<usize>,
        keep_going: &mut dyn FnMut() -> bool,
    ) -> Result<Vec<DiarizationSegment>, String> {
        ensure_soniqo_diarization_within_limit(samples.len())?;
        let plan = anlg_transcribe_soniqo::ChunkPlan::new(
            SONIQO_DIARIZATION_CHUNK_SAMPLES,
            SONIQO_DIARIZATION_MIN_TAIL_SAMPLES,
        )
        .map_err(|error| error.to_string())?;

        struct Observer<'a> {
            keep_going: &'a mut dyn FnMut() -> bool,
        }
        impl anlg_transcribe_soniqo::ChunkObserver for Observer<'_> {
            fn should_continue(&mut self, _finished: usize, _total: usize) -> bool {
                (self.keep_going)()
            }
        }

        anlg_transcribe_soniqo::diarize_samples_chunked(
            anlg_transcribe_soniqo::SoniqoModel::ParakeetBatch,
            samples,
            speaker_count,
            plan,
            &mut Observer { keep_going },
        )
        .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
static TEST_DIARIZER: std::sync::Mutex<Option<Arc<dyn SystemDiarizer>>> =
    std::sync::Mutex::new(None);

/// Der Trenner des Laufs. Im Test austauschbar, damit der Einstieg
/// `run_batch` ohne Swift-Modell geprueft werden kann.
pub(super) fn system_diarizer() -> Arc<dyn SystemDiarizer> {
    #[cfg(test)]
    if let Some(diarizer) = TEST_DIARIZER.lock().unwrap().clone() {
        return diarizer;
    }
    Arc::new(SoniqoSystemDiarizer)
}

#[cfg(test)]
pub(super) fn set_test_diarizer(diarizer: Option<Arc<dyn SystemDiarizer>>) {
    *TEST_DIARIZER.lock().unwrap() = diarizer;
}

/// Eine geplante Trennung: der Kanal, eine Kopie seiner Datei, die
/// Sprecherzahl. Die Datei statt der Samples: gelesen wird erst, wenn der
/// Trennungs-Platz frei ist, und nicht schon waehrend Hochladen und Warten.
pub(super) struct SystemDiarizationJob {
    pub(super) channel_index: usize,
    pub(super) file: tempfile::NamedTempFile,
    pub(super) speaker_count: Option<usize>,
}

/// Unter so viel physischem Speicher keine Systemkanal-Trennung (Entscheid
/// 27.09.2026): lokal gemessen braucht sie fuer 101 min Ton 6,48 GB Spitze,
/// dazu laeuft die App. Auf 8-GB-Macs waere das ein Absturzrisiko.
pub(super) const SYSTEM_DIARIZATION_MIN_MEMORY_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Hinweis-Code in den Antwort-Metadaten (`system_diarization_skipped`).
pub(super) const SKIPPED_INSUFFICIENT_MEMORY: &str = "insufficient_memory";

#[cfg(test)]
static TEST_MEMORY_BYTES: std::sync::Mutex<Option<u64>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub(super) fn set_test_memory_bytes(bytes: Option<u64>) {
    *TEST_MEMORY_BYTES.lock().unwrap() = bytes;
}

pub(super) fn physical_memory_bytes() -> u64 {
    #[cfg(test)]
    if let Some(bytes) = *TEST_MEMORY_BYTES.lock().unwrap() {
        return bytes;
    }
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory()
}

/// Kopiert die Kanal-Datei, bevor der Buendler sie verbraucht.
pub(super) fn copy_channel_file(
    file: &ResampledChannelFile,
) -> Result<tempfile::NamedTempFile, String> {
    let copy = match file.file.path().parent() {
        Some(parent) => tempfile::Builder::new()
            .prefix("anarlog_diarize_")
            .suffix(".wav")
            .tempfile_in(parent)
            .or_else(|_| tempfile::NamedTempFile::new()),
        None => tempfile::NamedTempFile::new(),
    }
    .map_err(|e| e.to_string())?;
    std::fs::copy(file.file.path(), copy.path()).map_err(|e| e.to_string())?;
    Ok(copy)
}

/// Wird der Systemkanal getrennt? Nur bei genau zwei Kanaelen (Mikrofon,
/// Systemton), und nur nach der Regel des lokalen Weges. `None` heisst: ein
/// Gegenueber wie bisher.
pub(super) fn plan_system_diarization(
    files: &[ResampledChannelFile],
    num_speakers: Option<u32>,
) -> Option<(usize, Option<usize>)> {
    if files.len() != 2 {
        return None;
    }
    let channel_rms = files.iter().map(|file| file.rms).collect::<Vec<_>>();
    let mut plans = (0..files.len())
        .map(|index| soniqo_diarization_speaker_count(num_speakers, &channel_rms, index))
        .collect::<Vec<_>>();
    let counts = files
        .iter()
        .map(|file| file.sample_count)
        .collect::<Vec<_>>();
    if soniqo_apply_diarization_limit(&mut plans, &counts) {
        tracing::warn!("cloud_system_diarization_skipped_for_long_recording");
    }
    match plans[1] {
        SoniqoChannelDiarization::Skip => None,
        SoniqoChannelDiarization::Exact(count) => Some((1, Some(count))),
        SoniqoChannelDiarization::Inferred => Some((1, None)),
    }
}

/// Liest die ganze Kanal-Datei (32-bit float, mono, 16 kHz).
pub(super) fn read_channel_samples(path: &std::path::Path) -> Result<Vec<f32>, String> {
    hound::WavReader::open(path)
        .map_err(|e| e.to_string())?
        .samples::<f32>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// Setzt beim Fallenlassen den Stopp: bricht der Lauf ab (Fehler beim
/// Hochladen, Abbruch), haelt die Trennung am naechsten Abschnitt an, statt
/// minutenlang fuer nichts weiterzurechnen.
pub(super) struct StopOnDrop(pub(super) Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
