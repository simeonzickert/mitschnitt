//! Reads everything a mirrored session needs out of the database.
//!
//! Read-only by construction: this module issues no writes and the sync path
//! only ever opens the pool for `SELECT`s. The mirror is a one-way projection.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use anlg_db_app::{
    SessionActionItemRow, SessionDocumentRow, SessionParticipantRow, SessionRow,
    SessionTranscriptRow, list_session_action_items, list_session_documents,
    list_session_participants, list_session_transcripts,
};

/// Bumped whenever the rendered output changes shape. It is part of the
/// fingerprint, so a renderer change makes every existing mirror stale and the
/// next sync rewrites it -- without this, an improved renderer would silently
/// leave old folders in the old format forever.
pub const MIRROR_FORMAT_VERSION: u32 = 1;

/// One session, fully loaded, plus the fingerprint of the rows it was built
/// from.
#[derive(Debug, Clone)]
pub struct MirrorSource {
    pub session: SessionRow,
    pub documents: Vec<SessionDocumentRow>,
    pub transcripts: Vec<SessionTranscriptRow>,
    pub participants: Vec<SessionParticipantRow>,
    pub action_items: Vec<SessionActionItemRow>,
    pub fingerprint: String,
}

impl MirrorSource {
    /// The timestamp the folder name is dated by: when the meeting happened,
    /// falling back to when its record was created.
    pub fn occurred_at(&self) -> &str {
        if self.session.started_at.trim().is_empty() {
            &self.session.created_at
        } else {
            &self.session.started_at
        }
    }

    pub fn note(&self) -> Option<&SessionDocumentRow> {
        self.documents
            .iter()
            .find(|document| document.kind == "note" && document.id == self.session.id)
            .or_else(|| {
                self.documents
                    .iter()
                    .find(|document| document.kind == "note")
            })
    }

    pub fn summaries(&self) -> Vec<&SessionDocumentRow> {
        self.documents
            .iter()
            .filter(|document| document.kind != "note")
            .collect()
    }

    /// The app derives "who is the microphone" from the transcript owner; the
    /// mirror has to use the same rule or the speaker labels would differ from
    /// what the app shows.
    pub fn self_human_id(&self) -> Option<&str> {
        self.transcripts
            .iter()
            .map(|transcript| transcript.owner_user_id.as_str())
            .find(|owner| !owner.is_empty())
    }

    pub fn duration_ms(&self) -> Option<i64> {
        let start = self
            .transcripts
            .iter()
            .map(|transcript| transcript.started_at_ms)
            .min()?;
        let end = self
            .transcripts
            .iter()
            .filter_map(|transcript| transcript.ended_at_ms)
            .max()?;
        (end > start).then_some(end - start)
    }
}

pub async fn load(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<MirrorSource>, sqlx::Error> {
    let Some(session) = anlg_db_app::get_session(pool, session_id).await? else {
        return Ok(None);
    };

    let documents = list_session_documents(pool, session_id).await?;
    let transcripts = list_session_transcripts(pool, session_id).await?;
    let participants = list_session_participants(pool, session_id).await?;
    let action_items = list_session_action_items(pool, session_id).await?;

    let fingerprint = fingerprint(
        &session,
        &documents,
        &transcripts,
        &participants,
        &action_items,
    );

    Ok(Some(MirrorSource {
        session,
        documents,
        transcripts,
        participants,
        action_items,
        fingerprint,
    }))
}

/// A cheap listing of every live session with the timestamp the fingerprint is
/// most likely to move with. Used by the audit to decide what to look at
/// without loading every session in full.
pub async fn list_session_ids(pool: &SqlitePool) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT id, title FROM sessions WHERE deleted_at IS NULL ORDER BY id",
    )
    .fetch_all(pool)
    .await
}

/// Hashes every row that the rendered output depends on, including each row's
/// `updated_at`.
///
/// Deliberately *not* a hash of the rendered files: a mirror whose file was
/// edited by hand must still count as up to date against the database, and the
/// question the audit answers is "did the source move since we mirrored it",
/// not "does the file still look exactly like our renderer's output".
fn fingerprint(
    session: &SessionRow,
    documents: &[SessionDocumentRow],
    transcripts: &[SessionTranscriptRow],
    participants: &[SessionParticipantRow],
    action_items: &[SessionActionItemRow],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(MIRROR_FORMAT_VERSION.to_be_bytes());

    let mut field = |value: &str| {
        hasher.update(value.as_bytes());
        hasher.update([0x1f]);
    };

    field(&session.id);
    field(&session.title);
    field(&session.updated_at);
    field(&session.started_at);
    field(&session.ended_at);
    field(&session.created_at);
    field(&session.language);
    field(&session.timezone);

    for document in documents {
        field(&document.id);
        field(&document.kind);
        field(&document.title);
        field(&document.updated_at);
    }
    for transcript in transcripts {
        field(&transcript.id);
        field(&transcript.updated_at);
        field(&transcript.provider);
        field(&transcript.model);
    }
    for participant in participants {
        field(&participant.id);
        field(&participant.display_name);
        field(&participant.updated_at);
    }
    for action_item in action_items {
        field(&action_item.id);
        field(&action_item.updated_at);
    }

    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionRow {
        SessionRow {
            id: "s1".into(),
            workspace_id: String::new(),
            owner_user_id: String::new(),
            title: "Titel".into(),
            kind: "meeting".into(),
            status: "active".into(),
            created_at: "2026-05-05T08:00:00Z".into(),
            updated_at: "2026-05-05T08:00:00Z".into(),
            started_at: String::new(),
            ended_at: String::new(),
            timezone: String::new(),
            language: String::new(),
            event_id: String::new(),
            external_event_id: String::new(),
            external_provider: String::new(),
            series_id: String::new(),
            source_apps_json: String::new(),
            event_json: String::new(),
            folder_path: String::new(),
            slug: String::new(),
            metadata_json: String::new(),
            locked: 0,
        }
    }

    #[test]
    fn the_fingerprint_moves_when_the_title_changes() {
        let before = fingerprint(&session(), &[], &[], &[], &[]);
        let mut renamed = session();
        renamed.title = "Anderer Titel".into();
        assert_ne!(before, fingerprint(&renamed, &[], &[], &[], &[]));
    }

    #[test]
    fn the_fingerprint_moves_when_a_document_is_touched() {
        let document = SessionDocumentRow {
            id: "d1".into(),
            workspace_id: String::new(),
            session_id: "s1".into(),
            kind: "summary".into(),
            template_id: String::new(),
            title: "Summary".into(),
            body_format: "prosemirror_json".into(),
            body: "{}".into(),
            source_hash: String::new(),
            generation_metadata_json: String::new(),
            sort_order: 0,
            created_by: String::new(),
            updated_by: String::new(),
            created_at: "2026-05-05T08:00:00Z".into(),
            updated_at: "2026-05-05T08:00:00Z".into(),
        };
        let before = fingerprint(&session(), std::slice::from_ref(&document), &[], &[], &[]);

        let mut touched = document.clone();
        touched.updated_at = "2026-05-05T09:00:00Z".into();
        assert_ne!(before, fingerprint(&session(), &[touched], &[], &[], &[]));
    }

    // A deleted summary leaves every remaining row untouched. Without hashing
    // the set itself the mirror would keep a dead summary file forever.
    #[test]
    fn the_fingerprint_moves_when_a_document_disappears() {
        let document = SessionDocumentRow {
            id: "d1".into(),
            workspace_id: String::new(),
            session_id: "s1".into(),
            kind: "summary".into(),
            template_id: String::new(),
            title: "Summary".into(),
            body_format: "prosemirror_json".into(),
            body: "{}".into(),
            source_hash: String::new(),
            generation_metadata_json: String::new(),
            sort_order: 0,
            created_by: String::new(),
            updated_by: String::new(),
            created_at: "2026-05-05T08:00:00Z".into(),
            updated_at: "2026-05-05T08:00:00Z".into(),
        };
        assert_ne!(
            fingerprint(&session(), &[document], &[], &[], &[]),
            fingerprint(&session(), &[], &[], &[], &[])
        );
    }

    #[test]
    fn the_fingerprint_is_stable_for_unchanged_input() {
        assert_eq!(
            fingerprint(&session(), &[], &[], &[], &[]),
            fingerprint(&session(), &[], &[], &[], &[])
        );
    }
}
