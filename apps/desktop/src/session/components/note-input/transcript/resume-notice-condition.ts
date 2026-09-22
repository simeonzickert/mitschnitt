import { canRegenerateTranscript } from "./regenerate-condition";

import type { SessionMode } from "~/store/zustand/listener/general";

/**
 * Ob der "mehr als ein Transkript zusammenfuehren"-Streifen angezeigt wird.
 * Traegt das Audio-Gate von canRegenerateTranscript mit (Fix-Runde B3a):
 * useRegenerateTranscript bricht ohne Audio-Datei sofort mit "Recording not
 * found" ab, also darf der Streifen sein Angebot ohne Audio gar nicht erst
 * machen. transcript/index.tsx (der einzige Aufrufer) nutzt dasselbe
 * Ergebnis auch, um den aelteren ZICK-318-Re-transcribe-Knopf auszublenden,
 * solange der Streifen schon dasselbe Angebot macht (Fix-Runde B3e) -- eine
 * Berechnung statt zwei, damit beide nie auseinanderlaufen.
 */
export function shouldShowTranscriptResumeNotice({
  audioExists,
  audioExistsResolved,
  postStopProcessing,
  sessionMode,
  transcriptCount,
}: {
  audioExists: boolean;
  audioExistsResolved: boolean;
  postStopProcessing: boolean;
  sessionMode: SessionMode;
  transcriptCount: number;
}): boolean {
  return (
    !postStopProcessing &&
    transcriptCount > 1 &&
    canRegenerateTranscript({ audioExists, audioExistsResolved, sessionMode })
  );
}
