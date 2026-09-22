use std::path::{Path, PathBuf};

/// Ton haengt an der Konvention, nicht an der Datenbank. Gemessen an der
/// Testquelle (10.09.2026): 146 Tondateien unter `sessions/<uuid>/` (136 mp3,
/// 9 wav, 1 m4a, 6,65 GB), aber nur 39 Zeilen in `session_attachments` und
/// NULL Transkripte mit `audio_attachment_id`. Wer der Datenbank folgt,
/// verliert ueber hundert Aufnahmen -- deshalb sucht der Import im Dateibaum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAudio {
    pub session_id: String,
    pub path: PathBuf,
    /// Relativ zum Sitzungsordner, wie es die eigene App schreibt
    /// (`audio.mp3`, `attachments/notiz.m4a`).
    pub relative_path: String,
    pub filename: String,
    pub content_type: &'static str,
    pub size_bytes: u64,
}

pub fn content_type_for(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "mp3" => Some("audio/mpeg"),
        "wav" => Some("audio/wav"),
        "m4a" => Some("audio/mp4"),
        _ => None,
    }
}

/// Sammelt die Tondateien eines Quellordners. Ein Ordner unter `sessions/`,
/// dessen Name keine Sitzungskennung ist, faellt einfach nicht auf -- der
/// Abgleich gegen die tatsaechlich importierten Sitzungen passiert spaeter.
pub fn discover(source_root: &Path) -> Vec<SourceAudio> {
    let sessions_dir = source_root.join("sessions");
    let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let session_id = entry.file_name().to_string_lossy().into_owned();
        if session_id.starts_with('.') {
            continue;
        }
        let session_dir = entry.path();
        collect_in(&session_dir, &session_id, None, &mut found);
        collect_in(
            &session_dir.join("attachments"),
            &session_id,
            Some("attachments"),
            &mut found,
        );
    }

    found.sort_by(|left, right| {
        (&left.session_id, &left.relative_path).cmp(&(&right.session_id, &right.relative_path))
    });
    found
}

fn collect_in(dir: &Path, session_id: &str, prefix: Option<&str>, found: &mut Vec<SourceAudio>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(content_type) = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(content_type_for)
        else {
            continue;
        };
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let size_bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        let relative_path = match prefix {
            Some(prefix) => format!("{prefix}/{filename}"),
            None => filename.clone(),
        };
        found.push(SourceAudio {
            session_id: session_id.to_owned(),
            path,
            relative_path,
            filename,
            content_type,
            size_bytes,
        });
    }
}

pub fn total_bytes(audio: &[SourceAudio]) -> u64 {
    audio.iter().map(|entry| entry.size_bytes).sum()
}

/// Die eigene App vergibt `session-audio:<sitzung>` fuer die Hauptaufnahme --
/// gemessen an der Installation. Diese Form wird uebernommen, damit ein
/// zweiter Lauf dieselbe Zeile trifft statt eine zweite anzulegen; Nebendateien
/// bekommen den Pfad angehaengt, damit sie sich nicht gegenseitig ueberschreiben.
pub fn attachment_id(entry: &SourceAudio) -> String {
    if entry.relative_path == "audio.mp3" {
        format!("session-audio:{}", entry.session_id)
    } else {
        format!("session-audio:{}:{}", entry.session_id, entry.relative_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findet_ton_nach_konvention_und_ignoriert_fremde_endungen() {
        let root = tempfile::tempdir().unwrap();
        let session = root.path().join("sessions").join("sitzung-eins");
        std::fs::create_dir_all(session.join("attachments")).unwrap();
        std::fs::write(session.join("audio.mp3"), b"tondaten").unwrap();
        std::fs::write(session.join("audio_mic.wav"), b"mikro").unwrap();
        std::fs::write(session.join("notizen.txt"), b"kein ton").unwrap();
        std::fs::write(session.join("attachments").join("nachtrag.m4a"), b"x").unwrap();

        let found = discover(root.path());
        let paths: Vec<_> = found
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec!["attachments/nachtrag.m4a", "audio.mp3", "audio_mic.wav"]
        );
        assert!(found.iter().all(|entry| entry.session_id == "sitzung-eins"));
        assert_eq!(total_bytes(&found), 8 + 5 + 1);
    }

    #[test]
    fn kennungen_kollidieren_nicht_zwischen_haupt_und_nebendatei() {
        let haupt = SourceAudio {
            session_id: "s1".into(),
            path: PathBuf::from("/x/audio.mp3"),
            relative_path: "audio.mp3".into(),
            filename: "audio.mp3".into(),
            content_type: "audio/mpeg",
            size_bytes: 1,
        };
        let neben = SourceAudio {
            relative_path: "attachments/audio.mp3".into(),
            ..haupt.clone()
        };
        assert_eq!(attachment_id(&haupt), "session-audio:s1");
        assert_ne!(attachment_id(&haupt), attachment_id(&neben));
    }

    #[test]
    fn fehlender_sessions_ordner_ist_leer_statt_fehler() {
        let root = tempfile::tempdir().unwrap();
        assert!(discover(root.path()).is_empty());
    }
}
