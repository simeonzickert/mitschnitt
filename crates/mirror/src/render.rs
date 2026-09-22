//! Renders one session into the three mirror files.
//!
//! Wording note: headings and speaker labels are English because the speaker
//! labels come out of the shared Rust labeler (`You`, `Speaker 2`) and the
//! existing export in `crates/agent-access` already uses `Notes` / `Summary` /
//! `Action items`. One language beats a mixture; switching the headings is a
//! local change in this file.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use anlg_db_app::SessionDocumentRow;
use anlg_transcript::{RenderTranscriptHuman, render_transcript_segments};

use crate::layout;
use crate::source::{MIRROR_FORMAT_VERSION, MirrorSource};

/// The rendered content of one mirrored session, ready to be written.
#[derive(Debug, Clone)]
pub struct MirrorFiles {
    pub dir_name: String,
    pub transcript_md: String,
    pub summary_md: String,
    pub meta_json: String,
}

#[derive(Debug, Serialize)]
struct MetaParticipant {
    human_id: String,
    display_name: String,
    email: String,
    role: String,
}

#[derive(Debug, Serialize)]
struct MetaTranscript {
    id: String,
    source: String,
    provider: String,
    model: String,
    language: String,
    started_at_ms: i64,
    ended_at_ms: Option<i64>,
    word_count: usize,
}

#[derive(Debug, Serialize)]
struct MetaAudio {
    /// Relative to the application data folder, e.g. `sessions/<id>`: where
    /// the machine room keeps this session. Since F16 the recording is also
    /// present in this very folder as a hard link, so the field is provenance
    /// rather than a place anyone has to go.
    session_folder: String,
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Meta {
    mirror_format_version: u32,
    session_id: String,
    title: String,
    occurred_at: String,
    created_at: String,
    started_at: String,
    ended_at: String,
    duration_ms: Option<i64>,
    timezone: String,
    language: String,
    participants: Vec<MetaParticipant>,
    transcripts: Vec<MetaTranscript>,
    audio: MetaAudio,
    mirrored_at: String,
    source_fingerprint: String,
}

/// `sessions_root` is the app's own `sessions/` folder; it is only read to list
/// which audio files exist next to the session.
pub fn render(source: &MirrorSource, sessions_root: &Path, mirrored_at: &str) -> MirrorFiles {
    MirrorFiles {
        dir_name: layout::session_dir_name(
            &source.session.id,
            &source.session.title,
            source.occurred_at(),
        ),
        transcript_md: transcript_markdown(source),
        summary_md: summary_markdown(source),
        meta_json: meta_json(source, sessions_root, mirrored_at),
    }
}

fn title(source: &MirrorSource) -> &str {
    let title = source.session.title.trim();
    if title.is_empty() {
        "Untitled meeting"
    } else {
        title
    }
}

fn transcript_markdown(source: &MirrorSource) -> String {
    let mut out = format!("# {} — Transcript\n\n", title(source));
    out.push_str(&format!("_Session `{}`_\n", source.session.id));
    out.push_str(&format!("_Date: {}_\n\n", source.occurred_at()));

    // A nameless participant needs care. The labeler resolves an assigned
    // speaker by looking the name up by id, so handing it an empty name yields
    // an empty heading, and withholding the name entirely yields the raw uuid.
    // Both were reproduced on the real database (`**[00:00:00] **`).
    //
    // So: the recording owner is named "You" when the contact has no name, and
    // any other nameless participant is left out of the request altogether,
    // which lets the labeler number them ("Speaker 2") the way it does when no
    // identity is known at all.
    let self_human_id = source.self_human_id();
    let mut humans: Vec<RenderTranscriptHuman> = Vec::new();
    for participant in &source.participants {
        if participant.human_id.is_empty() {
            continue;
        }
        let name = participant.display_name.trim();
        let name = match (
            name.is_empty(),
            Some(participant.human_id.as_str()) == self_human_id,
        ) {
            (false, _) => name.to_string(),
            (true, true) => "You".to_string(),
            (true, false) => continue,
        };
        humans.push(RenderTranscriptHuman {
            human_id: participant.human_id.clone(),
            name,
        });
    }
    let participant_human_ids: Vec<String> =
        humans.iter().map(|human| human.human_id.clone()).collect();

    let request = crate::speaker::build_request(
        &source.transcripts,
        humans,
        participant_human_ids,
        self_human_id.map(str::to_string),
    );

    let segments = request.map(render_transcript_segments).unwrap_or_default();

    if segments.is_empty() {
        out.push_str("_No transcript recorded for this session._\n");
        return out;
    }

    for segment in segments {
        // A label can still come back empty if the labeler has nothing to go on;
        // an empty heading reads as a bug, so the timecode stands alone.
        let heading = match segment.speaker_label.trim() {
            "" => timecode(segment.start_ms),
            label => format!("[{}] {label}", timecode(segment.start_ms)),
        };
        out.push_str(&format!("**{heading}**\n\n{}\n\n", segment.text.trim()));
    }

    out
}

fn summary_markdown(source: &MirrorSource) -> String {
    let mut out = format!("# {}\n\n", title(source));
    out.push_str(&format!("_Session `{}`_\n", source.session.id));
    out.push_str(&format!("_Date: {}_\n", source.occurred_at()));

    let people: Vec<&str> = source
        .participants
        .iter()
        .map(|participant| participant.display_name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    if !people.is_empty() {
        out.push_str(&format!("_Participants: {}_\n", people.join(", ")));
    }
    out.push('\n');

    let mut wrote_body = false;

    if let Some(note) = source.note() {
        let body = document_markdown(note);
        if !body.is_empty() {
            out.push_str(&format!("## Notes\n\n{body}\n\n"));
            wrote_body = true;
        }
    }

    for summary in source.summaries() {
        let body = document_markdown(summary);
        if body.is_empty() {
            continue;
        }
        let heading = if summary.title.trim().is_empty() {
            "Summary"
        } else {
            summary.title.trim()
        };
        out.push_str(&format!("## {heading}\n\n{body}\n\n"));
        wrote_body = true;
    }

    if !source.action_items.is_empty() {
        out.push_str("## Action items\n\n");
        for item in &source.action_items {
            let done = matches!(item.status.as_str(), "done" | "completed");
            out.push_str(&format!(
                "- [{}] {}\n",
                if done { "x" } else { " " },
                item.text.trim()
            ));
        }
        out.push('\n');
        wrote_body = true;
    }

    if !wrote_body {
        out.push_str("_No notes or summary for this session yet._\n");
    }

    out
}

/// Same conversion the export path uses: ProseMirror JSON becomes Markdown,
/// anything else is passed through unchanged rather than mangled.
fn document_markdown(document: &SessionDocumentRow) -> String {
    if document.body_format != "prosemirror_json" {
        return document.body.trim().to_string();
    }

    serde_json::from_str::<Value>(&document.body)
        .ok()
        .and_then(|value| anlg_tiptap::tiptap_json_to_md(&value).ok())
        .unwrap_or_else(|| document.body.clone())
        .trim()
        .to_string()
}

fn meta_json(source: &MirrorSource, sessions_root: &Path, mirrored_at: &str) -> String {
    let session_folder = sessions_root.join(&source.session.id);
    let mut files: Vec<String> = std::fs::read_dir(&session_folder)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_file())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    files.sort();

    let meta = Meta {
        mirror_format_version: MIRROR_FORMAT_VERSION,
        session_id: source.session.id.clone(),
        title: source.session.title.clone(),
        occurred_at: source.occurred_at().to_string(),
        created_at: source.session.created_at.clone(),
        started_at: source.session.started_at.clone(),
        ended_at: source.session.ended_at.clone(),
        duration_ms: source.duration_ms(),
        timezone: source.session.timezone.clone(),
        language: source.session.language.clone(),
        participants: source
            .participants
            .iter()
            .map(|participant| MetaParticipant {
                human_id: participant.human_id.clone(),
                display_name: participant.display_name.clone(),
                email: participant.email.clone(),
                role: participant.role.clone(),
            })
            .collect(),
        transcripts: source
            .transcripts
            .iter()
            .map(|transcript| MetaTranscript {
                id: transcript.id.clone(),
                source: transcript.source.clone(),
                provider: transcript.provider.clone(),
                model: transcript.model.clone(),
                language: transcript.language.clone(),
                started_at_ms: transcript.started_at_ms,
                ended_at_ms: transcript.ended_at_ms,
                word_count: serde_json::from_str::<Vec<Value>>(&transcript.words_json)
                    .map(|words| words.len())
                    .unwrap_or(0),
            })
            .collect(),
        audio: MetaAudio {
            session_folder: format!("sessions/{}", source.session.id),
            files,
        },
        mirrored_at: mirrored_at.to_string(),
        source_fingerprint: source.fingerprint.clone(),
    };

    // Pretty-printed on purpose: the whole point of the mirror is that a human
    // can open the file.
    serde_json::to_string_pretty(&meta).unwrap_or_else(|_| "{}".to_string())
}

fn timecode(ms: i64) -> String {
    let total_seconds = ms.max(0) / 1000;
    format!(
        "{:02}:{:02}:{:02}",
        total_seconds / 3600,
        (total_seconds % 3600) / 60,
        total_seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecodes_are_hours_minutes_seconds() {
        assert_eq!(timecode(0), "00:00:00");
        assert_eq!(timecode(1_200), "00:00:01");
        assert_eq!(timecode(3_661_000), "01:01:01");
        // A negative offset can appear when a second transcript starts before
        // the base; it must not render as a wrapped or negative timecode.
        assert_eq!(timecode(-5_000), "00:00:00");
    }
}
