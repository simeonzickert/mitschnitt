import { Trans } from "@lingui/react/macro";

import { Button } from "@anlg/ui/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@anlg/ui/components/ui/dialog";

import { useAudioRetentionLabels } from "./audio-retention-labels";
import type { AudioRetentionPolicy } from "./audio-retention-policy";

/**
 * The one question an installation is asked about its retention deadline.
 *
 * Mitschnitt-Fork (F18). It appears when the cleanup pass first finds
 * recordings past the deadline on an installation that has never armed it --
 * which is the ordinary case after a Time Machine restore, a copied database,
 * or a new device joining a cloud sync, where the library is older than the
 * answer written at startup.
 *
 * Both buttons end the question for good. Closing it is the third way out and
 * it is deliberately NOT an answer: nothing is written, and the next pass
 * raises the same question a minute later. A deadline can never arm itself by
 * being dismissed, and a person who is not ready to decide about three years of
 * recordings can go and look at them first.
 *
 * `onDismiss` has to be wired for that to be true. `DialogContent` always
 * renders a close X, and `Escape` and the backdrop go the same way, so a
 * `<Dialog open>` with no `onOpenChange` does not ship a dialog that keeps
 * asking -- it ships a modal that cannot be dismissed at all, with a visible
 * button that does nothing. Found by the second-look review; the sentence above
 * was describing behaviour the code did not have.
 *
 * "Keep everything" is deliberately not just an answer -- it moves the deadline
 * to "never delete". Recording the answer without changing the deadline would
 * leave a setting on screen that says six months next to a library nobody is
 * ever going to clear, and the next person to touch that picker would be told
 * they are shortening something that was never in force.
 */
export function AudioRetentionConsentDialog({
  policy,
  expiring,
  onConfirmDeleting,
  onKeepEverything,
  onDismiss,
}: {
  policy: AudioRetentionPolicy;
  expiring: number;
  onConfirmDeleting: () => void;
  onKeepEverything: () => void;
  onDismiss: () => void;
}) {
  const labels = useAudioRetentionLabels();
  const deadline = labels[policy] ?? policy;

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onDismiss();
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            <Trans>Start deleting recordings that are past the deadline?</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>
              {expiring} of your recordings are older than {deadline}, which is
              how long this installation is set to keep them. Nothing has been
              deleted. If you clean up automatically from now on, they are
              removed at the next pass, from the meeting folder as well as from
              the app. Transcripts and summaries stay either way.
            </Trans>
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onKeepEverything}>
            <Trans>Keep everything</Trans>
          </Button>
          <Button variant="destructive" onClick={onConfirmDeleting}>
            <Trans>Clean up automatically from now on</Trans>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
