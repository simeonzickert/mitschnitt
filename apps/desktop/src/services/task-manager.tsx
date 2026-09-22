import { useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";

import { events as appleCalendarEvents } from "@anlg/plugin-calendar";

import {
  AUDIO_RETENTION_INTERVAL,
  AUDIO_RETENTION_TASK_ID,
  cleanupExpiredAudio,
  type AudioCleanupResult,
} from "./audio-retention";
import {
  retentionDeadlineAnswered,
  retentionKeepEverything,
} from "./audio-retention-answers";
import { AudioRetentionConsentDialog } from "./audio-retention-consent";
import { retentionForCleanup } from "./audio-retention-policy";
import {
  CALENDAR_SYNC_TASK_ID,
  scheduleCalendarSync,
  syncCalendarEvents,
} from "./calendar";
import {
  checkEventNotifications,
  EVENT_NOTIFICATION_INTERVAL,
  EVENT_NOTIFICATION_TASK_ID,
  type NotifiedEventsMap,
} from "./event-notification";
import { cleanupOrphanedNoteAttachments } from "./note-attachment-retention";
import { cleanupExpiredVoiceprintCandidates } from "./voiceprint";

import {
  useRegisterTask,
  useScheduleTaskRun,
  useTaskScheduler,
} from "~/services/task-scheduler";
import { useSetSettingValues, useStoredSettingValue } from "~/settings/queries";
import { useConfigValue } from "~/shared/config";
import { useMountEffect } from "~/shared/hooks/useMountEffect";

const CALENDAR_SYNC_INTERVAL = 60 * 1000; // 60 sec
const CALENDAR_SYNC_MAX_DURATION = 120 * 1000; // 2 min

// Long-running tasks need explicit deadlines so a hung provider cannot block
// later deadline-driven work indefinitely.
const AUDIO_RETENTION_MAX_DURATION = 10 * 60 * 1000; // 10 min
const EVENT_NOTIFICATION_MAX_DURATION = 60 * 1000; // 60 sec
const REPEATING_TASK_MAX_RETRIES = 3;

export function TaskManager() {
  const queryClient = useQueryClient();
  const manager = useTaskScheduler();

  const notificationEvent = useConfigValue("notification_event");
  // Mitschnitt-Fork (F17). The stored value, not the resolved one: a machine
  // that has never written down how long it keeps recordings gets `null`, and
  // `cleanupExpiredAudio` deletes nothing on `null`. Reading it through
  // `useConfigValue` would hand the cleanup the six-month default and delete a
  // year of meetings nobody was asked about -- the startup task that writes the
  // answer is racing this one, which is scheduled at zero delay.
  const storedAudioRetention = useStoredSettingValue("audio_retention");
  const audioRetention = retentionForCleanup(
    storedAudioRetention.value,
    storedAudioRetention.hasValue,
  );
  // Mitschnitt-Fork (F18). Whether anybody agreed that the deadline may delete
  // what is already on THIS installation. Read from the stored row rather than
  // through `useConfigValue`, for the same reason as the deadline above: only a
  // written `true` arms the deletion, and an unreadable or missing row means
  // not armed.
  const storedRetentionConfirmed = useStoredSettingValue(
    "audio_retention_confirmed",
  );
  const retentionConfirmed = storedRetentionConfirmed.value === true;
  const setSettingValues = useSetSettingValues();
  const [retentionQuestion, setRetentionQuestion] =
    useState<AudioCleanupResult["awaitingConfirmation"]>(null);
  const notifiedEventsRef = useRef<NotifiedEventsMap>(new Map());

  useRegisterTask(
    CALENDAR_SYNC_TASK_ID,
    async (_arg, signal) => {
      await syncCalendarEvents({ signal });
    },
    { maxDuration: CALENDAR_SYNC_MAX_DURATION },
  );

  useMountEffect(() => {
    if (!manager) return;

    let timeoutId: number | undefined;
    const clearNextSync = () => {
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
        timeoutId = undefined;
      }
    };
    const scheduleNextSync = () => {
      clearNextSync();
      timeoutId = window.setTimeout(() => {
        timeoutId = undefined;
        scheduleCalendarSync(manager);
      }, CALENDAR_SYNC_INTERVAL);
    };
    const taskRunListenerId = manager.addTaskRunRunningListener(
      CALENDAR_SYNC_TASK_ID,
      null,
      (_manager, _taskId, _taskRunId, running) => {
        if (running === undefined) {
          scheduleNextSync();
        } else {
          clearNextSync();
        }
      },
    );
    const unlisten = appleCalendarEvents.calendarChangedEvent.listen(() => {
      scheduleCalendarSync(manager);
    });
    scheduleCalendarSync(manager);

    return () => {
      clearNextSync();
      manager.delListener(taskRunListenerId);
      unlisten.then((fn) => fn());
    };
  });

  useRegisterTask(
    EVENT_NOTIFICATION_TASK_ID,
    async () => {
      await checkEventNotifications(
        notificationEvent,
        notifiedEventsRef.current,
      );
    },
    {
      maxDuration: EVENT_NOTIFICATION_MAX_DURATION,
      maxRetries: REPEATING_TASK_MAX_RETRIES,
      retryDelay: EVENT_NOTIFICATION_INTERVAL,
    },
  );

  useScheduleTaskRun(EVENT_NOTIFICATION_TASK_ID, undefined, 0, {
    repeatDelay: EVENT_NOTIFICATION_INTERVAL,
  });

  useRegisterTask(
    AUDIO_RETENTION_TASK_ID,
    async () => {
      const { deletedSessionIds, awaitingConfirmation } =
        await cleanupExpiredAudio(
          audioRetention,
          Date.now(),
          retentionConfirmed,
        );
      setRetentionQuestion(awaitingConfirmation);
      for (const sessionId of deletedSessionIds) {
        void queryClient.invalidateQueries({
          queryKey: ["audio", sessionId, "exist"],
        });
        void queryClient.invalidateQueries({
          queryKey: ["audio", sessionId, "url"],
        });
      }
      void cleanupExpiredVoiceprintCandidates();
      // Mitschnitt-Fork (ISA N8): pictures taken out of a note, once their
      // tombstone is older than the grace window. Same cadence as the audio
      // deadline; its own failure never reaches the audio pass above.
      await cleanupOrphanedNoteAttachments().catch((error) => {
        console.error("[note-attachments] cleanup failed", { error });
      });
    },
    {
      maxDuration: AUDIO_RETENTION_MAX_DURATION,
      maxRetries: REPEATING_TASK_MAX_RETRIES,
      retryDelay: AUDIO_RETENTION_INTERVAL,
    },
  );

  useScheduleTaskRun(AUDIO_RETENTION_TASK_ID, undefined, 0, {
    repeatDelay: AUDIO_RETENTION_INTERVAL,
  });

  // Every hook above runs unconditionally, and must keep doing so: a hook added
  // below this line would be skipped on the passes where there is no question,
  // and React would change its mind about the hook order between renders.
  if (retentionQuestion === null) {
    return null;
  }

  return (
    <AudioRetentionConsentDialog
      policy={retentionQuestion.policy}
      expiring={retentionQuestion.expiring}
      onConfirmDeleting={() => {
        setRetentionQuestion(null);
        // The span the dialog displayed, not whatever is stored by now: the
        // answer has to be to the question that was asked.
        setSettingValues(retentionDeadlineAnswered(retentionQuestion.policy));
      }}
      onKeepEverything={() => {
        setRetentionQuestion(null);
        setSettingValues(retentionKeepEverything());
      }}
      // Not an answer: nothing is written, so the marker stays false and the
      // next pass raises the same question. Clearing the state is only what
      // lets the person reach the rest of the app in the meantime.
      onDismiss={() => setRetentionQuestion(null)}
    />
  );
}
