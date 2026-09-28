import { t } from "@lingui/core/macro";
import { useCallback } from "react";

import { commands as fsSyncCommands } from "@anlg/plugin-fs-sync";
import { sonnerToast } from "@anlg/ui/components/ui/toast";

import { getEnhancerService } from "~/services/enhancer";
import { useListener } from "~/stt/contexts";
import { isStoppedTranscriptionError, useRunBatch } from "~/stt/useRunBatch";

// Sitzungen, deren Neu-Transkription gerade gestartet wird oder laeuft.
//
// Die Bestaetigungsdialoge schliessen, sobald der Lauf angestossen ist
// (Forge-Nachpruefung M6), und halten keinen eigenen "pending"-Zustand mehr.
// Zwischen Klick und dem Moment, in dem der Sitzungszustand auf den Lauf
// umspringt (erst `audioPath`, dann `runBatch`), liegt aber eine Luecke; ein
// zweiter Start wuerde im Plugin mit "session already running" scheitern und
// den laufenden Batch in der Oberflaeche als fehlgeschlagen markieren. Diese
// Sperre gilt fuer ALLE Aufrufer (Reiter, Streifen, Hinweis, Menues).
const regeneratingSessions = new Set<string>();

export function useRegenerateTranscript(sessionId: string) {
  const runBatch = useRunBatch(sessionId);
  const handleBatchFailed = useListener((state) => state.handleBatchFailed);

  return useCallback(async () => {
    if (regeneratingSessions.has(sessionId)) {
      return;
    }
    regeneratingSessions.add(sessionId);

    const regenerateOnce = async () => {
      const result = await fsSyncCommands.audioPath(sessionId);
      if (result.status === "error") {
        sonnerToast.error(t`Recording not found. It may have been deleted.`, {
          id: `transcript-regenerate-audio-missing-${sessionId}`,
        });
        return;
      }

      const audioPath = result.data;

      try {
        await runBatch(audioPath, {
          promotion: { scope: "whole_session" },
        });
        await getEnhancerService()?.queueAutoEnhanceIfSummaryEmpty(sessionId);
      } catch (error) {
        if (isStoppedTranscriptionError(error)) {
          return;
        }
        const msg = error instanceof Error ? error.message : String(error);
        handleBatchFailed(sessionId, msg);
        sonnerToast.error(t`Re-transcription failed`, {
          id: `transcript-regenerate-failed-${sessionId}`,
          description: msg,
        });
      }
    };

    try {
      await regenerateOnce();
    } finally {
      regeneratingSessions.delete(sessionId);
    }
  }, [handleBatchFailed, runBatch, sessionId]);
}
