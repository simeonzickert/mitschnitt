-- Mitschnitt-Fork: vier Vorlagen statt sieben (Entscheid 06.10.2026).
--
-- Nach diesem Step liefert Mitschnitt genau VIER Vorlagen aus:
--   'mitschnitt-kompakt'          -> heisst jetzt „Standard“, steht gepinnt ganz oben
--   'default-one-on-one-meeting'  -> unveraendert („1:1-Gespraech“)
--   'default-client-kickoff'      -> unveraendert („Kunden-Kickoff“)
--   'default-lecture-notes'       -> „Vortrag & Schulung“ (vorher „Vorlesungsnotizen“)
-- Entfernt werden 'mitschnitt-standard', 'default-sprint-planning' und
-- 'default-sprint-retrospective'. 'mitschnitt-adaptive-minutes' hat eine feste
-- Seed-ID, ist aber schon seit 20260904150000 aus der Auslieferung entfernt;
-- eine bearbeitete Fassung bleibt dort bewusst stehen und bleibt es auch hier.
--
-- ID BLEIBT: 'mitschnitt-kompakt' behaelt seine ID, damit Auswahl
-- (app_settings.selected_template_id) und Dokumente mit Vorlagen-Verweis
-- weiter aufloesen. Nur Titel, Beschreibung und der Zitate-Abschnitt aendern sich.
--
-- ZITATE NEU: hoechstens drei woertliche Aussagen, nur solche, die etwas
-- hinzufuegen, was die Zusammenfassung nicht schon sagt; ohne solche Aussage
-- entfaellt der Abschnitt samt Ueberschrift. Das Weglassen erlaubt der Rahmen
-- bereits: enhance.system.md.jinja, „# Hard Rules“: „Drop an empty section
-- entirely“ -- eine Aenderung am Rahmen ist deshalb nicht noetig, und keine
-- andere Vorlage wird beruehrt.
--
-- WAS GEAENDERT/GELOESCHT WIRD, IST GENAU DER AUSLIEFERUNGSZUSTAND -- nichts
-- sonst. Jede Vorbedingung traegt den Stand nach 20260907120000 Feld fuer Feld
-- (Verfahren wie 20260904150000/20260904160000: der Inhalt entscheidet, nie ein
-- Zeitstempel). Vom Nutzer angelegte oder importierte Vorlagen (zufaellige
-- UUIDs) stehen nie in diesen Bedingungen und bleiben unberuehrt. Wer eine der
-- drei entfernten Vorlagen bearbeitet oder angeheftet hat, behaelt sie.
--
-- 'pinned'/'pin_order' setzt der Step fuer 'mitschnitt-kompakt' bedingungslos:
-- die Vorlage war nie angeheftet, ein Abwaehlen kann es vorher nicht gegeben
-- haben. 'icon_json' wird NICHT genannt (die Spalte fehlt im Reparaturpfad,
-- siehe 20260904150000), also kein Stern-Symbol.
--
-- Die Auswahl fasst dieser Step nicht an (Reparaturpfad spielt ihn nach):
-- siehe 20260911120100_wahl_nach_vorlagen_standard.sql.
--
-- Idempotent: ein zweiter Lauf findet keine Zeile mit altem Stand. Downgrade-
-- sicher: nur Daten, keine Schemaaenderung.

UPDATE templates
SET title = 'Standard',
    description = 'Die Voreinstellung und die Vorlage für fast jedes Gespräch: TL;DR, Entscheidungen, Themen aus dem Gespräch statt fester Liste, Aufgaben als Tabelle mit Wer und Bis, offene Fragen, Zitate. Passt für Beratungs- und Projektgespräche, Besprechungen und Telefonate, in denen hinterher zählt, wer was bis wann macht. Abschnitte ohne Inhalt entfallen.',
    sections_json = '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Entscheidungen", "description": "Was tatsächlich entschieden wurde, ein Stichpunkt je Entscheidung. Eine Entscheidung braucht jemanden, der sie getroffen und ausgesprochen hat. Eine Absicht, ein Plan, ein Wunsch, eine Erwartung oder ein Vorschlag, dem nur niemand widersprochen hat, ist KEINE Entscheidung; er gehört zu den Aufgaben oder nirgendwo hin."}, {"title": "Themen", "description": "Schreib KEINE Überschrift, die wörtlich „Themen“ heisst. Bau diesen Teil aus dem Gespräch selbst: eine eigene Überschrift je Thema, das wirklich besprochen wurde, benannt nach diesem Thema. Folge den echten Themen und ihrer Gewichtung, nie einer festen Liste; überspring, was nicht vorkam. Fass zusammen, was zusammengehört: lieber vier bis sechs tragende Überschriften als ein Dutzend feine. Eine Randnotiz bekommt keine eigene Überschrift, sondern einen Stichpunkt unter dem Thema, zu dem sie gehört. Eine Entscheidung, die oben schon steht, wird hier nicht noch einmal ausgeschrieben; hier steht nur, was zu ihr geführt hat. Jeder Inhalt gehört unter eine dieser Überschriften, nie ein Textblock ohne Überschrift darüber. Darunter Stichpunkte, nie Fliesstext: dieser Teil wird überflogen, nicht gelesen. Drei zusammenhängende Sätze in einem Absatz sind falsch, auch wenn jeder Satz darin stimmt -- zerleg sie in ihre einzelnen Punkte."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}, {"title": "Zitate", "description": "Höchstens drei wörtliche Aussagen, und nur solche, die beim Lesen etwas hinzufügen, was die Zusammenfassung nicht schon sagt: eine Haltung, eine Zusage oder eine Zahl in den eigenen Worten des Sprechers, jeweils mit dem Sprecher, aber nur bei eindeutiger Zuordnung. Gibt es keine solche Aussage, entfällt der Abschnitt ganz, ohne Überschrift und ohne Füll-Zitate. Ist die Formulierung im Transkript verstümmelt, lass das Zitat weg, statt es zu reparieren."}]'
WHERE id = 'mitschnitt-kompakt'
  AND title = 'Mitschnitt Kompakt'
  AND description = 'Sechs Abschnitte: TL;DR, Entscheidungen, Themen aus dem Gespräch statt fester Liste, Aufgaben als Tabelle mit Wer und Bis, offene Fragen, Zitate. Für Beratungs- und Projektgespräche, in denen hinterher zählt, wer was bis wann macht.'
  AND sections_json = '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Entscheidungen", "description": "Was tatsächlich entschieden wurde, ein Stichpunkt je Entscheidung. Eine Entscheidung braucht jemanden, der sie getroffen und ausgesprochen hat. Eine Absicht, ein Plan, ein Wunsch, eine Erwartung oder ein Vorschlag, dem nur niemand widersprochen hat, ist KEINE Entscheidung; er gehört zu den Aufgaben oder nirgendwo hin."}, {"title": "Themen", "description": "Schreib KEINE Überschrift, die wörtlich „Themen“ heisst. Bau diesen Teil aus dem Gespräch selbst: eine eigene Überschrift je Thema, das wirklich besprochen wurde, benannt nach diesem Thema. Folge den echten Themen und ihrer Gewichtung, nie einer festen Liste; überspring, was nicht vorkam. Fass zusammen, was zusammengehört: lieber vier bis sechs tragende Überschriften als ein Dutzend feine. Eine Randnotiz bekommt keine eigene Überschrift, sondern einen Stichpunkt unter dem Thema, zu dem sie gehört. Eine Entscheidung, die oben schon steht, wird hier nicht noch einmal ausgeschrieben; hier steht nur, was zu ihr geführt hat. Jeder Inhalt gehört unter eine dieser Überschriften, nie ein Textblock ohne Überschrift darüber. Darunter Stichpunkte, nie Fliesstext: dieser Teil wird überflogen, nicht gelesen. Drei zusammenhängende Sätze in einem Absatz sind falsch, auch wenn jeder Satz darin stimmt -- zerleg sie in ihre einzelnen Punkte."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}, {"title": "Zitate", "description": "Ganz am Schluss: bis zu drei WÖRTLICHE Aussagen, Wort für Wort, nie umformuliert oder geglättet, die einen Kernpunkt des Gesprächs treffen, jeweils mit dem Sprecher, aber nur bei eindeutiger Zuordnung. Ist die Formulierung im Transkript verstümmelt, lass das Zitat weg, statt es zu reparieren."}]';

UPDATE templates
SET pinned = 1,
    pin_order = 0
WHERE id = 'mitschnitt-kompakt'
  AND (pinned <> 1 OR pin_order IS NOT 0);

UPDATE templates
SET title = 'Vortrag & Schulung',
    description = 'Für Vorträge, Talks und Schulungen. Der Schwerpunkt liegt auf Begriffen und Beispielen: die tragenden Gedanken, die Definitionen dazu und die Beispiele, an denen sie erklärt wurden.'
WHERE id = 'default-lecture-notes'
  AND title = 'Vorlesungsnotizen'
  AND description = 'Für Vorlesungen, Vorträge und Schulungen. Der Schwerpunkt liegt auf Begriffen und Beispielen: die tragenden Gedanken, die Definitionen dazu und die Beispiele, an denen sie erklärt wurden.';

--
-- NACHTRAG zur Loeschbedingung: Kategorie und Zielgruppen stehen wie in
-- 20260904150000 im Vergleich; 'icon_json' bewusst NICHT (die Spalte fehlt im
-- Reparaturpfad, ein Statement, das sie nennt, reisst den Start mit). Die
-- Beschreibung von 'mitschnitt-standard' wird in zwei Fassungen akzeptiert:
-- Bestands-DBs tragen noch den Wortlaut vor dem Namens-Scrub vom 21.09.
WITH auslieferungsstand(id, title, description, category, targets_json, sections_json) AS (
  VALUES
    ('default-sprint-planning', 'Sprint-Planung', 'Für die Sprint-Planung im Entwicklungsteam. Der Schwerpunkt liegt auf Ziel, Kapazität und Definition von fertig: was der Sprint erreichen soll, wer wie viel Zeit hat und woran „fertig“ gemessen wird.', 'Engineering', '["Engineering Manager","Product Manager","Tech Lead"]', '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Sprint-Ziel", "description": "Das Hauptziel dieses Sprints in einem oder zwei Stichpunkten, so wie es ausgesprochen wurde."}, {"title": "Sprint-Backlog", "description": "Die Stories und Arbeitspakete, auf die sich das Team festgelegt hat, mit ihren Schätzungen, wo welche fielen."}, {"title": "Kapazität und Verfügbarkeit", "description": "Wer wie viel Zeit hat, Urlaube, Abwesenheiten, genannte Zahlen."}, {"title": "Abhängigkeiten und Risiken", "description": "Was von außen kommen muss und was schiefgehen kann, ein Stichpunkt je Punkt."}, {"title": "Definition von fertig", "description": "Woran „fertig“ gemessen wird: Abnahmekriterien und Standards, wie sie besprochen wurden."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}]'),
    ('default-sprint-retrospective', 'Sprint-Retrospektive', 'Für den Rückblick am Sprint-Ende. Der Schwerpunkt liegt auf dem Dreischritt: was gut lief, was nicht lief, was daraus gelernt wurde.', 'Engineering', '["Engineering Manager","Scrum Master","Tech Lead"]', '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Was gut lief", "description": "Was funktioniert hat, ein Stichpunkt je Sache, mit dem konkreten Beleg aus dem Sprint."}, {"title": "Was nicht gut lief", "description": "Probleme und Reibung, ein Stichpunkt je Sache. Benenn sie so, wie sie im Gespräch benannt wurden, ohne sie zu entschärfen."}, {"title": "Was wir gelernt haben", "description": "Die Einsichten, die das Team ausgesprochen hat, getrennt von den Maßnahmen darunter."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}]'),
    ('mitschnitt-standard', 'Mitschnitt Standard', 'Eigener Schnitt: Kurzfassung, Entscheidungen, Bälle, offene Fragen.', 'Mitschnitt', '["Berater","Projektleiter","Geschäftsführung"]', '[{"title":"Kurzfassung","description":"Drei Sätze: worum ging es, was ist das Ergebnis, was passiert als Nächstes. Fließtext, keine Aufzählung."},{"title":"Entscheidungen","description":"Was wurde entschieden, je ein Stichpunkt, mit dem Grund, wenn er genannt wurde. Nur echte Entscheidungen, keine Vorschläge oder Ideen."},{"title":"Bälle","description":"Wer macht was bis wann. Ein Stichpunkt je Ball: Name · Aufgabe · Termin (oder ‚kein Termin genannt‘). Ohne klaren Verantwortlichen ist es kein Ball, sondern eine offene Frage."},{"title":"Offene Fragen","description":"Was unentschieden oder unklar blieb, und wer die Antwort schuldet."}]')
)
DELETE FROM templates
WHERE pinned = 0
  AND pin_order IS NULL
  AND EXISTS (
    SELECT 1
    FROM auslieferungsstand AS stand
    WHERE stand.id = templates.id
      AND stand.title = templates.title
      AND (
        stand.description = templates.description
        -- Bestands-DBs vor dem Klarnamen-Scrub (Commit a69e4d9b0f) tragen
        -- hier eine Fassung mit anderem Praefix vor " Schnitt: ...".
        OR (
          stand.id = 'mitschnitt-standard'
          AND templates.description LIKE '% Schnitt: Kurzfassung, Entscheidungen, Bälle, offene Fragen.'
        )
      )
      AND stand.category IS templates.category
      AND stand.targets_json IS templates.targets_json
      AND stand.sections_json = templates.sections_json
  );
