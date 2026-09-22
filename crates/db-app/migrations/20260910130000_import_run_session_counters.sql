-- Der Laufbericht zaehlte bisher nur Zeilen ueber sechs Tabellen. Am gemessenen
-- Bestand sind das 2.355 Zeilen fuer 395 Gespraeche -- Faktor sechs. Die
-- Oberflaeche schrieb "conversations" darueber und widersprach damit dem Scan
-- direkt daneben, der 395 sagte. Beides ist wahr, aber nur eins heisst
-- Gespraech. Additiv mit DEFAULT, damit aeltere Builds die Datenbank weiter
-- oeffnen koennen.
ALTER TABLE migration_import_runs ADD COLUMN sessions_discovered INTEGER NOT NULL DEFAULT 0;
ALTER TABLE migration_import_runs ADD COLUMN sessions_imported INTEGER NOT NULL DEFAULT 0;
