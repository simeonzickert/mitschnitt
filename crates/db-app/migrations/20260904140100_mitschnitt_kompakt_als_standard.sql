-- Mitschnitt-Fork: 'mitschnitt-kompakt' wird die Standard-Vorlage.
--
-- Getrennt vom Vorlagen-INSERT (20260904140000), weil der Reparaturpfad in
-- db-app (replay_template_seeds) die Vorlagen-Seeds nachspielt und dabei nie
-- eine Einstellung anfassen darf. Ein String traegt in value_json seine
-- Anfuehrungszeichen selbst, sonst verwirft der Parser das Setting still.
--
-- Vier Zustaende, drei Antworten:
--
--   Zeile fehlt                 -> INSERT  (Auslieferungszustand)
--   'null' (Spalten-Default)    -> UPDATE  (nie bewusst geschrieben)
--   '"mitschnitt-standard"'     -> UPDATE  (siehe unten)
--   alles andere, auch '""'     -> bleibt, samt updated_at
--
-- Der dritte Fall ist der einzige, der eine bestehende Zeile ueberschreibt,
-- und er braucht seine Begruendung. '"mitschnitt-standard"' kann nur aus zwei
-- Quellen stammen: aus unserem eigenen Seed 20260902001100, der ihn ungefragt
-- gesetzt hat, oder aus einer bewussten Wahl genau der Vorlage, die diese hier
-- ersetzt. In beiden Faellen ist das Umstellen das, was am 04.09.2026
-- angeordnet wurde („die neue kommt dazu und wird der neue Standard“), und der
-- Rueckweg ist ein Klick in der Vorlagen-Liste -- 'mitschnitt-standard' bleibt
-- vollstaendig erhalten. Jede ANDERE gewaehlte Vorlage bleibt unberuehrt: die
-- kann nur aus einer echten Wahl kommen, die niemand ungefragt kippt. '""' ist
-- die Wahl „Auto“ (templates/template-form.tsx schreibt sie beim Abwaehlen)
-- und bleibt ebenfalls stehen.
--
-- ai_language wird hier NICHT angefasst -- das erledigt 20260902001100 und
-- seine Begruendung gilt unveraendert weiter.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

INSERT OR IGNORE INTO app_settings (id, value_json)
VALUES ('selected_template_id', '"mitschnitt-kompakt"');

UPDATE app_settings
SET value_json = '"mitschnitt-kompakt"',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id = 'selected_template_id'
  AND value_json IN ('null', '"mitschnitt-standard"');
