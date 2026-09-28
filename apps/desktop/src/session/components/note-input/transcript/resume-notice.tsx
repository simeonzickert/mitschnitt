import { Trans, useLingui } from "@lingui/react/macro";
import { ArrowsClockwise } from "@phosphor-icons/react";
import { useCallback, useState } from "react";

import { Button } from "@anlg/ui/components/ui/button";

import { DestructiveConfirmationDialog } from "~/shared/ui/destructive-confirmation-dialog";

// After a resumed recording, "keep recording" leaves the session with more
// than one transcript (and more than one speaker set) -- nothing merges
// them on its own (ZICK-319 Strecke B, ISC 4). This strip offers the
// one-shot fix: re-run the whole recording through the transcriber so it
// comes back as a single transcript with consistent speakers.
//
// Visibility (audio must exist, resolved, session inactive, no post-stop
// lease, more than one transcript) is decided once by the caller via
// shouldShowTranscriptResumeNotice -- this component only renders the
// strip and the confirmation, it never re-checks any of that itself
// (Fix-Runde B3a/e).
//
// Re-transcribing replaces manual speaker/text corrections, so it asks
// first instead of firing on the strip's own button click (Fix-Runde B3d),
// and closes as soon as the run starts (Forge 27.09.2026: a dialog left open
// for the whole run sat modally over "Stop transcription"). The double-start
// guard that used to live here as isPending (Fix-Runde B3b) now sits in
// useRegenerateTranscript itself, so it covers every caller.
export function TranscriptResumeNotice({
  onRegenerate,
}: {
  onRegenerate: () => Promise<void>;
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
      data-testid="transcript-resume-notice"
      className="border-border bg-muted/50 text-muted-foreground flex items-start gap-2 border-b px-3 py-2 text-xs"
    >
      <ArrowsClockwise
        className="mt-0.5 size-3.5 shrink-0"
        aria-hidden="true"
      />
      <div className="flex flex-1 flex-col gap-0.5">
        <span>
          <Trans>
            This session has more than one recording and more than one
            transcript.
          </Trans>
        </span>
      </div>
      <Button
        type="button"
        size="sm"
        variant="outline"
        className="text-foreground shrink-0 rounded-md"
        onClick={() => setConfirmOpen(true)}
      >
        <Trans>Re-transcribe all</Trans>
      </Button>
      <DestructiveConfirmationDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t`Re-transcribe the whole recording?`}
        description={t`This creates one transcript with consistent speakers. Manual speaker and text corrections will be replaced.`}
        confirmLabel={t`Re-transcribe all`}
        onConfirm={handleConfirm}
      />
    </div>
  );
}
