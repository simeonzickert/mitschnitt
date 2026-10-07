import type {
  LiveTranscriptDelta,
  RenderTranscriptHuman,
} from "@anlg/plugin-transcription";
import { commands as transcriptionCommands } from "@anlg/plugin-transcription";

import { executeTransaction, liveQueryClient, useLiveQuery } from "~/db";
import { enqueueDatabaseWrite } from "~/db/write-queue";
import type { SegmentKey } from "~/stt/live-segment";
import { SESSION_PARTICIPANT_HUMAN_IDS_SQL } from "~/stt/session-participant-sql";
import {
  buildRenderTranscriptRequestFromRows,
  resolveScopedWordHumanIds,
} from "~/stt/render-transcript";
import { coalesceLiveTranscriptDeltas } from "~/stt/transcript-persistence-worker";
import type { SpeakerHintWithId, WordWithId } from "~/stt/types";
import {
  applyLiveTranscriptDelta,
  createTranscriptAccumulator,
  findSpeakerAssignmentAnchorWordId,
  parseTranscriptWords,
  parseTranscriptHints,
  updateTranscriptHints,
  updateTranscriptWords,
  mergeTranscriptSegmentAssignments,
  upsertSpeakerAssignment,
} from "~/stt/utils";

type TranscriptSqlRow = {
  id: string;
  owner_user_id: string;
  session_id: string;
  started_at_ms: number;
  ended_at_ms: number | null;
  words_json: string;
  speaker_hints_json: string;
  content_revision: number;
  pending_deltas_json: string;
};

type TranscriptMetadataSqlRow = {
  id: string;
  session_id: string;
  started_at_ms: number;
  ended_at_ms: number | null;
  has_words: number;
};

type ParticipantHumanSqlRow = { human_id: string };
type HumanSqlRow = { id: string; name: string };
type TranscriptMutationSqlRow = {
  words_json: string;
  speaker_hints_json: string;
  content_revision: number;
  pending_deltas_json: string;
};

type TranscriptInsert = {
  id: string;
  sessionId: string;
  ownerUserId: string;
  createdAt: string;
  startedAt: number;
  endedAt?: number;
  memo?: string;
  source?: string;
  provider?: string;
  model?: string;
  language?: string;
  words?: WordWithId[];
  speakerHints?: SpeakerHintWithId[];
  replaceSession?: boolean;
  replaceTranscriptId?: string;
};

export type TranscriptRecord = {
  id: string;
  ownerUserId: string;
  sessionId: string;
  startedAt: number;
  endedAt?: number;
  words: WordWithId[];
  speakerHints: SpeakerHintWithId[];
};

export type TranscriptMetadata = {
  id: string;
  sessionId: string;
  startedAt: number;
  endedAt?: number;
  hasWords: boolean;
};

const EMPTY_TRANSCRIPTS: TranscriptRecord[] = [];
const EMPTY_TRANSCRIPT_METADATA: TranscriptMetadata[] = [];
const EMPTY_IDS: string[] = [];
const EMPTY_HUMANS: RenderTranscriptHuman[] = [];

const TRANSCRIPT_COLUMNS = `
  transcript.id,
  transcript.owner_user_id,
  transcript.session_id,
  transcript.started_at_ms,
  transcript.ended_at_ms,
  transcript.words_json,
  transcript.speaker_hints_json,
  transcript.content_revision,
  COALESCE((
    SELECT json_group_array(json(ordered_delta.delta_json))
    FROM (
      SELECT delta.delta_json
      FROM transcript_live_deltas AS delta
      WHERE delta.transcript_id = transcript.id
      ORDER BY delta.sequence
    ) AS ordered_delta
  ), '[]') AS pending_deltas_json
`;

const TRANSCRIPT_BASE_COLUMNS = `
  transcript.id,
  transcript.owner_user_id,
  transcript.session_id,
  transcript.started_at_ms,
  transcript.ended_at_ms,
  transcript.words_json,
  transcript.speaker_hints_json,
  transcript.content_revision,
  '[]' AS pending_deltas_json
`;

const TRANSCRIPT_METADATA_COLUMNS = `
  transcript.id,
  transcript.session_id,
  transcript.started_at_ms,
  transcript.ended_at_ms,
  CASE
    WHEN trim(transcript.words_json) NOT IN ('', '[]', '{}', 'null')
    THEN 1
    ELSE 0
  END AS has_words
`;

const TRANSCRIPT_METADATA_WITH_PENDING_COLUMNS = `
  transcript.id,
  transcript.session_id,
  transcript.started_at_ms,
  transcript.ended_at_ms,
  CASE
    WHEN trim(transcript.words_json) NOT IN ('', '[]', '{}', 'null')
    OR EXISTS (
      SELECT 1
      FROM transcript_live_deltas AS delta
      WHERE delta.transcript_id = transcript.id
        AND json_valid(delta.delta_json)
        AND COALESCE(
          json_array_length(delta.delta_json, '$.new_words'),
          0
        ) > 0
    ) THEN 1
    ELSE 0
  END AS has_words
`;

export function useSessionTranscripts(sessionId: string): TranscriptRecord[] {
  const { data = EMPTY_TRANSCRIPTS } = useLiveQuery<
    TranscriptSqlRow,
    TranscriptRecord[]
  >({
    sql: `
      SELECT ${TRANSCRIPT_COLUMNS}
      FROM transcripts AS transcript
      WHERE transcript.session_id = ? AND transcript.deleted_at IS NULL
      ORDER BY transcript.started_at_ms, transcript.created_at, transcript.id
    `,
    params: [sessionId],
    enabled: Boolean(sessionId),
    mapRows: (rows) => rows.map(mapTranscriptRow),
  });
  return sessionId ? data : EMPTY_TRANSCRIPTS;
}

export function useTranscript(
  transcriptId: string,
  includePendingDeltas = true,
): TranscriptRecord | null {
  const { data = null } = useLiveQuery<
    TranscriptSqlRow,
    TranscriptRecord | null
  >({
    sql: `
      SELECT ${includePendingDeltas ? TRANSCRIPT_COLUMNS : TRANSCRIPT_BASE_COLUMNS}
      FROM transcripts AS transcript
      WHERE transcript.id = ? AND transcript.deleted_at IS NULL
      LIMIT 1
    `,
    params: [transcriptId],
    enabled: Boolean(transcriptId),
    mapRows: (rows) => (rows[0] ? mapTranscriptRow(rows[0]) : null),
  });
  return transcriptId ? data : null;
}

export function useSessionTranscriptMetadata(
  sessionId: string,
  includePendingDeltas = true,
): TranscriptMetadata[] {
  const { data = EMPTY_TRANSCRIPT_METADATA } = useLiveQuery<
    TranscriptMetadataSqlRow,
    TranscriptMetadata[]
  >({
    sql: `
      SELECT ${
        includePendingDeltas
          ? TRANSCRIPT_METADATA_WITH_PENDING_COLUMNS
          : TRANSCRIPT_METADATA_COLUMNS
      }
      FROM transcripts AS transcript
      WHERE transcript.session_id = ? AND transcript.deleted_at IS NULL
      ORDER BY transcript.started_at_ms, transcript.created_at, transcript.id
    `,
    params: [sessionId],
    enabled: Boolean(sessionId),
    mapRows: (rows) => rows.map(mapTranscriptMetadataRow),
  });
  return sessionId ? data : EMPTY_TRANSCRIPT_METADATA;
}

export function useTranscriptMetadata(
  transcriptId: string,
  includePendingDeltas = true,
): TranscriptMetadata | null {
  const { data = null } = useLiveQuery<
    TranscriptMetadataSqlRow,
    TranscriptMetadata | null
  >({
    sql: `
      SELECT ${
        includePendingDeltas
          ? TRANSCRIPT_METADATA_WITH_PENDING_COLUMNS
          : TRANSCRIPT_METADATA_COLUMNS
      }
      FROM transcripts AS transcript
      WHERE transcript.id = ? AND transcript.deleted_at IS NULL
      LIMIT 1
    `,
    params: [transcriptId],
    enabled: Boolean(transcriptId),
    mapRows: (rows) => (rows[0] ? mapTranscriptMetadataRow(rows[0]) : null),
  });
  return transcriptId ? data : null;
}

export async function getTranscriptRecord(
  transcriptId: string,
): Promise<TranscriptRecord | null> {
  if (!transcriptId) {
    return null;
  }

  const rows = await liveQueryClient.execute<TranscriptSqlRow>(
    `
      SELECT ${TRANSCRIPT_COLUMNS}
      FROM transcripts AS transcript
      WHERE transcript.id = ? AND transcript.deleted_at IS NULL
      LIMIT 1
    `,
    [transcriptId],
  );

  return rows[0] ? mapTranscriptRow(rows[0]) : null;
}

export function useSessionParticipantHumanIds(sessionId: string): string[] {
  const { data = EMPTY_IDS } = useLiveQuery<ParticipantHumanSqlRow, string[]>({
    sql: SESSION_PARTICIPANT_HUMAN_IDS_SQL,
    params: [sessionId],
    enabled: Boolean(sessionId),
    mapRows: (rows) => rows.map((row) => row.human_id),
  });
  return sessionId ? data : EMPTY_IDS;
}

export function useTranscriptHumans(
  humanIds: readonly string[],
): RenderTranscriptHuman[] {
  const uniqueIds = [...new Set(humanIds.filter(Boolean))].sort();
  const placeholders = uniqueIds.map(() => "?").join(", ");
  const { data = EMPTY_HUMANS } = useLiveQuery<
    HumanSqlRow,
    RenderTranscriptHuman[]
  >({
    sql: `
      SELECT id, name
      FROM humans
      WHERE id IN (${placeholders || "NULL"})
        AND name <> ''
        AND deleted_at IS NULL
      ORDER BY id
    `,
    params: uniqueIds,
    enabled: uniqueIds.length > 0,
    mapRows: (rows) =>
      rows.map((row) => ({ human_id: row.id, name: row.name })),
  });
  return uniqueIds.length > 0 ? data : EMPTY_HUMANS;
}

export function createTranscript(input: TranscriptInsert): Promise<void> {
  return enqueueDatabaseWrite(`transcript:${input.id}`, async () => {
    const now = new Date().toISOString();
    const statements: Array<{ sql: string; params: unknown[] }> = [];

    if (input.replaceSession) {
      statements.push({
        sql: `
          UPDATE transcripts
          SET deleted_at = ?, updated_at = ?
          WHERE session_id = ? AND deleted_at IS NULL
        `,
        params: [now, now, input.sessionId],
      });
    } else if (input.replaceTranscriptId) {
      statements.push({
        sql: `
          UPDATE transcripts
          SET deleted_at = ?, updated_at = ?
          WHERE id = ? AND session_id = ? AND deleted_at IS NULL
        `,
        params: [now, now, input.replaceTranscriptId, input.sessionId],
      });
    }

    statements.push({
      sql: `
        INSERT INTO transcripts (
          id, workspace_id, owner_user_id, session_id, source, provider,
          model, language, started_at_ms, ended_at_ms, audio_attachment_id,
          memo, words_json, speaker_hints_json, metadata_json, created_at,
          updated_at, deleted_at
        )
        SELECT ?, session.workspace_id,
          COALESCE(NULLIF(?, ''), session.owner_user_id),
          session.id, ?, ?, ?, ?, ?, ?, '',
          ?, ?, ?, '{}', ?, ?, NULL
        FROM sessions AS session
        WHERE session.id = ? AND session.deleted_at IS NULL
      `,
      params: [
        input.id,
        input.ownerUserId,
        input.source ?? "",
        input.provider ?? "",
        input.model ?? "",
        input.language ?? "",
        input.startedAt,
        input.endedAt ?? null,
        input.memo ?? "",
        JSON.stringify(input.words ?? []),
        JSON.stringify(input.speakerHints ?? []),
        input.createdAt,
        now,
        input.sessionId,
      ],
    });

    await executeTransaction(statements);
  });
}

export function createLiveTranscript(
  input: Omit<TranscriptInsert, "words" | "speakerHints">,
  delta: LiveTranscriptDelta,
): Promise<void> {
  const snapshot = mutateTranscriptSnapshot("[]", "[]", input.id, (store) =>
    applyLiveTranscriptDelta(store, input.id, delta),
  );

  return createTranscript({
    ...input,
    words: JSON.parse(snapshot.wordsJson) as WordWithId[],
    speakerHints: JSON.parse(snapshot.hintsJson) as SpeakerHintWithId[],
  });
}

export async function transcriptExists(transcriptId: string): Promise<boolean> {
  const rows = await liveQueryClient.execute<{ id: string }>(
    `
      SELECT id
      FROM transcripts
      WHERE id = ? AND deleted_at IS NULL
      LIMIT 1
    `,
    [transcriptId],
  );
  return Boolean(rows[0]);
}

export function applyLiveTranscriptDeltaToDatabase(
  transcriptId: string,
  delta: LiveTranscriptDelta,
): Promise<void> {
  return enqueueDatabaseWrite(`transcript:${transcriptId}`, async () => {
    const now = new Date().toISOString();
    const journalId = `${transcriptId}:${crypto.randomUUID()}`;
    const [, inserted = 0] = await executeTransaction([
      {
        sql: `
          INSERT OR IGNORE INTO transcript_live_state (
            transcript_id, next_sequence, updated_at
          )
          SELECT id, 0, ?
          FROM transcripts
          WHERE id = ? AND deleted_at IS NULL
        `,
        params: [now, transcriptId],
      },
      {
        sql: `
          INSERT INTO transcript_live_deltas (
            id, transcript_id, sequence, delta_json, created_at
          )
          SELECT ?, transcript_id, next_sequence, ?, ?
          FROM transcript_live_state
          WHERE transcript_id = ?
        `,
        params: [journalId, JSON.stringify(delta), now, transcriptId],
      },
      {
        sql: `
          UPDATE transcript_live_state
          SET next_sequence = next_sequence + 1, updated_at = ?
          WHERE transcript_id = ?
        `,
        params: [now, transcriptId],
      },
    ]);

    if (inserted !== 1) {
      throw new Error(`Transcript ${transcriptId} does not exist`);
    }
  });
}

export function flushLiveTranscriptDeltasToDatabase(
  transcriptId: string,
): Promise<void> {
  return mutateTranscript(transcriptId);
}

export function appendTranscriptWordsAndHints(
  transcriptId: string,
  words: WordWithId[],
  hints: SpeakerHintWithId[],
  options?: { mode?: "append" | "replace" },
): Promise<void> {
  return mutateTranscript(transcriptId, (store) => {
    const accumulator = createTranscriptAccumulator(store, transcriptId);
    accumulator.appendWordsAndHints(words, hints, options);
    accumulator.dispose();
  });
}

export function mergeTranscriptSegments({
  transcriptId,
  segmentKey,
  wordIds,
}: {
  transcriptId: string;
  segmentKey: SegmentKey;
  wordIds: string[];
}): Promise<void> {
  return mutateTranscript(transcriptId, (store) => {
    mergeTranscriptSegmentAssignments(store, transcriptId, segmentKey, wordIds);
  });
}

export function assignTranscriptSpeaker({
  transcriptId,
  segmentKey,
  humanId,
  anchorWordId,
  mode,
  wordIds,
  extendToAdjacent,
  assertEditable,
}: {
  transcriptId: string;
  segmentKey: SegmentKey;
  humanId: string;
  anchorWordId: string;
  mode?: "all" | "segment";
  wordIds?: string[];
  extendToAdjacent?: boolean;
  assertEditable?: () => void;
}): Promise<void> {
  return assignSpeakerInTranscript({
    transcriptId,
    segmentKey,
    humanId,
    anchorWordId,
    mode,
    wordIds,
    extendToAdjacent,
    assertEditable,
  });
}

export async function assignSessionTranscriptSpeaker({
  sessionId,
  transcriptId,
  segmentKey,
  humanId,
  anchorWordId,
  wordIds,
  assertEditable,
}: {
  sessionId: string;
  transcriptId: string;
  segmentKey: SegmentKey;
  humanId: string;
  anchorWordId: string;
  wordIds?: string[];
  assertEditable?: () => void;
}): Promise<void> {
  const transcripts = await liveQueryClient.execute<{ id: string }>(
    `
      SELECT id
      FROM transcripts
      WHERE session_id = ? AND deleted_at IS NULL
      ORDER BY started_at_ms, created_at, id
    `,
    [sessionId],
  );
  // The query above took time: the session may have locked meanwhile.
  assertEditable?.();
  if (!transcripts.some((transcript) => transcript.id === transcriptId)) {
    throw new Error(
      `Transcript ${transcriptId} is no longer in session ${sessionId}`,
    );
  }

  await Promise.all(
    transcripts
      .filter(
        (transcript) =>
          transcript.id === transcriptId || segmentKey.speaker_human_id,
      )
      .map((transcript) =>
        transcript.id === transcriptId
          ? assignSpeakerInTranscript({
              transcriptId,
              segmentKey,
              humanId,
              anchorWordId,
              wordIds,
              mode: "all",
              assertEditable,
            })
          : mutateTranscript(
              transcript.id,
              (store) => {
              const input = buildRenderTranscriptRequestFromRows([
                {
                  words: parseTranscriptWords(store, transcript.id),
                  speaker_hints: parseTranscriptHints(store, transcript.id),
                },
              ])?.transcripts[0];
              if (!input) return false;
              const matchingWordIds = [...resolveScopedWordHumanIds(input)]
                .filter(
                  ([, assignedHumanId]) =>
                    assignedHumanId === segmentKey.speaker_human_id,
                )
                .map(([wordId]) => wordId);
              const anchor = matchingWordIds[0];
              if (!anchor) return false;
              upsertSpeakerAssignment(
                store,
                transcript.id,
                segmentKey,
                humanId,
                anchor,
                {
                  mode: "segment",
                  wordIds: matchingWordIds,
                  extendToAdjacent: false,
                },
              );
              },
              assertEditable,
            ),
      ),
  );
}

async function assignSpeakerInTranscript({
  transcriptId,
  segmentKey,
  humanId,
  anchorWordId,
  mode,
  wordIds,
  extendToAdjacent,
  assertEditable,
}: {
  transcriptId: string;
  segmentKey: SegmentKey;
  humanId: string;
  anchorWordId?: string;
  mode?: "all" | "segment";
  wordIds?: string[];
  extendToAdjacent?: boolean;
  assertEditable?: () => void;
}): Promise<void> {
  let assigned = false;
  await mutateTranscript(transcriptId, (store) => {
    const resolvedAnchorWordId =
      anchorWordId ??
      findSpeakerAssignmentAnchorWordId(
        parseTranscriptWords(store, transcriptId),
        parseTranscriptHints(store, transcriptId),
        segmentKey,
      );
    if (!resolvedAnchorWordId) {
      return false;
    }

    upsertSpeakerAssignment(
      store,
      transcriptId,
      segmentKey,
      humanId,
      resolvedAnchorWordId,
      { mode, wordIds, extendToAdjacent },
    );
    assigned = true;
    return true;
  }, assertEditable);

  if (
    !assigned ||
    (mode ?? "all") !== "all" ||
    !Number.isInteger(segmentKey.speaker_index)
  ) {
    return;
  }

  const channel =
    segmentKey.channel === "DirectMic"
      ? 0
      : segmentKey.channel === "RemoteParty"
        ? 1
        : 2;
  void transcriptionCommands
    .promoteVoiceprintCandidates(
      transcriptId,
      channel,
      typeof segmentKey.speaker_index === "number"
        ? segmentKey.speaker_index
        : null,
      humanId,
    )
    .then((result) => {
      if (result.status === "error") {
        console.error("[voiceprint] promotion failed", result.error);
      }
    })
    .catch((error) => {
      console.error("[voiceprint] promotion failed", error);
    });
}

/**
 * Rewrites the text of the given words. Resolves `false` when none of the
 * word ids exists any more (the transcript was replaced under the caller),
 * so the caller can tell "saved" from "silently nothing happened".
 */
export async function updateTranscriptSegmentText({
  transcriptId,
  wordIds,
  text,
  assertEditable,
}: {
  transcriptId: string;
  wordIds: string[];
  text: string;
  assertEditable?: () => void;
}): Promise<boolean> {
  let matched = false;
  await mutateTranscript(transcriptId, (store) => {
    matched = false;
    const selectedWordIds = new Set(wordIds);
    const words = parseTranscriptWords(store, transcriptId);
    const selectedWords = words.filter((word) => selectedWordIds.has(word.id));
    if (selectedWords.length === 0) {
      return;
    }
    matched = true;

    const tokens = text.match(/\S+/g) ?? [];
    const textByWordId = new Map<string, string>();
    for (const [index, word] of selectedWords.entries()) {
      const isLastWord = index === selectedWords.length - 1;
      textByWordId.set(
        word.id,
        isLastWord ? tokens.slice(index).join(" ") : (tokens[index] ?? ""),
      );
    }

    updateTranscriptWords(
      store,
      transcriptId,
      words.map((word) => {
        const nextText = textByWordId.get(word.id);
        return nextText === undefined || nextText === word.text
          ? word
          : { ...word, text: nextText };
      }),
    );
  }, assertEditable);
  return matched;
}

/**
 * Blanks the given words (block delete) and resolves the texts they had, so
 * the delete can be undone with `restoreTranscriptWordTexts`.
 */
export async function clearTranscriptWordTexts({
  transcriptId,
  wordIds,
}: {
  transcriptId: string;
  wordIds: string[];
}): Promise<Record<string, string>> {
  let previous: Record<string, string> = {};
  await mutateTranscript(transcriptId, (store) => {
    previous = {};
    const selected = new Set(wordIds);
    const words = parseTranscriptWords(store, transcriptId);
    if (!words.some((word) => selected.has(word.id))) {
      return;
    }
    for (const word of words) {
      if (selected.has(word.id)) previous[word.id] = word.text ?? "";
    }
    updateTranscriptWords(
      store,
      transcriptId,
      words.map((word) =>
        selected.has(word.id) ? { ...word, text: "" } : word,
      ),
    );
  });
  return previous;
}

/**
 * Writes back the word texts captured by `clearTranscriptWordTexts`, but only
 * into words that are STILL blank: a word the user has typed into since the
 * delete is not overwritten. Resolves how many words were restored.
 */
export async function restoreTranscriptWordTexts({
  transcriptId,
  texts,
}: {
  transcriptId: string;
  texts: Record<string, string>;
}): Promise<number> {
  let restored = 0;
  await mutateTranscript(transcriptId, (store) => {
    restored = 0;
    const words = parseTranscriptWords(store, transcriptId);
    const next = words.map((word) => {
      const previous = texts[word.id];
      if (previous === undefined || (word.text ?? "") !== "") return word;
      restored += 1;
      return { ...word, text: previous };
    });
    if (restored === 0) {
      return;
    }
    updateTranscriptWords(store, transcriptId, next);
  });
  return restored;
}

export async function splitTranscriptSpeaker({
  transcriptId,
  segmentKey,
  wordIds,
  text,
  offset,
  humanId,
  assertEditable,
}: {
  transcriptId: string;
  segmentKey: SegmentKey;
  wordIds: string[];
  text: string;
  offset: number;
  humanId: string;
  assertEditable?: () => void;
}): Promise<boolean> {
  const splitWordId = crypto.randomUUID();
  let matched = false;
  await mutateTranscript(transcriptId, (store) => {
    matched = false;
    const words = parseTranscriptWords(store, transcriptId);
    const selectedIds = new Set(wordIds);
    const selected = words.filter((word) => selectedIds.has(word.id));
    const tokens = [...text.matchAll(/\S+/g)];
    if (!selected.length || !text.slice(offset).trim()) return false;

    const lastToken = tokens[tokens.length - 1];
    const textEnd = lastToken
      ? lastToken.index + lastToken[0].length
      : text.length;
    const replacements = new Map<string, WordWithId[]>();
    const followingIds: string[] = [];
    for (const [index, word] of selected.entries()) {
      const start = tokens[index]?.index ?? text.length;
      const end =
        index === selected.length - 1
          ? textEnd
          : start + (tokens[index]?.[0].length ?? 0);
      const nextText = text.slice(start, end).trim();
      if (offset > start && offset < end) {
        const before = text.slice(start, offset).trim();
        const after = text.slice(offset, end).trim();
        if (before && after) {
          const boundary =
            word.start_ms !== undefined && word.end_ms !== undefined
              ? word.start_ms +
                (word.end_ms - word.start_ms) *
                  ((offset - start) / (end - start))
              : undefined;
          replacements.set(word.id, [
            {
              ...word,
              text: before,
              ...(boundary === undefined ? {} : { end_ms: boundary }),
            },
            {
              ...word,
              id: splitWordId,
              text: after,
              ...(boundary === undefined ? {} : { start_ms: boundary }),
            },
          ]);
          followingIds.push(splitWordId);
          continue;
        }
      }
      replacements.set(word.id, [{ ...word, text: nextText }]);
      if (nextText && start >= offset) followingIds.push(word.id);
    }
    if (!followingIds.length) return false;
    updateTranscriptWords(
      store,
      transcriptId,
      words.flatMap((word) => replacements.get(word.id) ?? [word]),
    );
    upsertSpeakerAssignment(
      store,
      transcriptId,
      segmentKey,
      humanId,
      followingIds[0],
      {
        mode: "segment",
        wordIds: followingIds,
        extendToAdjacent: false,
      },
    );
    matched = true;
  }, assertEditable);
  return matched;
}

export function softDeleteTranscript(transcriptId: string): Promise<void> {
  return enqueueDatabaseWrite(`transcript:${transcriptId}`, async () => {
    const now = new Date().toISOString();
    await executeTransaction([
      {
        sql: `
          UPDATE transcripts
          SET deleted_at = ?, updated_at = ?
          WHERE id = ? AND deleted_at IS NULL
        `,
        params: [now, now, transcriptId],
      },
    ]);
  });
}

export async function removeHumanSpeakerAssignments(
  sessionId: string,
  humanId: string,
): Promise<void> {
  const transcripts = await liveQueryClient.execute<{ id: string }>(
    `
      SELECT id
      FROM transcripts
      WHERE session_id = ? AND deleted_at IS NULL
      ORDER BY started_at_ms, created_at, id
    `,
    [sessionId],
  );

  await Promise.all(
    transcripts.map((transcript) =>
      mutateTranscript(transcript.id, (store) => {
        const hints = parseTranscriptHints(store, transcript.id);
        const filtered = hints.filter(
          (hint) =>
            (hint.type !== "automatic_speaker_assignment" &&
              hint.type !== "user_speaker_assignment") ||
            parseAssignedHumanId(hint.value) !== humanId,
        );
        if (filtered.length !== hints.length) {
          updateTranscriptHints(store, transcript.id, filtered);
        }
      }),
    ),
  );
}

function mapTranscriptRow(row: TranscriptSqlRow): TranscriptRecord {
  const snapshot = materializeTranscriptSnapshot(
    row.words_json,
    row.speaker_hints_json,
    row.id,
    row.pending_deltas_json,
  );
  return {
    id: row.id,
    ownerUserId: row.owner_user_id,
    sessionId: row.session_id,
    startedAt: Number(row.started_at_ms),
    endedAt: row.ended_at_ms === null ? undefined : Number(row.ended_at_ms),
    words: parseJsonArray(snapshot.wordsJson, row.id, "words"),
    speakerHints: parseJsonArray(snapshot.hintsJson, row.id, "speaker hints"),
  };
}

function mapTranscriptMetadataRow(
  row: TranscriptMetadataSqlRow,
): TranscriptMetadata {
  return {
    id: row.id,
    sessionId: row.session_id,
    startedAt: Number(row.started_at_ms),
    endedAt: row.ended_at_ms === null ? undefined : Number(row.ended_at_ms),
    hasWords: row.has_words === 1,
  };
}

function parseJsonArray<T>(value: string, rowId: string, field: string): T[] {
  try {
    const parsed = JSON.parse(value);
    if (Array.isArray(parsed)) return parsed as T[];
  } catch (error) {
    console.error(`[transcript] failed to parse ${field} for ${rowId}`, error);
  }

  return [];
}

async function mutateTranscript(
  transcriptId: string,
  mutation?: (store: MemoryTranscriptStore) => boolean | void,
  // User-triggered writes pass the guard's check: it runs again right before
  // each attempt reads and writes, after any time spent in the write queue.
  assertEditable?: () => void,
): Promise<void> {
  return enqueueDatabaseWrite(`transcript:${transcriptId}`, async () => {
    for (let attempt = 0; attempt < 5; attempt += 1) {
      assertEditable?.();
      const rows = await liveQueryClient.execute<TranscriptMutationSqlRow>(
        `
          SELECT
            transcript.words_json,
            transcript.speaker_hints_json,
            transcript.content_revision,
            COALESCE((
              SELECT json_group_array(json(ordered_delta.delta_json))
              FROM (
                SELECT delta.delta_json
                FROM transcript_live_deltas AS delta
                WHERE delta.transcript_id = transcript.id
                ORDER BY delta.sequence
              ) AS ordered_delta
            ), '[]') AS pending_deltas_json
          FROM transcripts AS transcript
          WHERE transcript.id = ? AND transcript.deleted_at IS NULL
          LIMIT 1
        `,
        [transcriptId],
      );
      const current = rows[0];
      if (!current) {
        if (!mutation) return;
        throw new Error(`Transcript ${transcriptId} does not exist`);
      }

      assertJsonArray(current.words_json, transcriptId, "words");
      assertJsonArray(
        current.speaker_hints_json,
        transcriptId,
        "speaker hints",
      );
      const pendingDeltas = parseLiveTranscriptDeltas(
        current.pending_deltas_json,
        transcriptId,
      );
      if (!mutation && pendingDeltas.length === 0) return;

      const materialized = materializeTranscriptSnapshot(
        current.words_json,
        current.speaker_hints_json,
        transcriptId,
        current.pending_deltas_json,
      );
      let shouldPersist = true;
      const next = mutation
        ? mutateTranscriptSnapshot(
            materialized.wordsJson,
            materialized.hintsJson,
            transcriptId,
            (store) => {
              shouldPersist = mutation(store) !== false;
            },
          )
        : materialized;
      if (!shouldPersist) return;
      // The SELECT above may have waited: check the lock once more, right
      // before the write.
      assertEditable?.();
      const now = new Date().toISOString();
      const [updated = 0] = await executeTransaction([
        {
          sql: `
            UPDATE transcripts
            SET words_json = ?,
              speaker_hints_json = ?,
              content_revision = content_revision + 1,
              updated_at = ?
            WHERE id = ?
              AND content_revision = ?
              AND deleted_at IS NULL
          `,
          params: [
            next.wordsJson,
            next.hintsJson,
            now,
            transcriptId,
            Number(current.content_revision ?? 0),
          ],
        },
        {
          sql: `
            DELETE FROM transcript_live_state
            WHERE transcript_id = ? AND changes() = 1
          `,
          params: [transcriptId],
        },
      ]);

      if (updated === 1) return;
    }

    throw new Error(`Transcript ${transcriptId} changed too frequently`);
  });
}

function materializeTranscriptSnapshot(
  wordsJson: string,
  hintsJson: string,
  transcriptId: string,
  pendingDeltasJson: string,
) {
  const deltas = parseLiveTranscriptDeltas(pendingDeltasJson, transcriptId);
  if (deltas.length === 0) return { wordsJson, hintsJson };

  return mutateTranscriptSnapshot(wordsJson, hintsJson, transcriptId, (store) =>
    applyLiveTranscriptDelta(
      store,
      transcriptId,
      coalesceLiveTranscriptDeltas(deltas),
    ),
  );
}

function parseLiveTranscriptDeltas(
  value: string | undefined,
  transcriptId: string,
): LiveTranscriptDelta[] {
  if (!value) return [];
  try {
    const parsed = JSON.parse(value);
    if (Array.isArray(parsed)) return parsed as LiveTranscriptDelta[];
  } catch (error) {
    console.error(
      `[transcript] failed to parse live deltas for ${transcriptId}`,
      error,
    );
  }
  return [];
}

type MemoryTranscriptStore = {
  getCell: (
    tableId: "transcripts",
    rowId: string,
    cellId: "words" | "speaker_hints",
  ) => string;
  setCell: (
    tableId: "transcripts",
    rowId: string,
    cellId: "words" | "speaker_hints",
    value: string,
  ) => void;
};

function mutateTranscriptSnapshot(
  wordsJson: string,
  hintsJson: string,
  transcriptId: string,
  mutation: (store: MemoryTranscriptStore) => void,
) {
  const snapshot = { wordsJson, hintsJson };
  const store: MemoryTranscriptStore = {
    getCell: (_tableId, rowId, cellId) => {
      if (rowId !== transcriptId) return "[]";
      return cellId === "words" ? snapshot.wordsJson : snapshot.hintsJson;
    },
    setCell: (_tableId, rowId, cellId, value) => {
      if (rowId !== transcriptId) return;
      if (cellId === "words") {
        snapshot.wordsJson = value;
      } else {
        snapshot.hintsJson = value;
      }
    },
  };

  mutation(store);
  return snapshot;
}

function assertJsonArray(value: string, rowId: string, field: string): void {
  try {
    if (Array.isArray(JSON.parse(value))) return;
  } catch {
    // Report the same corruption error for malformed and non-array payloads.
  }

  throw new Error(`Transcript ${rowId} has invalid ${field} data`);
}

function parseAssignedHumanId(value: unknown): string | undefined {
  let parsed = value;
  if (typeof value === "string") {
    try {
      parsed = JSON.parse(value);
    } catch {
      return undefined;
    }
  }

  if (!parsed || typeof parsed !== "object" || !("human_id" in parsed)) {
    return undefined;
  }

  const humanId = (parsed as { human_id?: unknown }).human_id;
  return typeof humanId === "string" ? humanId : undefined;
}
