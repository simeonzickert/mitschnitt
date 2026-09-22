import { useCallback, useRef } from "react";

import { commands as fsSyncCommands } from "@anlg/plugin-fs-sync";
import { sonnerToast } from "@anlg/ui/components/ui/toast";

import { useListener } from "./contexts";
import { cancelMeetingRecordingDisclosure } from "./meeting-disclosure";
import { persistTranscriptWrite } from "./persist-retry";
import { createTranscriptPersistenceWorker } from "./transcript-persistence-worker";
import {
  canRunBatchTranscription,
  isStoppedTranscriptionError,
  isTerminalTranscriptionError,
  useRunBatch,
} from "./useRunBatch";
import { useSTTConnection } from "./useSTTConnection";

import { requestMainAutoEnhance } from "~/ai/task-window-sync";
import {
  deleteProcessedAudioForRetention,
  normalizeAudioRetention,
} from "~/services/audio-retention";
import { getEnhancerService } from "~/services/enhancer";
import { maybeExtractVoiceprintCandidates } from "~/services/voiceprint";
import {
  catalogLocalSessionAudio,
  markSessionAudioTranscriptionComplete,
} from "~/session/attachments";
import { enqueueSessionAudioOperation } from "~/session/audio-operations";
import { flushCanonicalSessionEditorChanges } from "~/session/editor-activity";
import { frageVorZusammenfassung } from "~/session/nachfrage/gate";
import { useSession, useSessionTranscriptExistence } from "~/session/queries";
import { requestAppAttention } from "~/shared/app-attention";
import { useConfigValue } from "~/shared/config";
import { id } from "~/shared/utils";
import type {
  LiveTranscriptPersistCallback,
  OnStoppedCallback,
} from "~/store/zustand/listener/transcript";
import { isRealtimeLocalModel } from "~/stt/capabilities";
import {
  type CaptureLifecycleMarker,
  clearCaptureLifecycleMarker,
  saveCaptureLifecycleMarker,
} from "~/stt/capture-lifecycle-storage";
import { requestCaptureRecovery } from "~/stt/capture-recovery-requests";
import {
  applyLiveTranscriptDeltaToDatabase,
  createLiveTranscript,
  flushLiveTranscriptDeltasToDatabase,
  softDeleteTranscript,
  transcriptExists,
  useSessionParticipantHumanIds,
} from "~/stt/queries";
import { waitForSessionSearchIndex } from "~/stt/search-index-consistency";

export async function getAudioDurationMs(audioPath: string) {
  try {
    const metadataResult = await fsSyncCommands.audioSourceMetadata(audioPath);
    if (metadataResult.status === "error") {
      return null;
    }

    const durationMs = metadataResult.data.durationMs;
    return typeof durationMs === "number" && Number.isFinite(durationMs)
      ? Math.max(0, durationMs)
      : null;
  } catch {
    return null;
  }
}

async function getExistingAudioDurationMs(sessionId: string) {
  try {
    const pathResult = await fsSyncCommands.audioPath(sessionId);
    if (pathResult.status === "error") {
      return 0;
    }

    return (await getAudioDurationMs(pathResult.data)) ?? 0;
  } catch {
    return 0;
  }
}

async function requestCaptureRecoverySafely(sessionId: string) {
  try {
    await requestCaptureRecovery(sessionId);
  } catch (error) {
    console.error("[listener] failed to request capture recovery", error);
  }
}

type PostCaptureDetails = {
  audioPath: string | null;
  liveTranscriptionActive: boolean;
  needsBatchRepair: boolean;
  refineSpeakerDiarization?: boolean;
  transcriptWriteFailed?: boolean;
};

export type PostCaptureRepairReason =
  | "live_transcription_unavailable"
  | "live_stream_incomplete"
  | "settled_speaker_diarization"
  | "transcript_persistence_failed";

export function getPostCaptureRepairReasons(
  details: PostCaptureDetails,
): PostCaptureRepairReason[] {
  const reasons: PostCaptureRepairReason[] = [];
  if (!details.liveTranscriptionActive) {
    reasons.push("live_transcription_unavailable");
  }
  if (details.needsBatchRepair) {
    reasons.push("live_stream_incomplete");
  }
  if (details.refineSpeakerDiarization) {
    reasons.push("settled_speaker_diarization");
  }
  if (details.transcriptWriteFailed) {
    reasons.push("transcript_persistence_failed");
  }
  return reasons;
}

export function getPostCaptureAction(
  details: PostCaptureDetails,
  canRunBatch: boolean,
) {
  const liveTranscriptComplete =
    details.liveTranscriptionActive &&
    !details.needsBatchRepair &&
    !details.transcriptWriteFailed;

  if (liveTranscriptComplete && !details.refineSpeakerDiarization) {
    return "enhance_only" as const;
  }

  if (!!details.audioPath && canRunBatch) {
    return "batch_then_enhance" as const;
  }

  if (liveTranscriptComplete) {
    return "enhance_only" as const;
  }

  return "none" as const;
}

export function useCaptureLifecycle(sessionId: string) {
  const session = useSession(sessionId);
  const transcriptExistence = useSessionTranscriptExistence(sessionId);
  const participantHumanIds = useSessionParticipantHumanIds(sessionId);
  const audioRetention = normalizeAudioRetention(
    useConfigValue("audio_retention"),
  );
  const rememberSpeakers = useConfigValue("remember_speakers") === true;
  const { conn, localBatchDiarizationAvailable } = useSTTConnection();
  const runBatch = useRunBatch(sessionId);
  const setBatchTranscriptionPending = useListener(
    (state) => state.setBatchTranscriptionPending,
  );

  const runBatchRef = useRef(runBatch);
  const canRunBatchRef = useRef(canRunBatchTranscription(conn));
  const localBatchDiarizationAvailableRef = useRef(
    localBatchDiarizationAvailable,
  );
  const stopMeetingChatCaptureRef = useRef<(() => Promise<void>) | null>(null);
  runBatchRef.current = runBatch;
  canRunBatchRef.current = canRunBatchTranscription(conn);
  localBatchDiarizationAvailableRef.current = localBatchDiarizationAvailable;

  const stopMeetingChatTasks = useCallback(async () => {
    const stop = stopMeetingChatCaptureRef.current;
    if (!stop) {
      return;
    }
    await stop();
    if (stopMeetingChatCaptureRef.current === stop) {
      stopMeetingChatCaptureRef.current = null;
    }
  }, []);
  const setStopMeetingChatCapture = useCallback(
    (stop: (() => Promise<void>) | null) => {
      stopMeetingChatCaptureRef.current = stop;
    },
    [],
  );

  const createCaptureLifecycle = useCallback(
    (recoveredMarker?: CaptureLifecycleMarker) => {
      const transcriptId = recoveredMarker?.transcriptId ?? id();
      let transcriptCreated: boolean | null = recoveredMarker ? null : false;
      let transcriptTouched = false;
      const startedAt = recoveredMarker?.startedAt ?? Date.now();
      const memoMd = recoveredMarker?.memo ?? session?.raw_md ?? "";
      const createdAt = recoveredMarker?.createdAt ?? new Date().toISOString();
      const preserveExistingTranscript =
        recoveredMarker?.preserveExistingTranscript ??
        transcriptExistence !== false;
      const ownerUserId =
        recoveredMarker?.ownerUserId ?? session?.user_id ?? "";
      const provider = recoveredMarker?.provider ?? conn?.provider;
      const model = recoveredMarker?.model ?? conn?.model;
      const hasMultipleRemoteParticipants =
        new Set(
          participantHumanIds.filter(
            (humanId) => humanId && humanId !== ownerUserId,
          ),
        ).size > 1;
      const shouldUseLocalBatchForSpeakerDiarization = () =>
        hasMultipleRemoteParticipants &&
        localBatchDiarizationAvailableRef.current &&
        isRealtimeLocalModel(model);
      const shouldRefineSpeakerDiarization = () =>
        hasMultipleRemoteParticipants &&
        ((provider === "anarlog" && model === "cloud") ||
          shouldUseLocalBatchForSpeakerDiarization());
      let pendingSummaryMode = recoveredMarker?.summaryMode;
      let capturePhase =
        recoveredMarker?.phase ??
        (recoveredMarker?.summaryMode ? "finalizing" : "capturing");
      const existingAudioDurationPromise = recoveredMarker
        ? Promise.resolve(recoveredMarker.audioOffsetMs)
        : preserveExistingTranscript
          ? getExistingAudioDurationMs(sessionId)
          : Promise.resolve(0);
      let transcriptWriteError: unknown;
      let recoveryPending = Boolean(recoveredMarker);
      let recoveryStateCleared = false;
      let batchTranscriptionPending = false;
      const updateBatchTranscriptionPending = (pending: boolean) => {
        if (batchTranscriptionPending === pending) {
          return;
        }
        batchTranscriptionPending = pending;
        setBatchTranscriptionPending(sessionId, pending);
      };
      const transcriptPersistence = createTranscriptPersistenceWorker(
        (delta) =>
          persistTranscriptWrite(async () => {
            transcriptCreated ??= await transcriptExists(transcriptId);
            if (!transcriptCreated) {
              await createLiveTranscript(
                {
                  id: transcriptId,
                  sessionId,
                  ownerUserId,
                  createdAt,
                  startedAt,
                  memo: memoMd,
                  source: "live_capture",
                  provider,
                  model,
                },
                delta,
              );
              transcriptCreated = true;
              return;
            }

            await applyLiveTranscriptDeltaToDatabase(transcriptId, delta);
          }),
        (error) => {
          transcriptWriteError = error;
          console.error("[listener] failed to persist transcript", error);
        },
        {
          afterFlush: () =>
            persistTranscriptWrite(() =>
              flushLiveTranscriptDeltasToDatabase(transcriptId),
            ),
        },
      );
      const marker = async (): Promise<CaptureLifecycleMarker> => ({
        version: 1,
        phase: capturePhase,
        sessionId,
        transcriptId,
        startedAt,
        createdAt,
        audioOffsetMs: await existingAudioDurationPromise,
        preserveExistingTranscript,
        ownerUserId,
        memo: memoMd,
        ...(provider ? { provider } : {}),
        ...(model ? { model } : {}),
        ...(pendingSummaryMode ? { summaryMode: pendingSummaryMode } : {}),
      });
      const finalizeStoppedInner = async (
        details: Parameters<OnStoppedCallback>[1],
        requestRecoveryOnFailure: boolean,
      ) => {
        sonnerToast.dismiss("recording-without-transcription");
        sonnerToast.dismiss("live-transcription-stalled");
        sonnerToast.dismiss("meeting-disclosure-send-failed");
        const notifyFailure = (message: string, id: string) => {
          if (requestRecoveryOnFailure) {
            sonnerToast.error(message, { id });
          }
        };
        const requestRecovery = async () => {
          recoveryPending = true;
          if (requestRecoveryOnFailure) {
            await requestCaptureRecoverySafely(sessionId);
          }
        };
        const finishCaptureSyncDeferral = async (): Promise<boolean> => {
          if (capturePhase !== "finalizing") {
            capturePhase = "finalizing";
            try {
              await persistTranscriptWrite(async () => {
                await saveCaptureLifecycleMarker(await marker());
              });
            } catch (error) {
              console.error(
                "[listener] failed to finalize capture recovery state",
                error,
              );
              await requestRecovery();
              return false;
            }
          }
          return true;
        };
        cancelMeetingRecordingDisclosure(sessionId);
        await stopMeetingChatTasks();
        if (details.audioPath) {
          try {
            await enqueueSessionAudioOperation(sessionId, () =>
              catalogLocalSessionAudio(sessionId),
            );
          } catch (error) {
            console.error("[listener] failed to catalog recorded audio", error);
          }
        }
        await transcriptPersistence.flush();
        transcriptCreated ??= await transcriptExists(transcriptId);
        const useLocalBatchForSpeakerDiarization =
          shouldUseLocalBatchForSpeakerDiarization();
        const refineSpeakerDiarization = shouldRefineSpeakerDiarization();
        if (transcriptCreated) {
          try {
            await waitForSessionSearchIndex(sessionId);
          } catch (error) {
            console.warn(
              "[listener] search index finalization is still pending",
              error,
            );
          }
        }

        const postCaptureAction = pendingSummaryMode
          ? ("enhance_only" as const)
          : getPostCaptureAction(
              {
                ...details,
                refineSpeakerDiarization,
                transcriptWriteFailed: Boolean(transcriptWriteError),
              },
              canRunBatchRef.current,
            );
        const repairReasons = pendingSummaryMode
          ? []
          : getPostCaptureRepairReasons({
              ...details,
              refineSpeakerDiarization,
              transcriptWriteFailed: Boolean(transcriptWriteError),
            });

        let batchCompleted = false;
        if (postCaptureAction === "batch_then_enhance") {
          updateBatchTranscriptionPending(true);
          console.info("[listener] starting post-stop transcript repair", {
            sessionId,
            reasons: repairReasons,
          });
          try {
            const existingAudioDurationMs = await existingAudioDurationPromise;
            const finalAudioDurationMs = preserveExistingTranscript
              ? await getAudioDurationMs(details.audioPath!)
              : null;
            const audioOffsetMs =
              existingAudioDurationMs > 0 &&
              finalAudioDurationMs !== null &&
              finalAudioDurationMs + 1_000 >= existingAudioDurationMs
                ? Math.min(existingAudioDurationMs, finalAudioDurationMs)
                : 0;
            await runBatchRef.current(details.audioPath!, {
              deferAudioFinalization: true,
              notifyOnCompletion: !details.liveTranscriptionActive,
              ...(useLocalBatchForSpeakerDiarization
                ? {
                    provider: "soniqo",
                    model: "soniqo-parakeet-batch",
                    baseUrl: "soniqo://local",
                    apiKey: "",
                  }
                : {}),
              promotion:
                preserveExistingTranscript || transcriptCreated
                  ? {
                      scope: "current_capture",
                      audioOffsetMs,
                      ...(transcriptCreated
                        ? { replaceTranscriptId: transcriptId }
                        : {}),
                      startedAt,
                    }
                  : { scope: "whole_session" },
            });
            batchCompleted = true;
            console.info("[listener] completed post-stop transcript repair", {
              sessionId,
              reasons: repairReasons,
            });
          } catch (error) {
            if (isStoppedTranscriptionError(error)) {
              await requestRecovery();
              return;
            }
            console.error("[listener] post-stop transcript repair failed", {
              sessionId,
              reasons: repairReasons,
              error,
            });
            if (transcriptWriteError || !details.liveTranscriptionActive) {
              notifyFailure(
                "Mitschnitt could not finish saving the transcript. The recording was kept so you can try again.",
                "post-capture-transcript-incomplete",
              );
            } else {
              notifyFailure(
                "Post-meeting transcription failed. The recording was kept so you can try again.",
                "post-capture-batch-failed",
              );
            }
            if (isTerminalTranscriptionError(error)) {
              try {
                await clearCaptureLifecycleMarker(sessionId, transcriptId);
                recoveryPending = false;
                recoveryStateCleared = true;
              } catch (clearError) {
                console.error(
                  "[listener] failed to stop automatic capture recovery",
                  clearError,
                );
                await requestRecovery();
              }
              return;
            }
            await requestRecovery();
            return;
          }
        }

        if (
          transcriptWriteError &&
          postCaptureAction !== "batch_then_enhance"
        ) {
          notifyFailure(
            details.audioPath
              ? "Mitschnitt could not finish saving the transcript. The recording was kept so you can try again."
              : "Mitschnitt could not save part of the live transcript.",
            details.audioPath
              ? "post-capture-transcript-incomplete"
              : "live-transcript-persist-failed",
          );
        }

        const emptyFreshCapture =
          !recoveredMarker &&
          !details.audioPath &&
          !transcriptTouched &&
          !transcriptWriteError;
        const transcriptIsComplete =
          Boolean(pendingSummaryMode) ||
          batchCompleted ||
          postCaptureAction === "enhance_only" ||
          emptyFreshCapture;
        if (!transcriptIsComplete) {
          await requestRecovery();
          return;
        }
        if (!(await finishCaptureSyncDeferral())) {
          return;
        }

        // Batch repair already requests attention when runBatchSession
        // finishes; a recovered summary-only pass has no new transcript.
        if (
          !batchCompleted &&
          !pendingSummaryMode &&
          (transcriptTouched || preserveExistingTranscript)
        ) {
          void requestAppAttention();
        }

        try {
          await flushCanonicalSessionEditorChanges(sessionId);
        } catch (error) {
          console.error(
            "[listener] failed to flush session notes before completing capture",
            error,
          );
          await requestRecovery();
          return;
        }

        const hasTranscriptEvidence =
          Boolean(pendingSummaryMode) ||
          preserveExistingTranscript ||
          transcriptTouched ||
          batchCompleted;
        const shouldEnhance =
          hasTranscriptEvidence &&
          (transcriptIsComplete ||
            (postCaptureAction === "none" &&
              preserveExistingTranscript &&
              !transcriptWriteError));

        let summaryScheduled = true;
        if (shouldEnhance) {
          const summaryMode =
            pendingSummaryMode ??
            (preserveExistingTranscript && (transcriptTouched || batchCompleted)
              ? "regenerate"
              : "if_empty");
          if (!pendingSummaryMode) {
            pendingSummaryMode = summaryMode;
            try {
              await persistTranscriptWrite(async () => {
                await saveCaptureLifecycleMarker(await marker());
              });
            } catch (error) {
              pendingSummaryMode = undefined;
              console.error(
                "[listener] failed to persist summary recovery state",
                error,
              );
              notifyFailure(
                "The transcript was saved, but Mitschnitt could not start the summary. Try generating it again.",
                "post-capture-summary-failed",
              );
              await requestRecovery();
              return;
            }
          }
          // Wer war dabei, und wie heisst das Gespraech: die eine Frage vor
          // der Zusammenfassung. Sie steht hier und nicht frueher, weil
          // Transkript und Wiederherstellungs-Merker zu diesem Zeitpunkt
          // geschrieben sind -- die Aufnahme wartet auf nichts. Ohne
          // montierten Dialog oder mit Kalendertermin kehrt der Aufruf sofort
          // zurueck.
          await frageVorZusammenfassung(sessionId);

          try {
            const service = getEnhancerService();
            if (!service) {
              await requestMainAutoEnhance(sessionId, summaryMode);
            } else {
              await service.requestAutoEnhance(sessionId, summaryMode);
            }
          } catch (error) {
            summaryScheduled = false;
            console.error("[listener] failed to schedule summary", error);
            notifyFailure(
              "The transcript was saved, but Mitschnitt could not start the summary. Try generating it again.",
              "post-capture-summary-failed",
            );
          }
        }

        const recoveryComplete =
          (!details.audioPath && !transcriptWriteError) ||
          (transcriptIsComplete && summaryScheduled);
        if (!recoveryComplete) {
          await requestRecovery();
          return;
        }

        try {
          if (details.audioPath && transcriptIsComplete) {
            await maybeExtractVoiceprintCandidates({
              enabled: rememberSpeakers,
              sessionId,
              transcriptId,
              audioPath: details.audioPath,
            });
            await persistTranscriptWrite(() =>
              markSessionAudioTranscriptionComplete(sessionId),
            );
          }
          await clearCaptureLifecycleMarker(sessionId, transcriptId);
          recoveryPending = false;
          recoveryStateCleared = true;
          if (hasTranscriptEvidence && !batchCompleted) {
          }
        } catch (error) {
          await requestRecovery();
          throw error;
        }

        // A failed batch repair — or a live transcript that never fully
        // persisted — keeps the recording around as the only source for a
        // later repair, regardless of the retention policy.
        if (
          (postCaptureAction !== "batch_then_enhance" || batchCompleted) &&
          !transcriptWriteError
        ) {
          await deleteProcessedAudioForRetention(audioRetention, sessionId);
        }
      };
      const finalizeStopped = async (
        details: Parameters<OnStoppedCallback>[1],
        requestRecoveryOnFailure: boolean,
      ) => {
        try {
          await finalizeStoppedInner(details, requestRecoveryOnFailure);
        } catch (error) {
          if (!recoveryStateCleared && !recoveryPending) {
            recoveryPending = true;
            if (requestRecoveryOnFailure) {
              await requestCaptureRecoverySafely(sessionId);
            }
          }
          throw error;
        } finally {
          updateBatchTranscriptionPending(false);
        }
      };
      const onStopped: OnStoppedCallback = (_sessionId, details) => {
        recoveryPending = false;
        markExpectedPostStopBatch(details);
        return finalizeStopped(details, true);
      };
      const markExpectedPostStopBatch = (
        details: Parameters<OnStoppedCallback>[1],
      ) => {
        if (
          !pendingSummaryMode &&
          details.audioPath &&
          canRunBatchRef.current &&
          (!details.liveTranscriptionActive ||
            details.needsBatchRepair ||
            shouldRefineSpeakerDiarization())
        ) {
          updateBatchTranscriptionPending(true);
        }
      };
      const recoverStopped: OnStoppedCallback = (_sessionId, details) => {
        markExpectedPostStopBatch(details);
        return finalizeStopped(details, false);
      };

      const handlePersist: LiveTranscriptPersistCallback = (delta) => {
        if (delta.new_words.length === 0 && delta.replaced_ids.length === 0) {
          return;
        }

        transcriptTouched = true;
        transcriptPersistence.enqueue(delta);
      };

      return {
        handlePersist,
        onStopped,
        recoverStopped,
        ready: existingAudioDurationPromise.then(() => undefined),
        persistMarker: async () => {
          await persistTranscriptWrite(async () => {
            await saveCaptureLifecycleMarker(await marker());
          });
        },
        cleanupFailedStart: async () => {
          await transcriptPersistence.flush();
          await clearCaptureLifecycleMarker(sessionId, transcriptId);
          if (transcriptCreated) {
            await softDeleteTranscript(transcriptId);
          }
        },
      };
    },
    [
      audioRetention,
      conn?.model,
      conn?.provider,
      participantHumanIds,
      rememberSpeakers,
      session?.raw_md,
      session?.user_id,
      sessionId,
      setBatchTranscriptionPending,
      stopMeetingChatTasks,
      transcriptExistence,
    ],
  );

  return {
    conn,
    createCaptureLifecycle,
    session,
    setStopMeetingChatCapture,
    stopMeetingChatTasks,
  };
}
