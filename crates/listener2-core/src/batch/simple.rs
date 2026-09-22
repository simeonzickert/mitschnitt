mod direct;
mod local;
pub(crate) mod tuning;

pub(super) use direct::run_direct_batch_for_adapter_kind;
pub(super) use local::{run_apple_speech_batch, run_soniqo_batch};

#[cfg(test)]
use super::upload::segment_plan;
#[cfg(test)]
use direct::{
    DIRECT_BATCH_TIMEOUT_CEILING, DIRECT_BATCH_TIMEOUT_FLOOR, direct_batch_timeout_for_audio,
    merge_segment_responses, run_direct_batch, run_direct_batch_with_timeout,
};
#[cfg(test)]
use local::{
    FixedSoniqoFileChunkIterator, LOCAL_BATCH_CANCELLED, MAX_LOCAL_BATCH_CHANNELS,
    ResampledChannelFile, SONIQO_BUNDLE_MAX_SPAN_SAMPLES, SONIQO_BUNDLE_SEAM_SAMPLES,
    SONIQO_CHANNEL_DIGITAL_SILENCE_RMS, SONIQO_CHANNEL_SILENT_RATIO, SONIQO_CHANNEL_SILENT_RMS,
    SONIQO_DIARIZATION_MAX_SAMPLES, SONIQO_DIARIZATION_PROGRESS_END,
    SONIQO_DIRECT_MIC_MIN_RMS, SONIQO_PARAKEET_MAX_CHUNK_SAMPLES, SONIQO_PROGRESS_MAX,
    SONIQO_PROGRESS_PLANNED, SoniqoChannelDiarization, SoniqoChunkBundler, SoniqoChunkStrategy,
    SoniqoProgressReporter, audio_rms, collect_soniqo_channel_transcripts,
    ensure_soniqo_diarization_within_limit, resample_audio_to_channel_files,
    resample_audio_to_channel_files_until, soniqo_apply_diarization_limit, soniqo_batch_progress,
    soniqo_bundle_max_samples, soniqo_bundle_transcript_chunks,
    soniqo_bundle_transcript_chunks_from_words, soniqo_chunk_strategy, soniqo_chunks_for_bundle,
    SoniqoChannelVoice, soniqo_channel_is_silent, soniqo_channel_silence_ratio,
    soniqo_channel_voice, soniqo_diarization_expected,
    soniqo_diarization_progress, soniqo_diarization_speaker_count,
    soniqo_diarization_within, soniqo_language_hint, soniqo_speech_channels, split_bundle_text,
    transcribe_soniqo_channels_in_parallel, with_diarization_heartbeat_every,
};

#[cfg(test)]
use tuning::SoniqoTuning;

#[cfg(test)]
mod tests;
