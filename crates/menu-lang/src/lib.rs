//! Eine einzige Rust-Stelle fuer die Sprache der nativen Oberflaeche: Tray-
//! und Dock-Menue, native Dialoge (Update-Check, "Ueber", "Vollstaendig
//! beenden"). Kein i18n-Framework -- eine schlichte Uebersetzungstabelle
//! en/de, weil das die einzigen zwei Sprachen sind, die die native
//! Oberflaeche kennt (siehe `resolve_lang`).
//!
//! Die eigentliche Katalog-Uebersetzung der Fenster-Oberflaeche laeuft ueber
//! Lingui (`apps/desktop/src/i18n/`) und kennt alle von der Oberflaeche
//! unterstuetzten Sprachen. Dieses Modul bildet NUR die binaere Vereinfachung
//! ab, die fuer Tray/Dock/Dialoge ausreicht (Orchestrator-Entscheid
//! 27.09.2026): "de" -> Deutsch, alles andere -> Englisch.

/// Die zwei Sprachen, die Tray, Dock und die nativen Dialoge kennen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    De,
}

/// Liest die Basissprache aus einer BCP-47-artigen Einstellung (`"de"`,
/// `"de-DE"`, `"en-US"`, ...) und bildet sie binaer ab -- genau wie
/// `resolveDisplayLocale` in `apps/desktop/src/i18n/locales.ts` es fuer die
/// Fenster-Oberflaeche tut (exaktes Match zuerst, dann Basissprache),
/// nur ohne die 109-Locale-Liste: alles, was nicht mit `de` beginnt, ist
/// fuer die native Oberflaeche Englisch.
///
/// `raw` ist der rohe Wert der Einstellung `ai_language`/"Main language",
/// wie die Oberflaeche ihn im Katalog abspeichert -- nicht vorverarbeitet.
pub fn resolve_lang(raw: Option<&str>) -> Lang {
    let Some(raw) = raw else {
        return Lang::En;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Lang::En;
    }
    let primary = trimmed
        .split(|c: char| c == '-' || c == '_')
        .next()
        .unwrap_or(trimmed);
    if primary.eq_ignore_ascii_case("de") {
        Lang::De
    } else {
        Lang::En
    }
}

/// Jeder uebersetzte Text in Tray, Dock und den drei nativen Dialogen
/// (Update-Check, "Ueber", "Vollstaendig beenden"). Ein Text, der in
/// mehreren Menue-Items identisch vorkommt (z. B. "Settings" in Tray UND
/// Dock), hat genau EINEN Eintrag hier -- beide Aufrufer teilen ihn sich.
///
/// Absichtlich NICHT hier: `TrayVersion`s Kanal-Woerter ("stable" /
/// "staging" / "dev") -- sie sind zugleich Vergleichswerte in
/// `get_channel`/Update-Logik, keine reine Anzeige; sie zu uebersetzen
/// wuerde diese Logik brechen. Dokumentiert am Aufrufort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Text {
    /// "Open {}" -- Tray + Dock (App-Name-Platzhalter).
    OpenApp,
    /// "New Note" -- Tray + Dock.
    NewNote,
    /// "Settings" -- Tray + Dock.
    Settings,
    /// "Hide" -- Tray (macOS-App-Menue).
    Hide,
    /// "Quit" -- Tray (macOS-App-Menue, cmd+q).
    Quit,
    /// "Show events in menu bar" -- Tray-Aufklapp-Menue (Checkbox-Item).
    ShowEventsInMenuBar,
    /// "Start a new meeting" -- Tray-Aufklapp-Menue.
    StartMeeting,
    /// "Check for Updates" -- Tray-Menue-Item (Ruhezustand) UND
    /// Dialogtitel bei "There are currently no updates available.".
    CheckForUpdates,
    /// "Downloading..." -- Tray-Menue-Item, deaktiviert.
    Downloading,
    /// "Restart to Apply Update" -- Tray-Menue-Item.
    RestartToApplyUpdate,
    /// "Update Failed" -- Dialogtitel, zweimal verwendet (Installieren
    /// UND Herunterladen schlagen fehl).
    UpdateFailed,
    /// "Failed to install update: {}" -- Dialogtext, `{}` = Fehlertext.
    FailedToInstallUpdate,
    /// "Update v{} is available!" -- Dialogtext, `{}` = Versionsnummer.
    UpdateAvailableBody,
    /// "Update Available" -- Dialogtitel.
    UpdateAvailableTitle,
    /// "Download" -- Dialog-Knopf.
    Download,
    /// "Later" -- Dialog-Knopf.
    Later,
    /// "Failed to download update: {}" -- Dialogtext, `{}` = Fehlertext.
    FailedToDownloadUpdate,
    /// "There are currently no updates available." -- Dialogtext.
    NoUpdatesAvailable,
    /// "Failed to check for updates: {}" -- Dialogtext, `{}` = Fehlertext.
    FailedToCheckForUpdates,
    /// "Update Check Failed" -- Dialogtitel.
    UpdateCheckFailed,
    /// "About {}" -- Tray-Menue-Item UND Dialogtitel, `{}` = App-Name.
    AboutApp,
    /// "- App Name: {}\n- App Version: {}\n- SHA:\n  {}" -- Dialogtext.
    /// "SHA" bleibt technisches Kuerzel, unuebersetzt.
    AboutBody,
    /// "Copy" -- Dialog-Knopf (Ueber-Dialog).
    Copy,
    /// "Cancel" -- Dialog-Knopf (Ueber-Dialog UND Vollstaendig-beenden-
    /// Dialog).
    Cancel,
    /// "Quit Completely…" -- Tray-Menue-Item.
    QuitCompletelyMenuItem,
    /// "{} will stop running in the background." -- Dialogtext.
    QuitWillStopInBackground,
    /// "Quit {} Completely?" -- Dialogtitel.
    QuitAppCompletelyQuestion,
    /// "Quit Completely" -- Dialog-Knopf (ohne Ellipse/Fragezeichen).
    QuitCompletelyButton,
    /// "Restart {}" -- Dock-Menue-Item.
    RestartApp,
    /// "Quit {} Completely" -- Dock-Menue-Item (ohne Fragezeichen, andere
    /// Wortstellung als `QuitAppCompletelyQuestion`).
    DockQuitCompletely,
    /// "Check for Updates..." -- Dock-Menue-Item (drei Punkte statt
    /// Ellipse-Zeichen, eigener String im Original).
    DockCheckForUpdates,
}

impl Text {
    /// Jede Variante genau einmal -- fuer die Vollstaendigkeitspruefung
    /// unten. Bei einer neuen Variante hier ergaenzen, sonst prueft der
    /// Test sie nicht mit.
    pub const ALL: &'static [Text] = &[
        Text::OpenApp,
        Text::NewNote,
        Text::Settings,
        Text::Hide,
        Text::Quit,
        Text::ShowEventsInMenuBar,
        Text::StartMeeting,
        Text::CheckForUpdates,
        Text::Downloading,
        Text::RestartToApplyUpdate,
        Text::UpdateFailed,
        Text::FailedToInstallUpdate,
        Text::UpdateAvailableBody,
        Text::UpdateAvailableTitle,
        Text::Download,
        Text::Later,
        Text::FailedToDownloadUpdate,
        Text::NoUpdatesAvailable,
        Text::FailedToCheckForUpdates,
        Text::UpdateCheckFailed,
        Text::AboutApp,
        Text::AboutBody,
        Text::Copy,
        Text::Cancel,
        Text::QuitCompletelyMenuItem,
        Text::QuitWillStopInBackground,
        Text::QuitAppCompletelyQuestion,
        Text::QuitCompletelyButton,
        Text::RestartApp,
        Text::DockQuitCompletely,
        Text::DockCheckForUpdates,
    ];
}

/// Die Uebersetzungstabelle selbst: (Englisch, Deutsch) je `Text`.
/// `{}`-Platzhalter bleiben in BEIDEN Spalten erhalten -- `tr_fmt` ersetzt
/// sie der Reihe nach, unabhaengig von ihrer Position im String (wichtig,
/// weil das Deutsche die Wortstellung an mehreren Stellen dreht, z. B.
/// "Open {}" -> "{} oeffnen").
fn table(text: Text) -> (&'static str, &'static str) {
    match text {
        Text::OpenApp => ("Open {}", "{} öffnen"),
        Text::NewNote => ("New Note", "Neue Notiz"),
        Text::Settings => ("Settings", "Einstellungen"),
        Text::Hide => ("Hide", "Ausblenden"),
        Text::Quit => ("Quit", "Beenden"),
        Text::ShowEventsInMenuBar => (
            "Show events in menu bar",
            "Termine in der Menüleiste anzeigen",
        ),
        Text::StartMeeting => ("Start a new meeting", "Neues Meeting starten"),
        Text::CheckForUpdates => ("Check for Updates", "Nach Updates suchen"),
        Text::Downloading => ("Downloading...", "Wird heruntergeladen…"),
        Text::RestartToApplyUpdate => (
            "Restart to Apply Update",
            "Neu starten, um Update zu übernehmen",
        ),
        Text::UpdateFailed => ("Update Failed", "Update fehlgeschlagen"),
        Text::FailedToInstallUpdate => (
            "Failed to install update: {}",
            "Update konnte nicht installiert werden: {}",
        ),
        Text::UpdateAvailableBody => ("Update v{} is available!", "Update v{} ist verfügbar!"),
        Text::UpdateAvailableTitle => ("Update Available", "Update verfügbar"),
        Text::Download => ("Download", "Herunterladen"),
        Text::Later => ("Later", "Später"),
        Text::FailedToDownloadUpdate => (
            "Failed to download update: {}",
            "Update konnte nicht heruntergeladen werden: {}",
        ),
        Text::NoUpdatesAvailable => (
            "There are currently no updates available.",
            "Derzeit sind keine Updates verfügbar.",
        ),
        Text::FailedToCheckForUpdates => (
            "Failed to check for updates: {}",
            "Updatesuche fehlgeschlagen: {}",
        ),
        Text::UpdateCheckFailed => ("Update Check Failed", "Updatesuche fehlgeschlagen"),
        Text::AboutApp => ("About {}", "Über {}"),
        Text::AboutBody => (
            "- App Name: {}\n- App Version: {}\n- SHA:\n  {}",
            "- App-Name: {}\n- App-Version: {}\n- SHA:\n  {}",
        ),
        Text::Copy => ("Copy", "Kopieren"),
        Text::Cancel => ("Cancel", "Abbrechen"),
        Text::QuitCompletelyMenuItem => ("Quit Completely…", "Vollständig beenden…"),
        Text::QuitWillStopInBackground => (
            "{} will stop running in the background.",
            "{} wird nicht mehr im Hintergrund laufen.",
        ),
        Text::QuitAppCompletelyQuestion => ("Quit {} Completely?", "{} vollständig beenden?"),
        Text::QuitCompletelyButton => ("Quit Completely", "Vollständig beenden"),
        Text::RestartApp => ("Restart {}", "{} neu starten"),
        Text::DockQuitCompletely => ("Quit {} Completely", "{} vollständig beenden"),
        Text::DockCheckForUpdates => ("Check for Updates...", "Nach Updates suchen…"),
    }
}

/// Ein Text ohne Platzhalter, direkt als `&'static str` -- fuer
/// `MenuItem::with_id`/`.title(...)`/`.message(...)`, die einen String
/// erwarten, aber ebenso `&'static str` annehmen.
pub fn tr(text: Text, lang: Lang) -> &'static str {
    let (en, de) = table(text);
    match lang {
        Lang::En => en,
        Lang::De => de,
    }
}

/// Wie `tr`, aber ersetzt `{}`-Platzhalter der Reihe nach durch `args` --
/// so wie `format!` es taete, nur mit einem zur Laufzeit gewaehlten
/// Format-String (den `format!` als Makro nicht annehmen wuerde, weil es
/// ein Literal verlangt). Ein fehlendes Argument fuer einen Platzhalter
/// laesst diesen Platzhalter stehen -- keiner der Texte hier hat mehr
/// Platzhalter, als `tr_fmt` Argumente bekommt (siehe Aufrufstellen).
pub fn tr_fmt(text: Text, lang: Lang, args: &[&str]) -> String {
    let mut result = tr(text, lang).to_string();
    for arg in args {
        result = result.replacen("{}", arg, 1);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_german_from_exact_and_base_language() {
        assert_eq!(resolve_lang(Some("de")), Lang::De);
        assert_eq!(resolve_lang(Some("de-DE")), Lang::De);
        assert_eq!(resolve_lang(Some("de-AT")), Lang::De);
        assert_eq!(resolve_lang(Some("de-CH")), Lang::De);
        assert_eq!(resolve_lang(Some("de_DE")), Lang::De);
        assert_eq!(resolve_lang(Some("DE")), Lang::De);
        assert_eq!(resolve_lang(Some("De-de")), Lang::De);
    }

    #[test]
    fn resolves_everything_else_to_english() {
        assert_eq!(resolve_lang(Some("en")), Lang::En);
        assert_eq!(resolve_lang(Some("en-US")), Lang::En);
        assert_eq!(resolve_lang(Some("fr")), Lang::En);
        assert_eq!(resolve_lang(Some("fr-FR")), Lang::En);
        assert_eq!(resolve_lang(Some("es")), Lang::En);
        assert_eq!(resolve_lang(Some("dennis")), Lang::En);
        assert_eq!(resolve_lang(Some("")), Lang::En);
        assert_eq!(resolve_lang(Some("   ")), Lang::En);
        assert_eq!(resolve_lang(None), Lang::En);
    }

    #[test]
    fn default_lang_is_english() {
        assert_eq!(Lang::default(), Lang::En);
    }

    /// Jede Zeile der Tabelle hat eine echte englische UND eine echte
    /// deutsche Fassung -- keine leere Zeile, keine (versehentlich)
    /// identische Uebersetzung, die eigentlich eine vergessene ist.
    #[test]
    fn every_text_has_a_non_empty_english_and_german_line() {
        for &text in Text::ALL {
            let en = tr(text, Lang::En);
            let de = tr(text, Lang::De);
            assert!(!en.is_empty(), "{text:?} hat keine englische Zeile");
            assert!(!de.is_empty(), "{text:?} hat keine deutsche Zeile");
            assert_ne!(
                en, de,
                "{text:?} hat identischen en/de-Text -- vermutlich vergessen"
            );
        }
    }

    /// `Text::ALL` selbst muss jede Variante genau einmal listen, sonst
    /// prueft der Test oben eine unvollstaendige Liste, ohne es zu merken.
    /// Ueber die Anzahl kann Rust keine Exhaustivitaet erzwingen (`ALL` ist
    /// ein Array, kein `match`), daher dieser Gegen-Check: die Anzahl
    /// bekannter Varianten wird hier von Hand nachgezaehlt und muss zur
    /// Tabellenzahl passen.
    #[test]
    fn all_lists_every_known_variant_exactly_once() {
        assert_eq!(
            Text::ALL.len(),
            31,
            "Text::ALL fehlt eine Variante oder hat eine doppelt"
        );
        let mut seen = std::collections::HashSet::new();
        for &text in Text::ALL {
            assert!(seen.insert(text), "{text:?} steht doppelt in Text::ALL");
        }
    }

    #[test]
    fn tr_fmt_replaces_a_single_placeholder_regardless_of_position() {
        assert_eq!(
            tr_fmt(Text::OpenApp, Lang::En, &["Mitschnitt"]),
            "Open Mitschnitt"
        );
        assert_eq!(
            tr_fmt(Text::OpenApp, Lang::De, &["Mitschnitt"]),
            "Mitschnitt öffnen"
        );
        assert_eq!(
            tr_fmt(Text::QuitAppCompletelyQuestion, Lang::De, &["Mitschnitt"]),
            "Mitschnitt vollständig beenden?"
        );
    }

    #[test]
    fn tr_fmt_replaces_multiple_placeholders_in_argument_order() {
        assert_eq!(
            tr_fmt(
                Text::AboutBody,
                Lang::De,
                &["Mitschnitt", "0.1.6", "abc123"]
            ),
            "- App-Name: Mitschnitt\n- App-Version: 0.1.6\n- SHA:\n  abc123"
        );
    }

    #[test]
    fn tr_without_placeholders_needs_no_formatting() {
        assert_eq!(tr(Text::Settings, Lang::En), "Settings");
        assert_eq!(tr(Text::Settings, Lang::De), "Einstellungen");
    }
}
