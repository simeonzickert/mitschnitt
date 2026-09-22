// Was von der Gegenstelle des Originals (api.anarlog.so) bleibt: die
// Sprach-Tabellen. `AdapterKind::Anarlog` ("anarlog") ist auf dem Draht und in
// der Datenbank die Sammel-Kennung, unter der die Oberflaeche die lokalen
// Modelle fuehrt (`current_stt_provider`, stt/capabilities.ts, useRunBatch.ts);
// die `soniqo-*`-Tabellen unten gehoeren dazu.
//
// Was NICHT mehr hier ist (Grok-Review 02.09.2026, F4): die Implementierungen
// von `RealtimeSttAdapter` und `BatchSttAdapter` (live.rs, batch.rs). Bis dahin
// hing die Unerreichbarkeit des Netz-Wegs an Filtern in der Oberflaeche und an
// einem Kommentar; jetzt gibt es fuer diese Kennung keinen Client-Typ mehr --
// `BatchClient::<AnarlogAdapter>` und `ListenClient::<AnarlogAdapter>` sind
// Typfehler, und die Verteiler in listener2-core/listener-core antworten mit
// `AdapterKind::network_client_unavailable`. Wer den Weg wieder oeffnet,
// oeffnet ihn zu fremder Infrastruktur.
use super::{LanguageQuality, LanguageSupport};

#[derive(Clone, Default)]
pub struct AnarlogAdapter;

impl AnarlogAdapter {
    pub fn language_support_live(
        languages: &[anlg_language::Language],
        model: Option<&str>,
    ) -> LanguageSupport {
        match soniqo_language_support(languages, model, true) {
            Some(support) => support,
            None => LanguageSupport::Supported {
                quality: LanguageQuality::NoData,
            },
        }
    }

    pub fn language_support_batch(
        languages: &[anlg_language::Language],
        model: Option<&str>,
    ) -> LanguageSupport {
        match soniqo_language_support(languages, model, false) {
            Some(support) => support,
            None => LanguageSupport::Supported {
                quality: LanguageQuality::NoData,
            },
        }
    }
}

fn soniqo_language_support(
    languages: &[anlg_language::Language],
    model: Option<&str>,
    live: bool,
) -> Option<LanguageSupport> {
    let model = model?;

    match model {
        "soniqo-parakeet-streaming" => Some(parakeet_language_support(languages)),
        "soniqo-parakeet-batch" => Some(if live {
            LanguageSupport::NotSupported
        } else {
            parakeet_language_support(languages)
        }),
        model if model.starts_with("soniqo-") => Some(if live {
            LanguageSupport::NotSupported
        } else {
            LanguageSupport::Supported {
                quality: LanguageQuality::NoData,
            }
        }),
        _ => None,
    }
}

fn parakeet_language_support(languages: &[anlg_language::Language]) -> LanguageSupport {
    if languages
        .iter()
        .all(anlg_language::is_parakeet_tdt_v3_language)
    {
        LanguageSupport::Supported {
            quality: LanguageQuality::NoData,
        }
    } else {
        LanguageSupport::NotSupported
    }
}
