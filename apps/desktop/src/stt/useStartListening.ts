import { useCallback } from "react";

import { sonnerToast } from "@anlg/ui/components/ui/toast";

import { useCaptureLifecycle } from "./capture-lifecycle";
import { useListener } from "./contexts";
import { startMeetingChatCapture } from "./meeting-chat-capture";
import {
  MEETING_DISCLOSURE_MESSAGE,
  startMeetingRecordingDisclosure,
} from "./meeting-disclosure";

import { useShell } from "~/contexts/shell";
import { useConfigValue } from "~/shared/config";
import type { LiveStartBlockReason } from "~/store/zustand/listener/general-shared";
import { useTabs } from "~/store/zustand/tabs";
import {
  getLiveTranscriptionConfig,
  getTranscriptionLanguages,
} from "~/stt/capabilities";
import {
  clearCaptureLifecycleMarker,
  loadCaptureLifecycleMarker,
} from "~/stt/capture-lifecycle-storage";
import { cancelCaptureRecovery } from "~/stt/capture-recovery-requests";
import { useSessionParticipantHumanIds } from "~/stt/queries";

export {
  getPostCaptureAction,
  getPostCaptureRepairReasons,
  type PostCaptureRepairReason,
} from "./capture-lifecycle";
export {
  MEETING_DISCLOSURE_MESSAGE,
  sendMeetingRecordingDisclosure,
} from "./meeting-disclosure";
export { useResumeListeningLifecycle } from "./resume-listening";

export function useStartListening(sessionId: string) {
  const {
    conn,
    createCaptureLifecycle,
    session,
    setStopMeetingChatCapture,
    stopMeetingChatTasks,
  } = useCaptureLifecycle(sessionId);
  const participantHumanIds = useSessionParticipantHumanIds(sessionId);
  const getSessionMode = useListener((state) => state.getSessionMode);
  const canStartLiveSession = useListener((state) => state.canStartLiveSession);

  const aiLanguage = useConfigValue("ai_language");
  const spokenLanguages = useConfigValue("spoken_languages");
  const dictionaryTerms = useConfigValue("personalization_dictionary_terms");
  const microphoneDevice = useConfigValue("microphone_device");
  const speakerDevice = useConfigValue("speaker_device");
  const meetingDisclosureAutoSendChat = useConfigValue(
    "consent_auto_send_chat",
  );

  const start = useListener((state) => state.start);
  const stop = useListener((state) => state.stop);
  const { leftsidebar } = useShell();
  const setLeftSidebarExpanded = leftsidebar.setExpanded;
  const openNew = useTabs((state) => state.openNew);

  // Resolves true only once the native capture is actually running. Every
  // early exit below is silent by design (a duplicate click, a session that
  // is busy), but a caller that wants to report the outcome needs to tell
  // "recording" from "quietly did nothing".
  const startListening = useCallback(async (): Promise<boolean> => {
    if (!canStartLiveSession(sessionId)) {
      return false;
    }
    await stopMeetingChatTasks();
    const lifecycle = createCaptureLifecycle();
    await lifecycle.ready;
    const { getSessionKeywords } = await import("./useKeywords");
    const keywords = await getSessionKeywords({
      sessionId,
      dictionaryTerms,
    });
    const languages = getTranscriptionLanguages(aiLanguage, spokenLanguages);
    const liveTranscriptionConfig = await getLiveTranscriptionConfig({
      provider: conn?.provider,
      model: conn?.model,
      languages,
    });
    if (!canStartLiveSession(sessionId)) {
      return false;
    }
    try {
      await lifecycle.persistMarker();
    } catch (error) {
      console.error(
        "[listener] failed to prepare durable capture state",
        error,
      );
      try {
        await lifecycle.cleanupFailedStart();
      } catch (cleanupError) {
        console.error(
          "[listener] failed to clean up capture state",
          cleanupError,
        );
      }
      sonnerToast.error(
        "Mitschnitt could not safely start recording. Please try again.",
        { id: "capture-state-persist-failed" },
      );
      return false;
    }

    let started = false;
    try {
      started = await start(
        {
          session_id: sessionId,
          languages: liveTranscriptionConfig.languages,
          onboarding: false,
          model: conn?.model ?? "",
          base_url: conn?.baseUrl ?? "",
          api_key: conn?.apiKey ?? "",
          keywords,
          mic_device: microphoneDevice || null,
          speaker_device: speakerDevice || null,
          transcription_mode: liveTranscriptionConfig.transcriptionMode,
          participant_human_ids: participantHumanIds,
          self_human_id: session?.user_id || null,
        },
        {
          handlePersist: lifecycle.handlePersist,
          onStopped: lifecycle.onStopped,
        },
      );
    } catch (error) {
      console.error("[listener] failed to start recording", error);
      try {
        await lifecycle.cleanupFailedStart();
      } catch (cleanupError) {
        console.error(
          "[listener] failed to clean up capture state",
          cleanupError,
        );
      }
      sonnerToast.error(
        "Mitschnitt could not safely start recording. Please try again.",
        { id: "capture-state-persist-failed" },
      );
      return false;
    }

    if (!started) {
      await stopMeetingChatTasks();
      try {
        await lifecycle.cleanupFailedStart();
      } catch (error) {
        console.error("[listener] failed to clean up capture state", error);
        sonnerToast.error(
          "Mitschnitt could not safely start recording. Please try again.",
          { id: "capture-state-persist-failed" },
        );
      }
      return false;
    }

    if (!conn) {
      sonnerToast.warning("Live transcription is not configured", {
        id: "recording-without-transcription",
        duration: Infinity,
        description:
          "Audio is being saved. Choose a transcription provider to ensure this recording can be transcribed.",
        action: {
          label: "Configure",
          onClick: () => {
            openNew({
              type: "settings",
              state: { tab: "transcription" },
            });
          },
        },
      });
    }

    setLeftSidebarExpanded(false);

    setStopMeetingChatCapture(
      startMeetingChatCapture({
        sessionId,
        excludedTexts: [MEETING_DISCLOSURE_MESSAGE],
        onParticipantDeclined: () => {
          sonnerToast.warning(
            "A participant declined recording. Mitschnitt stopped listening.",
            { id: "meeting-consent-declined", duration: Infinity },
          );
          stop();
        },
      }),
    );

    if (meetingDisclosureAutoSendChat) {
      startMeetingRecordingDisclosure(
        sessionId,
        () => getSessionMode(sessionId) === "active",
      );
    }

    return true;
  }, [
    aiLanguage,
    canStartLiveSession,
    conn,
    createCaptureLifecycle,
    dictionaryTerms,
    getSessionMode,
    microphoneDevice,
    speakerDevice,
    openNew,
    participantHumanIds,
    session,
    sessionId,
    setStopMeetingChatCapture,
    setLeftSidebarExpanded,
    meetingDisclosureAutoSendChat,
    spokenLanguages,
    start,
    stop,
    stopMeetingChatTasks,
  ]);

  return startListening;
}

export type ResumeAfterStopResult =
  | { status: "started" }
  | {
      status: "blocked";
      reason: LiveStartBlockReason | "start_failed" | null;
    };

// One resume per session at a time. A second click while the first is still
// clearing the way would retire the marker the first one just wrote and race
// it into a second capture.
const resumesInFlight = new Map<string, Promise<ResumeAfterStopResult>>();

// A capture that was stopped by accident leaves a marker behind whenever the
// repair run that owned it was cancelled. The marker carries a foreign
// transcript id, and saveCaptureLifecycleMarker refuses to overwrite one (its
// UPSERT only updates a row with a matching transcript id and then reports
// zero affected rows), so the next start would die on a marker write instead
// of recording. Retire it deliberately and say so.
// Never throws: both reads and the delete race a recovery run that may clear
// the same row (the delete asserts one affected row and the Rust side throws
// when it is zero). A marker we failed to retire only costs the start attempt
// that follows, and that one reports its own failure.
async function retireStrandedCaptureMarker(sessionId: string) {
  try {
    const marker = await loadCaptureLifecycleMarker(sessionId);
    if (!marker) {
      return;
    }

    console.warn(
      "[listener] retiring a stranded capture marker before resuming",
      { sessionId, transcriptId: marker.transcriptId },
    );
    await clearCaptureLifecycleMarker(sessionId, marker.transcriptId);
  } catch (error) {
    console.warn(
      "[listener] could not retire a stranded capture marker",
      error,
    );
  }
}

// One button: keep recording, whatever the session is doing right now. Aborts
// a running transcription, waits out the post-stop bookkeeping, then starts a
// capture that appends to the existing recording.
export function useResumeAfterStop(sessionId: string) {
  const startListening = useStartListening(sessionId);
  const resumeAfterStop = useListener((state) => state.resumeAfterStop);
  const abandonCaptureRecoveryFinalization = useListener(
    (state) => state.abandonCaptureRecoveryFinalization,
  );
  const getLiveStartBlockReason = useListener(
    (state) => state.getLiveStartBlockReason,
  );

  return useCallback((): Promise<ResumeAfterStopResult> => {
    const running = resumesInFlight.get(sessionId);
    if (running) {
      return running;
    }

    const resume = async (): Promise<ResumeAfterStopResult> => {
      // Held across the whole resume: aborting the transcription makes the
      // repair run ask for a recovery, which would finalize the session again.
      const releaseRecoveryGuard = cancelCaptureRecovery(sessionId);
      try {
        if ((await resumeAfterStop(sessionId)) === "blocked") {
          // Last resort, never a shortcut. A recovery run whose component was
          // unmounted mid-flight leaves its post-stop lease behind and nobody
          // will ever release it; a LIVING run holds the same flag, and cutting
          // that one loose would let it delete the audio of the recording we
          // are about to start. A living run always ends by itself, so the
          // wait above simply succeeds for it. Only once the wait gave up on
          // that very flag do we break the lease - and then exactly one more
          // attempt. The price is the full timeout in the dead case.
          const blockReason = getLiveStartBlockReason(sessionId);
          const brokeDeadLease =
            blockReason === "post_stop_processing" &&
            abandonCaptureRecoveryFinalization(sessionId);

          if (
            !brokeDeadLease ||
            (await resumeAfterStop(sessionId)) === "blocked"
          ) {
            return {
              status: "blocked",
              reason: getLiveStartBlockReason(sessionId) ?? blockReason,
            };
          }
        }

        await retireStrandedCaptureMarker(sessionId);
        if (!(await startListening())) {
          // startListening bails out silently on its own guards; without this
          // the button would report success for a session that is not
          // recording.
          return {
            status: "blocked",
            reason: getLiveStartBlockReason(sessionId) ?? "start_failed",
          };
        }
        return { status: "started" };
      } finally {
        releaseRecoveryGuard();
      }
    };

    const pending = resume().finally(() => {
      if (resumesInFlight.get(sessionId) === pending) {
        resumesInFlight.delete(sessionId);
      }
    });
    resumesInFlight.set(sessionId, pending);
    return pending;
  }, [
    abandonCaptureRecoveryFinalization,
    getLiveStartBlockReason,
    resumeAfterStop,
    sessionId,
    startListening,
  ]);
}
