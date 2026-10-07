-- Mitschnitt-Fork: die Standard-Wahl darf nach dem Vorlagen-Schnitt vom
-- 06.10.2026 nicht ins Leere zeigen (Nachbar von 20260911120000).
--
-- Zeigt 'selected_template_id' auf eine der drei entfernten Vorlagen UND ist
-- diese Zeile wirklich weg, wird die Wahl 'mitschnitt-kompakt' („Standard“).
-- Anders als beim Aufraeumen vom 04.09. (dort '""' = Auto, Step
-- 20260904150100) ordnet der Entscheid vom 06.10. ausdruecklich den Standard
-- als Fallback an; wer 'mitschnitt-standard' (den frueheren Rueckweg) oder
-- eine Sprint-Vorlage gewaehlt hatte, bekommt damit die Vorlage, die deren
-- Aufgabe uebernimmt.
--
-- Bedingung 'NOT IN (SELECT id FROM templates)': hat der Nutzer genau diese
-- Vorlage bearbeitet oder angeheftet, blieb sie stehen -- dann bleibt auch
-- seine Wahl. Jede andere Wahl ('""' = Auto, 'null', UUIDs, die drei
-- behaltenen Vorlagen) bleibt samt updated_at.
--
-- Eigener Step, weil der Reparaturpfad (replay_template_seeds) die
-- Vorlagen-Steps nachspielt und dabei nie eine Einstellung anfassen darf.
-- Vorlagen-Verweise in Dokumenten (documents.template_id) bleiben unveraendert:
-- sie sind Historie („mit welcher Vorlage entstand diese Notiz“), keine Auswahl.
-- Kalender-Ordner-Regeln tragen keine Vorlagen-ID (gemessen: nur
-- app_settings und documents verweisen auf Vorlagen-IDs).
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

UPDATE app_settings
SET value_json = '"mitschnitt-kompakt"',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id = 'selected_template_id'
  AND value_json IN (
    '"mitschnitt-standard"',
    '"default-sprint-planning"',
    '"default-sprint-retrospective"'
  )
  AND json_extract(value_json, '$') NOT IN (SELECT id FROM templates);
