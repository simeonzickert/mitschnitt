import { t } from "@lingui/core/macro";
import { Trans } from "@lingui/react/macro";
import {
  ArrowsClockwise,
  CheckCircle,
  PencilSimple,
} from "@phosphor-icons/react";
import type { RefObject } from "react";
import { useCallback } from "react";

import { Button } from "@anlg/ui/components/ui/button";
import { cn } from "@anlg/utils";

import { useRegenerateTranscript } from "./actions";
import { canRegenerateTranscript } from "./regenerate-condition";
import { TranscriptViewer } from "./renderer";
import { TranscriptResumeNotice } from "./resume-notice";
import { shouldShowTranscriptResumeNotice } from "./resume-notice-condition";
import { BatchState } from "./screens/batch";
import { TranscriptEmptyState } from "./screens/empty";
import { TranscriptListeningState } from "./screens/listening";
import { useTranscriptScreen } from "./state";

import { useAudioPlayer } from "~/audio-player";
import { TranscriptTimingNotice } from "~/imports/provenance-view";
import { useListener } from "~/stt/contexts";
import { useSessionTranscriptMetadata } from "~/stt/queries";
import { useUploadFile } from "~/stt/useUploadFile";

export function TranscriptEditButton({
  editMode,
  onEditModeChange,
}: {
  editMode: boolean;
  onEditModeChange: (editMode: boolean) => void;
}) {
  return (
    <div className="mr-1 shrink-0">
      <button
        type="button"
        data-tauri-drag-region="false"
        aria-pressed={editMode}
        onClick={() => onEditModeChange(!editMode)}
        className={cn([
          "border-border bg-card text-foreground flex h-7 items-center gap-1.5 rounded-full border px-2 text-sm font-medium @max-[480px]:w-7 @max-[480px]:justify-center @max-[480px]:gap-0 @max-[480px]:px-0",
          "hover:bg-accent focus-visible:ring-ring transition-colors focus-visible:ring-2 focus-visible:outline-hidden",
          editMode ? "border-primary/30 bg-primary/10 text-primary" : null,
        ])}
      >
        {editMode ? (
          <CheckCircle aria-hidden className="size-3.5" />
        ) : (
          <PencilSimple aria-hidden className="size-3.5" />
        )}
        <span className="@max-[480px]:sr-only">
          {editMode ? <Trans>Done</Trans> : <Trans>Edit</Trans>}
        </span>
      </button>
    </div>
  );
}

export function Transcript({
  sessionId,
  scrollRef,
  editMode = false,
}: {
  sessionId: string;
  scrollRef: RefObject<HTMLDivElement | null>;
  editMode?: boolean;
}) {
  return (
    <TranscriptContent
      key={sessionId}
      sessionId={sessionId}
      scrollRef={scrollRef}
      editMode={editMode}
    />
  );
}

function TranscriptContent({
  sessionId,
  scrollRef,
  editMode,
}: {
  sessionId: string;
  scrollRef: RefObject<HTMLDivElement | null>;
  editMode: boolean;
}) {
  const screen = useTranscriptScreen({ sessionId });
  const { uploadAudio, uploadTranscript } = useUploadFile(sessionId);
  const regenerateTranscript = useRegenerateTranscript(sessionId);
  const stopTranscription = useListener((state) => state.stopTranscription);
  const sessionMode = useListener((state) => state.getSessionMode(sessionId));
  const postStopProcessing = useListener(
    (state) => state.live.postStopProcessingBySession[sessionId] ?? false,
  );
  const { audioExists, audioExistsResolved } = useAudioPlayer();
  const transcripts = useSessionTranscriptMetadata(sessionId);
  // ZICK-318: derselbe Knopf, den das Rechtsklick-Menue auf dem
  // Transkript-Tab-Icon schon anbietet (header-transcript.tsx), jetzt auch
  // direkt sichtbar oben im Reiter. Bedingung geteilt via
  // regenerate-condition.ts, damit beide Stellen nie auseinanderlaufen.
  const canRegenerate = canRegenerateTranscript({
    audioExists,
    audioExistsResolved,
    sessionMode,
  });
  // ZICK-319 Fix-Runde B3e: the resume strip below offers the exact same
  // "re-transcribe everything" action once there is more than one
  // transcript to merge -- showing the plain button on top of it would be
  // two controls doing the same thing. Computed once here (not inside the
  // strip) so the two decisions can never disagree.
  const showResumeNotice = shouldShowTranscriptResumeNotice({
    audioExists,
    audioExistsResolved,
    postStopProcessing,
    sessionMode,
    transcriptCount: transcripts.length,
  });
  const handleStopTranscription = useCallback(() => {
    void stopTranscription(sessionId);
  }, [sessionId, stopTranscription]);
  const handleRegenerateClick = useCallback(() => {
    void regenerateTranscript();
  }, [regenerateTranscript]);

  return (
    <div className="relative flex h-full flex-col overflow-hidden">
      {screen.kind === "running_batch" && (
        <TranscriptEmptyState
          isBatching
          percentage={screen.percentage}
          phase={screen.phase}
          onStopTranscription={
            screen.phase === "importing" ? undefined : handleStopTranscription
          }
        />
      )}
      {screen.kind === "batch_fallback" && (
        <BatchState
          requestedLiveTranscription={screen.requestedLiveTranscription}
          error={screen.error}
        />
      )}
      {screen.kind === "listening" && (
        <TranscriptListeningState status={screen.status} />
      )}
      {screen.kind === "empty" && (
        <TranscriptEmptyState
          isBatching={false}
          hasAudio={screen.hasAudio}
          error={screen.error}
          onRetranscribe={regenerateTranscript}
          onUploadAudio={uploadAudio}
          onUploadTranscript={uploadTranscript}
        />
      )}
      {screen.kind === "ready" && (
        <TranscriptTimingNotice sessionId={sessionId} />
      )}
      {screen.kind === "ready" && canRegenerate && !showResumeNotice && (
        <div className="flex justify-end pb-2">
          <Button
            size="sm"
            className="gap-2"
            onClick={handleRegenerateClick}
          >
            <ArrowsClockwise className="size-4" />
            {t`Re-transcribe`}
          </Button>
        </div>
      )}
      {screen.kind === "ready" && showResumeNotice && (
        <TranscriptResumeNotice onRegenerate={regenerateTranscript} />
      )}
      {screen.kind === "ready" && (
        <TranscriptViewer
          transcriptIds={screen.transcriptIds}
          liveSegments={screen.liveSegments}
          currentActive={screen.currentActive}
          captureGeneration={screen.captureGeneration}
          scrollRef={scrollRef}
          editMode={editMode && !screen.currentActive}
        />
      )}
    </div>
  );
}
