-- Mitschnitt-Fork: die Auswahl darf nach dem Schnitt auf drei Vorlagen
-- (20260912120000_vorlagen_drei, Entscheid 06.10.2026) nicht auf „Auto“ oder
-- ins Leere zeigen. Nachbar von 20260911120100.
--
-- „Auto“ speichert die App als '""' (Auswahl-Einstellung leer) oder, in
-- aelteren Staenden, als '"__auto__"' (AUTO_TEMPLATE_ID). Beides wird
-- 'mitschnitt-kompakt' („Standard“). Dasselbe gilt fuer die entfernte Vorlage
-- 'default-one-on-one-meeting', aber nur, wenn diese Zeile wirklich weg ist:
-- hat der Nutzer sie bearbeitet oder angeheftet, blieb sie stehen, und seine
-- Wahl bleibt. Jede andere Wahl (die drei behaltenen Vorlagen, UUIDs, 'null')
-- bleibt samt updated_at.
--
-- Eigener Step, weil der Reparaturpfad (replay_template_seeds) die
-- Vorlagen-Steps nachspielt und dabei nie eine Einstellung anfassen darf.
-- Dokumente mit Vorlagen-Verweis (documents.template_id) bleiben unveraendert:
-- sie sind Historie, keine Auswahl.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

UPDATE app_settings
SET value_json = '"mitschnitt-kompakt"',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id = 'selected_template_id'
  AND (
    value_json IN ('""', '"__auto__"')
    OR (
      value_json = '"default-one-on-one-meeting"'
      AND json_extract(value_json, '$') NOT IN (SELECT id FROM templates)
    )
  );
