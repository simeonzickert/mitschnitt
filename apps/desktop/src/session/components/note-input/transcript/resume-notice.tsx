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
// and the confirm button's own isPending state blocks a second click from
// starting a second batch before the store has caught up (Fix-Runde B3b).
export function TranscriptResumeNotice({
  onRegenerate,
}: {
  onRegenerate: () => Promise<void>;
}) {
  const { t } = useLingui();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [pending, setPending] = useState(false);

  const handleConfirm = useCallback(() => {
    if (pending) {
      return;
    }

    setPending(true);
    void onRegenerate().finally(() => {
      setPending(false);
      setConfirmOpen(false);
    });
  }, [onRegenerate, pending]);

  return (
    <div
      data-testid="transcript-resume-notice"
      className="border-border bg-muted/50 text-muted-foreground flex items-start gap-2 border-b px-3 py-2 text-xs"
    >
      <ArrowsClockwise className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
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
        onOpenChange={(open) => {
          if (!pending) {
            setConfirmOpen(open);
          }
        }}
        title={t`Re-transcribe the whole recording?`}
        description={t`This creates one transcript with consistent speakers. Manual speaker and text corrections will be replaced.`}
        confirmLabel={t`Re-transcribe all`}
        pendingLabel={t`Re-transcribing…`}
        isPending={pending}
        onConfirm={handleConfirm}
      />
    </div>
  );
}
