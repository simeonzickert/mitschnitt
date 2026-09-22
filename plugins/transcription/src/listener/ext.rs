use ractor::{ActorRef, call_t, registry};

use crate::{CaptureConfigUpdate, CaptureParams, CaptureSnapshot, CaptureState, SessionStateCache};
use anlg_transcription_core::listener::{
    StartSessionError,
    actors::{RootActor, RootMsg, SessionParams, SourceActor, SourceMsg},
};

fn capture_snapshot_from_result<E: std::fmt::Debug>(
    result: Result<anlg_transcription_core::listener::Snapshot, E>,
) -> crate::Result<CaptureSnapshot> {
    result.map(CaptureSnapshot::from).map_err(|error| {
        tracing::warn!(?error, "capture_snapshot_unavailable");
        crate::Error::CaptureSnapshotUnavailable
    })
}

fn hydrate_session_state(
    snapshot: &mut CaptureSnapshot,
    session_id: String,
    cached: Option<(
        bool,
        bool,
        Vec<anlg_transcription_core::listener::LiveTranscriptSegment>,
    )>,
) {
    snapshot.live_segments_session_id = Some(session_id);
    if let Some((requested, active, segments)) = cached {
        snapshot.requested_live_transcription = Some(requested);
        snapshot.live_transcription_active = Some(active);
        snapshot.live_segments = Some(segments);
    } else {
        snapshot.live_segments = Some(Vec::new());
    }
}

/// The meetings the recorder is holding right now: being recorded, or being
/// finalised.
///
/// Mitschnitt-Fork (F16d). The Markdown mirror has to know this before it copies
/// a recording into a readable folder, and it runs in a background task with no
/// `AppHandle` to hand -- so this asks the actor registry directly, the same way
/// [`Listener::get_capture_state`] does, and needs no manager.
///
/// Three answers, and the difference matters to the caller:
///
/// * `Some(ids)` -- the recorder answered and holds exactly these.
/// * `Some(empty)` -- there is no recorder actor at all, so nothing is being
///   recorded. Not an unknown: an app without a running recorder is not
///   recording, and `get_capture_state` reads the same absence the same way.
/// * `None` -- the actor is there but did not answer in time. That is an
///   unknown, not an "all clear", and the caller has to treat it as such.
pub async fn held_session_ids() -> Option<Vec<String>> {
    let Some(cell) = registry::where_is(RootActor::name()) else {
        return Some(Vec::new());
    };
    let actor: ActorRef<RootMsg> = cell.into();
    // 500 ms, not the 100 ms the interactive snapshot uses. A busy recorder is
    // the one most likely to miss a short deadline, and a miss here drops the
    // mirror back to guessing from the file -- the exact failure this call
    // exists to prevent. Nobody is waiting on it: the mirror pass is debounced
    // and runs at most once a minute. Same budget as
    // `get_current_microphone_device`.
    let snapshot = match call_t!(actor, RootMsg::GetSnapshot, 500) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::warn!(?error, "recorder_snapshot_unavailable");
            return None;
        }
    };

    let mut held = snapshot.finalizing_session_ids;
    held.extend(snapshot.active_session_id);
    Some(held)
}

pub struct Listener<'a, R: tauri::Runtime, M: tauri::Manager<R>> {
    #[allow(unused)]
    manager: &'a M,
    _runtime: std::marker::PhantomData<fn() -> R>,
}

impl<'a, R: tauri::Runtime, M: tauri::Manager<R>> Listener<'a, R, M> {
    #[tracing::instrument(skip_all)]
    pub async fn list_microphone_devices(&self) -> Result<Vec<String>, crate::Error> {
        let audio = self
            .manager
            .state::<std::sync::Arc<dyn anlg_audio::AudioProvider>>();
        Ok(audio.list_mic_devices())
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_speaker_devices(&self) -> Result<Vec<String>, crate::Error> {
        let audio = self
            .manager
            .state::<std::sync::Arc<dyn anlg_audio::AudioProvider>>();
        Ok(audio.list_speaker_devices())
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_current_microphone_device(&self) -> Result<Option<String>, crate::Error> {
        if let Some(cell) = registry::where_is(SourceActor::name()) {
            let actor: ActorRef<SourceMsg> = cell.into();
            match call_t!(actor, SourceMsg::GetMicDevice, 500) {
                Ok(device_name) => Ok(device_name),
                Err(_) => Ok(None),
            }
        } else {
            Err(crate::Error::ActorNotFound(SourceActor::name()))
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_capture_state(&self) -> CaptureState {
        if let Some(cell) = registry::where_is(RootActor::name()) {
            let actor: ActorRef<RootMsg> = cell.into();
            match call_t!(actor, RootMsg::GetState, 100) {
                Ok(fsm_state) => CaptureState::from(fsm_state),
                Err(_) => CaptureState::Inactive,
            }
        } else {
            CaptureState::Inactive
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_capture_snapshot(&self) -> Result<CaptureSnapshot, crate::Error> {
        let cell = registry::where_is(RootActor::name())
            .ok_or_else(|| crate::Error::ActorNotFound(RootActor::name().to_string()))?;
        let actor: ActorRef<RootMsg> = cell.into();
        let mut snapshot = capture_snapshot_from_result(call_t!(actor, RootMsg::GetSnapshot, 100))?;

        let session_id = snapshot
            .active_session_id
            .as_ref()
            .or_else(|| snapshot.finalizing_session_ids.first())
            .cloned();
        if let Some(session_id) = session_id {
            let cached = self
                .manager
                .try_state::<SessionStateCache>()
                .and_then(|cache| {
                    let cache = cache.lock().ok()?;
                    let state = cache.get(&session_id)?;
                    Some((
                        state.requested_live_transcription,
                        state.live_transcription_active,
                        state.live_segments.clone(),
                    ))
                });
            hydrate_session_state(&mut snapshot, session_id, cached);
        }

        Ok(snapshot)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_mic_muted(&self) -> bool {
        if let Some(cell) = registry::where_is(SourceActor::name()) {
            let actor: ActorRef<SourceMsg> = cell.into();
            call_t!(actor, SourceMsg::GetMicMute, 100).unwrap_or_default()
        } else {
            false
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn set_mic_muted(&self, muted: bool) {
        if let Some(cell) = registry::where_is(SourceActor::name()) {
            let actor: ActorRef<SourceMsg> = cell.into();
            let _ = actor.cast(SourceMsg::SetMicMute(muted));
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn start_capture(&self, params: CaptureParams) -> Result<(), crate::Error> {
        let params: SessionParams = params.into();
        if let Some(cell) = registry::where_is(RootActor::name()) {
            let actor: ActorRef<RootMsg> = cell.into();
            match ractor::call!(actor, RootMsg::StartSession, params) {
                Ok(Ok(())) => Ok(()),
                Ok(Err(StartSessionError::SessionAlreadyRunning)) => {
                    Err(crate::Error::SessionAlreadyRunning)
                }
                Ok(Err(_)) => Err(crate::Error::StartSessionFailed),
                Err(_) => Err(crate::Error::StartSessionFailed),
            }
        } else {
            Err(crate::Error::ActorNotFound(RootActor::name().to_string()))
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn stop_capture(&self) {
        if let Some(cell) = registry::where_is(RootActor::name()) {
            let actor: ActorRef<RootMsg> = cell.into();
            let _ = ractor::call!(actor, RootMsg::StopSession);
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn update_capture_config(&self, update: CaptureConfigUpdate) {
        if let Some(cell) = registry::where_is(RootActor::name()) {
            let actor: ActorRef<RootMsg> = cell.into();
            let update = update.into();
            let _ = ractor::call!(actor, RootMsg::UpdateSessionConfig, update);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_snapshot_failure_is_not_reported_as_inactive() {
        let error = capture_snapshot_from_result::<&str>(Err("timed out"))
            .expect_err("snapshot failure must remain retryable");

        assert!(matches!(error, crate::Error::CaptureSnapshotUnavailable));
    }

    #[test]
    fn hydrated_segments_are_labeled_with_their_session() {
        let mut snapshot = CaptureSnapshot {
            state: CaptureState::Finalizing,
            active_session_id: None,
            finalizing_session_ids: vec!["session-a".to_string()],
            requested_live_transcription: None,
            live_transcription_active: None,
            live_segments_session_id: None,
            live_segments: None,
        };

        hydrate_session_state(&mut snapshot, "session-a".to_string(), None);

        assert_eq!(
            snapshot.live_segments_session_id.as_deref(),
            Some("session-a")
        );
        assert_eq!(snapshot.live_segments, Some(Vec::new()));
    }
}

pub trait ListenerPluginExt<R: tauri::Runtime> {
    fn listener(&self) -> Listener<'_, R, Self>
    where
        Self: tauri::Manager<R> + Sized;
}

impl<R: tauri::Runtime, T: tauri::Manager<R>> ListenerPluginExt<R> for T {
    fn listener(&self) -> Listener<'_, R, Self>
    where
        Self: Sized,
    {
        Listener {
            manager: self,
            _runtime: std::marker::PhantomData,
        }
    }
}
