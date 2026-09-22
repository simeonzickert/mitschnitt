import type { SessionMode } from "~/store/zustand/listener/general";

/**
 * Ob ein Klick auf "Resume" fuer diese Sitzung jetzt sicher und sinnvoll ist.
 * Geteilte Bedingung fuer die Kopf-Pille (outer-header/index.tsx), das
 * Rechtsklick-Menue auf dem Transkript-Tab-Icon (header-transcript.tsx) und
 * das Overflow-Menue (overflow/listening.tsx) -- ohne sie bot das
 * Kontextmenue Resume auch bei einem reinen Batch-Import an (running_batch
 * ohne jedes vorherige Audio oder Transkript), was useResumeAfterStop dazu
 * bringt, den laufenden Import ueber seine eigene stop-transcription-
 * Wiederholschleife abzubrechen (ZICK-319 Fix-Runde B2).
 *
 * `hasContent` ist an jeder Aufrufstelle bereits vorhanden (audioExists ||
 * hasTranscript) -- die Funktion unterscheidet die beiden Quellen nie, also
 * nimmt sie die schon zusammengefasste Aussage entgegen statt sie ein
 * zweites Mal auseinanderzuziehen.
 */
export function canResumeSession({
  sessionMode,
  hasContent,
}: {
  sessionMode: SessionMode;
  hasContent: boolean;
}): boolean {
  if (sessionMode === "active") {
    return false;
  }

  if (sessionMode === "running_batch") {
    // A pure import (no earlier recording, no earlier transcript on this
    // session) has nothing to resume into.
    return hasContent;
  }

  return true;
}
