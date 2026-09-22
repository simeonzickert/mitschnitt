-- Der Quellordner-Import zieht seit 2026-09-10 auch Tondateien mit. Die
-- Laufprotokoll-Tabelle zaehlte bisher nur Zeilen, nicht Dateien: ohne diese
-- beiden Spalten kann die Oberflaeche nicht sagen, wie viele Aufnahmen
-- angekommen sind. Additiv mit DEFAULT, damit aeltere Builds die Datenbank
-- weiter oeffnen koennen.
ALTER TABLE migration_import_runs ADD COLUMN audio_copied INTEGER NOT NULL DEFAULT 0;
ALTER TABLE migration_import_runs ADD COLUMN audio_bytes_copied INTEGER NOT NULL DEFAULT 0;
