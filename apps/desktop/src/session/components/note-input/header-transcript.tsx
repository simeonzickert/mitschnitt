import { useLingui } from "@lingui/react/macro";
import { Waveform } from "@phosphor-icons/react";
import { useCallback, useMemo } from "react";

import { DancingSticks } from "@anlg/ui/components/ui/dancing-sticks";
import { Spinner } from "@anlg/ui/components/ui/spinner";
import { sonnerToast } from "@anlg/ui/components/ui/toast";
import { cn } from "@anlg/utils";

import { IconHeaderView, copyTextToClipboard } from "./header-shared";

import * as AudioPlayer from "~/audio-player";
import { useRegenerateTranscript } from "~/session/components/note-input/transcript/actions";
import {
  buildTranscriptExportSegments,
  formatTranscriptExportSegments,
} from "~/session/components/note-input/transcript/export-data";
import { canRegenerateTranscript } from "~/session/components/note-input/transcript/regenerate-condition";
import { useSessionTranscriptRenderData } from "~/session/components/note-input/transcript/render-request-hooks";
import { useHasTranscript } from "~/session/components/shared";
import {
  type MenuItemDef,
  useNativeContextMenu,
} from "~/shared/hooks/useNativeContextMenu";
import { useListener } from "~/stt/contexts";
import { canResumeSession } from "~/stt/resume-condition";
import { resumeBlockedMessage } from "~/stt/resume-blocked-message";
import { useResumeAfterStop } from "~/stt/useStartListening";
import {
  isMainWebviewWindow,
  requestMainListenerControl,
} from "~/stt/window-control";

export function HeaderViewTranscript({
  isActive,
  isTranscribing,
  onClick = () => {},
  sessionId,
}: {
  isActive: boolean;
  isTranscribing: boolean;
  onClick?: () => void;
  sessionId: string;
}) {
  const liveState = useTranscriptLiveViewState(sessionId);

  if (!isActive) {
    return (
      <HeaderViewTranscriptButton
        isActive={isActive}
        isTranscribing={isTranscribing}
        onClick={onClick}
        live={liveState.live}
      />
    );
  }

  return (
    <HeaderViewTranscriptActive
      isActive={isActive}
      isTranscribing={isTranscribing}
      onClick={onClick}
      sessionId={sessionId}
      live={liveState.live}
    />
  );
}

function HeaderViewTranscriptButton({
  isActive,
  isTranscribing,
  onClick,
  onContextMenu,
  live,
}: {
  isActive: boolean;
  isTranscribing: boolean;
  onClick?: () => void;
  onContextMenu?: React.MouseEventHandler<HTMLButtonElement>;
  live?: {
    amplitude: number;
    degraded: boolean;
    muted: boolean;
  };
}) {
  const { t } = useLingui();

  return (
    <IconHeaderView
      isActive={isActive}
      label={t`Transcript`}
      hoverLabel={undefined}
      icon={
        live ? (
          <HeaderViewTranscriptLiveIcon live={live} />
        ) : isTranscribing ? (
          <Spinner size={16} className="shrink-0" />
        ) : (
          <Waveform className="size-4" />
        )
      }
      onClick={onClick}
      onContextMenu={onContextMenu}
      title={undefined}
      className={cn([
        live
          ? [
              "group/transcript-live",
              isActive
                ? "w-[98px] min-w-[98px] gap-1.5 px-2 @max-[480px]:w-10 @max-[480px]:min-w-10 @max-[480px]:gap-0"
                : null,
              isActive
                ? live.degraded
                  ? [
                      "bg-amber-50 text-amber-500 hover:bg-amber-100 hover:text-amber-600",
                      "dark:bg-amber-950/50 dark:text-amber-300 dark:hover:bg-amber-950 dark:hover:text-amber-200",
                    ]
                  : [
                      "bg-red-50 text-red-500 hover:bg-red-100 hover:text-red-600",
                      "dark:bg-red-950/50 dark:text-red-300 dark:hover:bg-red-950 dark:hover:text-red-200",
                    ]
                : null,
            ]
          : null,
      ])}
    />
  );
}

function HeaderViewTranscriptLiveIcon({
  live,
}: {
  live: {
    amplitude: number;
    degraded: boolean;
    muted: boolean;
  };
}) {
  const color = live.degraded ? "#f59e0b" : "#ef4444";

  return (
    <span className="relative flex size-4 items-center justify-center">
      {live.muted ? (
        <Waveform className="size-4" />
      ) : (
        <DancingSticks
          amplitude={live.amplitude}
          color={color}
          height={16}
          width={16}
        />
      )}
    </span>
  );
}

function useTranscriptLiveViewState(sessionId: string) {
  const { amplitude, degraded, mode, muted } = useListener((state) => {
    const mode = state.getSessionMode(sessionId);
    return {
      amplitude: state.live.amplitude,
      degraded: state.live.degraded,
      mode,
      muted: state.live.muted,
    };
  });
  return {
    live:
      mode === "active"
        ? {
            amplitude: Math.min(
              Math.hypot(amplitude.mic, amplitude.speaker),
              1,
            ),
            degraded: Boolean(degraded),
            muted,
          }
        : undefined,
  };
}

function HeaderViewTranscriptActive({
  isActive,
  isTranscribing,
  onClick,
  sessionId,
  live,
}: {
  isActive: boolean;
  isTranscribing: boolean;
  onClick?: () => void;
  sessionId: string;
  live?: {
    amplitude: number;
    degraded: boolean;
    muted: boolean;
  };
}) {
  const regenerate = useRegenerateTranscript(sessionId);
  const resumeAfterStop = useResumeAfterStop(sessionId);
  const { request: transcriptExportRequest } =
    useSessionTranscriptRenderData(sessionId);
  const {
    audioExists,
    audioExistsResolved,
    deleteRecording,
    isDeletingRecording,
  } = AudioPlayer.useAudioPlayer();
  const hasTranscript = useHasTranscript(sessionId);
  const sessionMode = useListener((state) => state.getSessionMode(sessionId));
  const canCopyTranscript = Boolean(transcriptExportRequest);
  const handleCopyTranscript = useCallback(async () => {
    if (!transcriptExportRequest) {
      return;
    }

    try {
      const transcriptSegments = await buildTranscriptExportSegments(
        transcriptExportRequest,
      );
      const transcriptText = formatTranscriptExportSegments(transcriptSegments);
      if (!transcriptText) {
        return;
      }

      await copyTextToClipboard(transcriptText, {
        success: "Transcript copied to clipboard",
        error: "Failed to copy transcript",
      });
    } catch (error) {
      console.error("Failed to copy transcript", error);
      sonnerToast.error("Failed to copy transcript");
    }
  }, [transcriptExportRequest]);
  const handleDeleteRecording = useCallback(() => {
    void deleteRecording();
  }, [deleteRecording]);
  const handleResumeListening = useCallback(async () => {
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
  }, [sessionId, resumeAfterStop]);
  const contextMenu = useMemo<MenuItemDef[]>(() => {
    const items: MenuItemDef[] = [
      {
        id: `copy-transcript-${sessionId}`,
        text: "Copy",
        action: () => {
          void handleCopyTranscript();
        },
        disabled: !canCopyTranscript,
      },
    ];

    // "Resume" reaches the session in every state except one that is
    // already recording, or a running batch with nothing to resume into (a
    // pure import) -- finalizing and a batch repairing existing content
    // cancel whatever is in flight and keep recording, the same path the
    // header pill and the overflow menu use (ZICK-319 Strecke B/Fix-Runde
    // B2). Offering it during a pure import would make useResumeAfterStop
    // abort the import via its own stop-transcription retry loop.
    if (
      canResumeSession({
        sessionMode,
        hasContent: audioExists || hasTranscript,
      })
    ) {
      items.push({
        id: `resume-listening-${sessionId}`,
        text: "Resume listening",
        action: () => {
          void handleResumeListening();
        },
      });
    }

    if (
      canRegenerateTranscript({ audioExists, audioExistsResolved, sessionMode })
    ) {
      items.push({
        id: `regenerate-transcript-${sessionId}`,
        text: "Re-transcribe",
        action: () => {
          void regenerate();
        },
      });
    }

    if (audioExists) {
      items.push({
        id: `delete-recording-${sessionId}`,
        text: "Delete recording",
        action: handleDeleteRecording,
        disabled: isDeletingRecording,
      });
    }

    return items;
  }, [
    audioExists,
    audioExistsResolved,
    canCopyTranscript,
    handleCopyTranscript,
    handleDeleteRecording,
    handleResumeListening,
    hasTranscript,
    isDeletingRecording,
    regenerate,
    sessionMode,
    sessionId,
  ]);
  const showContextMenu = useNativeContextMenu(contextMenu);

  return (
    <HeaderViewTranscriptButton
      isActive={isActive}
      isTranscribing={isTranscribing}
      onClick={onClick}
      onContextMenu={showContextMenu}
      live={live}
    />
  );
}
