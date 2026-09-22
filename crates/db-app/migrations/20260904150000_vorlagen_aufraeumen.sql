-- Mitschnitt-Fork: dreizehn Vorlagen verlassen die Liste (Entscheid 04.09.2026).
--
-- Zwoelf davon stammen aus dem Upstream-Seed 20260524000000_default_templates
-- und haben mit der Arbeit hier nichts zu tun (Investor Pitch, Daily Standup,
-- Performance Review …). Die dreizehnte ist unsere eigene
-- 'mitschnitt-adaptive-minutes' -- das Vergleichsstueck vom 04.09., dessen
-- Wert inzwischen in 'mitschnitt-kompakt' steckt („alter schnitt raus").
--
-- Uebrig bleiben SIEBEN: seine fuenf behaltenen Upstream-Vorlagen
-- (1:1 Meeting, Client Kickoff, Lecture Notes, Sprint Planning, Sprint
-- Retrospective) plus 'mitschnitt-kompakt' (der Standard) und
-- 'mitschnitt-standard' (sein Rueckweg).
--
-- WAS GELOESCHT WIRD, IST GENAU DER AUSLIEFERUNGSZUSTAND -- nichts sonst.
-- Der Block unten traegt jede Zeile so, wie ihr Seed sie eingesetzt hat, und
-- geloescht wird nur, was Feld fuer Feld noch damit uebereinstimmt: Titel,
-- Beschreibung, Kategorie, Zielgruppen, Abschnitte, dazu ungepinnt.
-- Wer eine dieser Vorlagen bearbeitet oder angeheftet hat, behaelt sie.
--
-- EINE bekannte Luecke, bewusst in Kauf genommen: 'icon_json' steht NICHT im
-- Vergleich. Wer an einer dieser dreizehn Vorlagen nur das Icon geaendert und
-- sonst nichts angefasst hat, verliert sie trotzdem. Der Grund ist keine
-- Bequemlichkeit, sondern ein gemessener Startfehler: im Reparaturpfad legt
-- prepare_schema die verlorene 'templates'-Tabelle VOR migrate wieder an, und
-- zwar ohne 'icon_json' -- die Spalte kommt erst dahinter (has_icon_json in
-- crates/db-app/src/lib.rs). Ein DELETE, das die Spalte nennt, stirbt dort
-- beim Vorbereiten der Anweisung mit "no such column: icon_json" und reisst
-- den ganzen Start mit. Belegt vom Test
-- mitschnitt_standard::ohne_tabelle_und_mit_ausstehendem_step_laeuft_prepare_schema_durch,
-- der genau daran rot wurde.
--
-- Warum Inhalt statt Zeitstempel: 'created_at = updated_at' sieht wie „nie
-- angefasst" aus, ist es aber nicht. Der Legacy-Vault-Import legt eine vom
-- Nutzer BEARBEITETE Vorlage unter derselben 'default-'-ID neu an
-- (crates/db-app/src/template_ops.rs, insert_template_if_missing) und setzt
-- beide Zeitstempel in EINEM Statement -- SQLite haelt 'now' pro Statement
-- fest, also sind sie gleich. Ein Zeitstempel-Vergleich haette genau diese
-- Arbeit geloescht. Der Inhaltsvergleich kann das nicht.
--
-- RUECKWEG: die Zeilen unten sind zugleich die Wiederherstellung. Ein
-- 'INSERT OR IGNORE INTO templates (id, title, description, pinned, pin_order,
-- category, targets_json, sections_json)' mit denselben Werten (pinned 0,
-- pin_order NULL) bringt jede zurueck; im Wortlaut stehen sie ausserdem in
-- ihren Seeds 20260524000000_default_templates.sql und
-- 20260904120000_adaptive_minutes_vorlage.sql, die beide unangetastet
-- bleiben (deren Pruefsumme darf sich nie aendern).
--
-- Die gewaehlte Standard-Vorlage fasst dieser Step NICHT an -- das macht der
-- Nachbar 20260904150100, getrennt aus demselben Grund wie bei den beiden
-- Standard-Steps davor: der Reparaturpfad (replay_template_seeds in
-- crates/db-app/src/lib.rs) spielt Vorlagen-Steps nach und darf dabei nie
-- eine Einstellung anfassen.
--
-- Downgrade-sicher: nur Daten, keine Schemaaenderung.

WITH auslieferungsstand(id, title, description, category, targets_json, sections_json) AS (
  VALUES
    ('default-board-meeting', 'Board Meeting', 'For board meetings and governance discussions', 'Leadership', '["Founder","CEO","Board Member"]', '[{"title":"Company Performance","description":"Financial and operational updates"},{"title":"Strategic Initiatives","description":"Progress on key strategic priorities"},{"title":"Financial Review","description":"Budget, runway, and financial health"},{"title":"Board Decisions","description":"Resolutions and governance matters"},{"title":"Risk & Compliance","description":"Risk management and compliance updates"},{"title":"Next Steps","description":"Action items and follow-ups"}]'),
    ('default-brainstorming-session', 'Brainstorming Session', 'For creative brainstorming and ideation sessions', 'Product', '["Product Manager","Designer","Marketing"]', '[{"title":"Session Goal","description":"What problem or opportunity are we exploring?"},{"title":"Ideas Generated","description":"All ideas captured during brainstorming"},{"title":"Promising Concepts","description":"Ideas worth exploring further"},{"title":"Evaluation Criteria","description":"How we''ll assess ideas"},{"title":"Next Steps","description":"Action items to move forward"}]'),
    ('default-customer-discovery', 'Customer Discovery Interview', 'For conducting customer discovery and user research interviews', 'Product', '["Product Manager","UX Researcher","Founder"]', '[{"title":"Background","description":"Customer context and background"},{"title":"Current Process","description":"How they currently solve the problem"},{"title":"Pain Points","description":"Challenges and frustrations"},{"title":"Needs & Goals","description":"What they''re trying to achieve"},{"title":"Feedback on Solution","description":"Reactions to proposed solution"},{"title":"Key Insights","description":"Main takeaways and learnings"}]'),
    ('default-daily-standup', 'Daily Standup', 'For quick daily syncs to share progress and blockers', 'Engineering', '["Software Engineer","Engineering Manager","Scrum Master"]', '[{"title":"Yesterday''s Accomplishments","description":"What did you complete yesterday?"},{"title":"Today''s Plan","description":"What are you working on today?"},{"title":"Blockers","description":"Any obstacles or help needed?"},{"title":"Team Updates","description":"Important announcements or information"}]'),
    ('default-executive-briefing', 'Executive Briefing', 'For capturing high-level strategic discussions and decisions', 'Leadership', '["Executive","CEO","VP"]', '[{"title":"Strategic Overview","description":"High-level context and objectives"},{"title":"Key Metrics","description":"Performance indicators and trends"},{"title":"Major Decisions","description":"Strategic decisions and rationale"},{"title":"Resource Allocation","description":"Budget and resource commitments"},{"title":"Risks & Opportunities","description":"Strategic risks and growth opportunities"},{"title":"Action Items","description":"Executive-level follow-ups"}]'),
    ('default-incident-postmortem', 'Incident Postmortem', 'For conducting blameless postmortems after incidents or outages', 'Engineering', '["Platform Engineer","SRE","DevOps Engineer"]', '[{"title":"Incident Summary","description":"What happened and when"},{"title":"Timeline of Events","description":"Chronological sequence of events"},{"title":"Root Cause Analysis","description":"What caused the incident"},{"title":"Impact Assessment","description":"Effect on users and business"},{"title":"Response & Resolution","description":"How the incident was resolved"},{"title":"Lessons Learned","description":"Key takeaways and insights"},{"title":"Action Items","description":"Preventive measures and improvements"}]'),
    ('default-investor-pitch', 'Investor Pitch Meeting', 'For pitching to investors and VCs', 'Fundraising', '["Founder","CEO","CFO"]', '[{"title":"Meeting Context","description":"Investor background and meeting purpose"},{"title":"Pitch Summary","description":"Key points presented"},{"title":"Questions Asked","description":"Investor questions and concerns"},{"title":"Feedback Received","description":"Investor reactions and feedback"},{"title":"Investment Interest","description":"Level of interest and next steps"},{"title":"Follow-up Items","description":"Information requested and action items"}]'),
    ('default-performance-review', 'Performance Review', 'For conducting employee performance reviews and career development discussions', 'Management', '["Manager","Engineering Manager","HR"]', '[{"title":"Review Period Summary","description":"Overview of the review period"},{"title":"Key Accomplishments","description":"Major achievements and contributions"},{"title":"Areas of Strength","description":"Skills and behaviors to continue"},{"title":"Areas for Development","description":"Growth opportunities and improvements"},{"title":"Career Goals","description":"Career aspirations and development path"},{"title":"Action Plan","description":"Development goals and next steps"}]'),
    ('default-product-roadmap-review', 'Product Roadmap Review', 'For reviewing and aligning on product roadmap priorities', 'Product', '["Product Manager","Engineering Manager","Executive"]', '[{"title":"Current Status","description":"Progress on in-flight initiatives"},{"title":"Upcoming Features","description":"Next quarter priorities and timeline"},{"title":"Customer Feedback","description":"Key insights from users and customers"},{"title":"Success Metrics","description":"KPIs and measurement approach"},{"title":"Resource Allocation","description":"Team assignments and capacity"},{"title":"Decisions Made","description":"Key decisions and trade-offs"}]'),
    ('default-project-kickoff', 'Project Kickoff', 'For starting new projects with clear objectives and alignment', 'Product', '["Product Manager","Engineering Manager","Designer"]', '[{"title":"Project Overview","description":""},{"title":"Goals & Success Metrics","description":"Define what success looks like"},{"title":"Stakeholders & Roles","description":""},{"title":"Timeline & Milestones","description":""},{"title":"Risks & Dependencies","description":""},{"title":"Next Steps","description":""}]'),
    ('default-sales-discovery-call', 'Sales Discovery Call', 'For qualifying leads and understanding customer needs', 'Sales', '["Account Executive","Sales Rep","BDR"]', '[{"title":"Company Overview","description":"Background on the prospect"},{"title":"Current Situation","description":"How they operate today"},{"title":"Challenges & Pain Points","description":"Problems they''re trying to solve"},{"title":"Goals & Success Criteria","description":"What success looks like"},{"title":"Budget & Timeline","description":"Financial and timing constraints"},{"title":"Next Steps","description":"Follow-up actions and timeline"}]'),
    ('default-technical-design-review', 'Technical Design Review', 'For reviewing technical designs and architecture decisions', 'Engineering', '["Software Engineer","Tech Lead","Platform Engineer"]', '[{"title":"Problem Statement","description":"What problem are we solving?"},{"title":"Proposed Solution","description":"Technical approach and architecture"},{"title":"Alternatives Considered","description":"Other options and trade-offs"},{"title":"Implementation Plan","description":"Breakdown of work and timeline"},{"title":"Testing Strategy","description":"How we''ll validate the solution"},{"title":"Risks & Mitigation","description":"Potential issues and contingency plans"},{"title":"Decisions & Action Items","description":"Outcomes and next steps"}]'),
    ('mitschnitt-adaptive-minutes', 'Adaptive Minutes (alter Schnitt)', 'Der ausgefeilte Schnitt aus dem Swift-Vorgänger: TL;DR, Entscheidungen, Themen aus dem Gespräch statt fester Liste, Aufgaben mit Wer und Bis, offene Fragen, Zitate. Drei harte Regeln stehen über allen Abschnitten und schlagen Vollständigkeit. Erstens: erfinde nichts. Jede Zahl, jeder Name, jedes Datum muss im Transkript stehen; verstümmelt das Transkript eine Zahl, schreib „[unklar]“ statt eines plausiblen Werts. Zweitens: ein Vorschlag ist weder Entscheidung noch Aufgabe. Widerspruch, eine abtuende Bemerkung, offene Skepsis oder ein Themenwechsel heisst, es ist nicht passiert; ein Vorschlag, den du als abgelehnt beschreibst, darf nirgends wieder auftauchen, auch nicht im TL;DR. Drittens: schreib eine Aussage nie einer Person zu, deren Zuordnung unsicher ist, und glätte keine Lücken. Kein Füllwerk, keine Meta-Kommentare, die Länge folgt der Substanz. Ein Abschnitt ohne Inhalt fällt ganz weg, statt kurz zu bleiben.', 'Mitschnitt', '["Berater", "Projektleiter", "Geschäftsführung"]', '[{"title": "TL;DR", "description": "Die drei bis fünf wichtigsten Punkte, Entscheidungen und Aufgaben zuerst. Für jemanden mit 30 Sekunden. Nur was wirklich besprochen wurde; erfinde nichts. Was du weiter unten als abgelehnt beschreibst, steht hier nicht."}, {"title": "Entscheidungen", "description": "Was tatsächlich entschieden wurde, ein Stichpunkt je Entscheidung. Eine Entscheidung braucht jemanden, der sie getroffen und ausgesprochen hat. Eine Absicht, ein Plan, ein Wunsch, eine Erwartung oder ein Vorschlag, dem nur niemand widersprochen hat, ist KEINE Entscheidung; er gehört zu den Aufgaben oder nirgendwo hin. Wurde nichts entschieden, lass diesen Abschnitt ganz weg."}, {"title": "Themen", "description": "Schreib KEINE Überschrift, die wörtlich „Themen“ heisst. Bau diesen Teil aus dem Gespräch selbst: eine eigene Überschrift je Thema, das wirklich besprochen wurde, benannt nach diesem Thema. Folge den echten Themen und ihrer Gewichtung, nie einer festen Liste; überspring, was nicht vorkam. Jeder Inhalt gehört unter eine dieser Überschriften, nie ein Textblock ohne Überschrift darüber. Darunter Stichpunkte, nie Fliesstext: dieser Teil wird überflogen, nicht gelesen. Ein Punkt je Stichpunkt, ein bis zwei Sätze; braucht ein Stichpunkt mehr, war er mehr als ein Punkt. Die Stichpunkte tragen die Substanz: Namen, Zahlen, Positionen, das konkrete Detail. „Budget besprochen“ ist wertlos, „Budget von 50.000 Euro genannt, Ole hält das für zu niedrig“ ist ein Stichpunkt. Tiefe kommt aus der ANZAHL der Stichpunkte, nie aus längeren: ein wichtiges Thema bekommt acht, eine Randnotiz einen."}, {"title": "Aufgaben", "description": "Als Aufzählung, ein Stichpunkt je Aufgabe in der Form: Aufgabe — Wer — Bis. Niemals eine Tabelle. Nimm eine Aufgabe nur auf, wo das Transkript zeigt, dass sie wirklich angenommen wurde. Lies vorher die Themen-Abschnitte oben noch einmal: steht dort, dass ein Vorschlag abgelehnt, abgetan oder skeptisch aufgenommen wurde, darf er hier nicht als Stichpunkt erscheinen. Dein eigener Text oben schlägt jeden Drang zur Vollständigkeit. Eine Aufgabe braucht auch jemanden, der sie übernommen hat: nenn einen Namen nur, wo das Transkript diese Person annehmen zeigt, sonst „[unklar]“. „[unklar]“ heisst, dass der Mensch hinter einer übernommenen Aufgabe unklar ist, nie „niemand hat sie übernommen“. Hat niemand sie übernommen, gibt es keinen Stichpunkt. Termine: rechne eine relative Frist („nächste Woche“) nur dann in ein Datum um, wenn dir das Datum des Gesprächs als Angabe vorliegt. Ein im Gespräch GENANNTES Datum ist keine solche Angabe. Liegt keins vor, lass die relative Formulierung wörtlich stehen und rechne nie aus deinem eigenen Gefühl für heute. Unbekannte Frist heisst „offen“. Wurden keine Aufgaben genannt, lass den Abschnitt ganz weg."}, {"title": "Offene Fragen", "description": "Was angesprochen, aber nicht geklärt wurde, dazu jede Stelle, an der das Transkript widersprüchlich oder unverständlich war. Benenn eine Unsicherheit ehrlich, statt sie zu glätten. War alles klar, lass den Abschnitt weg."}, {"title": "Zitate", "description": "Ganz am Schluss: bis zu drei WÖRTLICHE Aussagen, Wort für Wort, nie umformuliert oder geglättet, die einen Kernpunkt des Gesprächs treffen, jeweils mit dem Sprecher, aber nur bei eindeutiger Zuordnung. Ist die Formulierung im Transkript verstümmelt, lass das Zitat weg, statt es zu reparieren. Ein Zitat mit unklarem Sprecher fällt weg; gibt es keine, fällt der ganze Abschnitt weg."}]')
)
DELETE FROM templates
WHERE pinned = 0
  AND pin_order IS NULL
  AND EXISTS (
    SELECT 1
    FROM auslieferungsstand AS stand
    WHERE stand.id = templates.id
      AND stand.title = templates.title
      AND stand.description = templates.description
      AND stand.category IS templates.category
      AND stand.targets_json IS templates.targets_json
      AND stand.sections_json = templates.sections_json
  );
