-- Mitschnitt-Fork: doppelte Kalender (und die Doppel-Termine darin) vereinigen.
--
-- Befund (an einer Kopie der einzigen Installation am 06.10.2026 gemessen):
-- 50 Zeilen in `calendars`, aber nur 25 verschiedene tracking_id_calendar,
-- alle mit demselben provider und derselben connection_id. Je Kennung eine
-- Zeile vom 31.08. (vom Kalender-Sync angelegt) und eine vom 11.09. (vom
-- anarlog-Import angelegt). Der Sync schrieb nach dem Import in beide Zeilen:
-- ctx.ts baut die Zuordnung Kennung -> id ueber `created_at`, bei zwei
-- Zeilen gewinnt die spaetere, also lief zeitweise die Import-Zeile mit. Ursache:
-- der Import fuegte Kalender nur mit `ON CONFLICT(id)` ein; die Import-ID
-- (die alte anarlog-ID) ist eine andere als die des Syncs, also entstand eine
-- zweite Zeile. Der Sync findet seine Kalender ueber (provider, connection_id,
-- tracking_id_calendar) und laedt Termine nur fuer AKTIVIERTE Kalender; die
-- Termine verteilten sich dadurch auf beide Zeilen, und welche gerade gepflegt
-- wurde, hing von Aktivierung und Reihenfolge ab.
--
-- Was dieser Step tut. Gruppe = nicht geloeschte Kalender mit gleichem
-- (provider, connection_id, tracking_id_calendar), tracking_id_calendar nicht
-- leer, mehr als eine Zeile:
--   1. Behalten wird die Zeile mit dem juengsten updated_at (bei Gleichstand
--      die aelteste, dann die kleinste id) -- die vom Sync gepflegte.
--      Sie behaelt ihren eigenen Wert fuer `enabled`, Name und Farbe: sie ist
--      die in Mitschnitt gepflegte Zeile (der Sync schreibt Name und Farbe
--      ohnehin neu); ein "aktiviert" der Import-Zeile stammt aus anarlog und
--      wird nicht uebernommen.
--   2. Verweise auf die anderen Zeilen wandern auf die behaltene. Das Schema
--      kennt genau zwei: `events.calendar_id` und das JSON-Feld
--      `sessions.event_json.$.calendar_id` (calendar/queries.ts vergleicht es
--      mit events.calendar_id). Gemessen, dass nichts sonst auf eine
--      Kalender-ID zeigt: app_settings (ignored_events haengt an
--      tracking_id, nicht an IDs), sessions.metadata_json, Anhaenge, Vorschlaege,
--      Mentions, Suchindex -- 0 Treffer. migration_import_targets haelt nur die
--      Import-Buchfuehrung und bleibt.
--   3. Die uebrigen Kalender-Zeilen werden NICHT hart geloescht, sondern wie
--      calendar_ops::delete_calendar mit deleted_at markiert. Grund: die
--      Import-ID bleibt als Grabstein belegt, ein erneuter Import legt sie
--      nicht wieder an (und loest ueber den Grabstein die behaltene Zeile auf,
--      siehe legacy_import/kalender.rs).
--
-- Termine. Nach dem Umhaengen koennen am behaltenen Kalender mehrere Termine
-- mit gleicher tracking_id_event und GLEICHEM Beginn liegen (gemessen: 84
-- Gruppen; 79 mal eine lebende + eine geloeschte Zeile, 5 mal nur geloeschte).
-- Ein verschobener Termin (gleiche tracking_id_event, anderer Beginn) ist
-- keine Dublette und wird nie angefasst. Je Gruppe gleicher Beginn, Rang:
-- nicht geloescht zuerst, dann die meisten nicht geloeschten Sitzungen (sessions.event_id), dann
-- juengstes updated_at, dann kleinste id. Gibt es in der Gruppe einen
-- lebenden Termin, dann:
--   - jede Sitzung, die auf einen anderen Termin der Gruppe zeigt, zeigt
--     danach auf den behaltenen (sonst legt creation.ts fuer denselben Termin
--     eine zweite Sitzung an);
--   - jeder weitere LEBENDE Termin der Gruppe bekommt deleted_at (Grabstein,
--     wie delete_event; der Sync erwartet Tombstones und laedt lebende vor
--     geloeschten, loadEventsForSync ORDER BY deleted_at IS NOT NULL).
-- Bereits geloeschte Zwillinge bleiben unveraendert. Gruppen ohne lebenden
-- Termin bleiben unberuehrt. Es wird keine Sitzung, Notiz oder Teilnehmerzeile
-- geloescht; sessions.updated_at bleibt (der Suchindex-Trigger markiert die
-- Zeile bei jedem UPDATE selbst).
--
-- Dieselbe Datei laeuft nach jedem Import noch einmal
-- (calendar_ops::consolidate_calendar_duplicates): ein Quell-Import (Ordner,
-- anarlog-SQLite, kombiniert) kann Sitzungen mit alten IDs zurueckschreiben.
--
-- Idempotent: ein zweiter Lauf findet keine Gruppe mit mehr als einer nicht
-- geloeschten Zeile und aendert nichts. Der Migrationslaeufer fuehrt den Step
-- in einer Transaktion aus. Die Hilfstabellen sind TEMP und werden am Ende
-- wieder entfernt.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

DROP TABLE IF EXISTS temp._kal_karte;
DROP TABLE IF EXISTS temp._termin_rang;
DROP TABLE IF EXISTS temp._termin_karte;
DROP TABLE IF EXISTS temp._kal_kanon;
DROP TABLE IF EXISTS temp._sitz_ziel;

CREATE TEMP TABLE _kal_karte AS
WITH rang AS (
  SELECT
    id, provider, connection_id, tracking_id_calendar,
    ROW_NUMBER() OVER (
      PARTITION BY provider, connection_id, tracking_id_calendar
      ORDER BY updated_at DESC, created_at ASC, id ASC
    ) AS r
  FROM calendars
  WHERE deleted_at IS NULL AND tracking_id_calendar <> ''
)
SELECT alt.id AS alt_id, neu.id AS neu_id
FROM rang AS alt
JOIN rang AS neu
  ON neu.provider = alt.provider
 AND neu.connection_id = alt.connection_id
 AND neu.tracking_id_calendar = alt.tracking_id_calendar
 AND neu.r = 1
WHERE alt.r > 1;

-- 2a. Termine (lebende wie geloeschte) auf den behaltenen Kalender.
UPDATE events
SET calendar_id = (SELECT neu_id FROM _kal_karte WHERE alt_id = events.calendar_id)
WHERE calendar_id IN (SELECT alt_id FROM _kal_karte);

-- 2b. event_json.calendar_id der Sitzungen. event_json ist TEXT NOT NULL
-- DEFAULT '' und oft kein JSON; jedes json_extract steht deshalb in einem CASE.
UPDATE sessions
SET event_json = json_set(
  event_json,
  '$.calendar_id',
  (SELECT neu_id FROM _kal_karte
    WHERE alt_id = CASE WHEN json_valid(event_json)
                        THEN json_extract(event_json, '$.calendar_id') END)
)
WHERE CASE WHEN json_valid(event_json)
           THEN json_extract(event_json, '$.calendar_id')
      END IN (SELECT alt_id FROM _kal_karte);

-- 3. Die uebrigen Kalender-Zeilen werden Grabsteine.
UPDATE calendars
SET deleted_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
    updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE id IN (SELECT alt_id FROM _kal_karte);

-- Kanon: jeder Kalender-Grabstein mit lebendem Kalender gleicher Kennung. Nach
-- einem Import koennen auch Grabsteine, die nicht aus diesem Lauf stammen,
-- Termine tragen (eine juengere Quell-Zeile schreibt die alte Kalender-ID
-- zurueck); sie wandern vor dem Zusammenlegen der Termine auf den lebenden.
CREATE TEMP TABLE _kal_kanon AS
SELECT alt.id AS alt_id,
  (SELECT k.id FROM calendars AS k
    WHERE k.deleted_at IS NULL AND k.provider = alt.provider
      AND k.connection_id = alt.connection_id
      AND k.tracking_id_calendar = alt.tracking_id_calendar
    ORDER BY k.updated_at DESC, k.created_at ASC, k.id ASC LIMIT 1) AS neu_id
FROM calendars AS alt
WHERE alt.deleted_at IS NOT NULL AND alt.tracking_id_calendar <> '';
DELETE FROM _kal_kanon WHERE neu_id IS NULL;

UPDATE events
SET calendar_id = (SELECT neu_id FROM _kal_kanon WHERE alt_id = events.calendar_id)
WHERE calendar_id IN (SELECT alt_id FROM _kal_kanon);

-- Termine: Dubletten am behaltenen Kalender.
CREATE TEMP TABLE _termin_rang AS
SELECT
  e.id,
  e.deleted_at,
  ROW_NUMBER() OVER (
    PARTITION BY e.calendar_id, e.tracking_id_event, julianday(e.started_at)
    ORDER BY
      (e.deleted_at IS NULL) DESC,
      (SELECT COUNT(*) FROM sessions AS s WHERE s.event_id = e.id AND s.deleted_at IS NULL) DESC,
      e.updated_at DESC,
      e.id ASC
  ) AS r,
  e.calendar_id AS calendar_id,
  e.tracking_id_event AS tracking_id_event,
  julianday(e.started_at) AS beginn
FROM events AS e
WHERE e.tracking_id_event <> ''
  AND julianday(e.started_at) IS NOT NULL
  AND e.calendar_id IN (
    SELECT neu_id FROM _kal_karte UNION SELECT neu_id FROM _kal_kanon
  );

-- alt = jeder weitere Termin einer Gruppe, deren Rang-1-Termin lebt.
CREATE TEMP TABLE _termin_karte AS
SELECT alt.id AS alt_id, neu.id AS neu_id, alt.deleted_at AS alt_geloescht
FROM _termin_rang AS alt
JOIN _termin_rang AS neu
  ON neu.calendar_id = alt.calendar_id
 AND neu.tracking_id_event = alt.tracking_id_event
 AND neu.beginn = alt.beginn
 AND neu.r = 1
 AND neu.deleted_at IS NULL
WHERE alt.r > 1;

UPDATE sessions
SET event_id = (SELECT neu_id FROM _termin_karte WHERE alt_id = sessions.event_id)
WHERE event_id IN (SELECT alt_id FROM _termin_karte);

-- Teilnehmer: hat der behaltene Termin keine, uebernimmt er die des juengsten
-- Zwillings, der welche hat (nur die Luecke fuellen, keine Listen mischen).
UPDATE events
SET participants_json = (
  SELECT alt.participants_json
  FROM _termin_karte AS karte
  JOIN events AS alt ON alt.id = karte.alt_id
  WHERE karte.neu_id = events.id
    AND alt.participants_json IS NOT NULL
    AND alt.participants_json NOT IN ('', '[]', 'null')
  ORDER BY alt.updated_at DESC, alt.id ASC
  LIMIT 1
)
WHERE id IN (SELECT neu_id FROM _termin_karte)
  AND (participants_json IS NULL OR participants_json IN ('', '[]', 'null'))
  AND EXISTS (
    SELECT 1
    FROM _termin_karte AS karte
    JOIN events AS alt ON alt.id = karte.alt_id
    WHERE karte.neu_id = events.id
      AND alt.participants_json IS NOT NULL
      AND alt.participants_json NOT IN ('', '[]', 'null')
  );

UPDATE events
SET deleted_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
    updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE deleted_at IS NULL
  AND id IN (SELECT alt_id FROM _termin_karte);

-- Sitzungen auf lebende Ziele (global, nicht nur fuer die Gruppen dieses Laufs --
-- dieselbe Datei laeuft nach jedem Import noch einmal, siehe
-- calendar_ops::consolidate_calendar_duplicates; der Kanon steht weiter oben):
--   a. event_json.calendar_id, das auf einen Kalender-Grabstein zeigt, wandert
--      auf den lebenden Kalender gleicher Kennung;
--   b. event_id, die auf einen geloeschten Termin zeigt, wandert auf den
--      lebenden Termin am selben Kalender mit gleicher tracking_id_event und
--      gleichem Beginn; findet sich keiner (verschobener Termin), auf den
--      einzigen lebenden Termin gleicher Kennung -- gibt es mehrere, bleibt der
--      Verweis. Der Rueckfall gilt nur fuer Einzeltermine: Quelle und Ziel ohne
--      Wiederholungsregel und ohne Serien-ID (eine Serie hat viele Termine mit
--      derselben Kennung-Familie, "einziger" waere dort Zufall). Wirkung bei Datensaetzen ohne Kennung: keine.
UPDATE sessions
SET event_json = json_set(
  event_json,
  '$.calendar_id',
  (SELECT neu_id FROM _kal_kanon
    WHERE alt_id = CASE WHEN json_valid(event_json)
                        THEN CAST(json_extract(event_json, '$.calendar_id') AS TEXT) END)
)
WHERE CASE WHEN json_valid(event_json)
           THEN CAST(json_extract(event_json, '$.calendar_id') AS TEXT)
      END IN (SELECT alt_id FROM _kal_kanon);

CREATE TEMP TABLE _sitz_ziel AS
SELECT sitzung_id, COALESCE(genau, einziger) AS neu_id
FROM (
  SELECT s.id AS sitzung_id,
    (SELECT t.id FROM events AS t
      WHERE t.calendar_id = d.calendar_id AND t.tracking_id_event = d.tracking_id_event
        AND julianday(t.started_at) = julianday(d.started_at)
        AND t.deleted_at IS NULL AND t.id <> d.id
      ORDER BY t.updated_at DESC, t.id ASC LIMIT 1) AS genau,
    (SELECT CASE WHEN COUNT(*) = 1 THEN MIN(t.id) END FROM events AS t
      WHERE t.calendar_id = d.calendar_id AND t.tracking_id_event = d.tracking_id_event
        AND t.deleted_at IS NULL AND t.id <> d.id
        AND d.has_recurrence_rules = 0 AND d.recurrence_series_id = ''
        AND t.has_recurrence_rules = 0 AND t.recurrence_series_id = '') AS einziger
  FROM sessions AS s
  JOIN events AS d ON d.id = s.event_id
  WHERE d.deleted_at IS NOT NULL AND d.tracking_id_event <> ''
)
WHERE COALESCE(genau, einziger) IS NOT NULL;

UPDATE sessions
SET event_id = (SELECT neu_id FROM _sitz_ziel WHERE sitzung_id = sessions.id)
WHERE id IN (SELECT sitzung_id FROM _sitz_ziel);

DROP TABLE temp._sitz_ziel;
DROP TABLE temp._kal_kanon;
DROP TABLE temp._termin_karte;
DROP TABLE temp._termin_rang;
DROP TABLE temp._kal_karte;
