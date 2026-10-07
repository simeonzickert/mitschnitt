-- Mitschnitt-Fork: drei Vorlagen statt vier (Entscheid 06.10.2026, 22:10).
--
-- Nach diesem Step liefert Mitschnitt genau DREI Vorlagen aus:
--   'mitschnitt-kompakt'      -> „Standard“, unveraendert
--   'default-client-kickoff'  -> „Kunden-Kickoff“, neue Abschnittsfolge
--   'default-lecture-notes'   -> „Vortrag & Schulung“, mit Zitaten, Aufgaben zuletzt
-- Entfernt wird 'default-one-on-one-meeting' („1:1-Gespraech“).
--
-- KICKOFF: Reihenfolge TL;DR, Projektueberblick, Beteiligte und Rollen,
-- Zeitplan und Meilensteine, Entscheidungen, Erfolgskriterien, Kommunikation,
-- Weitere Themen, Aufgaben, Offene Fragen, Zitate. „Entscheidungen“ und
-- „Zitate“ tragen den Wortlaut aus 'mitschnitt-kompakt' (Runde 1, Step
-- 20260911120000); „Weitere Themen“ faengt auf, was in keinen festen
-- Abschnitt passt. Die Beschreibung nennt zusaetzlich, dass Entscheidungen und
-- Zusagen mitgeschrieben werden.
-- VORTRAG: TL;DR, Kernaussagen, Begriffe und Definitionen, Beispiele und
-- Anwendungen, Offene Fragen, Zitate, Aufgaben (zuletzt; der Rahmen laesst
-- leere Abschnitte weg). Keine „Entscheidungen“.
--
-- WAS GEAENDERT/GELOESCHT WIRD, IST GENAU DER STAND NACH RUNDE 1: jede
-- Vorbedingung traegt Titel, Beschreibung, Abschnitte, Kategorie, Zielgruppen
-- und Pin-Zustand Feld fuer Feld (der Inhalt entscheidet, nie ein Zeitstempel).
-- Vom Nutzer angelegte oder importierte Vorlagen (UUIDs) stehen nie in diesen
-- Bedingungen. Wer eine der drei Vorlagen bearbeitet oder angeheftet hat,
-- behaelt sie unveraendert. 'icon_json' wird nicht genannt (die Spalte fehlt im
-- Reparaturpfad). Die Auswahl fasst dieser Step nicht an: siehe
-- 20260912120100_wahl_nach_vorlagen_drei.sql.
--
-- Idempotent: ein zweiter Lauf findet keine Zeile mit altem Stand. Downgrade-
-- sicher: nur Daten, keine Schemaaenderung.

UPDATE templates
SET description = 'Für den Auftakt eines neuen Kundenprojekts. Der Schwerpunkt liegt auf Beteiligten, Zeitplan und Erfolgskriterien: wer macht was, bis wann, und woran wird am Ende gemessen. Entscheidungen und Zusagen werden mitgeschrieben.',
    sections_json = '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Projektüberblick", "description": "Umfang, Ziel und Liefergegenstände, wie sie im Gespräch benannt wurden."}, {"title": "Beteiligte und Rollen", "description": "Wer mitarbeitet und wofür zuständig ist. Nenn eine Rolle nur, wo sie im Gespräch zugeordnet wurde."}, {"title": "Zeitplan und Meilensteine", "description": "Termine und Meilensteine, wie sie ausgesprochen wurden, ein Stichpunkt je Punkt."}, {"title": "Entscheidungen", "description": "Was tatsächlich entschieden wurde, ein Stichpunkt je Entscheidung. Eine Entscheidung braucht jemanden, der sie getroffen und ausgesprochen hat. Eine Absicht, ein Plan, ein Wunsch, eine Erwartung oder ein Vorschlag, dem nur niemand widersprochen hat, ist KEINE Entscheidung; er gehört zu den Aufgaben oder nirgendwo hin."}, {"title": "Erfolgskriterien", "description": "Woran das Projekt gemessen wird. Nur Kriterien, auf die man sich im Gespräch bezogen hat, mit ihren Zahlen."}, {"title": "Kommunikation", "description": "Wie und wie oft man sich abstimmt: Termine, Kanäle, Berichtswege."}, {"title": "Weitere Themen", "description": "Nur Themen, die im Gespräch wirklich vorkamen und in keinen der Abschnitte oben passen: eine eigene Überschrift je Thema, benannt nach dem, worum es ging. Gibt es keine, entfällt dieser Teil ganz."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}, {"title": "Zitate", "description": "Höchstens drei wörtliche Aussagen, und nur solche, die beim Lesen etwas hinzufügen, was die Zusammenfassung nicht schon sagt: eine Haltung, eine Zusage oder eine Zahl in den eigenen Worten des Sprechers, jeweils mit dem Sprecher, aber nur bei eindeutiger Zuordnung. Gibt es keine solche Aussage, entfällt der Abschnitt ganz, ohne Überschrift und ohne Füll-Zitate. Ist die Formulierung im Transkript verstümmelt, lass das Zitat weg, statt es zu reparieren."}]'
WHERE id = 'default-client-kickoff'
  AND title = 'Kunden-Kickoff'
  AND description = 'Für den Auftakt eines neuen Kundenprojekts. Der Schwerpunkt liegt auf Beteiligten, Zeitplan und Erfolgskriterien: wer macht was, bis wann, und woran wird am Ende gemessen.'
  AND category IS 'Customer Success'
  AND targets_json IS '["Customer Success Manager","Account Manager","Implementation Lead"]'
  AND sections_json = '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Projektüberblick", "description": "Umfang, Ziel und Liefergegenstände, wie sie im Gespräch benannt wurden."}, {"title": "Beteiligte und Rollen", "description": "Wer mitarbeitet und wofür zuständig ist. Nenn eine Rolle nur, wo sie im Gespräch zugeordnet wurde."}, {"title": "Zeitplan und Meilensteine", "description": "Termine und Meilensteine, wie sie ausgesprochen wurden, ein Stichpunkt je Punkt."}, {"title": "Kommunikation", "description": "Wie und wie oft man sich abstimmt: Termine, Kanäle, Berichtswege."}, {"title": "Erfolgskriterien", "description": "Woran das Projekt gemessen wird. Nur Kriterien, auf die man sich im Gespräch bezogen hat, mit ihren Zahlen."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}]'
  AND pinned = 0
  AND pin_order IS NULL;

UPDATE templates
SET sections_json = '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Kernaussagen", "description": "Die tragenden Gedanken und Theorien, ein Stichpunkt je Gedanke."}, {"title": "Begriffe und Definitionen", "description": "Jeder erklärte Begriff mit seiner Definition, so wie sie gesagt wurde. Ein Stichpunkt je Begriff, Begriff zuerst."}, {"title": "Beispiele und Anwendungen", "description": "Die Beispiele, an denen die Begriffe erklärt wurden, jeweils mit dem Begriff, zu dem sie gehören."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}, {"title": "Zitate", "description": "Höchstens drei wörtliche Aussagen, und nur solche, die beim Lesen etwas hinzufügen, was die Zusammenfassung nicht schon sagt: eine Haltung, eine Zusage oder eine Zahl in den eigenen Worten des Sprechers, jeweils mit dem Sprecher, aber nur bei eindeutiger Zuordnung. Gibt es keine solche Aussage, entfällt der Abschnitt ganz, ohne Überschrift und ohne Füll-Zitate. Ist die Formulierung im Transkript verstümmelt, lass das Zitat weg, statt es zu reparieren."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}]'
WHERE id = 'default-lecture-notes'
  AND title = 'Vortrag & Schulung'
  AND description = 'Für Vorträge, Talks und Schulungen. Der Schwerpunkt liegt auf Begriffen und Beispielen: die tragenden Gedanken, die Definitionen dazu und die Beispiele, an denen sie erklärt wurden.'
  AND category IS 'Education'
  AND targets_json IS '["Student","Graduate Student","Researcher"]'
  AND sections_json = '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Kernaussagen", "description": "Die tragenden Gedanken und Theorien, ein Stichpunkt je Gedanke."}, {"title": "Begriffe und Definitionen", "description": "Jeder erklärte Begriff mit seiner Definition, so wie sie gesagt wurde. Ein Stichpunkt je Begriff, Begriff zuerst."}, {"title": "Beispiele und Anwendungen", "description": "Die Beispiele, an denen die Begriffe erklärt wurden, jeweils mit dem Begriff, zu dem sie gehören."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}]'
  AND pinned = 0
  AND pin_order IS NULL;

DELETE FROM templates
WHERE id = 'default-one-on-one-meeting'
  AND title = '1:1-Gespräch'
  AND description = 'Für das regelmäßige Vieraugengespräch mit einem Teammitglied. Der Schwerpunkt liegt auf Zielen und Feedback: was seit dem letzten Mal passiert ist, wo es hakt, wie die Ziele stehen und was in beide Richtungen zurückgemeldet wurde.'
  AND category IS 'Management'
  AND targets_json IS '["Manager","Engineering Manager","Team Lead"]'
  AND sections_json = '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst, für jemanden mit dreißig Sekunden."}, {"title": "Seit dem letzten Mal", "description": "Was seit dem vorigen Gespräch passiert ist, ein Stichpunkt je Sache."}, {"title": "Erfolge und Hürden", "description": "Was gelungen ist und was im Weg stand, getrennt in einzelne Stichpunkte."}, {"title": "Ziele und Fortschritt", "description": "Die besprochenen Ziele und wo sie stehen. Nur Ziele, die im Gespräch wirklich vorkamen; ein Stichpunkt je Ziel, mit dem konkreten Stand."}, {"title": "Feedback", "description": "Rückmeldungen in beide Richtungen, mit Sprecher, wo die Zuordnung eindeutig ist. Lob und Kritik im Wortlaut der Sache, nicht geglättet."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage."}]'
  AND pinned = 0
  AND pin_order IS NULL;
