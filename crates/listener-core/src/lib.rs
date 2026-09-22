pub mod actors;
mod events;
mod live_transcript;
mod runtime;

pub use events::*;
pub use live_transcript::*;
pub use runtime::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum State {
    Active,
    Inactive,
    Finalizing,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub state: State,
    pub active_session_id: Option<String>,
    pub finalizing_session_ids: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TranscriptionMode {
    #[default]
    Live,
    Batch,
}

/// Ermittelt, wie viele Sprecher der STT-Anbieter fuer EINEN Audiokanal erwarten soll.
///
/// Die Berechnung haengt von der Kanal-Lage `mode` ab:
/// `MicOnly`: der Nutzer sitzt MIT auf dem einen Kanal (Raumaufnahme, nur Mikrofon).
/// Die Zahl ist die Menge aus Teilnehmern und `self_human_id`, dupliziert bereinigt.
/// `SpeakerOnly` und `MicAndSpeaker`: unveraendertes Verhalten. Ferne Teilnehmer ohne
/// den Nutzer, dedupliziert; sind es keine und es gibt einen Nutzer, bleibt es bei 1.
/// Wichtig bei `MicAndSpeaker`: zum Startzeitpunkt kann diese Funktion NICHT unterscheiden,
/// ob der Mikrofonkanal einen Raum oder einen Anruf traegt, weil das eine akustische Messung
/// ist, die erst im Stapel-Weg vorliegt (`soniqo_diarization_speaker_count` in listener2-core).
/// Die Zahl kommt dort aus der Oberflaeche und schliesst den Nutzer ein.
pub(crate) fn expected_speakers_per_channel(
    participant_human_ids: &[String],
    self_human_id: Option<&str>,
    mode: crate::actors::ChannelMode,
) -> Option<u32> {
    let count = match mode {
        crate::actors::ChannelMode::MicOnly => {
            let mut everyone_on_channel = participant_human_ids.to_vec();
            if let Some(self_id) = self_human_id {
                everyone_on_channel.push(self_id.to_string());
            }
            everyone_on_channel.sort();
            everyone_on_channel.dedup();
            everyone_on_channel.len()
        }
        crate::actors::ChannelMode::SpeakerOnly | crate::actors::ChannelMode::MicAndSpeaker => {
            let mut remote_participants = participant_human_ids
                .iter()
                .filter(|participant| Some(participant.as_str()) != self_human_id)
                .cloned()
                .collect::<Vec<_>>();
            remote_participants.sort();
            remote_participants.dedup();

            if remote_participants.is_empty() && self_human_id.is_some() {
                1
            } else {
                remote_participants.len()
            }
        }
    };

    u32::try_from(count).ok().filter(|count| *count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::ChannelMode;

    // Prueft den MicOnly-Zweig: Vereinigung aus Teilnehmern und dem Nutzer,
    // der Nutzer fehlt noch in der Teilnehmerliste.
    #[test]
    fn mic_only_counts_participants_plus_missing_self() {
        let participants = vec![
            "remote-a".to_string(),
            "remote-b".to_string(),
            "remote-c".to_string(),
        ];

        assert_eq!(
            expected_speakers_per_channel(&participants, Some("self"), ChannelMode::MicOnly),
            Some(4)
        );
    }

    // Prueft den MicOnly-Zweig: der Nutzer steht schon in der Teilnehmerliste,
    // die Vereinigung darf ihn nicht doppelt zaehlen.
    #[test]
    fn mic_only_does_not_double_count_self_already_in_list() {
        let participants = vec![
            "self".to_string(),
            "remote-a".to_string(),
            "remote-b".to_string(),
            "remote-c".to_string(),
        ];

        assert_eq!(
            expected_speakers_per_channel(&participants, Some("self"), ChannelMode::MicOnly),
            Some(4)
        );
    }

    // Prueft den MicAndSpeaker-Zweig: unveraendertes Verhalten, der Nutzer wird
    // aus der Teilnehmerliste herausgefiltert statt mitgezaehlt.
    #[test]
    fn mic_and_speaker_excludes_self_from_count() {
        let participants = vec![
            "self".to_string(),
            "remote-a".to_string(),
            "remote-b".to_string(),
            "remote-c".to_string(),
        ];

        assert_eq!(
            expected_speakers_per_channel(&participants, Some("self"), ChannelMode::MicAndSpeaker),
            Some(3)
        );
    }

    // Prueft den SpeakerOnly-Zweig: derselbe Filter wie MicAndSpeaker, nur eine
    // Gegenseite bleibt uebrig.
    #[test]
    fn speaker_only_excludes_self_from_count() {
        let participants = vec!["self".to_string(), "remote-a".to_string()];

        assert_eq!(
            expected_speakers_per_channel(&participants, Some("self"), ChannelMode::SpeakerOnly),
            Some(1)
        );
    }

    // Prueft den alten Rueckfall im MicAndSpeaker-Zweig: keine fernen Teilnehmer,
    // aber ein Nutzer vorhanden, ergibt weiter 1.
    #[test]
    fn mic_and_speaker_falls_back_to_one_without_remote_participants() {
        assert_eq!(
            expected_speakers_per_channel(&[], Some("self"), ChannelMode::MicAndSpeaker),
            Some(1)
        );
    }

    // Prueft die u32::try_from(0)-Grenze am Ende der Funktion: ganz ohne
    // Teilnehmer und ohne Nutzer bleibt es None, in beiden betroffenen Modi.
    #[test]
    fn no_participants_and_no_self_yields_none_in_both_modes() {
        assert_eq!(
            expected_speakers_per_channel(&[], None, ChannelMode::MicOnly),
            None
        );
        assert_eq!(
            expected_speakers_per_channel(&[], None, ChannelMode::MicAndSpeaker),
            None
        );
    }

    // Prueft das dedup() im MicOnly-Zweig: doppelte Eintraege in der
    // Teilnehmerliste zaehlen nur einmal.
    #[test]
    fn mic_only_deduplicates_repeated_participant_entries() {
        let participants = vec!["remote-a".to_string(), "remote-a".to_string()];

        assert_eq!(
            expected_speakers_per_channel(&participants, Some("self"), ChannelMode::MicOnly),
            Some(2)
        );
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type")]
pub enum DegradedError {
    #[serde(rename = "authentication_failed")]
    AuthenticationFailed { provider: String },
    #[serde(rename = "upstream_unavailable")]
    UpstreamUnavailable { message: String },
    #[serde(rename = "connection_timeout")]
    ConnectionTimeout,
    #[serde(rename = "provider_configuration")]
    ProviderConfiguration { provider: String, message: String },
    #[serde(rename = "stream_error")]
    StreamError { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type")]
pub enum StartSessionError {
    #[serde(rename = "session_already_running")]
    SessionAlreadyRunning,
    #[serde(rename = "failed_to_resolve_sessions_dir")]
    FailedToResolveSessionsDir,
    #[serde(rename = "failed_to_start_session")]
    FailedToStartSession,
}
