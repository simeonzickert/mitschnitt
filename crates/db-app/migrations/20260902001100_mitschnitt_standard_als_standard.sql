-- Mitschnitt-Fork (F6): die Vorlage 'mitschnitt-standard' wird Standard, und
-- ein nie geschriebener Sprachwert wird deutsch -- beides nur dort, wo niemand
-- gewaehlt hat.
--
-- app_settings.value_json ist JSON-Text in einer STRICT-Tabelle (NOT NULL,
-- Spalten-Default 'null'); jeder Schreiber serialisiert, SQL-NULL kommt nicht
-- vor. Drei Zustaende je Zeile, drei Antworten fuer selected_template_id:
--
--   Zeile fehlt               -> INSERT  (Auslieferungszustand)
--   'null' (Spalten-Default)  -> UPDATE  (nie bewusst geschrieben)
--   alles andere              -> bleibt, samt updated_at
--
-- "Alles andere" schliesst '""' ein: das schreibt das Frontend, wenn der
-- Nutzer den Standard abwaehlt (templates/template-form.tsx) -- eine Wahl fuer
-- "Auto", keine Nicht-Wahl.
--
-- ai_language bekommt NUR den UPDATE, keinen INSERT (Opus-Review der
-- Vorlagen-Runde, 02.09.2026): eine fehlende Zeile ist die Vorbedingung dafuer,
-- dass initializeApplicationSettings (apps/desktop/src/settings/queries.ts)
-- beim ersten Start die Systemsprache eintraegt -- und ai_language treibt auch
-- die Oberflaechensprache (i18n/provider.tsx). Ein Seed '"de"' haette jeder
-- Neuinstallation deutsche Oberflaeche gegeben, unabhaengig vom System.
-- "de-DE", "en" und '""' bleiben; nur der Spalten-Default 'null' wird deutsch.
--
-- Getrennt vom Vorlagen-INSERT (20260902001000), weil der Reparaturpfad in
-- db-app die Vorlagen-Seeds nachspielt und dabei nie eine Einstellung anfassen
-- darf. Ein String traegt in value_json seine Anfuehrungszeichen selbst,
-- sonst verwirft der Parser das Setting still.
--
-- Herkunft und Einfrieren: dieser Step war ausser in Test-Kopien nirgends
-- angewandt, als er am 02.09.2026 zuletzt geaendert wurde -- die einzige Live-Datenbank
-- endet in _sqlx_migrations bei 20260901120000 (gemessen an einer Kopie samt
-- -wal/-shm, 63 Zeilen), nichts ist gepusht oder ausgeliefert. Ab jetzt
-- eingefroren: jede weitere Aenderung ist ein neuer Step >= 20260902001200.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

INSERT OR IGNORE INTO app_settings (id, value_json)
VALUES ('selected_template_id', '"mitschnitt-standard"');

UPDATE app_settings
SET value_json = '"mitschnitt-standard"',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id = 'selected_template_id'
  AND value_json = 'null';

UPDATE app_settings
SET value_json = '"de"',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id = 'ai_language'
  AND value_json = 'null';
