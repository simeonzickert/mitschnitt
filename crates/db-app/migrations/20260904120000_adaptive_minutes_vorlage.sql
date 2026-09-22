-- Mitschnitt-Fork: „Adaptive Minutes“ als ZUSAETZLICHE Vorlage.
--
-- Herkunft: `ProtocolGenerator.protocolPrompt` aus dem eingefrorenen
-- Swift-Vorgaenger (~/Code/meeting-transcriber). Der Prompt ist dort ueber
-- Wochen gehaertet worden (ZICK-128 und Folgerunden) und traegt Regeln, die
-- der urspruenglichen anarlog-Vorlage (`adaptive-minutes-template.json`)
-- fehlen: das Abgelehnt-Gate, die Existenzbedingung fuer Aufgaben, die
-- Datumsregel, das woertliche Zitat statt „verbatim or faithful“.
--
-- SIE ERSETZT NICHTS. 'mitschnitt-standard' bleibt Standard und bleibt
-- unangetastet, keine Einstellung wird angefasst -- diese Datei enthaelt genau
-- ein INSERT. Der Nutzer waehlt sie in der Vorlagen-Liste aus und vergleicht
-- selbst.
--
-- Wie die Nachbarn: INSERT OR IGNORE, damit eine vom Nutzer bearbeitete
-- Fassung stehen bleibt; eigene ID ohne 'default-'-Praefix, damit ein
-- kuenftiger Upstream-Seed nie kollidiert; kein icon_json, der Spalten-Default
-- gilt. Neuer Step statt Aenderung an 20260902001000 -- der ist eingefroren.
--
-- Was der alte Prompt kann und diese Form NICHT ausdrueckt, steht nicht hier,
-- sondern im Bericht zum Lauf: die Vorlage wird in `enhance.user.md.jinja`
-- UNTER dem Transkript gerendert, und genau diese Platzierung hat der
-- Swift-Bau am 21.07.2026 als Fehler behoben. Was hier steht, ist die Haelfte,
-- die die Vorlagen-Form tragen kann.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

INSERT OR IGNORE INTO templates (
  id, title, description, pinned, pin_order, category, targets_json, sections_json
) VALUES (
  'mitschnitt-adaptive-minutes',
  'Adaptive Minutes (alter Schnitt)',
  'Der ausgefeilte Schnitt aus dem Swift-Vorgänger: TL;DR, Entscheidungen, Themen aus dem Gespräch statt fester Liste, Aufgaben mit Wer und Bis, offene Fragen, Zitate. Drei harte Regeln stehen über allen Abschnitten und schlagen Vollständigkeit. Erstens: erfinde nichts. Jede Zahl, jeder Name, jedes Datum muss im Transkript stehen; verstümmelt das Transkript eine Zahl, schreib „[unklar]“ statt eines plausiblen Werts. Zweitens: ein Vorschlag ist weder Entscheidung noch Aufgabe. Widerspruch, eine abtuende Bemerkung, offene Skepsis oder ein Themenwechsel heisst, es ist nicht passiert; ein Vorschlag, den du als abgelehnt beschreibst, darf nirgends wieder auftauchen, auch nicht im TL;DR. Drittens: schreib eine Aussage nie einer Person zu, deren Zuordnung unsicher ist, und glätte keine Lücken. Kein Füllwerk, keine Meta-Kommentare, die Länge folgt der Substanz. Ein Abschnitt ohne Inhalt fällt ganz weg, statt kurz zu bleiben.',
  0,
  NULL,
  'Mitschnitt',
  '["Berater", "Projektleiter", "Geschäftsführung"]',
  '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst. Für jemanden mit 30 Sekunden. Nur was wirklich besprochen wurde; erfinde nichts. Was du weiter unten als abgelehnt beschreibst, steht hier nicht."}, {"title": "Entscheidungen", "description": "Was tatsächlich entschieden wurde, ein Stichpunkt je Entscheidung. Eine Entscheidung braucht jemanden, der sie getroffen und ausgesprochen hat. Eine Absicht, ein Plan, ein Wunsch, eine Erwartung oder ein Vorschlag, dem nur niemand widersprochen hat, ist KEINE Entscheidung; er gehört zu den Aufgaben oder nirgendwo hin. Wurde nichts entschieden, lass diesen Abschnitt ganz weg."}, {"title": "Themen", "description": "Schreib KEINE Überschrift, die wörtlich „Themen“ heisst. Bau diesen Teil aus dem Gespräch selbst: eine eigene Überschrift je Thema, das wirklich besprochen wurde, benannt nach diesem Thema. Folge den echten Themen und ihrer Gewichtung, nie einer festen Liste; überspring, was nicht vorkam. Jeder Inhalt gehört unter eine dieser Überschriften, nie ein Textblock ohne Überschrift darüber. Darunter Stichpunkte, nie Fliesstext: dieser Teil wird überflogen, nicht gelesen. Ein Punkt je Stichpunkt, ein bis zwei Sätze; braucht ein Stichpunkt mehr, war er mehr als ein Punkt. Die Stichpunkte tragen die Substanz: Namen, Zahlen, Positionen, das konkrete Detail. „Budget besprochen“ ist wertlos, „Budget von 50.000 Euro genannt, Ole hält das für zu niedrig“ ist ein Stichpunkt. Tiefe kommt aus der ANZAHL der Stichpunkte, nie aus längeren: ein wichtiges Thema bekommt acht, eine Randnotiz einen."}, {"title": "Aufgaben", "description": "Als Aufzählung, ein Stichpunkt je Aufgabe in der Form: Aufgabe — Wer — Bis. Niemals eine Tabelle. Nimm eine Aufgabe nur auf, wo das Transkript zeigt, dass sie wirklich angenommen wurde. Lies vorher die Themen-Abschnitte oben noch einmal: steht dort, dass ein Vorschlag abgelehnt, abgetan oder skeptisch aufgenommen wurde, darf er hier nicht als Stichpunkt erscheinen. Dein eigener Text oben schlägt jeden Drang zur Vollständigkeit. Eine Aufgabe braucht auch jemanden, der sie übernommen hat: nenn einen Namen nur, wo das Transkript diese Person annehmen zeigt, sonst „[unklar]“. „[unklar]“ heisst, dass der Mensch hinter einer übernommenen Aufgabe unklar ist, nie „niemand hat sie übernommen“. Hat niemand sie übernommen, gibt es keinen Stichpunkt. Termine: rechne eine relative Frist („nächste Woche“) nur dann in ein Datum um, wenn dir das Datum des Gesprächs als Angabe vorliegt. Ein im Gespräch GENANNTES Datum ist keine solche Angabe. Liegt keins vor, lass die relative Formulierung wörtlich stehen und rechne nie aus deinem eigenen Gefühl für heute. Unbekannte Frist heisst „offen“. Wurden keine Aufgaben genannt, lass den Abschnitt ganz weg."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Benenn eine Unsicherheit ehrlich, statt sie zu glätten. War alles klar, lass den Abschnitt weg."}, {"title": "Zitate", "description": "Ganz am Schluss: bis zu drei WÖRTLICHE Aussagen, Wort für Wort, nie umformuliert oder geglättet, die einen Kernpunkt des Gesprächs treffen, jeweils mit dem Sprecher, aber nur bei eindeutiger Zuordnung. Ist die Formulierung im Transkript verstümmelt, lass das Zitat weg, statt es zu reparieren. Ein Zitat mit unklarem Sprecher fällt weg; gibt es keine, fällt der ganze Abschnitt weg."}]'
);
