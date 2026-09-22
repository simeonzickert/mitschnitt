use std::path::Path;

use super::session_span;
use crate::actors::recorder::resolve_final_audio_path;
use crate::{ListenerRuntime, SessionLifecycleEvent};

pub(crate) fn emit_session_ended(
    runtime: &dyn ListenerRuntime,
    sessions_base: &Path,
    session_id: &str,
    failure_reason: Option<String>,
) {
    let span = session_span(session_id);
    let _guard = span.enter();
    let audio_path = resolve_final_audio_path(sessions_base, session_id)
        .map(|path| path.to_string_lossy().into_owned());

    runtime.emit_lifecycle(SessionLifecycleEvent::Inactive {
        session_id: session_id.to_string(),
        audio_path,
        error: failure_reason.clone(),
    });

    if let Some(reason) = failure_reason {
        tracing::info!(anarlog.session.stop_reason = %reason, "session_stopped");
    } else {
        tracing::info!("session_stopped");
    }
}
