import { useLingui } from "@lingui/react/macro";
import { useCallback, useState } from "react";

import { DestructiveConfirmationDialog } from "~/shared/ui/destructive-confirmation-dialog";

/**
 * Jede Neu-Transkription des ganzen Gespraechs ueber EINE Bestaetigung
 * (Forge M6). Sie ersetzt Hand-Korrekturen an Text und Sprechern; vorher gab
 * es die Rueckfrage nur im Fortsetzungs-Streifen und im Sprechertrennungs-
 * Hinweis, der einfache Knopf und das Rechtsklick-Menue liefen ungefragt.
 *
 * Gefragt wird, sobald ein Transkript mit Woertern existiert -- nicht nur bei
 * nachweisbaren Korrekturen: Textkorrekturen hinterlassen keine Markierung
 * (`updateTranscriptSegmentText` aendert nur `words_json`), sie lassen sich
 * also nicht erkennen. Die sichere Richtung ist eine Rueckfrage zu viel.
 */
export function useConfirmedRegenerate(
  regenerate: () => Promise<void>,
  needsConfirmation: boolean,
) {
  const { t } = useLingui();
  const [open, setOpen] = useState(false);

  const request = useCallback(() => {
    if (needsConfirmation) {
      setOpen(true);
      return;
    }
    void regenerate();
  }, [needsConfirmation, regenerate]);

  // Der Dialog schliesst, sobald der Lauf angestossen ist -- nicht erst, wenn
  // er fertig ist (Forge-Nachpruefung M6): sonst liegt er modal ueber
  // "Stop transcription". Einen zweiten Start verhindern die Sitzung selbst
  // (Knoepfe nur bei sessionMode "inactive", `regenerate-condition.ts`) und
  // das Plugin (`reserve_batch_session`: "session already running").
  const confirm = useCallback(() => {
    setOpen(false);
    void regenerate();
  }, [regenerate]);

  const dialog = (
    <DestructiveConfirmationDialog
      open={open}
      onOpenChange={setOpen}
      title={t`Re-transcribe the whole recording?`}
      description={t`Manual speaker and text corrections will be replaced.`}
      confirmLabel={t`Re-transcribe`}
      onConfirm={confirm}
    />
  );

  return { request, dialog };
}
