import {
  parseSessionImportProvenance,
  parseTranscriptTimingQuality,
  worstTimingQuality,
  type SessionImportProvenance,
  type TranscriptTimingQuality,
} from "./provenance";

import { useLiveQuery } from "~/db";

/**
 * Eigene, schmale Abfragen statt einer Erweiterung von `SESSION_SELECT_SQL` und
 * `SessionRecord`: die Herkunft ist eine Randnotiz und darf nicht die Typen und
 * Schreibpfade des Kern-Datensatzes mitziehen.
 */
export function useSessionImportProvenance(
  sessionId: string,
): SessionImportProvenance | null {
  const { data } = useLiveQuery<
    { metadata_json: string | null },
    SessionImportProvenance | null
  >({
    sql: `
      SELECT metadata_json
      FROM sessions
      WHERE id = ? AND deleted_at IS NULL
      LIMIT 1
    `,
    params: [sessionId],
    enabled: Boolean(sessionId),
    mapRows: (rows) => parseSessionImportProvenance(rows[0]?.metadata_json),
  });

  return data ?? null;
}

export function useSessionTimingQuality(
  sessionId: string,
): TranscriptTimingQuality | null {
  const { data } = useLiveQuery<
    { metadata_json: string | null },
    TranscriptTimingQuality | null
  >({
    sql: `
      SELECT metadata_json
      FROM transcripts
      WHERE session_id = ? AND deleted_at IS NULL
    `,
    params: [sessionId],
    enabled: Boolean(sessionId),
    mapRows: (rows) =>
      worstTimingQuality(
        rows.map((row) => parseTranscriptTimingQuality(row.metadata_json)),
      ),
  });

  return data ?? null;
}
