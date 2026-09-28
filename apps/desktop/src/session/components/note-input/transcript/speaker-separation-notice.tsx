import { Trans, useLingui } from "@lingui/react/macro";
import { UsersThree, X } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useCallback, useState } from "react";

import { commands as fsSyncCommands } from "@anlg/plugin-fs-sync";
import { commands as transcriptionCommands } from "@anlg/plugin-transcription";
import { Button } from "@anlg/ui/components/ui/button";

import {
  dismissSpeakerSeparationNotice,
  isSpeakerSeparationNoticeDismissed,
  type TranscriptChannelSummary,
  unsplitTranscriptCandidate,
} from "./speaker-separation-notice-condition";

import { useLiveQuery } from "~/db";
import { DestructiveConfirmationDialog } from "~/shared/ui/destructive-confirmation-dialog";

type ChannelSummaryRow = {
  provider: string | null;
  model: string | null;
  word_count: number | null;
  session_channels: number | null;
  session_provider_speakers: number | null;
  session_assigned_humans: number | null;
};

const EMPTY: TranscriptChannelSummary[] = [];

// Hinweiswerte liegen als Objekt oder als JSON-Text vor; `json_extract` auf
// das Ergebnis von `json_extract(hint, '$.value')` liest beide Formen.
const HINT_VALUE = "json_extract(hint.value, '$.value')";

function useTranscriptChannelSummaries(
  sessionId: string,
): TranscriptChannelSummary[] {
  const { data = EMPTY } = useLiveQuery<
    ChannelSummaryRow,
    TranscriptChannelSummary[]
  >({
    sql: `
      WITH session_transcripts AS (
        SELECT * FROM transcripts
        WHERE session_id = ? AND deleted_at IS NULL
      )
      SELECT
        transcript.provider AS provider,
        transcript.model AS model,
        CASE WHEN json_valid(transcript.words_json)
          THEN json_array_length(transcript.words_json) ELSE 0 END AS word_count,
        (
          SELECT COUNT(DISTINCT json_extract(word.value, '$.channel'))
          FROM session_transcripts AS t, json_each(t.words_json) AS word
          WHERE json_valid(t.words_json)
        ) AS session_channels,
        (
          SELECT COUNT(DISTINCT
            COALESCE(json_extract(${HINT_VALUE}, '$.channel'), '') || ':' ||
            json_extract(${HINT_VALUE}, '$.speaker_index'))
          FROM session_transcripts AS t, json_each(t.speaker_hints_json) AS hint
          WHERE json_valid(t.speaker_hints_json)
            AND json_extract(hint.value, '$.type') = 'provider_speaker_index'
            AND json_extract(${HINT_VALUE}, '$.speaker_index') IS NOT NULL
        ) AS session_provider_speakers,
        (
          SELECT COUNT(DISTINCT json_extract(${HINT_VALUE}, '$.human_id'))
          FROM session_transcripts AS t, json_each(t.speaker_hints_json) AS hint
          WHERE json_valid(t.speaker_hints_json)
            AND json_extract(hint.value, '$.type') = 'automatic_speaker_assignment'
        ) AS session_assigned_humans
      FROM session_transcripts AS transcript
      ORDER BY transcript.started_at_ms, transcript.created_at, transcript.id
    `,
    params: [sessionId],
    enabled: Boolean(sessionId),
    mapRows: (rows) =>
      rows.map((row) => ({
        provider: row.provider || null,
        model: row.model || null,
        wordCount: row.word_count ?? 0,
        sessionChannelCount: row.session_channels ?? 0,
        sessionProviderSpeakerCount: row.session_provider_speakers ?? 0,
        sessionAssignedHumanCount: row.session_assigned_humans ?? 0,
      })),
  });
  return data;
}

/**
 * Soll der Hinweis erscheinen? Erst der billige Vorfilter (alles auf einem
 * Kanal), dann Rust mit der Aufnahme. Ohne Tondatei kein Hinweis.
 */
export function useSpeakerSeparationNotice({
  sessionId,
  enabled,
}: {
  sessionId: string;
  enabled: boolean;
}) {
  const summaries = useTranscriptChannelSummaries(sessionId);
  const candidate = unsplitTranscriptCandidate(summaries);
  const [dismissed, setDismissed] = useState(() =>
    isSpeakerSeparationNoticeDismissed(sessionId),
  );

  const wanted = enabled && !dismissed && candidate !== null;

  // Welche Aufnahme? Pfad und Aenderungszeit gehoeren in den Schluessel der
  // Pruefung (Forge m8): eine ausgetauschte Aufnahme wird neu geprueft.
  const { data: audio, isPending: audioPending } = useQuery({
    queryKey: ["speaker-separation-audio", sessionId],
    enabled: wanted,
    staleTime: 0,
    queryFn: async () => {
      const path = await fsSyncCommands.audioPath(sessionId);
      if (path.status === "error") {
        return null;
      }
      const metadata = await fsSyncCommands.audioSourceMetadata(path.data);
      return {
        path: path.data,
        modifiedAt:
          metadata.status === "ok" ? (metadata.data.modifiedAt ?? "") : "",
      };
    },
  });

  const enabledQuery = wanted && Boolean(audio);
  const { data: available = false, isPending } = useQuery({
    queryKey: [
      "speaker-separation-available",
      sessionId,
      candidate?.provider,
      candidate?.model,
      audio?.path,
      audio?.modifiedAt,
    ],
    enabled: enabledQuery,
    staleTime: Infinity,
    queryFn: async () => {
      if (!candidate || !audio) {
        return false;
      }
      const result = await transcriptionCommands.cloudChannelSplitAvailable(
        candidate.provider,
        candidate.model,
        audio.path,
      );
      return result.status === "ok" && result.data;
    },
  });

  const dismiss = useCallback(() => {
    dismissSpeakerSeparationNotice(sessionId);
    setDismissed(true);
  }, [sessionId]);

  // Drei Zustaende statt zwei (Forge M6): solange die Pruefung laeuft, ist
  // noch nicht entschieden, ob der Hinweis kommt -- in der Zeit darf der
  // einfache Knopf nicht aufblitzen.
  const status: "hidden" | "checking" | "show" = !wanted
    ? "hidden"
    : audioPending
      ? "checking"
      : !enabledQuery
        ? "hidden"
        : isPending
          ? "checking"
          : available
            ? "show"
            : "hidden";

  return {
    status,
    show: status === "show",
    dismiss,
  };
}

// Neu-Transkribieren ersetzt Hand-Korrekturen und kostet beim Anbieter
// erneut, deshalb fragt der Knopf einmal nach -- wie der Streifen fuer
// fortgesetzte Aufnahmen (resume-notice.tsx).
export function TranscriptSpeakerSeparationNotice({
  onRegenerate,
  onDismiss,
}: {
  onRegenerate: () => Promise<void>;
  onDismiss: () => void;
}) {
  const { t } = useLingui();
  const [confirmOpen, setConfirmOpen] = useState(false);

  // Schliesst beim Start, nicht beim Ende des Laufs (Forge-Nachpruefung M6):
  // sonst liegt der Dialog modal ueber "Stop transcription". Doppelstart
  // verhindern Sitzungszustand und Plugin (`reserve_batch_session`).
  const handleConfirm = useCallback(() => {
    setConfirmOpen(false);
    void onRegenerate();
  }, [onRegenerate]);

  return (
    <div
      data-testid="transcript-speaker-separation-notice"
      className="border-border bg-muted/50 text-muted-foreground flex items-start gap-2 border-b px-3 py-2 text-xs"
    >
      <UsersThree className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <div className="flex flex-1 flex-col gap-0.5">
        <span>
          <Trans>
            This transcript is from before speaker separation: everything is
            shown as one speaker. Re-transcribing separates you from the others.
          </Trans>
        </span>
        <span>
          <Trans>Your cloud provider charges for this again.</Trans>
        </span>
      </div>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="text-foreground shrink-0 rounded-md"
        onClick={() => setConfirmOpen(true)}
      >
        <Trans>Re-transcribe</Trans>
      </Button>
      <button
        type="button"
        aria-label={t`Dismiss`}
        className="hover:text-foreground mt-0.5 shrink-0"
        onClick={onDismiss}
      >
        <X className="size-3.5" aria-hidden="true" />
      </button>
      <DestructiveConfirmationDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t`Re-transcribe with speaker separation?`}
        description={t`Your cloud provider charges for this again. Manual speaker and text corrections will be replaced.`}
        confirmLabel={t`Re-transcribe`}
        onConfirm={handleConfirm}
      />
    </div>
  );
}
