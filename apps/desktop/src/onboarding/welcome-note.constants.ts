// Bis zum 02.09.2026 stand hier WELCOME_NOTE_DEMO_URL, eine Adresse auf der
// Webseite des Originals (anarlog.so/onboarding-demo) samt Sonderweg, der
// dieser fremden Seite einen Localhost-Rueckruf mitgab. Entscheid
// (ISA N5): Link raus, Text bleibt. Die Willkommens-Sitzung traegt seitdem
// keinen meeting_link mehr; der Notiztext ist unveraendert.
//
// NICHT UMBENENNEN: liegt als `event_json.tracking_id` in der Datenbank und
// ist der einzige Schluessel, ueber den eine bestehende Willkommens-Sitzung
// wiedergefunden wird. Ein neuer Wert findet die alte nicht mehr und legt
// still eine zweite an. Aendern nur mit einem Lesepfad fuer beide Werte.
export const WELCOME_NOTE_TRACKING_ID = "anarlog-onboarding-demo-v1";

// The description a welcome session carries today. Also what a legacy row's
// old description is read as (session/utils.ts, getSessionEvent).
export const WELCOME_NOTE_DESCRIPTION = "An introduction to Mitschnitt.";
