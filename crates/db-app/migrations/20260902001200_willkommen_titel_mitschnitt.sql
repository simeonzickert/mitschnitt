-- Mitschnitt-Fork: der Titel einer BESTEHENDEN Willkommens-Sitzung.
--
-- Der Quellcode legt seit da7f3d41c8 "Welcome to Mitschnitt" an
-- (apps/desktop/src/onboarding/welcome-note.ts). Wer die Sitzung vorher hatte,
-- traegt den alten Titel weiter -- gemessen an der einzigen Installation am
-- 02.09.2026 (Kopie samt -wal/-shm): sessions.title = 'Welcome to Anarlog' UND
-- event_json.title = 'Welcome to Anarlog', Sitzung
-- e5f6a7b8-0000-4000-8000-000000000001.
--
-- Warum eine Migration und nicht die Lesestelle: der Link derselben Sitzung
-- faellt beim Lesen in getSessionEvent (session/utils.ts) -- das reicht dort,
-- weil ihn nur die Oberflaeche zeigt. Der Titel steht in ZWEI Feldern mit
-- verschiedenen Lesern. `sessions.title` liest die Seitenleiste, die
-- Titelzeile, der Suchindex (search_index_sessions_update markiert die Zeile
-- bei diesem UPDATE selbst als schmutzig) und der Markdown-Spiegel im
-- Rust-Werkzeug (apps/cli/src/commands/mirror.rs). Keiner davon geht durch
-- getSessionEvent; ein Lesepfad in TypeScript erreicht sie gar nicht.
-- `event_json.title` liest die Oberflaeche (metadata/index.tsx), die
-- Stichwoerter der Transkription (stt/useKeywords.ts) und die
-- Vorbereitungs-Notiz (session/insights/pre-meeting.ts).
--
-- Eingegrenzt, dreifach:
--   1. nur die Zeile mit tracking_id 'anarlog-onboarding-demo-v1' -- der
--      Schluessel, ueber den die App ihre Willkommens-Sitzung wiederfindet
--      (welcome-note.constants.ts, "NICHT UMBENENNEN");
--   2. je Feld nur der exakte Alt-Titel 'Welcome to Anarlog'. Ein selbst
--      umbenannter Titel bleibt stehen -- auch dann, wenn das jeweils andere
--      Feld noch den Alten traegt. Kein LIKE, kein '%narlog%';
--   3. der WHERE-Teil verlangt, dass mindestens eines der beiden Felder noch
--      alt ist. Ein zweiter Lauf trifft null Zeilen: updated_at bleibt, und
--      der Suchindex wird nicht erneut angestossen.
--
-- Gemessen, warum jedes json_extract in ein CASE gehoert: event_json ist TEXT
-- NOT NULL DEFAULT '' -- in der Live-Datenbank sind 4 von 9 Sitzungen ohne
-- gueltiges JSON, und json_extract('', ...) bricht mit "malformed JSON" ab.
-- SQLite darf AND-Terme umordnen, ein vorangestelltes json_valid() schuetzt
-- also nicht zuverlaessig; die Pruefung sitzt deshalb in jedem Ausdruck selbst
-- (dasselbe Muster wie die Suche in welcome-note.ts).
--
-- Ein einziges UPDATE, damit beide Felder unteilbar zusammen fallen,
-- unabhaengig davon, wie der Migrationslaeufer Transaktionen setzt.
-- json_set behaelt Reihenfolge und Schreibweise des uebrigen JSON bei (an der
-- Kopie der Live-Datenbank nachgesehen: nur der Titel unterscheidet sich).
--
-- Was diese Migration NICHT anfasst: meeting_link und description derselben
-- Zeile tragen weiter die Werte des Originals. Die faengt getSessionEvent beim
-- Lesen ab; sie stehen hier bewusst nicht mit drin, weil dieser Step genau
-- eine Sache tut.
--
-- Nicht rueckwaerts anwendbar im Sinne von "Titel zurueck" -- aber
-- downgrade-sicher: nur Daten, keine Schemaaenderung, kein Datenverlust.

UPDATE sessions
SET
  title = CASE
    WHEN title = 'Welcome to Anarlog' THEN 'Welcome to Mitschnitt'
    ELSE title
  END,
  event_json = CASE
    WHEN CASE WHEN json_valid(event_json)
              THEN json_extract(event_json, '$.title')
         END = 'Welcome to Anarlog'
    THEN json_set(event_json, '$.title', 'Welcome to Mitschnitt')
    ELSE event_json
  END
WHERE CASE WHEN json_valid(event_json)
           THEN json_extract(event_json, '$.tracking_id')
      END = 'anarlog-onboarding-demo-v1'
  AND (
    title = 'Welcome to Anarlog'
    OR CASE WHEN json_valid(event_json)
            THEN json_extract(event_json, '$.title')
       END = 'Welcome to Anarlog'
  );
