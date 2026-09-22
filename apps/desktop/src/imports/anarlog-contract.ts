// Duenne Weiterleitung auf die erzeugten Bindungen.
//
// Bis zum Zusammenfuehren am 10.09.2026 stand hier eine handgeschriebene
// Vertragsdatei, weil Oberflaeche und Rust-Seite parallel gebaut wurden. Die
// Rust-Seite liefert die Formen jetzt selbst; diese Datei bleibt als EINE
// Stelle bestehen, an der die Oberflaeche ihre Kommandos bezieht, damit die
// sieben Aufrufer unveraendert bleiben.
//
// Die Aufrufform folgt dem Hausstil aus `plugins/db/js/index.ts`: duenner
// `invoke`-Wrapper, der im Fehlerfall wirft.

export {
  findImportSources,
  getImportRun,
  runSourceImport,
  scanImportSource,
} from "@anlg/plugin-db";

export type {
  ImportRunReport,
  ImportRunStatus,
  ImportSourceKind,
  ImportSourceScan,
} from "@anlg/plugin-db";
