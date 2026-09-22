use std::collections::HashMap;

use crate::{
    LocalModel, LocalSttPluginExt, SUPPORTED_MODELS, ServerInfo, SttModelInfo, server::ServerType,
    stt_model_info,
};

#[tauri::command]
#[specta::specta]
pub async fn models_dir<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<String, String> {
    Ok(app.local_stt().models_dir().to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn soniqo_model_dir<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    model: LocalModel,
) -> Result<String, String> {
    Ok(app
        .local_stt()
        .soniqo_model_dir(&model)
        .await
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn list_supported_models() -> Result<Vec<SttModelInfo>, String> {
    Ok(SUPPORTED_MODELS
        .iter()
        .filter(|m| m.is_available_on_current_platform())
        .map(stt_model_info)
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn inspect_custom_model_path<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    path: String,
) -> Result<crate::CustomSttModelInfo, String> {
    app.local_stt()
        .inspect_custom_model_path(&path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn is_model_downloaded<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    model: LocalModel,
) -> Result<bool, String> {
    app.local_stt()
        .is_model_downloaded(&model)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn is_model_downloading<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    model: LocalModel,
) -> Result<bool, String> {
    app.local_stt()
        .is_model_downloading(&model)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn download_model<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    model: LocalModel,
) -> Result<(), String> {
    app.local_stt()
        .download_model(model)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    model: LocalModel,
) -> Result<bool, String> {
    app.local_stt()
        .cancel_download(model)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_model<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    model: LocalModel,
) -> Result<(), String> {
    app.local_stt()
        .delete_model(&model)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn start_server<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    model: LocalModel,
) -> Result<String, String> {
    app.local_stt()
        .start_server(model)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn start_server_for_path<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    path: String,
) -> Result<String, String> {
    app.local_stt()
        .start_server_for_path(&path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn stop_server<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    server_type: Option<ServerType>,
) -> Result<bool, String> {
    app.local_stt()
        .stop_server(server_type)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_server_for_model<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    model: LocalModel,
) -> Result<Option<ServerInfo>, String> {
    app.local_stt()
        .get_server_for_model(&model)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_servers<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<HashMap<ServerType, ServerInfo>, String> {
    app.local_stt()
        .get_servers()
        .await
        .map_err(|e| e.to_string())
}

/// Welche Verhoerungen der Nachlauf verwerfen wuerde -- aus DERSELBEN
/// Entscheidung, die er spaeter faellt.
///
/// Bis zum 03.09.2026 stand daneben eine 962-zeilige TypeScript-Kopie der
/// deutschen Stoppwortliste. Sie verglich sich per Test gegen die Rust-Liste,
/// aber nur die WORTMENGE -- die beiden Normalisierer (`stopwords::normalize`
/// hier, eine handgeschriebene Schleife dort) waren unabhaengiger Code. Wer in
/// Rust an der Normalisierung dreht, haette die gelbe Warnung in den
/// Einstellungen still zur Luege gemacht, und der Test waere gruen geblieben.
///
/// Eine Kopie weniger ist eine Wahrheit mehr.
#[tauri::command]
#[specta::specta]
pub async fn vocabulary_risky_aliases(
    terms: Vec<String>,
) -> Result<Vec<crate::RiskyAlias>, String> {
    Ok(anlg_vocabulary::Vocabulary::parse(&terms)
        .risky_aliases()
        .into_iter()
        .map(|risky| crate::RiskyAlias {
            canonical: risky.canonical,
            alias: risky.alias,
        })
        .collect())
}

/// Sucht fehlende Woerterbuch-Eintraege in vorhandenen Transkripten.
///
/// Zwei Netze, beide ohne Sprachmodell (`anlg_vocabulary::proposals`):
/// Wiederkehr mit Schwankung ueber mehrere Gespraeche, und Klangvergleich
/// gegen Teilnehmer, Titel, Notiz und die Woerterbuchliste selbst.
///
/// Gibt VORSCHLAEGE zurueck und schreibt nichts. Der Befehl ist rein: er
/// bekommt die Gespraeche, die Liste und die schon verworfenen Paare herein
/// und haelt keinen Zustand -- dieselbe Bauart wie
/// [`vocabulary_risky_aliases`].
#[tauri::command]
#[specta::specta]
pub async fn vocabulary_proposals(
    sessions: Vec<crate::ProposalScanSession>,
    terms: Vec<String>,
    dismissed: Vec<crate::DismissedProposal>,
) -> Result<Vec<crate::VocabularyProposal>, String> {
    let sessions: Vec<anlg_vocabulary::ScanSession> = sessions
        .into_iter()
        .map(|session| anlg_vocabulary::ScanSession {
            session_id: session.session_id,
            words: session
                .words
                .into_iter()
                .map(|word| anlg_vocabulary::ScanWord {
                    text: word.text,
                    measured_confidence: word.measured_confidence,
                })
                .collect(),
            context_names: session.context_names,
            context_emails: session.context_emails,
            context_text: session.context_text,
            generated_title: session.generated_title,
        })
        .collect();
    let dismissed: Vec<(String, String)> = dismissed
        .into_iter()
        .map(|pair| (pair.canonical, pair.alias))
        .collect();

    Ok(anlg_vocabulary::scan(
        &sessions,
        &terms,
        &dismissed,
        &anlg_vocabulary::proposals::Options::default(),
    )
    .into_iter()
    .map(|proposal| crate::VocabularyProposal {
        kind: match proposal.kind {
            anlg_vocabulary::ProposalKind::Recurrence => crate::VocabularyProposalKind::Recurrence,
            anlg_vocabulary::ProposalKind::Context => crate::VocabularyProposalKind::Context,
        },
        canonical: proposal.canonical,
        alias: proposal.alias,
        occurrences: proposal.occurrences,
        session_ids: proposal.session_ids,
        evidence: proposal.evidence,
        source: proposal.source,
    })
    .collect())
}

#[cfg(test)]
mod tests {
    /// Der Befehl, der die 962-zeilige TypeScript-Kopie ersetzt hat.
    ///
    /// "Kling" ist gewoehnliches Deutsch ("Es macht Kling und die Tuer geht
    /// auf") und wuerde vom Nachlauf verworfen; "Sarnec" nicht. Wenn diese
    /// Grenze verrutscht, verrutscht ab jetzt auch die Warnung in den
    /// Einstellungen mit -- genau das war vorher nicht der Fall.
    #[tokio::test]
    async fn nennt_genau_die_verhoerungen_die_der_nachlauf_verwirft() {
        let risky = super::vocabulary_risky_aliases(vec![
            "Glinck => Kling".to_string(),
            "Ohlandez => Sarnec".to_string(),
        ])
        .await
        .unwrap();

        assert_eq!(risky.len(), 1);
        assert_eq!(risky[0].canonical, "Glinck");
        assert_eq!(risky[0].alias, "Kling");
    }
}
