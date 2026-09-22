import { create as mutate } from "mutative";
import type { StoreApi } from "zustand";

import {
  commands as listenerCommands,
  type CaptureParams,
} from "@anlg/plugin-transcription";
import type { TranscriptionParams } from "@anlg/plugin-transcription";

import type { BatchActions, BatchState } from "./batch";
import { runBatchSession } from "./general-batch";
import {
  attachLiveSession,
  startLiveSession,
  stopLiveSession,
  updateLiveSessionConfig,
} from "./general-live";
import {
  type GeneralState,
  type LiveStartBlockReason,
  type SessionMode,
  getLiveStartBlockReason,
  initialGeneralState,
  markLiveStartRequested,
  setLiveState,
} from "./general-shared";
import type {
  BatchPersistCallback,
  LiveTranscriptPersistCallback,
  OnStoppedCallback,
  TranscriptActions,
  TranscriptState,
} from "./transcript";

import { enqueueSessionAudioOperation } from "~/session/audio-operations";

export type { GeneralState, SessionMode } from "./general-shared";

export type GeneralActions = {
  start: (
    params: CaptureParams,
    options?: {
      handlePersist?: LiveTranscriptPersistCallback;
      onStopped?: OnStoppedCallback;
    },
  ) => Promise<boolean>;
  stop: () => void;
  attachLiveSession: (
    sessionId: string,
    options?: {
      handlePersist?: LiveTranscriptPersistCallback;
      onStopped?: OnStoppedCallback;
    },
  ) => Promise<"attached" | "inactive" | "error">;
  beginCaptureRecoveryFinalization: (sessionId: string) => boolean;
  finishCaptureRecoveryFinalization: (sessionId: string) => void;
  abandonCaptureRecoveryFinalization: (sessionId: string) => boolean;
  setMuted: (value: boolean) => void;
  setBatchTranscriptionPending: (sessionId: string, pending: boolean) => void;
  setTriggerAppIds: (appIds: string[] | null) => void;
  updateCaptureConfig: (
    update: Pick<
      CaptureParams,
      "session_id" | "languages" | "participant_human_ids" | "self_human_id"
    >,
  ) => Promise<void>;
  startTranscription: (
    params: TranscriptionParams,
    options?: {
      handlePersist?: BatchPersistCallback;
      notifyOnCompletion?: boolean;
    },
  ) => Promise<void>;
  stopTranscription: (sessionId: string) => Promise<void>;
  resumeAfterStop: (
    sessionId: string,
    options?: { timeoutMs?: number },
  ) => Promise<ResumeAfterStopOutcome>;
  canStartLiveSession: (sessionId: string) => boolean;
  getLiveStartBlockReason: (sessionId: string) => LiveStartBlockReason | null;
  getSessionMode: (sessionId: string) => SessionMode;
};

// "started" does not mean this action started a capture: it never does. It
// means the session was cleared of everything that would make `start` return
// silently, so the caller may start now. The start itself stays in
// useStartListening, which owns the lifecycle and the marker.
export type ResumeAfterStopOutcome = "started" | "blocked";

export const RESUME_AFTER_STOP_TIMEOUT_MS = 10_000;
const RESUME_AFTER_STOP_POLL_MS = 25;
const RESUME_AFTER_STOP_RETRY_STOP_MS = 500;

// A block reason that only a running operation can clear. Anything else
// (another session recording, this session already recording) will not change
// by waiting, so waiting would only stall the button.
const isTransientBlockReason = (reason: LiveStartBlockReason | null) =>
  reason === "post_stop_processing" ||
  reason === "session_finalizing" ||
  reason === "start_in_progress";

const waitUntil = async (
  predicate: () => boolean,
  deadlineMs: number,
): Promise<boolean> => {
  while (!predicate()) {
    if (Date.now() >= deadlineMs) {
      return false;
    }
    await new Promise((resolve) =>
      setTimeout(resolve, RESUME_AFTER_STOP_POLL_MS),
    );
  }
  return true;
};

export const createGeneralSlice = <
  T extends GeneralState &
    GeneralActions &
    TranscriptState &
    TranscriptActions &
    BatchActions &
    BatchState,
>(
  set: StoreApi<T>["setState"],
  get: StoreApi<T>["getState"],
): GeneralState & GeneralActions => ({
  ...initialGeneralState,
  start: async (params: CaptureParams, options) => {
    const targetSessionId = params.session_id;

    if (!targetSessionId) {
      console.error("[listener] 'start' requires a session_id");
      return false;
    }

    return enqueueSessionAudioOperation(targetSessionId, async () => {
      const currentMode = get().getSessionMode(targetSessionId);
      if (currentMode === "running_batch") {
        console.warn(
          `[listener] cannot start live session while batch processing session ${targetSessionId}`,
        );
        return false;
      }

      const blockReason = getLiveStartBlockReason(get().live, targetSessionId);
      if (blockReason) {
        console.warn(`[listener] cannot start live session: ${blockReason}`);
        return false;
      }

      setLiveState(set, (live) => {
        markLiveStartRequested(live, targetSessionId);
      });

      if (options?.handlePersist) {
        get().setTranscriptPersist(targetSessionId, options.handlePersist);
      }
      if (options?.onStopped) {
        get().setOnStopped(targetSessionId, options.onStopped);
      }

      const started = await startLiveSession(set, get, targetSessionId, params);
      if (!started) {
        if (options?.handlePersist) {
          get().setTranscriptPersist(targetSessionId, undefined);
        }
        if (options?.onStopped) {
          get().setOnStopped(targetSessionId, undefined);
        }
      }

      return started;
    });
  },
  stop: () => {
    stopLiveSession(set, get);
  },
  attachLiveSession: async (sessionId, options) => {
    if (!sessionId) {
      return "inactive";
    }

    return attachLiveSession(set, get, sessionId, options);
  },
  beginCaptureRecoveryFinalization: (sessionId) => {
    let started = false;
    setLiveState(set, (live) => {
      if (live.postStopProcessingBySession[sessionId]) {
        return;
      }
      live.postStopProcessingBySession[sessionId] = true;
      live.recoveryFinalizationBySession[sessionId] = true;
      started = true;
    });
    return started;
  },
  // Only ever gives back what a recovery run actually took. A straggler whose
  // lease was already broken from the outside would otherwise delete the
  // post-stop processing of an unrelated normal stop, and worse: reset
  // loading/sessionId, which is exactly the state of a start that is running
  // right now (markLiveStartRequested).
  finishCaptureRecoveryFinalization: (sessionId) => {
    setLiveState(set, (live) => {
      if (!live.recoveryFinalizationBySession[sessionId]) {
        return;
      }
      delete live.postStopProcessingBySession[sessionId];
      delete live.recoveryFinalizationBySession[sessionId];
      if (live.sessionId === sessionId && live.status === "inactive") {
        live.loading = false;
        live.sessionId = null;
      }
    });
  },
  // The escape hatch for a recovery run that can no longer release its own
  // lease: its component was unmounted between two retries, so the ref that
  // remembered the ownership died with it and the session would stay blocked
  // until the app restarts. Only leases taken by a recovery run are dropped -
  // the post-stop processing of a normal stop still has to be waited out.
  //
  // A LIVING run must never be cut loose this way: it may be inside
  // recoverStopped, which runs for minutes and ends in
  // deleteProcessedAudioForRetention - it would delete the audio of the new
  // recording this resume just started. Callers therefore use this only after
  // the wait already timed out; see useResumeAfterStop.
  abandonCaptureRecoveryFinalization: (sessionId) => {
    if (!sessionId || !get().live.recoveryFinalizationBySession[sessionId]) {
      return false;
    }

    console.warn(
      `[listener] releasing an abandoned capture recovery lease for session ${sessionId}`,
    );
    get().finishCaptureRecoveryFinalization(sessionId);
    return true;
  },
  setMuted: (value) => {
    set((state) =>
      mutate(state, (draft) => {
        draft.live.muted = value;
        void listenerCommands.setMicMuted(value);
      }),
    );
  },
  setBatchTranscriptionPending: (sessionId, pending) => {
    setLiveState(set, (live) => {
      if (pending) {
        live.batchTranscriptionPendingBySession[sessionId] = true;
      } else {
        delete live.batchTranscriptionPendingBySession[sessionId];
      }
    });
  },
  setTriggerAppIds: (appIds) => {
    setLiveState(set, (live) => {
      live.triggerAppIds = appIds;
    });
  },
  updateCaptureConfig: async (update) => {
    const live = get().live;
    if (live.status !== "active" || live.sessionId !== update.session_id) {
      return;
    }

    await updateLiveSessionConfig({
      session_id: update.session_id,
      languages: update.languages,
      participant_human_ids: update.participant_human_ids ?? [],
      self_human_id: update.self_human_id ?? null,
    });
  },
  startTranscription: async (params, options) => {
    const sessionId = params.session_id;

    if (!sessionId) {
      throw new Error(
        "[listener] startTranscription requires params.session_id",
      );
    }

    const mode = get().getSessionMode(sessionId);
    if (mode === "active" || mode === "finalizing") {
      throw new Error(
        `[listener] cannot start batch processing while session ${sessionId} is live`,
      );
    }

    if (mode === "running_batch") {
      throw new Error(
        `[listener] session ${sessionId} is already processing in batch mode`,
      );
    }

    if (options?.handlePersist) {
      get().setBatchPersist(sessionId, options.handlePersist);
    }

    await runBatchSession(get, sessionId, params, {
      notifyOnCompletion: options?.notifyOnCompletion,
    });
  },
  stopTranscription: async (sessionId) => {
    if (!sessionId) {
      return;
    }

    await listenerCommands.stopTranscription(sessionId).catch(console.error);
  },
  // Clears the road for a deliberate "keep recording" after a stop: aborts a
  // running batch repair, then waits out the post-stop bookkeeping. It never
  // starts a capture itself - see ResumeAfterStopOutcome.
  //
  // Known and deliberately not handled here (own card): the Rust side emits
  // "stopped" before it actually aborts the task, and the Soniqo/Apple Speech
  // providers are not aborted at all. A late BATCH run can therefore still be
  // reading audio.mp3 while the new recorder decodes it to WAV and deletes
  // it. That one costs work, not data: its result is discarded, because the
  // stopped rejection makes startTranscription throw and no transcript is
  // created.
  //
  // A late CAPTURE RECOVERY run is a different animal and does touch data - it
  // ends in deleteProcessedAudioForRetention and would delete the audio of the
  // recording that started meanwhile. That is why nobody breaks a recovery
  // lease up front; see abandonCaptureRecoveryFinalization.
  resumeAfterStop: async (sessionId, options) => {
    if (!sessionId) {
      return "blocked";
    }

    const deadlineMs =
      Date.now() + (options?.timeoutMs ?? RESUME_AFTER_STOP_TIMEOUT_MS);

    if (get().getSessionMode(sessionId) === "running_batch") {
      // The store enters running_batch before the native registry has the
      // entry, and the "stopped" listener is only installed inside the batch
      // promise. A single stop can therefore land in a window where it finds
      // nothing to stop and no event ever arrives. So keep asking, and stop
      // asking as soon as the store left the mode.
      //
      // stopTranscription resolves when the Rust command returns; the store
      // only leaves running_batch once the "stopped" event reaches the
      // listener. Reporting before that would hand the caller a session that
      // still refuses to start.
      let nextStopRequestAtMs = 0;
      const stopped = await waitUntil(() => {
        if (get().getSessionMode(sessionId) !== "running_batch") {
          return true;
        }
        if (Date.now() >= nextStopRequestAtMs) {
          nextStopRequestAtMs = Date.now() + RESUME_AFTER_STOP_RETRY_STOP_MS;
          void get().stopTranscription(sessionId);
        }
        return false;
      }, deadlineMs);

      if (!stopped) {
        console.warn(
          `[listener] resumeAfterStop timed out waiting for the batch of session ${sessionId} to stop`,
        );
        return "blocked";
      }
    }

    const cleared = await waitUntil(() => {
      if (get().canStartLiveSession(sessionId)) {
        return true;
      }
      // Bail out of the wait loop for reasons that never resolve on their own;
      // the check below turns that into "blocked".
      return !isTransientBlockReason(
        get().getLiveStartBlockReason(sessionId),
      );
    }, deadlineMs);

    if (!cleared || !get().canStartLiveSession(sessionId)) {
      console.warn(
        `[listener] resumeAfterStop refused for session ${sessionId}: ${
          get().getLiveStartBlockReason(sessionId) ?? "unknown"
        }`,
      );
      return "blocked";
    }

    return "started";
  },
  canStartLiveSession: (sessionId) => {
    if (!sessionId) {
      return false;
    }

    if (get().getSessionMode(sessionId) === "running_batch") {
      return false;
    }

    return getLiveStartBlockReason(get().live, sessionId) === null;
  },
  getLiveStartBlockReason: (sessionId) => {
    if (!sessionId) {
      return null;
    }

    if (get().getSessionMode(sessionId) === "running_batch") {
      return "running_batch";
    }

    return getLiveStartBlockReason(get().live, sessionId);
  },
  getSessionMode: (sessionId) => {
    if (!sessionId) {
      return "inactive";
    }

    const state = get();

    if (state.live.sessionId === sessionId) {
      return state.live.status;
    }

    if (state.live.finalizingBySession[sessionId]) {
      return "finalizing";
    }

    if (state.batch[sessionId] && !state.batch[sessionId].terminalReason) {
      return "running_batch";
    }

    return "inactive";
  },
});
