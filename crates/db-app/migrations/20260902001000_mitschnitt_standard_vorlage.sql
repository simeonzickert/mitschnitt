-- Mitschnitt-Fork (F6): die Zusammenfassungs-Vorlage im eigenen Schnitt.
--
-- Nur die Vorlage, nichts sonst. Sie liegt in jeder Datenbank -- wie die 17
-- Upstream-Seeds (20260524000000_default_templates.sql) per INSERT OR IGNORE:
-- hat der Nutzer sie bearbeitet, bleibt seine Fassung stehen. Eigene ID ohne
-- 'default-'-Praefix, damit ein kuenftiger Upstream-Seed nie mit ihr
-- kollidiert. Kein icon_json: der Spalten-Default gilt.
--
-- Dass sie Standard wird und der Fliesstext deutsch kommt, steht getrennt in
-- 20260902001100_mitschnitt_standard_als_standard.sql. Der Grund fuer die
-- Trennung: der Reparaturpfad in db-app (repair_missing_core_tables) spielt
-- die Vorlagen-Seeds nach, wenn die Tabelle verschwunden war -- und darf
-- dabei nie eine Einstellung anfassen.
--
-- Herkunft und Einfrieren (C8, Review 02.09.2026): drei externe Reviews
-- warnten, ein umgeschriebener Step breche die Checksumme angewandter
-- Datenbanken. Widerlegt mit Beleg: dieser Step war ausser in Test-Kopien
-- nirgends angewandt -- die einzige Live-Datenbank endete in _sqlx_migrations bei
-- 20260901120000 (gemessen 02.09.2026 an einer Kopie samt -wal/-shm, 63
-- Zeilen), nichts ist gepusht oder ausgeliefert. Ab jetzt eingefroren: jede
-- weitere Aenderung ist ein neuer Step >= 20260902001200.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

INSERT OR IGNORE INTO templates (
  id, title, description, pinned, pin_order, category, targets_json, sections_json
) VALUES (
  'mitschnitt-standard',
  'Mitschnitt Standard',
  'Eigener Schnitt: Kurzfassung, Entscheidungen, Bälle, offene Fragen, Zahlen und Namen zur Gegenprüfung.',
  0,
  NULL,
  'Mitschnitt',
  '["Berater","Projektleiter","Geschäftsführung"]',
  '[{"title":"Kurzfassung","description":"Drei Sätze: worum ging es, was ist das Ergebnis, was passiert als Nächstes. Fließtext, keine Aufzählung."},{"title":"Entscheidungen","description":"Was wurde entschieden, je ein Stichpunkt, mit dem Grund, wenn er genannt wurde. Nur echte Entscheidungen, keine Vorschläge oder Ideen."},{"title":"Bälle","description":"Wer macht was bis wann. Ein Stichpunkt je Ball: Name · Aufgabe · Termin (oder ‚kein Termin genannt‘). Ohne klaren Verantwortlichen ist es kein Ball, sondern eine offene Frage."},{"title":"Offene Fragen","description":"Was unentschieden oder unklar blieb, und wer die Antwort schuldet."},{"title":"Zahlen und Namen zur Gegenprüfung","description":"Alle Zahlen, Beträge, Termine und Eigennamen aus dem Gespräch, so wie sie im Transkript stehen, als kurze Liste. Sie dienen dem Gegenprüfen, weil sich die Transkription verhören kann; nichts ergänzen, nichts runden."}]'
);
