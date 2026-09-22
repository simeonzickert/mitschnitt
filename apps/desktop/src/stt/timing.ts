export type TranscriptTimingSource =
  | "provider_word"
  | "provider_segment_interpolated"
  | "synthetic_text";

export type TranscriptWordMetadata = Record<string, unknown>;

export function createTranscriptTimingMetadata(
  source: TranscriptTimingSource,
  metadata?: unknown,
): TranscriptWordMetadata {
  const base = isRecord(metadata) ? metadata : {};
  const timing = isRecord(base.timing) ? base.timing : {};

  return {
    ...base,
    timing: {
      ...timing,
      source,
    },
  };
}

/**
 * Fork (03.09.2026): die GEMESSENE Sicherheit des Erkenners an die Metadaten
 * eines Wortes haengen.
 *
 * Nur wenn wirklich eine Zahl vorliegt. Ein Vorgabewert waere hier das
 * Schlimmste: eine erfundene 1,0 sieht in jeder spaeteren Auswertung aus wie
 * eine Messung. Fehlt die Angabe, fehlt das Feld.
 */
export function withWordConfidence(
  metadata: TranscriptWordMetadata,
  confidence: number | null | undefined,
): TranscriptWordMetadata {
  if (typeof confidence !== "number" || !Number.isFinite(confidence)) {
    return metadata;
  }

  return { ...metadata, confidence };
}

export function getWordConfidence(metadata: unknown): number | null {
  if (!isRecord(metadata)) {
    return null;
  }

  const confidence = metadata.confidence;
  return typeof confidence === "number" && Number.isFinite(confidence)
    ? confidence
    : null;
}

/**
 * Fork (03.09.2026): dieselbe vorsichtige Regel wie der Rust-Nachlauf in
 * `crates/listener2-core/src/vocabulary.rs` (`split_replacement`).
 *
 * Das Minimum der Gruppe -- eine Ersetzung macht ein Wort nicht sicherer, als
 * sein schwaechstes Teilstueck war. Und nur, wenn JEDES Wort der Gruppe eine
 * Messung hat: fehlt einer die Angabe, waere das Minimum der uebrigen eine
 * Aussage ueber eine Teilmenge, die sich als Aussage ueber das ganze ersetzte
 * Wort liest. Dann lieber nichts.
 */
export function mergeMeasuredConfidence(
  metadatas: readonly unknown[],
): number | null {
  if (metadatas.length === 0) {
    return null;
  }

  let minimum = Number.POSITIVE_INFINITY;
  for (const metadata of metadatas) {
    const confidence = getWordConfidence(metadata);
    if (confidence === null) {
      return null;
    }
    minimum = Math.min(minimum, confidence);
  }

  return minimum;
}

export function withoutWordConfidence(
  metadata: TranscriptWordMetadata,
): TranscriptWordMetadata {
  if (!("confidence" in metadata)) {
    return metadata;
  }

  const { confidence: _dropped, ...rest } = metadata;
  return rest;
}

export function getTranscriptTimingSource(word: {
  metadata?: unknown;
}): TranscriptTimingSource {
  const metadata = word.metadata;
  if (!isRecord(metadata)) {
    return "provider_word";
  }

  const timing = metadata.timing;
  if (!isRecord(timing)) {
    return getValidTimingSource(metadata.timing_source) ?? "provider_word";
  }

  return (
    getValidTimingSource(timing.source) ??
    getValidTimingSource(metadata.timing_source) ??
    "provider_word"
  );
}

export function isTranscriptWordSeekable(word: { metadata?: unknown }) {
  return getTranscriptTimingSource(word) !== "synthetic_text";
}

export function getValidTimingSource(
  source: unknown,
): TranscriptTimingSource | undefined {
  return source === "provider_word" ||
    source === "provider_segment_interpolated" ||
    source === "synthetic_text"
    ? source
    : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
