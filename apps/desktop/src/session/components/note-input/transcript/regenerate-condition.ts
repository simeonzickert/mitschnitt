import type { SessionMode } from "~/store/zustand/listener/general";

/**
 * Ob "Re-transcribe" angeboten werden darf. Geteilte Bedingung fuer den
 * sichtbaren Knopf im Transkript-Reiter (ZICK-318) und das Rechtsklick-Menue
 * auf dem Transkript-Tab-Icon (header-transcript.tsx) -- beide muessen
 * dieselbe Antwort geben, deshalb steht die Bedingung nur hier.
 */
export function canRegenerateTranscript({
  audioExists,
  audioExistsResolved,
  sessionMode,
}: {
  audioExists: boolean;
  audioExistsResolved: boolean;
  sessionMode: SessionMode;
}): boolean {
  return audioExistsResolved && sessionMode === "inactive" && audioExists;
}
