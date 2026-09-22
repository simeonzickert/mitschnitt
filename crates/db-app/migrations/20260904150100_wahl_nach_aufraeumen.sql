-- Mitschnitt-Fork: die Standard-Wahl darf nach dem Aufraeumen nicht ins Leere
-- zeigen (Nachbar von 20260904150000).
--
-- Zeigt 'selected_template_id' auf eine der dreizehn entfernten Vorlagen UND
-- ist diese Zeile wirklich weg, faellt die Wahl auf "Auto" zurueck -- '""',
-- genau das, was das Abwaehlen im Formular schreibt (templates/template-form.tsx)
-- und was auch der Loeschweg der Oberflaeche tut (useDeleteTemplate in
-- apps/desktop/src/templates/queries.ts, Zwilling delete_template in
-- crates/db-app/src/template_ops.rs). Ohne das laedt useEnhancedNotes eine ID
-- ins Leere, loadTemplate liefert null, und die Zusammenfassung faellt STILL
-- auf Auto zurueck -- der Nutzer sieht ein anderes Ergebnis und erfaehrt den
-- Grund nie.
--
-- Warum nicht auf 'mitschnitt-kompakt' umstellen: eine Wahl, die der Nutzer
-- getroffen hat, wird nicht durch eine andere ersetzt, die er nicht getroffen
-- hat. "Auto" ist ein ehrlicher Zustand und ein Klick vom Ziel entfernt.
--
-- Die zweite Bedingung ist die wichtige: 'NOT IN (SELECT id FROM templates)'.
-- Hat der Nutzer genau diese Vorlage bearbeitet oder angeheftet, hat der
-- Aufraeum-Step sie stehen gelassen -- dann bleibt auch seine Wahl stehen.
-- Jede Wahl ausserhalb der dreizehn (auch '""' und 'null') bleibt ohnehin
-- unberuehrt, samt updated_at.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

UPDATE app_settings
SET value_json = '""',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id = 'selected_template_id'
  AND value_json IN (
    '"default-board-meeting"',
    '"default-brainstorming-session"',
    '"default-customer-discovery"',
    '"default-daily-standup"',
    '"default-executive-briefing"',
    '"default-incident-postmortem"',
    '"default-investor-pitch"',
    '"default-performance-review"',
    '"default-product-roadmap-review"',
    '"default-project-kickoff"',
    '"default-sales-discovery-call"',
    '"default-technical-design-review"',
    '"mitschnitt-adaptive-minutes"'
  )
  AND json_extract(value_json, '$') NOT IN (SELECT id FROM templates);
