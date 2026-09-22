import { Microphone, MicrophoneSlash } from "@phosphor-icons/react";

import { DropdownMenuItem } from "@anlg/ui/components/ui/dropdown-menu";
import { sonnerToast } from "@anlg/ui/components/ui/toast";

import { useListener } from "~/stt/contexts";
import { canResumeSession } from "~/stt/resume-condition";
import { resumeBlockedMessage } from "~/stt/resume-blocked-message";
import { useResumeAfterStop, useStartListening } from "~/stt/useStartListening";
import {
  isMainWebviewWindow,
  requestMainListenerControl,
} from "~/stt/window-control";

export function Listening({
  sessionId,
  resume,
}: {
  sessionId: string;
  resume: boolean;
}) {
  const { mode, stop } = useListener((state) => ({
    mode: state.getSessionMode(sessionId),
    stop: state.stop,
  }));
  const isActive = mode === "active";
  const isBatching = mode === "running_batch";
  // A running batch with nothing to resume into (a pure import, no prior
  // recording on this session) stays the dead "Batch processing" row it
  // always was. Everything else -- inactive, finalizing, or a batch that IS
  // repairing existing content -- reaches "resume" through the same cancel-
  // and-keep-recording path as the header pill and the context menu
  // (ZICK-319 Strecke B/Fix-Runde B2: canResumeSession is the single shared
  // check for this exclusion).
  const isDeadBatch =
    isBatching && !canResumeSession({ sessionMode: mode, hasContent: resume });
  const startListening = useStartListening(sessionId);
  const resumeAfterStop = useResumeAfterStop(sessionId);

  const handleResume = async () => {
    if (!isMainWebviewWindow()) {
      await requestMainListenerControl("resume", sessionId);
      return;
    }

    const result = await resumeAfterStop();
    if (result.status === "blocked") {
      sonnerToast.error(resumeBlockedMessage(result.reason), {
        id: "resume-after-stop-blocked",
      });
    }
  };

  const handleToggleListening = () => {
    if (isDeadBatch) {
      return;
    }

    if (isActive) {
      if (!isMainWebviewWindow()) {
        void requestMainListenerControl("stop", sessionId);
        return;
      }

      stop();
      return;
    }

    if (resume) {
      void handleResume();
      return;
    }

    if (!isMainWebviewWindow()) {
      void requestMainListenerControl("start", sessionId);
      return;
    }

    startListening();
  };

  const startLabel = resume ? "Resume listening" : "Start listening";

  return (
    <DropdownMenuItem
      className="cursor-pointer"
      onClick={handleToggleListening}
      disabled={isDeadBatch}
    >
      {isActive ? <MicrophoneSlash /> : <Microphone />}
      <span>
        {isDeadBatch
          ? "Batch processing"
          : isActive
            ? "Stop listening"
            : startLabel}
      </span>
    </DropdownMenuItem>
  );
}
