#[cfg(any(not(feature = "app-store"), test))]
use std::path::Path;

#[cfg(any(not(feature = "app-store"), test))]
const AGENTS_CONTENT: &str = include_str!("agents-content.md");

#[cfg(any(not(feature = "app-store"), test))]
pub fn write_agents_file(base_dir: &Path) -> std::io::Result<()> {
    let agents_path = base_dir.join("AGENTS.md");
    std::fs::write(agents_path, AGENTS_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_guidance_prefers_typed_meeting_interfaces() {
        // Die Befehle muessen die sein, die dieser Fork wirklich hat. Der
        // Vorgaenger-Test prueft auf `anarlog --json meetings list` und die
        // Cloud-Werkzeuge -- beides existiert hier nicht mehr, die Anleitung
        // haette also plausibel klingende, nicht ausfuehrbare Befehle in den
        // Datenordner des Nutzers geschrieben.
        for command in [
            "mitschnitt --json doctor",
            "mitschnitt --json mirror status",
            "mitschnitt --json proposals list",
        ] {
            assert!(
                AGENTS_CONTENT.contains(command),
                "`{command}` fehlt in der erzeugten Anleitung"
            );
        }

        // Was dieser Fork NICHT mehr kann, darf auch nicht empfohlen werden.
        for gone in [
            "meetings list",
            "list_meetings",
            "get_meeting_transcript",
            "anarlog",
            "docs.anarlog.so",
        ] {
            assert!(
                !AGENTS_CONTENT.contains(gone),
                "`{gone}` steht wieder in der Anleitung, existiert hier aber nicht"
            );
        }

        assert!(!AGENTS_CONTENT.contains("--base ."));
        assert!(AGENTS_CONTENT.contains("--db-path ABSOLUTE_APP_DB"));
        assert!(AGENTS_CONTENT.contains("Do not use `find`,"));
        assert!(AGENTS_CONTENT.contains("direct SQLite queries"));
    }
}
