/**
 * Was ein Import an einem Gespraech hinterlaesst, und wie man es wieder ausliest.
 *
 * Es gibt ZWEI Formen in `sessions.metadata_json`, und beide sind gueltig:
 *
 *  - Der Datei-Import (`imports/queries.ts`) schreibt seit jeher flach
 *    `{ importedFrom, sourcePath, sourceUrl, externalId }`. Er kennt keinen
 *    Zeitpunkt: `sessions.created_at` traegt das Datum des GESPRAECHS, nicht das
 *    des Imports.
 *  - Der Ordner-Import schreibt verschachtelt unter `$.import`, mit Zeitpunkt.
 *
 * Wer nur die neue Form liest, laesst jedes bereits importierte Gespraech ohne
 * Herkunft dastehen. Beide werden deshalb auf dieselbe Form gebracht.
 */
export type SessionImportProvenance = {
  sourceApp: string | null;
  sourceRoot: string | null;
  importedAt: string | null;
};

/** Wie die Wortzeiten eines Transkripts zustande kamen. */
export type TranscriptTimingQuality = "synthetic" | "provider" | "mixed";

export function parseSessionImportProvenance(
  metadataJson: string | null | undefined,
): SessionImportProvenance | null {
  const metadata = parseObject(metadataJson);
  if (!metadata) return null;

  const nested = asObject(metadata.import);
  if (nested) {
    const sourceApp = asText(nested.source_app);
    const sourceRoot = asText(nested.source_root);
    const importedAt = asText(nested.imported_at);
    if (!sourceApp && !sourceRoot && !importedAt) return null;
    return { sourceApp, sourceRoot, importedAt };
  }

  const importedFrom = asText(metadata.importedFrom);
  if (!importedFrom) return null;
  return {
    sourceApp: importedFrom,
    sourceRoot: asText(metadata.sourcePath),
    importedAt: null,
  };
}

/**
 * `synthetic` heisst: die Zeiten sind geschaetzt, ein Klick aufs Wort trifft den
 * Ton nicht. Alles, was nicht einer der drei vereinbarten Werte ist, wird
 * verworfen -- eine erfundene Zeitqualitaet ist schlimmer als keine, weil sie
 * Vertrauen erzeugt, das der Ton nicht deckt.
 */
export function parseTranscriptTimingQuality(
  metadataJson: string | null | undefined,
): TranscriptTimingQuality | null {
  const metadata = parseObject(metadataJson);
  if (!metadata) return null;
  const value = asText(metadata.timing_quality);
  return value === "synthetic" || value === "provider" || value === "mixed"
    ? value
    : null;
}

/**
 * Ein Gespraech kann mehrere Transkripte tragen (Kanaele, Nachlaeufe). Fuer die
 * Warnung zaehlt der SCHLECHTESTE Wert: steht auch nur an einer Stelle
 * "geschaetzt", darf der Mensch den Spruengen im Abspieler nicht trauen.
 * Unbekannte Werte werden ignoriert und machen aus "geschaetzt" nie "gemessen".
 */
export function worstTimingQuality(
  values: Array<TranscriptTimingQuality | null>,
): TranscriptTimingQuality | null {
  const known = values.filter(
    (value): value is TranscriptTimingQuality => value !== null,
  );
  if (known.length === 0) return null;
  if (known.includes("synthetic")) {
    return known.every((value) => value === "synthetic")
      ? "synthetic"
      : "mixed";
  }
  return known.includes("mixed") ? "mixed" : "provider";
}

function parseObject(value: string | null | undefined) {
  if (!value) return null;
  try {
    return asObject(JSON.parse(value));
  } catch {
    return null;
  }
}

function asObject(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asText(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value : null;
}
