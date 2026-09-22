-- Mitschnitt-Fork: „Mitschnitt Kompakt“ -- die neue Standard-Vorlage.
--
-- Herkunft: der Vergleich dreier Fassungen an derselben echten Sitzung
-- (eine Zwei-Stunden-Aufnahme, 02.09.2026); Belege in
-- einem Vergleichsordner ausserhalb des Repos.
-- Der Entscheid daraus: die Substanz des Swift-Prompts
-- (SWIFT-PROMPT-MIT-TABELLE.md) mit dem Gegenpruef-Abschnitt aus
-- 'mitschnitt-standard' -- der hat als einziger alle Verhoerungen an einem Ort
-- gezeigt („finde ich mega“). Sieben Abschnitte, Gegenpruefung an Stelle 6,
-- Zitate zuletzt.
--
-- Zwei Abweichungen gegen die gemessene Tabellen-Fassung, beide aus dem Lauf:
--   * Scanbarkeit ist die oberste Anforderung („nicht so verbose“). Sie steht
--     deshalb nicht nur bei den Themen, sondern in der Beschreibung ueber
--     allem: ein Stichpunkt traegt eine Sache, ein bis zwei Saetze.
--   * Die Tabellen-Fassung schrieb sechsmal „offen“, wo im Gespraech „jetzt“
--     oder „heute“ gesagt wurde. Eine im Gespraech ausgesprochene relative
--     Frist ist eine Frist und wird woertlich uebernommen; „offen“ bleibt
--     Aufgaben ohne jede Zeitangabe vorbehalten. Die Regel gegen
--     AUSGERECHNETE Datumsangaben bleibt davon unberuehrt -- sie ist der
--     eigentliche Zahn, und beide stehen nebeneinander in der Aufgaben-Regel.
--
-- SIE ERSETZT NICHTS. 'mitschnitt-standard' und 'mitschnitt-adaptive-minutes'
-- bleiben unangetastet in der Liste, damit man zurueckwechseln kann. Dass
-- diese hier Standard wird, steht getrennt in
-- 20260904140100_mitschnitt_kompakt_als_standard.sql -- der Reparaturpfad in
-- db-app spielt die Vorlagen-Seeds nach und darf dabei nie eine Einstellung
-- anfassen.
--
-- Wie die Nachbarn: INSERT OR IGNORE, damit eine vom Nutzer bearbeitete
-- Fassung stehen bleibt; eigene ID ohne 'default-'-Praefix, damit ein
-- kuenftiger Upstream-Seed nie kollidiert; kein icon_json, der Spalten-Default
-- gilt. Neuer Step statt Aenderung an 20260902001000/20260904120000 -- beide
-- sind eingefroren.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

INSERT OR IGNORE INTO templates (
  id, title, description, pinned, pin_order, category, targets_json, sections_json
) VALUES (
  'mitschnitt-kompakt',
  'Mitschnitt Kompakt',
  'Sieben Abschnitte: TL;DR, Entscheidungen, Themen aus dem Gespräch statt fester Liste, Aufgaben als Tabelle mit Wer und Bis, offene Fragen, Zahlen und Namen zur Gegenprüfung, Zitate. Scanbarkeit ist die oberste Anforderung: ein Stichpunkt trägt eine Sache, ein bis zwei Sätze; Tiefe kommt aus der ANZAHL der Stichpunkte, nie aus längeren. Drei harte Regeln stehen über allen Abschnitten und schlagen Vollständigkeit. Erstens: erfinde nichts. Jede Zahl, jeder Name, jedes Datum muss im Transkript stehen; verstümmelt das Transkript eine Zahl, schreib „[unklar]“ statt eines plausiblen Werts. Zweitens: ein Vorschlag ist weder Entscheidung noch Aufgabe. Widerspruch, eine abtuende Bemerkung, offene Skepsis oder ein Themenwechsel heisst, es ist nicht passiert; ein Vorschlag, den du als abgelehnt beschreibst, darf nirgends wieder auftauchen, auch nicht im TL;DR. Drittens: schreib eine Aussage nie einer Person zu, deren Zuordnung unsicher ist, und glätte keine Lücken -- eine fehlende oder unverständliche Stelle wird als solche benannt. Kein Füllwerk, kein Vorspann, keine Meta-Kommentare; die Länge folgt der Substanz. Ein Abschnitt ohne Inhalt fällt ganz weg, statt kurz zu bleiben.',
  0,
  NULL,
  'Mitschnitt',
  '["Berater", "Projektleiter", "Geschäftsführung"]',
  '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst. Für jemanden mit dreißig Sekunden. Nur was wirklich besprochen wurde; erfinde nichts. Was du weiter unten als abgelehnt beschreibst, steht hier nicht. Ein Stichpunkt trägt eine Sache, ein bis zwei Sätze."}, {"title": "Entscheidungen", "description": "Was tatsächlich entschieden wurde, ein Stichpunkt je Entscheidung, ein bis zwei Sätze. Eine Entscheidung braucht jemanden, der sie getroffen und ausgesprochen hat. Eine Absicht, ein Plan, ein Wunsch, eine Erwartung oder ein Vorschlag, dem nur niemand widersprochen hat, ist KEINE Entscheidung; er gehört zu den Aufgaben oder nirgendwo hin. Wurde nichts entschieden, lass diesen Abschnitt ganz weg."}, {"title": "Themen", "description": "Schreib KEINE Überschrift, die wörtlich „Themen“ heisst. Bau diesen Teil aus dem Gespräch selbst: eine eigene Überschrift je Thema, das wirklich besprochen wurde, benannt nach diesem Thema. Folge den echten Themen und ihrer Gewichtung, nie einer festen Liste; überspring, was nicht vorkam. Fass zusammen, was zusammengehört: lieber vier bis sechs tragende Überschriften als ein Dutzend feine. Eine Randnotiz bekommt keine eigene Überschrift, sondern einen Stichpunkt unter dem Thema, zu dem sie gehört. Eine Entscheidung, die oben schon steht, wird hier nicht noch einmal ausgeschrieben; hier steht nur, was zu ihr geführt hat. Jeder Inhalt gehört unter eine dieser Überschriften, nie ein Textblock ohne Überschrift darüber. Darunter Stichpunkte, nie Fliesstext: dieser Teil wird überflogen, nicht gelesen. Drei zusammenhängende Sätze in einem Absatz sind falsch, auch wenn jeder Satz darin stimmt -- zerleg sie in ihre einzelnen Punkte. Ein Punkt je Stichpunkt, ein bis zwei Sätze; braucht ein Stichpunkt mehr, war er mehr als ein Punkt. Die Stichpunkte tragen die Substanz: Namen, Zahlen, Positionen, das konkrete Detail. „Budget besprochen“ ist wertlos, „Budget von 50.000 Euro genannt, Ole hält das für zu niedrig“ ist ein Stichpunkt. Tiefe kommt aus der ANZAHL der Stichpunkte, nie aus längeren: ein wichtiges Thema bekommt acht, eine Randnotiz einen."}, {"title": "Aufgaben", "description": "Als Markdown-TABELLE mit genau diesen Spalten:\n\n| Aufgabe | Wer | Bis |\n|---|---|---|\n\nEine Zeile je Aufgabe. Schreib nie ein Pipe-Zeichen in eine Tabellenzelle, das bricht die Tabelle; formulier um oder ersetz es durch ein Komma. Nimm eine Aufgabe nur auf, wo das Transkript zeigt, dass sie wirklich angenommen wurde. Lies vorher die Themen-Abschnitte oben noch einmal: steht dort, dass ein Vorschlag abgelehnt, abgetan oder skeptisch aufgenommen wurde, darf er hier nicht als Zeile erscheinen. Dein eigener Text oben schlägt jeden Drang zur Vollständigkeit. Eine Aufgabe braucht auch jemanden, der sie übernommen hat: nenn einen Namen nur, wo das Transkript diese Person annehmen zeigt, sonst „[unklar]“. „[unklar]“ heisst, dass der Mensch hinter einer übernommenen Aufgabe unklar ist, nie „niemand hat sie übernommen“. Hat niemand sie übernommen, gibt es keine Zeile. Termine, zwei Regeln nebeneinander. Erstens: eine im Gespräch ausgesprochene relative Frist ist eine Frist -- „jetzt“, „heute“, „morgen“, „Freitag“, „nächste Woche“, „Anfang der Woche“ gehören wörtlich in die Spalte Bis, nie als „offen“. „offen“ ist nur für Aufgaben da, zu denen überhaupt keine Zeitangabe fiel. Zweitens: rechne eine relative Frist nur dann in ein Datum um, wenn dir das Datum des Gesprächs als Angabe vorliegt. Ein im Gespräch GENANNTES Datum ist keine solche Angabe. Liegt keins vor, lass die relative Formulierung wörtlich stehen und rechne nie aus deinem eigenen Gefühl für heute. Wurden keine Aufgaben genannt, lass den Abschnitt samt Tabelle ganz weg; gib nie eine leere Tabelle aus."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Ein Stichpunkt je Frage, ein bis zwei Sätze. Benenn eine Unsicherheit ehrlich, statt sie zu glätten. War alles klar, lass den Abschnitt weg."}, {"title": "Zahlen und Namen zur Gegenprüfung", "description": "Alle Zahlen, Beträge, Termine und Eigennamen aus dem Gespräch, so wie sie im Transkript stehen, als kurze Liste. Sie dienen dem Gegenprüfen, weil sich die Transkription verhören kann; nichts ergänzen, nichts runden. Schreibt das Transkript denselben Namen verschieden, nenn die Varianten nebeneinander, statt dich für eine zu entscheiden."}, {"title": "Zitate", "description": "Ganz am Schluss: bis zu drei WÖRTLICHE Aussagen, Wort für Wort, nie umformuliert oder geglättet, die einen Kernpunkt des Gesprächs treffen, jeweils mit dem Sprecher, aber nur bei eindeutiger Zuordnung. Ist die Formulierung im Transkript verstümmelt, lass das Zitat weg, statt es zu reparieren. Ein Zitat mit unklarem Sprecher fällt weg; gibt es keine, fällt der ganze Abschnitt weg."}]'
);
