import { useLingui } from "@lingui/react/macro";
import { Headset, Square, VideoCamera } from "@phosphor-icons/react";
import { useCallback, useRef, useState } from "react";

import { commands as openerCommands } from "@anlg/plugin-opener2";
import { Spinner } from "@anlg/ui/components/ui/spinner";
import { sonnerToast } from "@anlg/ui/components/ui/toast";
import { cn, safeParseDate } from "@anlg/utils";

import { FolderPicker } from "../folder-picker";
import { TranscriptEditButton } from "../note-input/transcript";
import { RecordingIcon, useHasTranscript } from "../shared";
import { TitleInput } from "../title-input";
import { MetadataButton } from "./metadata";
import { OverflowButton } from "./overflow";

import { useAudioPlayer } from "~/audio-player";
import { useNow } from "~/calendar/hooks";
import { useShell } from "~/contexts/shell";
import { useEventCountdown } from "~/session/hooks/useEventCountdown";
import { useMeetingAccessibilityActive } from "~/session/hooks/useMeetingAccessibilityActive";
import {
  getRemoteMeeting,
  type RemoteMeeting,
} from "~/session/hooks/useRemoteMeeting";
import { useSessionEvent } from "~/session/hooks/useSessionEvent";
import { useWindowControlsGutter } from "~/shared/hooks/useWindowControlsGutter";
import type { SessionMode } from "~/store/zustand/listener/general";
import type { EditorView, Tab } from "~/store/zustand/tabs/schema";
import { useListener } from "~/stt/contexts";
import { canResumeSession } from "~/stt/resume-condition";
import { resumeBlockedMessage } from "~/stt/resume-blocked-message";
import { useResumeAfterStop, useStartListening } from "~/stt/useStartListening";
import {
  isMainWebviewWindow,
  requestMainListenerControl,
} from "~/stt/window-control";

export function OuterHeader({
  sessionId,
  currentView,
  tab,
  standaloneWindow = false,
  viewSwitcher,
  transcriptEditMode = false,
  onTranscriptEditModeChange,
}: {
  sessionId: string;
  currentView: EditorView;
  tab?: Extract<Tab, { type: "sessions" }>;
  standaloneWindow?: boolean;
  viewSwitcher?: React.ReactNode;
  transcriptEditMode?: boolean;
  onTranscriptEditModeChange?: (editMode: boolean) => void;
}) {
  const { leftsidebar } = useShell();
  const sessionMode = useListener((state) => state.getSessionMode(sessionId));
  const sessionEvent = useSessionEvent(sessionId);
  const hasTranscript = useHasTranscript(sessionId);
  const { audioExists } = useAudioPlayer();
  const now = useNow();
  const showWindowControlsGutter = useWindowControlsGutter();
  const showSidebarTimelineHeaderGutter =
    !standaloneWindow && !leftsidebar.expanded;
  const endedAt = sessionEvent?.ended_at
    ? safeParseDate(sessionEvent.ended_at)
    : null;
  const ended = !!endedAt && endedAt.getTime() <= now.getTime();
  const isRecording =
    sessionMode === "active" || sessionMode === "running_batch";
  const isLiveMeeting = isRecording || sessionMode === "finalizing";
  const meetingOver = !isRecording && (ended || hasTranscript || audioExists);
  const showTitleInput = Boolean(tab) && !isLiveMeeting && !meetingOver;

  return (
    <div
      data-tauri-drag-region
      className={cn([
        "relative flex w-full items-center gap-[2px]",
        "h-12",
        standaloneWindow && (showWindowControlsGutter ? "pl-[76px]" : "pl-2"),
        !standaloneWindow && leftsidebar.expanded && "pl-2",
        showSidebarTimelineHeaderGutter &&
          (showWindowControlsGutter ? "pl-[108px]" : "pl-[32px]"),
      ])}
    >
      {viewSwitcher}
      {showTitleInput && tab ? (
        <div className="max-w-56 min-w-0 shrink">
          <TitleInput key={tab.id} tab={tab} variant="breadcrumb" />
        </div>
      ) : null}
      <div
        data-tauri-drag-region
        data-session-header-spacer
        className="min-h-full min-w-0 flex-1"
      />
      <div
        data-tauri-drag-region
        className="relative z-10 flex shrink-0 items-center pr-1"
      >
        <HeaderMeetingControl
          sessionId={sessionId}
          sessionMode={sessionMode}
          currentView={currentView}
          transcriptEditMode={transcriptEditMode}
          onTranscriptEditModeChange={onTranscriptEditModeChange}
        />
        <FolderPicker sessionId={sessionId} align="end" />
        <MetadataButton sessionId={sessionId} />
        <OverflowButton
          standaloneWindow={standaloneWindow}
          sessionId={sessionId}
          currentView={currentView}
        />
      </div>
    </div>
  );
}

function HeaderMeetingControl({
  sessionId,
  sessionMode,
  currentView,
  transcriptEditMode,
  onTranscriptEditModeChange,
}: {
  sessionId: string;
  sessionMode: SessionMode;
  currentView: EditorView;
  transcriptEditMode: boolean;
  onTranscriptEditModeChange?: (editMode: boolean) => void;
}) {
  const sessionEvent = useSessionEvent(sessionId);
  const hasTranscript = useHasTranscript(sessionId);
  const { audioExists, audioExistsResolved } = useAudioPlayer();
  // The exist query starts unresolved on every fresh mount and, until the
  // ZICK-319 Fix-Runde B1 fix in audio-player/provider.tsx lands its next
  // fetch, can also sit on a stale cached "false" after a recording just
  // finished. Either way, "no content" is not yet a known fact -- treat it
  // like every other busy/uncertain state below instead of falling through
  // to the calendar-event gate and disappearing.
  const postStopProcessing = useListener(
    (state) => state.live.postStopProcessingBySession[sessionId] ?? false,
  );
  const now = useNow();
  const endedAt = sessionEvent?.ended_at
    ? safeParseDate(sessionEvent.ended_at)
    : null;
  const ended = !!endedAt && endedAt.getTime() <= now.getTime();
  const canEditTranscript =
    currentView.type === "transcript" &&
    sessionMode === "inactive" &&
    hasTranscript &&
    (!sessionEvent || ended) &&
    onTranscriptEditModeChange;

  if (canEditTranscript) {
    return (
      <TranscriptEditButton
        editMode={transcriptEditMode}
        onEditModeChange={onTranscriptEditModeChange}
      />
    );
  }

  const canResume = audioExists || hasTranscript;
  const contentUnknown = !audioExistsResolved || postStopProcessing;

  // A session that already has content (recorded audio or a transcript)
  // stays actionable no matter what it is doing right now -- finalizing,
  // running a batch repair, or plain inactive -- because Resume must reach
  // it in every one of those states (ZICK-319). The same is true for any
  // session mid-recording/repair even without content yet: active,
  // finalizing and running_batch always render a pill (Stop, Resume-with-
  // spinner, or Stop-transcription; HeaderMeetingActionPill sorts out which).
  // A session whose content is not yet KNOWN (the exist query hasn't
  // resolved, or the post-stop lease is still held) gets the same treatment
  // -- it might turn out to have content once that settles (Fix-Runde B1).
  // Only a genuinely idle session with confirmed no content still depends on
  // the calendar event below.
  if (sessionMode !== "inactive" || canResume || contentUnknown) {
    return (
      <HeaderMeetingActionPill
        sessionId={sessionId}
        event={sessionEvent}
        sessionMode={sessionMode}
        hasTranscript={hasTranscript}
        audioExists={audioExists}
      />
    );
  }

  if (!sessionEvent) {
    return (
      <HeaderMeetingActionPill
        sessionId={sessionId}
        event={null}
        sessionMode={sessionMode}
        hasTranscript={hasTranscript}
        audioExists={audioExists}
      />
    );
  }

  if (ended) {
    return null;
  }

  return (
    <HeaderMeetingActionPill
      sessionId={sessionId}
      event={sessionEvent}
      sessionMode={sessionMode}
      hasTranscript={hasTranscript}
      audioExists={audioExists}
    />
  );
}

function HeaderMeetingActionPill({
  sessionId,
  event,
  sessionMode,
  hasTranscript,
  audioExists,
}: {
  sessionId: string;
  event: {
    meeting_link?: string;
    tracking_id?: string;
  } | null;
  sessionMode: SessionMode;
  hasTranscript: boolean;
  audioExists: boolean;
}) {
  const startListening = useStartListening(sessionId);
  const resumeAfterStop = useResumeAfterStop(sessionId);
  const { stop, stopTranscription } = useListener((state) => ({
    stop: state.stop,
    stopTranscription: state.stopTranscription,
  }));
  const postStopProcessing = useListener(
    (state) => state.live.postStopProcessingBySession[sessionId] ?? false,
  );
  const remote = getRemoteMeeting(event?.meeting_link);
  const meetingLink = event?.meeting_link || null;
  // Nur ein erkannter Meeting-Anbieter macht den Link zum Klickziel. Bis zum
  // 02.09.2026 galt das auch fuer die Willkommens-Notiz, deren Link auf die
  // Demo-Seite des Originals zeigte (anarlog.so) -- der Sonderweg ist weg,
  // und eine Bestands-Notiz mit dem alten Link bekommt so kein Klickziel.
  const canJoinFromHeader = Boolean(meetingLink && remote !== null);
  const meetingAccessibilityActive = useMeetingAccessibilityActive(
    canJoinFromHeader && sessionMode === "inactive",
  );
  const canResume = audioExists || hasTranscript;
  // Finalizing, a running batch, and the post-stop bookkeeping window all
  // leave the pill up rather than hiding it (ZICK-319 Strecke B) -- they are
  // "busy": clickable, spinning, and routed through resume rather than
  // disabled the way finalizing used to be before it was ever reachable here.
  const isBusy =
    sessionMode === "finalizing" ||
    sessionMode === "running_batch" ||
    postStopProcessing;
  const { t } = useLingui();
  const joiningMeetingRef = useRef(false);
  const [joiningMeeting, setJoiningMeeting] = useState(false);
  const start = useCallback(async () => {
    if (!isMainWebviewWindow()) {
      await requestMainListenerControl("start", sessionId);
      return;
    }

    await startListening();
  }, [sessionId, startListening]);
  const resume = useCallback(async () => {
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
  const openMeeting = useCallback(async () => {
    if (!meetingLink) {
      return;
    }

    void openerCommands.openUrl(meetingLink, null);
  }, [meetingLink]);
  const joinMeeting = useCallback(async () => {
    if (joiningMeetingRef.current) {
      return;
    }

    joiningMeetingRef.current = true;
    setJoiningMeeting(true);
    try {
      await Promise.all([openMeeting(), start()]);
    } finally {
      joiningMeetingRef.current = false;
      setJoiningMeeting(false);
    }
  }, [openMeeting, start]);
  const countdown = useEventCountdown(sessionId);
  const stopListening = useCallback(() => {
    if (!isMainWebviewWindow()) {
      void requestMainListenerControl("stop", sessionId);
      return;
    }

    stop();
  }, [sessionId, stop]);
  const stopBatch = useCallback(() => {
    void stopTranscription(sessionId);
  }, [sessionId, stopTranscription]);
  const action = (() => {
    if (sessionMode === "active") {
      return {
        label: t`Stop`,
        title: t`Stop listening`,
        icon: <Square className="size-3 text-red-500" weight="fill" />,
        onClick: stopListening,
      };
    }

    if (isBusy) {
      // A running batch with nothing to resume into (a pure import, no
      // audio and no earlier transcript on this session) keeps the old
      // "Stop transcription" action -- there is no recording to append to.
      // Everything else that is busy (finalizing, a batch repair over
      // content that already exists, or the post-stop lease) resumes.
      // canResumeSession is the single check shared with the context menu
      // and the overflow menu (ZICK-319 Fix-Runde B2).
      if (!canResumeSession({ sessionMode, hasContent: canResume })) {
        return {
          label: t`Stop`,
          title: t`Stop transcription`,
          icon: <Square className="size-3 text-red-500" weight="fill" />,
          onClick: stopBatch,
        };
      }

      return {
        label: t`Resume`,
        title: t`Resume listening`,
        icon: <Spinner size={12} />,
        onClick: () => {
          void resume();
        },
      };
    }

    // A session that already has content stays resumable even next to a
    // meeting link -- resuming the existing recording beats re-joining.
    if (canResume) {
      return {
        label: t`Resume`,
        title: t`Resume listening`,
        icon: <RecordingIcon />,
        onClick: () => {
          void resume();
        },
      };
    }

    if (canJoinFromHeader && !meetingAccessibilityActive) {
      return {
        label: t`Join & record`,
        title: t`Join meeting and record`,
        icon: remote ? getMeetingDisplay(remote.type).icon : undefined,
        onClick: () => {
          void joinMeeting();
        },
      };
    }

    return {
      label: t`Record`,
      title: t`Record`,
      icon: <RecordingIcon />,
      onClick: start,
    };
  })();
  // Busy states used to be unreachable (finalizing rendered null higher up)
  // or already carried their own live/click-through affordance
  // (running_batch); now that finalizing renders here too, nothing about
  // being busy disables the pill -- only an in-flight join does.
  const disabled = joiningMeeting;
  const isPrimaryCta = sessionMode === "inactive";
  const showCountdown =
    Boolean(countdown.label) &&
    sessionMode !== "active" &&
    sessionMode !== "running_batch" &&
    sessionMode !== "finalizing";

  return (
    <div className="relative mr-1 flex min-w-0 shrink-0 items-center">
      <button
        type="button"
        data-tauri-drag-region="false"
        aria-label={action.label}
        title={action.title}
        disabled={disabled}
        onClick={action.onClick}
        className={cn([
          "flex h-7 max-w-56 shrink-0 items-center gap-1.5 overflow-hidden rounded-full border pr-2.5 pl-1.5",
          "text-sm font-medium",
          "transition-colors",
          isPrimaryCta
            ? "border-primary bg-primary text-primary-foreground shadow-sm dark:border-white dark:bg-white dark:text-black"
            : "border-border bg-card text-foreground",
          !disabled &&
            (isPrimaryCta
              ? "hover:bg-primary/90 dark:hover:bg-white/90"
              : "hover:bg-accent"),
          disabled && "cursor-default opacity-60",
        ])}
      >
        {action.icon}
        <span className="truncate">{action.label}</span>
      </button>
      {showCountdown ? (
        <div
          data-header-meeting-countdown
          className="border-border bg-popover text-popover-foreground pointer-events-none absolute top-full left-1/2 z-20 mt-2 -translate-x-1/2 rounded-md border px-2.5 py-1 font-mono text-xs whitespace-nowrap tabular-nums shadow-sm"
        >
          <span
            data-header-meeting-countdown-tail
            aria-hidden="true"
            className="border-border bg-popover absolute -top-1.5 left-1/2 size-3 -translate-x-1/2 rotate-45 border-t border-l"
          />
          <span className="relative">{countdown.label}</span>
        </div>
      ) : null}
    </div>
  );
}

function getMeetingDisplay(type: RemoteMeeting["type"]) {
  switch (type) {
    case "zoom":
      return {
        name: "Zoom",
        icon: (
          <img
            src="/assets/zoom-icon.svg"
            alt=""
            className="size-3.5 shrink-0"
          />
        ),
      };
    case "google-meet":
      return {
        name: "Meet",
        icon: (
          <img
            src="/assets/google-meet.svg"
            alt=""
            className="size-3.5 shrink-0"
          />
        ),
      };
    case "webex":
      return {
        name: "Webex",
        icon: (
          <img src="/assets/webex.png" alt="" className="size-3.5 shrink-0" />
        ),
      };
    case "teams":
      return {
        name: "Teams",
        icon: (
          <img src="/assets/teams.png" alt="" className="size-3.5 shrink-0" />
        ),
      };
    case "cal-com":
      return {
        name: "Cal.com",
        icon: <VideoCamera className="size-3.5 shrink-0" />,
      };
    default:
      return {
        name: "Meeting",
        icon: <Headset className="size-3.5 shrink-0" />,
      };
  }
}
